// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::{AddAssign, MulAssign};

use leit_core::{FieldId, FilterEvaluator, QueryNodeId, Score, ScoredHit, TermId};
use leit_query::{ExecutionPlan, QueryNode, QueryProgram};
use leit_text::FieldAnalyzers;

use crate::search::FieldHit;
use crate::{IndexError, SearchScorer};

#[derive(Clone, Copy, Debug)]
struct ReferencePosting {
    doc_id: u32,
    term_freq: u32,
}

#[derive(Debug)]
struct ReferenceTerm {
    field_id: FieldId,
    term_id: TermId,
    text: String,
}

#[derive(Clone, Copy, Debug, Default)]
struct ReferenceFieldStats {
    doc_count: u32,
    total_terms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NodeKind {
    TermExpansion,
    Filter,
}

#[derive(Debug, PartialEq, Eq)]
enum ReferenceEvalError {
    UnsupportedNode(NodeKind),
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeMap;
    use alloc::vec;

    use leit_core::{NoFilter, QueryNodeId};
    use leit_query::{ExecutionPlan, FeatureSet, QueryNode, QueryProgram};
    use leit_text::FieldAnalyzers;

    use super::{NodeKind, ReferenceEvalError, ReferenceExecutionIndex, SearchScorer};

    #[test]
    fn unsupported_term_expansion_preserves_private_kind_before_mapping() {
        let index = ReferenceExecutionIndex::from_documents(FieldAnalyzers::new(), &[], &[])
            .expect("empty reference index");
        let program = QueryProgram::new(
            vec![
                QueryNode::Term {
                    field: leit_core::FieldId::new(1),
                    term: leit_core::TermId::new(0),
                    boost: 1.0,
                },
                QueryNode::ConstantScore {
                    child: QueryNodeId::new(0),
                    score: 1.0,
                },
                QueryNode::TermExpansion {
                    children: vec![QueryNodeId::new(1)],
                    fields: vec![],
                    boost: 1.0,
                    field_weights: BTreeMap::new(),
                },
            ],
            QueryNodeId::new(2),
            3,
        );
        let private_error = index
            .evaluate_node(program.root(), &program, SearchScorer::bm25f(), &NoFilter)
            .expect_err("non-term expansion child must be unsupported");
        assert_eq!(
            private_error,
            ReferenceEvalError::UnsupportedNode(NodeKind::TermExpansion)
        );

        let public_error = index
            .execute_snapshot(
                &ExecutionPlan {
                    program,
                    selectivity: 1.0,
                    cost: 3,
                    required_features: FeatureSet::basic(),
                },
                SearchScorer::bm25f(),
                &NoFilter,
                10,
            )
            .expect_err("façade must map the private error");
        assert_eq!(public_error, crate::IndexError::UnsupportedFilterPredicate);
    }
}

#[derive(Debug, Default)]
struct ReferenceEvalResult {
    matches: BTreeSet<u32>,
    scores: BTreeMap<u32, Score>,
}

/// Frozen independent index used only to compare benchmark behavior.
///
/// Storage, traversal, and evaluator composition are independent from the
/// optimized index. Primitive score calculation delegates to [`SearchScorer`];
/// parity tests pin its expected fixture outputs with hard-coded score bits.
#[doc(hidden)]
#[derive(Debug)]
pub struct ReferenceExecutionIndex {
    analyzers: FieldAnalyzers,
    documents: BTreeSet<u32>,
    terms_to_ids: BTreeMap<(FieldId, String), TermId>,
    terms: Vec<ReferenceTerm>,
    postings: BTreeMap<TermId, Vec<ReferencePosting>>,
    field_stats: BTreeMap<FieldId, ReferenceFieldStats>,
    _field_names: BTreeMap<String, FieldId>,
    field_doc_lengths: BTreeMap<(u32, FieldId), u32>,
}

impl ReferenceExecutionIndex {
    /// Build the frozen reference from the same aliases and documents as the optimized index.
    pub fn from_documents(
        analyzers: FieldAnalyzers,
        aliases: &[(FieldId, &str)],
        documents: &[(u32, &[(FieldId, &str)])],
    ) -> Result<Self, IndexError> {
        let mut index = Self {
            analyzers,
            documents: BTreeSet::new(),
            terms_to_ids: BTreeMap::new(),
            terms: Vec::new(),
            postings: BTreeMap::new(),
            field_stats: BTreeMap::new(),
            _field_names: aliases
                .iter()
                .map(|(field, name)| ((*name).into(), *field))
                .collect(),
            field_doc_lengths: BTreeMap::new(),
        };

        for &(doc_id, fields) in documents {
            index.add_document(doc_id, fields)?;
        }
        Ok(index)
    }

    /// Return primitive document and field statistics for parity assertions.
    #[must_use]
    pub fn statistics_snapshot(&self) -> (u32, Vec<(u32, u32, u32)>) {
        let fields = self
            .field_stats
            .iter()
            .map(|(field, stats)| (field.as_u32(), stats.doc_count, stats.total_terms))
            .collect();
        (
            u32::try_from(self.documents.len()).unwrap_or(u32::MAX),
            fields,
        )
    }

    /// Execute one already-planned query against the frozen representation.
    pub fn execute_snapshot<F: FilterEvaluator<u32>>(
        &self,
        plan: &ExecutionPlan,
        scorer: SearchScorer,
        filter: &F,
        limit: usize,
    ) -> Result<Vec<ScoredHit<u32>>, IndexError> {
        let result = self
            .evaluate_node(plan.program.root(), &plan.program, scorer, filter)
            .map_err(|ReferenceEvalError::UnsupportedNode(_)| {
                IndexError::UnsupportedFilterPredicate
            })?;
        let mut hits: Vec<_> = result
            .matches
            .into_iter()
            .map(|doc_id| {
                ScoredHit::new(
                    doc_id,
                    result.scores.get(&doc_id).copied().unwrap_or(Score::ZERO),
                )
            })
            .collect();
        hits.sort_unstable_by(|left, right| right.cmp(left));
        hits.truncate(limit);
        Ok(hits)
    }

    fn evaluate_node<F: FilterEvaluator<u32>>(
        &self,
        node_id: QueryNodeId,
        program: &QueryProgram,
        scorer: SearchScorer,
        filter: &F,
    ) -> Result<ReferenceEvalResult, ReferenceEvalError> {
        let Some(node) = program.get(node_id) else {
            return Ok(ReferenceEvalResult::default());
        };
        match node {
            QueryNode::Term { field, term, boost } => {
                Ok(self.eval_term(*field, *term, *boost, scorer))
            }
            QueryNode::TermExpansion {
                children,
                fields,
                boost,
                field_weights,
            } => {
                if matches!(scorer, SearchScorer::Bm25F(_)) {
                    self.eval_bm25f_term_expansion(
                        children,
                        fields,
                        *boost,
                        field_weights,
                        program,
                        scorer,
                    )
                    .ok_or(ReferenceEvalError::UnsupportedNode(NodeKind::TermExpansion))
                } else {
                    self.eval_disjunction(children, *boost, program, scorer, filter)
                }
            }
            QueryNode::Or { children, boost } => {
                self.eval_disjunction(children, *boost, program, scorer, filter)
            }
            QueryNode::And { children, boost } => {
                let mut children = children.iter();
                let Some(first) = children.next() else {
                    return Ok(ReferenceEvalResult::default());
                };
                let first = self.evaluate_node(*first, program, scorer, filter)?;
                let mut matches = first.matches.clone();
                let mut results = alloc::vec![first];
                for child in children {
                    let result = self.evaluate_node(*child, program, scorer, filter)?;
                    matches.retain(|doc_id| result.matches.contains(doc_id));
                    results.push(result);
                }
                let mut scores = BTreeMap::new();
                for result in results {
                    for (doc_id, score) in result.scores {
                        if matches.contains(&doc_id) {
                            scores
                                .entry(doc_id)
                                .or_insert(Score::ZERO)
                                .add_assign(score);
                        }
                    }
                }
                if (*boost - 1.0).abs() > f32::EPSILON {
                    for score in scores.values_mut() {
                        score.mul_assign(*boost);
                    }
                }
                Ok(ReferenceEvalResult { matches, scores })
            }
            QueryNode::Not { child } => {
                let excluded = self.evaluate_node(*child, program, scorer, filter)?.matches;
                let matches = self
                    .documents
                    .iter()
                    .filter(|doc_id| !excluded.contains(doc_id))
                    .copied()
                    .collect();
                Ok(ReferenceEvalResult {
                    matches,
                    scores: BTreeMap::new(),
                })
            }
            QueryNode::ConstantScore { child, score } => {
                let mut result = self.evaluate_node(*child, program, scorer, filter)?;
                result.scores.clear();
                let score = Score::try_from(*score).unwrap_or(Score::ZERO);
                for doc_id in &result.matches {
                    result.scores.insert(*doc_id, score);
                }
                Ok(result)
            }
            QueryNode::Filter { .. } => Err(ReferenceEvalError::UnsupportedNode(NodeKind::Filter)),
            QueryNode::ExternalFilter { input, slot } => {
                let mut result = self.evaluate_node(*input, program, scorer, filter)?;
                result
                    .matches
                    .retain(|doc_id| filter.evaluate(*slot, doc_id));
                result
                    .scores
                    .retain(|doc_id, _| result.matches.contains(doc_id));
                Ok(result)
            }
        }
    }

    fn eval_disjunction<F: FilterEvaluator<u32>>(
        &self,
        children: &[QueryNodeId],
        boost: f32,
        program: &QueryProgram,
        scorer: SearchScorer,
        filter: &F,
    ) -> Result<ReferenceEvalResult, ReferenceEvalError> {
        let mut matches = BTreeSet::new();
        let mut scores = BTreeMap::new();
        for child in children {
            let result = self.evaluate_node(*child, program, scorer, filter)?;
            matches.extend(result.matches);
            for (doc_id, score) in result.scores {
                scores
                    .entry(doc_id)
                    .or_insert(Score::ZERO)
                    .add_assign(score);
            }
        }
        if (boost - 1.0).abs() > f32::EPSILON {
            for score in scores.values_mut() {
                score.mul_assign(boost);
            }
        }
        Ok(ReferenceEvalResult { matches, scores })
    }

    fn eval_bm25f_term_expansion(
        &self,
        children: &[QueryNodeId],
        fields: &[FieldId],
        expansion_boost: f32,
        field_weights: &BTreeMap<FieldId, f32>,
        program: &QueryProgram,
        scorer: SearchScorer,
    ) -> Option<ReferenceEvalResult> {
        let mut terms = Vec::with_capacity(children.len());
        let mut seen_fields = BTreeSet::new();
        let mut expected_text: Option<&str> = None;
        let mut expected_boost: Option<f32> = None;
        for child in children {
            let QueryNode::Term { field, term, boost } = program.get(*child)? else {
                return None;
            };
            if !seen_fields.insert(*field) {
                return None;
            }
            let entry = self.terms.get(term.as_u32() as usize)?;
            if entry.term_id != *term || entry.field_id != *field {
                return None;
            }
            match expected_text {
                Some(text) if text != entry.text => return None,
                Some(_) => {}
                None => expected_text = Some(entry.text.as_str()),
            }
            match expected_boost {
                Some(value) if (value - *boost).abs() > f32::EPSILON => return None,
                Some(_) => {}
                None => expected_boost = Some(*boost),
            }
            terms.push((*field, *term));
        }

        let weight = |field: FieldId| field_weights.get(&field).copied().unwrap_or(1.0);
        let mut aggregation_fields = Vec::with_capacity(fields.len());
        let mut avg_doc_length = 0.0;
        for &field in fields {
            let average = self.avg_field_doc_length(field);
            avg_doc_length += average;
            aggregation_fields.push((field, average));
        }
        let mut hits = BTreeMap::<u32, BTreeMap<FieldId, FieldHit>>::new();
        for (field, term) in terms {
            let postings = self.postings.get(&term)?;
            let average = self.avg_field_doc_length(field);
            for posting in postings {
                hits.entry(posting.doc_id).or_default().insert(
                    field,
                    FieldHit {
                        field,
                        term_frequency: posting.term_freq,
                        field_length: self
                            .field_doc_lengths
                            .get(&(posting.doc_id, field))
                            .copied()
                            .unwrap_or_default(),
                        avg_field_length: average,
                        weight: weight(field),
                    },
                );
            }
        }
        let doc_count = u32::try_from(self.documents.len()).unwrap_or(u32::MAX);
        let doc_frequency = u32::try_from(hits.len()).unwrap_or(u32::MAX);
        let term_boost = expected_boost.unwrap_or(1.0);
        let mut scores = BTreeMap::new();
        for (doc_id, mut by_field) in hits {
            for &(field, average) in &aggregation_fields {
                by_field.entry(field).or_insert_with(|| FieldHit {
                    field,
                    term_frequency: 0,
                    field_length: self
                        .field_doc_lengths
                        .get(&(doc_id, field))
                        .copied()
                        .unwrap_or_default(),
                    avg_field_length: average,
                    weight: weight(field),
                });
            }
            let fields: Vec<_> = by_field.into_values().collect();
            let mut score = scorer.score_term_fields(
                &fields,
                avg_doc_length,
                doc_count,
                doc_frequency,
                term_boost,
            );
            if (expansion_boost - 1.0).abs() > f32::EPSILON {
                score *= expansion_boost;
            }
            scores.insert(doc_id, score);
        }
        Some(ReferenceEvalResult {
            matches: scores.keys().copied().collect(),
            scores,
        })
    }

    fn eval_term(
        &self,
        field: FieldId,
        term: TermId,
        boost: f32,
        scorer: SearchScorer,
    ) -> ReferenceEvalResult {
        let Some(postings) = self.postings.get(&term) else {
            return ReferenceEvalResult::default();
        };
        let avg_doc_length = self.avg_field_doc_length(field);
        let doc_count = u32::try_from(self.documents.len()).unwrap_or(u32::MAX);
        let doc_frequency = u32::try_from(postings.len()).unwrap_or(u32::MAX);
        let mut scores = BTreeMap::new();
        for posting in postings {
            let doc_length = self
                .field_doc_lengths
                .get(&(posting.doc_id, field))
                .copied()
                .unwrap_or_default();
            let mut score = scorer.score_term(
                field,
                posting.term_freq,
                doc_length,
                avg_doc_length,
                doc_count,
                doc_frequency,
            );
            if (boost - 1.0).abs() > f32::EPSILON {
                score.mul_assign(boost);
            }
            scores.insert(posting.doc_id, score);
        }
        ReferenceEvalResult {
            matches: scores.keys().copied().collect(),
            scores,
        }
    }

    fn avg_field_doc_length(&self, field: FieldId) -> f32 {
        self.field_stats.get(&field).map_or(0.0, |stats| {
            if stats.doc_count == 0 {
                0.0
            } else {
                stats.total_terms as f32 / stats.doc_count as f32
            }
        })
    }

    fn add_document(&mut self, doc_id: u32, fields: &[(FieldId, &str)]) -> Result<(), IndexError> {
        if !self.documents.insert(doc_id) {
            return Err(IndexError::DuplicateDocument(doc_id));
        }
        let mut pending = BTreeMap::<FieldId, (BTreeMap<String, u32>, u32)>::new();
        for &(field, text) in fields {
            let analyzer = self
                .analyzers
                .get(field)
                .ok_or(IndexError::MissingAnalyzer(field))?;
            let entry = pending.entry(field).or_insert_with(|| (BTreeMap::new(), 0));
            for (_, token) in analyzer.analyze(text) {
                entry.1 = entry.1.checked_add(1).ok_or(IndexError::ValueOutOfRange)?;
                let count = entry.0.entry(token).or_insert(0);
                *count = count.checked_add(1).ok_or(IndexError::ValueOutOfRange)?;
            }
        }

        for (field, (frequencies, length)) in pending {
            self.field_doc_lengths.insert((doc_id, field), length);
            let stats = self.field_stats.entry(field).or_default();
            stats.doc_count = stats
                .doc_count
                .checked_add(1)
                .ok_or(IndexError::ValueOutOfRange)?;
            stats.total_terms = stats
                .total_terms
                .checked_add(length)
                .ok_or(IndexError::ValueOutOfRange)?;
            for (text, term_freq) in frequencies {
                let key = (field, text.clone());
                let term_id = if let Some(term_id) = self.terms_to_ids.get(&key) {
                    *term_id
                } else {
                    let raw =
                        u32::try_from(self.terms.len()).map_err(|_| IndexError::ValueOutOfRange)?;
                    let term_id = TermId::new(raw);
                    self.terms_to_ids.insert(key, term_id);
                    self.terms.push(ReferenceTerm {
                        field_id: field,
                        term_id,
                        text,
                    });
                    term_id
                };
                self.postings
                    .entry(term_id)
                    .or_default()
                    .push(ReferencePosting { doc_id, term_freq });
            }
        }
        Ok(())
    }
}
