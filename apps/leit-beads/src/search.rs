// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{cli::Options, model::Issue, text};
use leit_collect::{CountCollector, TopKCollector, collectors};
use leit_core::{FieldId, FilterEvaluator, FilterSlotId};
use leit_index::{
    ExecutionStats, ExecutionWorkspace, InMemoryIndexBuilder, QueryBuilder, SearchScorer,
};
use leit_query::{Planner, PlannerScratch, PlanningContext};
use leit_text::FieldAnalyzers;
use std::time::Instant;

pub(crate) const FIELDS: [(&str, f32); 7] = [
    ("id", 5.0),
    ("title", 3.0),
    ("description", 1.5),
    ("design", 1.0),
    ("acceptance_criteria", 1.0),
    ("notes", 1.0),
    ("comments", 0.7),
];

#[derive(Debug)]
pub(crate) struct Hit {
    pub issue: usize,
    pub score: Option<f32>,
}

#[derive(Debug)]
pub(crate) struct Outcome {
    pub hits: Vec<Hit>,
    pub total: usize,
    pub exact_id: bool,
    pub terms: Vec<String>,
    pub index_ms: f64,
    pub search_ms: f64,
    pub stats: ExecutionStats,
}

struct Allowed(Vec<bool>);
impl FilterEvaluator<u32> for Allowed {
    fn evaluate(&self, _slot: FilterSlotId, id: &u32) -> bool {
        self.0.get(*id as usize).copied().unwrap_or(false)
    }
    fn slots(&self) -> &[FilterSlotId] {
        const SLOTS: [FilterSlotId; 1] = [FilterSlotId::new(0)];
        &SLOTS
    }
}

pub(crate) fn run(issues: &[Issue], options: &Options) -> Result<Outcome, String> {
    let start = Instant::now();
    let terms = text::terms(&options.query);
    let mut outcome = Outcome {
        hits: Vec::new(),
        total: 0,
        exact_id: false,
        terms,
        index_ms: 0.0,
        search_ms: 0.0,
        stats: ExecutionStats::default(),
    };
    if issues
        .iter()
        .any(|issue| issue.id.eq_ignore_ascii_case(&options.query))
    {
        outcome.exact_id = true;
        for (index, issue) in issues.iter().enumerate() {
            if issue.id.eq_ignore_ascii_case(&options.query) && issue.allowed(&options.filters) {
                outcome.total += 1;
                if outcome.hits.len() < options.limit {
                    outcome.hits.push(Hit {
                        issue: index,
                        score: None,
                    });
                }
            }
        }
        outcome.search_ms = start.elapsed().as_secs_f64() * 1000.0;
        return Ok(outcome);
    }
    if outcome.terms.is_empty() {
        return Err("query contains no searchable terms".into());
    }
    if outcome.terms.len() > 32 {
        return Err("query exceeds 32 distinct terms; use a shorter query".into());
    }
    if issues.is_empty() {
        return Ok(outcome);
    }
    let fields: Vec<_> = (0..FIELDS.len())
        .map(|i| FieldId::new(u32::try_from(i).unwrap()))
        .collect();
    let mut analyzers = FieldAnalyzers::new();
    for field in &fields {
        analyzers.set(*field, text::analyzer());
    }
    let mut builder = InMemoryIndexBuilder::new(analyzers);
    for (field, (name, _)) in fields.iter().zip(FIELDS) {
        builder.register_field_alias(*field, name);
    }
    for (id, issue) in issues.iter().enumerate() {
        let id = u32::try_from(id).map_err(|_| "too many issues for u32 document IDs")?;
        let values: Vec<_> = fields.iter().copied().zip(issue.fields()).collect();
        builder
            .index_document(id, &values)
            .map_err(|e| e.to_string())?;
    }
    let index = builder.build_index();
    outcome.index_ms = start.elapsed().as_secs_f64() * 1000.0;
    let search_start = Instant::now();
    let mut query = QueryBuilder::new();
    let children = outcome
        .terms
        .iter()
        .map(|term| query.term(term.as_str()))
        .collect();
    if options.any {
        query.or(children);
    } else {
        query.and(children);
    }
    let program = query.build().ok_or("could not build query")?;
    let weights = fields
        .iter()
        .copied()
        .zip(FIELDS.map(|(_, weight)| weight))
        .collect();
    let context = PlanningContext::new(&index, &index)
        .with_default_fields(fields)
        .with_field_weights(weights);
    let mut plan = Planner::new()
        .plan_program(&program, &context, &mut PlannerScratch::new())
        .map_err(|e| e.to_string())?;
    let allowed = Allowed(
        issues
            .iter()
            .map(|issue| issue.allowed(&options.filters))
            .collect(),
    );
    // Typed workspace planning has no field-weight option. Preserve its filter
    // wrapping contract explicitly while using the lower-level weighted planner.
    for slot in allowed.slots() {
        plan.wrap_external_filter(*slot);
    }
    let mut workspace = ExecutionWorkspace::new();
    let mut top = TopKCollector::new(options.limit);
    let mut count = CountCollector::new();
    workspace
        .execute(
            &index,
            &plan,
            Some(SearchScorer::bm25f()),
            &allowed,
            &mut collectors([&mut top, &mut count]),
        )
        .map_err(|e| e.to_string())?;
    outcome.total = count.finish();
    outcome.hits = top
        .finish()
        .into_iter()
        .map(|hit| Hit {
            issue: hit.id as usize,
            score: Some(hit.score.as_f32()),
        })
        .collect();
    outcome.stats = workspace.last_stats();
    outcome.search_ms = search_start.elapsed().as_secs_f64() * 1000.0;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options(query: &str) -> Options {
        Options::parse(["search", query].map(Into::into))
            .unwrap()
            .unwrap()
    }
    fn issue(id: &str, title: &str, description: &str) -> Issue {
        Issue {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            status: "open".into(),
            ..Issue::default()
        }
    }
    #[test]
    fn title_weight_and_exact_id_have_observable_effects() {
        let issues = [
            issue("one-1", "atlas", "other"),
            issue("two-1", "other", "atlas"),
        ];
        let result = run(&issues, &options("atlas")).unwrap();
        assert_eq!(result.hits[0].issue, 0);
        assert!(result.hits[0].score > result.hits[1].score);
        let exact = run(&issues, &options("TWO-1")).unwrap();
        assert!(exact.exact_id);
        assert_eq!(exact.hits[0].issue, 1);
        assert!(exact.hits[0].score.is_none());
    }
    #[test]
    fn filters_apply_before_limit_and_exact_lookup() {
        let mut issues = [
            issue("one-1", "atlas", "atlas"),
            issue("two-1", "atlas", ""),
        ];
        issues[0].status = "closed".into();
        issues[1].labels = vec!["api".into()];
        let mut opts = options("atlas");
        opts.limit = 1;
        opts.filters.statuses.push("open".into());
        opts.filters.labels.push("api".into());
        let result = run(&issues, &opts).unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.hits[0].issue, 1);
        opts.query = "one-1".into();
        assert_eq!(run(&issues, &opts).unwrap().total, 0);
    }
    #[test]
    fn punctuation_is_text_and_all_any_differ() {
        let issues = [
            issue("one-1", "Foo::bar()", "CAFÉ"),
            issue("two-1", "foo", ""),
        ];
        let mut opts = options("FOO cafe\u{301}");
        assert_eq!(run(&issues, &opts).unwrap().total, 1);
        opts.any = true;
        assert_eq!(run(&issues, &opts).unwrap().total, 2);
        assert_eq!(run(&issues, &options("Foo::bar()")).unwrap().total, 1);
        assert!(run(&issues, &options("()::")).is_err());
    }
}
