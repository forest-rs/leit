// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use core::fmt;

use leit_core::{FieldId, TermId};
use leit_text::{AnalysisSchemaId, FieldAnalyzers};

use crate::InMemoryIndex;
use crate::builder::build_posting_blocks;
use crate::memory::{DEFAULT_POSTINGS_BLOCK_SIZE, FieldMetadata, PostingEntry, TermEntry};

type DocumentRemap = Vec<(u32, u32)>;
type TermRemap = Vec<(TermId, TermId)>;
type RemapPlan = (Vec<DocumentRemap>, Vec<TermRemap>);

/// A validation failure encountered before logical merge execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeError {
    /// The output analyzer registry did not carry an explicit schema identity.
    UnspecifiedOutputAnalysisSchema,
    /// A source index did not capture an explicit schema identity.
    UnspecifiedSourceAnalysisSchema {
        /// Zero-based source ordinal.
        source: usize,
    },
    /// A source index identity differed from the requested output identity.
    AnalysisSchemaMismatch {
        /// Zero-based source ordinal.
        source: usize,
        /// Identity required by the output registry.
        expected: AnalysisSchemaId,
        /// Explicit identity captured by the source index.
        found: AnalysisSchemaId,
    },
    /// A source's field aliases differed from the first source.
    FieldSchemaMismatch {
        /// Zero-based source ordinal.
        source: usize,
    },
    /// The output registry lacked an analyzer for a source field.
    MissingOutputAnalyzer {
        /// Field without an output analyzer.
        field: FieldId,
    },
    /// The merged document count cannot be represented by v1 document IDs.
    DocumentCountOverflow {
        /// Planned logical document count.
        count: u64,
    },
    /// The merged term count cannot be represented by v1 term IDs.
    TermCountOverflow {
        /// Planned logical term count.
        count: u64,
    },
    /// A merged field's token count cannot be represented by v1 statistics.
    FieldTotalTermsOverflow {
        /// Field whose token count overflowed.
        field: FieldId,
        /// Planned token count for the field.
        total_terms: u64,
    },
    /// Summed posting frequencies overflowed while validating one field.
    FieldCollectionFrequencyOverflow {
        /// Field whose posting frequencies overflowed.
        field: FieldId,
    },
    /// Field token totals disagreed with collection frequency derived from postings.
    FieldTokenFrequencyMismatch {
        /// Field whose statistics disagreed.
        field: FieldId,
        /// Token total derived from per-document field lengths.
        total_terms: u64,
        /// Collection frequency derived by summing posting term frequencies.
        collection_frequency: u64,
    },
    /// A field-length entry referenced a document absent from its source.
    FieldLengthDocumentMissing {
        /// Zero-based source ordinal.
        source: usize,
        /// Missing source-local document.
        document: u32,
        /// Field carrying the orphaned length.
        field: FieldId,
    },
    /// A postings map key was absent from the source's canonical term map.
    PostingTermMissing {
        /// Zero-based source ordinal.
        source: usize,
        /// Orphaned source-local term.
        term: TermId,
    },
    /// A posting referenced a document absent from its source.
    PostingDocumentMissing {
        /// Zero-based source ordinal.
        source: usize,
        /// Source-local term containing the posting.
        term: TermId,
        /// Missing source-local document.
        document: u32,
    },
    /// Multiple canonical terms shared one source-local term identifier.
    DuplicateCanonicalTermId {
        /// Zero-based source ordinal.
        source: usize,
        /// Ambiguous source-local term identifier.
        term: TermId,
    },
}

impl fmt::Display for MergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnspecifiedOutputAnalysisSchema => {
                write!(
                    f,
                    "merge output requires an explicit analyzer schema identity"
                )
            }
            Self::UnspecifiedSourceAnalysisSchema { source } => {
                write!(
                    f,
                    "source {source} lacks an explicit analyzer schema identity"
                )
            }
            Self::AnalysisSchemaMismatch {
                source,
                expected,
                found,
            } => write!(
                f,
                "source {source} analyzer schema {} differs from expected {}",
                found.get(),
                expected.get()
            ),
            Self::FieldSchemaMismatch { source } => {
                write!(
                    f,
                    "source {source} field schema differs from the merge schema"
                )
            }
            Self::MissingOutputAnalyzer { field } => {
                write!(
                    f,
                    "output registry lacks analyzer for field {}",
                    field.as_u32()
                )
            }
            Self::DocumentCountOverflow { count } => {
                write!(f, "merged document count {count} exceeds u32")
            }
            Self::TermCountOverflow { count } => {
                write!(f, "merged term count {count} exceeds u32")
            }
            Self::FieldTotalTermsOverflow { field, total_terms } => write!(
                f,
                "merged field {} token count {total_terms} exceeds u32",
                field.as_u32()
            ),
            Self::FieldCollectionFrequencyOverflow { field } => write!(
                f,
                "merged field {} collection frequency exceeds u64",
                field.as_u32()
            ),
            Self::FieldTokenFrequencyMismatch {
                field,
                total_terms,
                collection_frequency,
            } => write!(
                f,
                "merged field {} token total {total_terms} differs from collection frequency {collection_frequency}",
                field.as_u32()
            ),
            Self::FieldLengthDocumentMissing {
                source,
                document,
                field,
            } => write!(
                f,
                "source {source} field {} length references missing document {document}",
                field.as_u32()
            ),
            Self::PostingTermMissing { source, term } => write!(
                f,
                "source {source} postings reference missing term {}",
                term.as_u32()
            ),
            Self::PostingDocumentMissing {
                source,
                term,
                document,
            } => write!(
                f,
                "source {source} term {} posting references missing document {document}",
                term.as_u32()
            ),
            Self::DuplicateCanonicalTermId { source, term } => write!(
                f,
                "source {source} maps multiple canonical terms to term {}",
                term.as_u32()
            ),
        }
    }
}

impl core::error::Error for MergeError {}

/// Owned inputs returned when merge preparation rejects a plan.
#[derive(Debug)]
pub struct MergeRejected {
    error: MergeError,
    sources: Vec<InMemoryIndex>,
    analyzers: FieldAnalyzers,
}

impl MergeRejected {
    /// Inspect the structured rejection reason.
    pub const fn error(&self) -> &MergeError {
        &self.error
    }

    /// Recover the error and every owned input without partial mutation.
    pub fn into_parts(self) -> (MergeError, Vec<InMemoryIndex>, FieldAnalyzers) {
        (self.error, self.sources, self.analyzers)
    }
}

/// Validated owned inputs for a future infallible logical merge execution.
#[derive(Debug)]
pub struct PreparedMerge {
    pub(crate) sources: Vec<InMemoryIndex>,
    pub(crate) analyzers: FieldAnalyzers,
    document_remaps: Vec<DocumentRemap>,
    term_remaps: Vec<TermRemap>,
}

/// The execution-capable result of a logical merge and its published ID remaps.
#[derive(Debug)]
pub struct MergedIndex {
    index: InMemoryIndex,
    document_remaps: Vec<DocumentRemap>,
    term_remaps: Vec<TermRemap>,
}

impl MergedIndex {
    /// Borrow the merged execution-capable index.
    pub const fn index(&self) -> &InMemoryIndex {
        &self.index
    }

    /// Consume the merge result and return its execution-capable index.
    pub fn into_index(self) -> InMemoryIndex {
        self.index
    }

    /// Inspect one source's complete `(source, merged)` document-ID remap.
    pub fn document_remap(&self, source: usize) -> Option<&[(u32, u32)]> {
        self.document_remaps.get(source).map(Vec::as_slice)
    }

    /// Inspect one source's complete `(source, merged)` term-ID remap.
    pub fn term_remap(&self, source: usize) -> Option<&[(TermId, TermId)]> {
        self.term_remaps.get(source).map(Vec::as_slice)
    }
}

impl PreparedMerge {
    /// Return the number of validated source indexes.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Return the shared explicit analyzer-schema identity.
    pub fn analysis_schema_id(&self) -> AnalysisSchemaId {
        self.analyzers
            .schema_id()
            .expect("prepared merges always carry an explicit schema identity")
    }

    /// Inspect one source's complete `(source, merged)` document-ID remap.
    pub fn document_remap(&self, source: usize) -> Option<&[(u32, u32)]> {
        self.document_remaps.get(source).map(Vec::as_slice)
    }

    /// Inspect one source's complete `(source, merged)` term-ID remap.
    pub fn term_remap(&self, source: usize) -> Option<&[(TermId, TermId)]> {
        self.term_remaps.get(source).map(Vec::as_slice)
    }

    /// Consume validated inputs and execute their infallible logical merge.
    pub fn execute(self) -> MergedIndex {
        let Self {
            sources,
            analyzers,
            document_remaps,
            term_remaps,
        } = self;

        let mut documents = BTreeSet::new();
        let mut terms_to_ids = BTreeMap::new();
        let mut postings = BTreeMap::<TermId, Vec<PostingEntry>>::new();
        let mut field_doc_lengths = BTreeMap::new();
        let field_names = sources
            .first()
            .map_or_else(BTreeMap::new, |source| source.field_names.clone());

        for (source_ordinal, source) in sources.iter().enumerate() {
            let document_remap = &document_remaps[source_ordinal];
            let term_remap = &term_remaps[source_ordinal];

            documents.extend(document_remap.iter().map(|&(_source, merged)| merged));
            for ((source_document, field), length) in &source.field_doc_lengths {
                field_doc_lengths.insert(
                    (
                        lookup_document_remap(document_remap, *source_document),
                        *field,
                    ),
                    *length,
                );
            }
            for ((field, term), source_term) in &source.terms_to_ids {
                terms_to_ids.insert(
                    (*field, term.clone()),
                    lookup_term_remap(term_remap, *source_term),
                );
            }
            for (source_term, source_postings) in &source.postings {
                let merged_term = lookup_term_remap(term_remap, *source_term);
                let merged_postings = postings.entry(merged_term).or_default();
                merged_postings.extend(source_postings.iter().map(|posting| PostingEntry {
                    doc_id: lookup_document_remap(document_remap, posting.doc_id),
                    term_freq: posting.term_freq,
                }));
            }
        }
        for merged_postings in postings.values_mut() {
            merged_postings.sort_by_key(|posting| posting.doc_id);
        }

        let term_entries: Vec<_> = terms_to_ids
            .iter()
            .map(|((field_id, term), &term_id)| TermEntry {
                field_id: *field_id,
                term_id,
                term: term.clone(),
            })
            .collect();
        debug_assert!(
            term_entries
                .iter()
                .enumerate()
                .all(|(index, entry)| entry.term_id.as_u32() as usize == index),
            "canonical term order must remain dense"
        );

        let mut field_stats = BTreeMap::<FieldId, FieldMetadata>::new();
        for (&(_document, field), &length) in &field_doc_lengths {
            let stats = field_stats.entry(field).or_insert(FieldMetadata {
                field_id: field,
                doc_count: 0,
                total_terms: 0,
            });
            stats.doc_count += 1;
            stats.total_terms = stats
                .total_terms
                .checked_add(length)
                .expect("merge statistics were validated during preparation");
        }

        let mut collection_frequency_by_field = BTreeMap::<FieldId, u64>::new();
        for entry in &term_entries {
            let collection_frequency = postings
                .get(&entry.term_id)
                .into_iter()
                .flatten()
                .map(|posting| u64::from(posting.term_freq))
                .sum::<u64>();
            let field_frequency = collection_frequency_by_field
                .entry(entry.field_id)
                .or_default();
            *field_frequency = field_frequency
                .checked_add(collection_frequency)
                .expect("in-memory collection frequency fits u64");
        }
        debug_assert!(
            field_stats.iter().all(|(field, stats)| {
                collection_frequency_by_field
                    .get(field)
                    .copied()
                    .unwrap_or(0)
                    == u64::from(stats.total_terms)
            }),
            "summed posting term frequencies must equal merged field token totals"
        );

        let posting_blocks = build_posting_blocks(
            &term_entries,
            &postings,
            &field_doc_lengths,
            DEFAULT_POSTINGS_BLOCK_SIZE,
        );
        let index = InMemoryIndex::new(
            analyzers,
            documents,
            terms_to_ids,
            term_entries,
            postings,
            posting_blocks,
            field_stats,
            field_names,
            field_doc_lengths,
        );
        MergedIndex {
            index,
            document_remaps,
            term_remaps,
        }
    }
}

fn lookup_document_remap(remap: &[(u32, u32)], source_document: u32) -> u32 {
    let index = remap
        .binary_search_by_key(&source_document, |&(source, _merged)| source)
        .expect("merge preparation validated every document reference");
    remap[index].1
}

fn lookup_term_remap(remap: &[(TermId, TermId)], source_term: TermId) -> TermId {
    let index = remap
        .binary_search_by_key(&source_term, |&(source, _merged)| source)
        .expect("merge preparation validated every term reference");
    remap[index].1
}

/// Validate whether logical document and term counts fit v1 identifiers.
pub(crate) const fn validate_merge_counts(
    document_count: u64,
    term_count: u64,
) -> Result<(), MergeError> {
    if document_count > u32::MAX as u64 {
        return Err(MergeError::DocumentCountOverflow {
            count: document_count,
        });
    }
    if term_count > u32::MAX as u64 {
        return Err(MergeError::TermCountOverflow { count: term_count });
    }
    Ok(())
}

/// Validate and take ownership of all inputs required for a logical merge.
pub fn prepare_merge(
    sources: Vec<InMemoryIndex>,
    analyzers: FieldAnalyzers,
) -> Result<PreparedMerge, MergeRejected> {
    let validation = validate_schema_inputs(&sources, &analyzers).and_then(|schema_id| {
        validate_exact_counts(&sources)?;
        Ok(schema_id)
    });
    finalize_preparation(sources, analyzers, validation)
}

fn finalize_preparation(
    sources: Vec<InMemoryIndex>,
    analyzers: FieldAnalyzers,
    validation: Result<AnalysisSchemaId, MergeError>,
) -> Result<PreparedMerge, MergeRejected> {
    match validation {
        Ok(_schema_id) => {
            let (document_remaps, term_remaps) = build_remap_plan(&sources);
            Ok(PreparedMerge {
                sources,
                analyzers,
                document_remaps,
                term_remaps,
            })
        }
        Err(error) => Err(MergeRejected {
            error,
            sources,
            analyzers,
        }),
    }
}

fn build_remap_plan(sources: &[InMemoryIndex]) -> RemapPlan {
    let mut next_document = 0_u64;
    let document_remaps = sources
        .iter()
        .map(|source| {
            source
                .documents
                .iter()
                .map(|&source_document| {
                    let merged_document = u32::try_from(next_document)
                        .expect("merge counts were validated before remap planning");
                    next_document += 1;
                    (source_document, merged_document)
                })
                .collect()
        })
        .collect();

    let canonical_terms = BTreeSet::from_iter(sources.iter().flat_map(|source| {
        source
            .terms_to_ids
            .keys()
            .map(|(field, term)| (*field, term.as_str()))
    }));
    let merged_terms: BTreeMap<_, _> = canonical_terms
        .into_iter()
        .enumerate()
        .map(|(merged_term, canonical)| {
            (
                canonical,
                TermId::new(
                    u32::try_from(merged_term)
                        .expect("merge counts were validated before remap planning"),
                ),
            )
        })
        .collect();
    let term_remaps = sources
        .iter()
        .map(|source| {
            let mut remap: Vec<_> = source
                .terms_to_ids
                .iter()
                .map(|((field, term), &source_term)| {
                    let merged_term = merged_terms[&(*field, term.as_str())];
                    (source_term, merged_term)
                })
                .collect();
            remap.sort_by_key(|&(source_term, _)| source_term);
            remap
        })
        .collect();

    (document_remaps, term_remaps)
}

fn validate_schema_inputs(
    sources: &[InMemoryIndex],
    analyzers: &FieldAnalyzers,
) -> Result<AnalysisSchemaId, MergeError> {
    let Some(schema_id) = analyzers.schema_id() else {
        return Err(MergeError::UnspecifiedOutputAnalysisSchema);
    };

    let expected_aliases = sources.first().map(|source| &source.field_names);
    let expected_fields = sources.first().map(|source| source.analysis_field_ids());

    for (source_ordinal, source) in sources.iter().enumerate() {
        let Some(source_schema_id) = source.analysis_schema_id else {
            return Err(MergeError::UnspecifiedSourceAnalysisSchema {
                source: source_ordinal,
            });
        };
        if source_schema_id != schema_id {
            return Err(MergeError::AnalysisSchemaMismatch {
                source: source_ordinal,
                expected: schema_id,
                found: source_schema_id,
            });
        }
        if expected_aliases.is_some_and(|aliases| aliases != &source.field_names)
            || expected_fields.is_some_and(|fields| fields != source.analysis_field_ids())
        {
            return Err(MergeError::FieldSchemaMismatch {
                source: source_ordinal,
            });
        }
        let mut required_output_fields =
            BTreeSet::from_iter(source.analysis_field_ids().iter().copied());
        required_output_fields.extend(source.field_names.values().copied());
        for field in required_output_fields {
            if analyzers.get(field).is_none() {
                return Err(MergeError::MissingOutputAnalyzer { field });
            }
        }
    }
    Ok(schema_id)
}

fn validate_exact_counts(sources: &[InMemoryIndex]) -> Result<(), MergeError> {
    validate_referential_integrity(sources)?;
    let mut document_count = 0_u64;
    for source in sources {
        let source_documents = u64::try_from(source.documents.len()).unwrap_or(u64::MAX);
        document_count = document_count
            .checked_add(source_documents)
            .ok_or(MergeError::DocumentCountOverflow { count: u64::MAX })?;
    }
    validate_merge_counts(document_count, count_distinct_canonical_terms(sources))?;

    let mut total_terms_by_field = BTreeMap::<FieldId, u64>::new();
    let mut collection_frequency_by_field = BTreeMap::<FieldId, u64>::new();
    for source in sources {
        for (&(_document, field), &length) in &source.field_doc_lengths {
            let total_terms = total_terms_by_field.entry(field).or_default();
            *total_terms = total_terms.checked_add(u64::from(length)).ok_or(
                MergeError::FieldTotalTermsOverflow {
                    field,
                    total_terms: u64::MAX,
                },
            )?;
            if *total_terms > u64::from(u32::MAX) {
                return Err(MergeError::FieldTotalTermsOverflow {
                    field,
                    total_terms: *total_terms,
                });
            }
        }
        for ((field, _term), term_id) in &source.terms_to_ids {
            for posting in source.postings.get(term_id).into_iter().flatten() {
                let collection_frequency = collection_frequency_by_field.entry(*field).or_default();
                *collection_frequency = collection_frequency
                    .checked_add(u64::from(posting.term_freq))
                    .ok_or(MergeError::FieldCollectionFrequencyOverflow { field: *field })?;
            }
        }
    }
    let fields = BTreeSet::from_iter(
        total_terms_by_field
            .keys()
            .chain(collection_frequency_by_field.keys())
            .copied(),
    );
    for field in fields {
        let total_terms = total_terms_by_field.get(&field).copied().unwrap_or(0);
        let collection_frequency = collection_frequency_by_field
            .get(&field)
            .copied()
            .unwrap_or(0);
        if total_terms != collection_frequency {
            return Err(MergeError::FieldTokenFrequencyMismatch {
                field,
                total_terms,
                collection_frequency,
            });
        }
    }
    Ok(())
}

fn validate_referential_integrity(sources: &[InMemoryIndex]) -> Result<(), MergeError> {
    for (source_ordinal, source) in sources.iter().enumerate() {
        for &(document, field) in source.field_doc_lengths.keys() {
            if !source.documents.contains(&document) {
                return Err(MergeError::FieldLengthDocumentMissing {
                    source: source_ordinal,
                    document,
                    field,
                });
            }
        }

        let mut canonical_terms = BTreeSet::new();
        for &term in source.terms_to_ids.values() {
            if !canonical_terms.insert(term) {
                return Err(MergeError::DuplicateCanonicalTermId {
                    source: source_ordinal,
                    term,
                });
            }
        }
        for (&term, postings) in &source.postings {
            if !canonical_terms.contains(&term) {
                return Err(MergeError::PostingTermMissing {
                    source: source_ordinal,
                    term,
                });
            }
            for posting in postings {
                if !source.documents.contains(&posting.doc_id) {
                    return Err(MergeError::PostingDocumentMissing {
                        source: source_ordinal,
                        term,
                        document: posting.doc_id,
                    });
                }
            }
        }
    }
    Ok(())
}

fn count_distinct_canonical_terms(sources: &[InMemoryIndex]) -> u64 {
    let mut canonical_terms = BTreeSet::new();
    for source in sources {
        for field_and_term in source.terms_to_ids.keys() {
            canonical_terms.insert((field_and_term.0, field_and_term.1.as_str()));
        }
    }
    u64::try_from(canonical_terms.len()).unwrap_or(u64::MAX)
}

#[cfg(test)]
fn prepare_merge_with_counts_for_test(
    sources: Vec<InMemoryIndex>,
    analyzers: FieldAnalyzers,
    document_count: u64,
    term_count: u64,
) -> Result<PreparedMerge, MergeRejected> {
    let validation = validate_schema_inputs(&sources, &analyzers).and_then(|schema_id| {
        validate_merge_counts(document_count, term_count)?;
        Ok(schema_id)
    });
    finalize_preparation(sources, analyzers, validation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    use leit_core::FieldId;
    use leit_query::TermDictionary;
    use leit_text::{
        AnalysisSchemaId, Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer,
    };

    use crate::{InMemoryIndex, InMemoryIndexBuilder};

    fn analyzers(schema: u64, fields: &[u32]) -> FieldAnalyzers {
        let mut analyzers = FieldAnalyzers::with_schema_id(
            AnalysisSchemaId::new(schema).expect("test schema is nonzero"),
        );
        for field in fields {
            analyzers.set(
                FieldId::new(*field),
                Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
            );
        }
        analyzers
    }

    fn index(schema: u64, alias: &str, field: u32) -> InMemoryIndex {
        let mut builder = InMemoryIndexBuilder::new(analyzers(schema, &[field]));
        builder.register_field_alias(FieldId::new(field), alias);
        builder
            .index_document(9, &[(FieldId::new(field), "alpha beta")])
            .expect("fixture indexes");
        builder.build_index()
    }

    fn unspecified_index(alias: &str, field: u32) -> InMemoryIndex {
        let mut registry = FieldAnalyzers::new();
        registry.set(
            FieldId::new(field),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
        let mut builder = InMemoryIndexBuilder::new(registry);
        builder.register_field_alias(FieldId::new(field), alias);
        builder
            .index_document(9, &[(FieldId::new(field), "alpha beta")])
            .expect("fixture indexes");
        builder.build_index()
    }

    fn assert_source_queryable(source: &InMemoryIndex, field: u32) {
        assert!(source.resolve_term(FieldId::new(field), "alpha").is_some());
    }

    fn assert_output_registry_usable(mut output: FieldAnalyzers, schema: Option<u64>, field: u32) {
        assert_eq!(output.schema_id().map(AnalysisSchemaId::get), schema);
        output.set(
            FieldId::new(field),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
        assert_eq!(
            output
                .get(FieldId::new(field))
                .expect("returned registry accepts analyzers")
                .analyze("ALPHA")
                .len(),
            1
        );
    }

    #[test]
    fn built_index_snapshots_schema_identity() {
        let schema_id = AnalysisSchemaId::new(11).expect("nonzero");
        let mut registry = FieldAnalyzers::with_schema_id(schema_id);
        registry.set(FieldId::new(1), Analyzer::new(WhitespaceTokenizer::new()));
        let mut builder = InMemoryIndexBuilder::new(registry);
        builder.register_field_alias(FieldId::new(1), "body");
        let mut built = builder.build_index();

        assert_eq!(built.analysis_schema_id(), Some(schema_id));
        assert_eq!(built.analysis_field_ids(), &[FieldId::new(1)]);
        built.analyzers.set(
            FieldId::new(2),
            Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
        );
        assert_eq!(built.analysis_schema_id(), Some(schema_id));
        assert_eq!(built.analysis_field_ids(), &[FieldId::new(1)]);

        prepare_merge(vec![built], analyzers(11, &[1]))
            .expect("post-build registry mutation must not change the field snapshot");
    }

    #[test]
    fn count_preflight_accepts_u32_max_and_rejects_plus_one() {
        assert_eq!(validate_merge_counts(u64::from(u32::MAX), 0), Ok(()));
        assert_eq!(validate_merge_counts(0, u64::from(u32::MAX)), Ok(()));
        assert_eq!(
            validate_merge_counts(u64::from(u32::MAX) + 1, 0),
            Err(MergeError::DocumentCountOverflow {
                count: u64::from(u32::MAX) + 1,
            })
        );
        assert_eq!(
            validate_merge_counts(0, u64::from(u32::MAX) + 1),
            Err(MergeError::TermCountOverflow {
                count: u64::from(u32::MAX) + 1,
            })
        );
    }

    #[test]
    fn canonical_term_count_deduplicates_terms_across_sources() {
        let left = index(12, "body", 1);
        let right = index(12, "body", 1);
        let other_field = index(12, "title", 2);

        assert_eq!(
            count_distinct_canonical_terms(&[left, right, other_field]),
            4
        );
    }

    #[test]
    fn synthetic_count_rejection_uses_owned_preparation_path() {
        let accepted = prepare_merge_with_counts_for_test(
            vec![index(13, "body", 1)],
            analyzers(13, &[1]),
            u64::from(u32::MAX),
            u64::from(u32::MAX),
        )
        .expect("u32 max counts remain representable");
        assert_eq!(accepted.source_count(), 1);

        for (documents, terms, expected) in [
            (
                u64::from(u32::MAX) + 1,
                0,
                MergeError::DocumentCountOverflow {
                    count: u64::from(u32::MAX) + 1,
                },
            ),
            (
                0,
                u64::from(u32::MAX) + 1,
                MergeError::TermCountOverflow {
                    count: u64::from(u32::MAX) + 1,
                },
            ),
        ] {
            let rejected = prepare_merge_with_counts_for_test(
                vec![index(13, "body", 1)],
                analyzers(13, &[1]),
                documents,
                terms,
            )
            .expect_err("synthetic overflow must reject owned preparation");
            assert_eq!(rejected.error(), &expected);
            let (_, sources, output) = rejected.into_parts();
            assert_eq!(sources.len(), 1);
            assert_source_queryable(&sources[0], 1);
            assert_output_registry_usable(output, Some(13), 1);
        }
    }

    #[test]
    fn preparation_rejects_unspecified_or_mismatched_schema_atomically() {
        let source = index(21, "body", 1);
        let rejected = prepare_merge(vec![source], FieldAnalyzers::new())
            .expect_err("unspecified output identity must fail");
        assert_eq!(
            rejected.error(),
            &MergeError::UnspecifiedOutputAnalysisSchema
        );
        let (error, sources, output) = rejected.into_parts();
        assert_eq!(error, MergeError::UnspecifiedOutputAnalysisSchema);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].document_count(), 1);
        assert_source_queryable(&sources[0], 1);
        assert_output_registry_usable(output, None, 1);

        let source = index(21, "body", 1);
        let rejected = prepare_merge(vec![source], analyzers(22, &[1]))
            .expect_err("mismatched identity must fail");
        assert!(matches!(
            rejected.error(),
            MergeError::AnalysisSchemaMismatch { source: 0, .. }
        ));
        let (_, sources, output) = rejected.into_parts();
        assert_eq!(sources.len(), 1);
        assert_source_queryable(&sources[0], 1);
        assert_output_registry_usable(output, Some(22), 1);
    }

    #[test]
    fn preparation_rejects_unspecified_source_schema_atomically() {
        let rejected = prepare_merge(vec![unspecified_index("body", 1)], analyzers(23, &[1]))
            .expect_err("unspecified source identity must fail");
        assert_eq!(
            rejected.error(),
            &MergeError::UnspecifiedSourceAnalysisSchema { source: 0 }
        );
        let (_, sources, output) = rejected.into_parts();
        assert_eq!(sources.len(), 1);
        assert_source_queryable(&sources[0], 1);
        assert_output_registry_usable(output, Some(23), 1);
    }

    #[test]
    fn preparation_rejects_alias_or_analyzer_coverage_mismatch_atomically() {
        let left = index(31, "body", 1);
        let right = index(31, "content", 1);
        let rejected = prepare_merge(vec![left, right], analyzers(31, &[1]))
            .expect_err("different aliases must fail");
        assert_eq!(
            rejected.error(),
            &MergeError::FieldSchemaMismatch { source: 1 }
        );
        let (_, sources, output) = rejected.into_parts();
        assert_eq!(sources.len(), 2);
        for source in &sources {
            assert_source_queryable(source, 1);
        }
        assert_output_registry_usable(output, Some(31), 1);

        let source = index(31, "body", 1);
        let rejected = prepare_merge(vec![source], analyzers(31, &[]))
            .expect_err("missing output analyzer must fail");
        assert_eq!(
            rejected.error(),
            &MergeError::MissingOutputAnalyzer {
                field: FieldId::new(1),
            }
        );
        let (_, sources, output) = rejected.into_parts();
        assert_eq!(sources[0].document_count(), 1);
        assert_source_queryable(&sources[0], 1);
        assert!(output.get(FieldId::new(1)).is_none());
        assert_output_registry_usable(output, Some(31), 1);
    }

    #[test]
    fn preparation_rejects_different_unaliased_source_field_sets_atomically() {
        fn configured_empty(schema: u64, field: u32) -> InMemoryIndex {
            let builder = InMemoryIndexBuilder::new(analyzers(schema, &[field]));
            builder.build_index()
        }

        let rejected = prepare_merge(
            vec![configured_empty(33, 1), configured_empty(33, 2)],
            analyzers(33, &[1, 2]),
        )
        .expect_err("different unaliased field sets must fail");
        assert_eq!(
            rejected.error(),
            &MergeError::FieldSchemaMismatch { source: 1 }
        );
        let (_, sources, output) = rejected.into_parts();
        assert_eq!(sources.len(), 2);
        for source in &sources {
            assert_eq!(source.document_count(), 0);
            source
                .to_segment_bytes()
                .expect("returned source is usable");
        }
        assert!(output.get(FieldId::new(1)).is_some());
        assert!(output.get(FieldId::new(2)).is_some());
    }

    #[test]
    fn preparation_accepts_same_configured_fields_with_different_populated_fields() {
        fn partially_populated(schema: u64, populated_field: u32) -> InMemoryIndex {
            let mut builder = InMemoryIndexBuilder::new(analyzers(schema, &[1, 2]));
            builder
                .index_document(1, &[(FieldId::new(populated_field), "alpha")])
                .expect("fixture indexes");
            builder.build_index()
        }

        let prepared = prepare_merge(
            vec![partially_populated(34, 1), partially_populated(34, 2)],
            analyzers(34, &[1, 2]),
        )
        .expect("configured field schema is independent of populated data");
        assert_eq!(prepared.source_count(), 2);
    }

    #[test]
    fn alias_targets_require_output_analyzer_coverage() {
        fn alias_only(schema: u64) -> InMemoryIndex {
            let registry = FieldAnalyzers::with_schema_id(
                AnalysisSchemaId::new(schema).expect("test schema is nonzero"),
            );
            let mut builder = InMemoryIndexBuilder::new(registry);
            builder.register_field_alias(FieldId::new(1), "body");
            builder.build_index()
        }

        let rejected = prepare_merge(vec![alias_only(35)], analyzers(35, &[]))
            .expect_err("an alias target needs an output analyzer");
        assert_eq!(
            rejected.error(),
            &MergeError::MissingOutputAnalyzer {
                field: FieldId::new(1),
            }
        );
        let (_, sources, output) = rejected.into_parts();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].document_count(), 0);
        sources[0]
            .to_segment_bytes()
            .expect("returned source remains usable");
        assert_output_registry_usable(output, Some(35), 1);

        let prepared = prepare_merge(vec![alias_only(35), alias_only(35)], analyzers(35, &[1]))
            .expect("covered alias targets prepare successfully");
        assert_eq!(prepared.source_count(), 2);
    }

    #[test]
    fn preparation_requires_output_analyzers_for_unaliased_indexed_fields() {
        let mut builder = InMemoryIndexBuilder::new(analyzers(32, &[2]));
        builder
            .index_document(1, &[(FieldId::new(2), "unaliased")])
            .expect("fixture indexes");

        let rejected = prepare_merge(vec![builder.build_index()], analyzers(32, &[]))
            .expect_err("every indexed field needs an output analyzer");
        assert_eq!(
            rejected.error(),
            &MergeError::MissingOutputAnalyzer {
                field: FieldId::new(2),
            }
        );
        let (_, sources, output) = rejected.into_parts();
        assert!(
            sources[0]
                .resolve_term(FieldId::new(2), "unaliased")
                .is_some()
        );
        assert_output_registry_usable(output, Some(32), 2);
    }

    #[test]
    fn valid_preparation_owns_inputs_without_exposing_partial_execution() {
        let prepared = prepare_merge(
            vec![index(41, "body", 1), index(41, "body", 1)],
            analyzers(41, &[1]),
        )
        .expect("matching inputs prepare");

        assert_eq!(prepared.source_count(), 2);
        assert_eq!(prepared.analysis_schema_id().get(), 41);
    }

    fn index_documents(schema: u64, documents: &[(u32, &str)]) -> InMemoryIndex {
        let mut builder = InMemoryIndexBuilder::new(analyzers(schema, &[1]));
        builder.register_field_alias(FieldId::new(1), "body");
        for &(document, text) in documents {
            builder
                .index_document(document, &[(FieldId::new(1), text)])
                .expect("fixture indexes");
        }
        builder.build_index()
    }

    fn index_two_fields(schema: u64, document: u32, fields: &[(FieldId, &str)]) -> InMemoryIndex {
        let mut builder = InMemoryIndexBuilder::new(analyzers(schema, &[1, 2]));
        builder.register_field_alias(FieldId::new(1), "body");
        builder.register_field_alias(FieldId::new(2), "title");
        builder
            .index_document(document, fields)
            .expect("fixture indexes");
        builder.build_index()
    }

    fn index_documents_two_fields(schema: u64, documents: &[(u32, &str, &str)]) -> InMemoryIndex {
        let mut builder = InMemoryIndexBuilder::new(analyzers(schema, &[1, 2]));
        builder.register_field_alias(FieldId::new(1), "body");
        builder.register_field_alias(FieldId::new(2), "title");
        for &(document, body, title) in documents {
            builder
                .index_document(
                    document,
                    &[(FieldId::new(1), body), (FieldId::new(2), title)],
                )
                .expect("fixture indexes");
        }
        builder.build_index()
    }

    fn assert_logical_index_matches_oracle(actual: &InMemoryIndex, oracle: &InMemoryIndex) {
        assert_eq!(actual.analysis_schema_id, oracle.analysis_schema_id);
        assert_eq!(actual.analysis_fields, oracle.analysis_fields);
        assert_eq!(actual.documents, oracle.documents);
        assert_eq!(actual.field_names, oracle.field_names);
        assert_eq!(actual.field_doc_lengths, oracle.field_doc_lengths);
        assert_eq!(actual.field_stats, oracle.field_stats);
        assert_eq!(
            actual.terms_to_ids.keys().collect::<Vec<_>>(),
            oracle.terms_to_ids.keys().collect::<Vec<_>>()
        );
        for canonical_term in actual.terms_to_ids.keys() {
            let actual_term = actual.terms_to_ids[canonical_term];
            let oracle_term = oracle.terms_to_ids[canonical_term];
            let actual_entry = &actual.term_entries[actual_term.as_u32() as usize];
            let oracle_entry = &oracle.term_entries[oracle_term.as_u32() as usize];
            assert_eq!(actual_entry.field_id, oracle_entry.field_id);
            assert_eq!(actual_entry.term, oracle_entry.term);
            assert_eq!(actual.postings[&actual_term], oracle.postings[&oracle_term]);
            assert_eq!(
                actual.posting_blocks[&actual_term],
                oracle.posting_blocks[&oracle_term]
            );
        }
    }

    #[test]
    fn preparation_plans_dense_deterministic_document_and_term_remaps() {
        let empty = prepare_merge(vec![], analyzers(51, &[1])).expect("empty input prepares");
        assert_eq!(empty.source_count(), 0);
        assert_eq!(empty.document_remap(0), None);
        assert_eq!(empty.term_remap(0), None);

        let singleton_source = index_documents(51, &[(9, "zulu"), (3, "alpha")]);
        let alpha = singleton_source
            .resolve_term(FieldId::new(1), "alpha")
            .expect("alpha exists");
        let zulu = singleton_source
            .resolve_term(FieldId::new(1), "zulu")
            .expect("zulu exists");
        let singleton =
            prepare_merge(vec![singleton_source], analyzers(51, &[1])).expect("singleton prepares");
        assert_eq!(singleton.document_remap(0), Some(&[(3, 0), (9, 1)][..]));
        let mut expected_singleton_terms = vec![(alpha, TermId::new(0)), (zulu, TermId::new(1))];
        expected_singleton_terms.sort_by_key(|&(source_term, _)| source_term);
        assert_eq!(
            singleton.term_remap(0),
            Some(expected_singleton_terms.as_slice())
        );

        let first = index_two_fields(51, 7, &[(FieldId::new(2), "ALPHA")]);
        let second = index_two_fields(
            51,
            7,
            &[(FieldId::new(1), "alpha"), (FieldId::new(2), "alpha")],
        );
        let third = index_two_fields(51, 7, &[(FieldId::new(1), "beta")]);
        let first_title_alpha = first
            .resolve_term(FieldId::new(2), "alpha")
            .expect("case-normalized title alpha exists");
        assert_eq!(
            first.resolve_term(FieldId::new(2), "ALPHA"),
            Some(first_title_alpha),
            "query normalization resolves the same canonical term"
        );
        let second_body_alpha = second
            .resolve_term(FieldId::new(1), "alpha")
            .expect("body alpha exists");
        let second_title_alpha = second
            .resolve_term(FieldId::new(2), "alpha")
            .expect("title alpha exists");
        let third_body_beta = third
            .resolve_term(FieldId::new(1), "beta")
            .expect("body beta exists");
        assert_eq!(
            first_title_alpha, second_body_alpha,
            "local IDs intentionally collide for different canonical terms"
        );
        assert_eq!(
            first_title_alpha, third_body_beta,
            "the third source repeats the same local-ID collision"
        );

        let prepared = prepare_merge(vec![first, second, third], analyzers(51, &[1, 2]))
            .expect("three sources prepare");
        assert_eq!(prepared.document_remap(0), Some(&[(7, 0)][..]));
        assert_eq!(prepared.document_remap(1), Some(&[(7, 1)][..]));
        assert_eq!(prepared.document_remap(2), Some(&[(7, 2)][..]));
        assert_eq!(
            prepared.term_remap(0),
            Some(&[(first_title_alpha, TermId::new(2))][..])
        );
        assert_eq!(
            prepared.term_remap(1),
            Some(
                &[
                    (second_body_alpha, TermId::new(0)),
                    (second_title_alpha, TermId::new(2)),
                ][..]
            )
        );
        assert_eq!(
            prepared.term_remap(2),
            Some(&[(third_body_beta, TermId::new(1))][..])
        );
        assert_eq!(prepared.document_remap(3), None);
        assert_eq!(prepared.term_remap(3), None);
    }

    #[test]
    fn execute_rebuilds_postings_and_statistics_like_fresh_index() {
        let left = index_documents_two_fields(
            61,
            &[(9, "alpha alpha beta", "red"), (3, "beta", "blue blue")],
        );
        let right = index_documents_two_fields(61, &[(9, "alpha gamma", "red green")]);
        let left_alpha = left
            .resolve_term(FieldId::new(1), "alpha")
            .expect("left alpha exists");
        let right_alpha = right
            .resolve_term(FieldId::new(1), "alpha")
            .expect("right alpha exists");
        let prepared = prepare_merge(vec![left, right], analyzers(61, &[1, 2]))
            .expect("matching sources prepare");

        let merged = prepared.execute();
        assert_eq!(merged.document_remap(0), Some(&[(3, 0), (9, 1)][..]));
        assert_eq!(merged.document_remap(1), Some(&[(9, 2)][..]));
        let merged_alpha = merged
            .index()
            .resolve_term(FieldId::new(1), "alpha")
            .expect("merged alpha exists");
        assert!(
            merged
                .term_remap(0)
                .is_some_and(|remap| { remap.contains(&(left_alpha, merged_alpha)) })
        );
        assert!(
            merged
                .term_remap(1)
                .is_some_and(|remap| { remap.contains(&(right_alpha, merged_alpha)) })
        );
        assert_eq!(
            merged
                .index()
                .analysis_schema_id()
                .map(AnalysisSchemaId::get),
            Some(61)
        );
        assert_eq!(
            merged.index().field_names.get("body"),
            Some(&FieldId::new(1))
        );

        let oracle = index_documents_two_fields(
            61,
            &[
                (0, "beta", "blue blue"),
                (1, "alpha alpha beta", "red"),
                (2, "alpha gamma", "red green"),
            ],
        );
        assert_logical_index_matches_oracle(merged.index(), &oracle);
        assert_eq!(
            merged.index().field_stats[&FieldId::new(1)],
            FieldMetadata {
                field_id: FieldId::new(1),
                doc_count: 3,
                total_terms: 6,
            }
        );
        assert_eq!(
            merged.index().field_stats[&FieldId::new(2)],
            FieldMetadata {
                field_id: FieldId::new(2),
                doc_count: 3,
                total_terms: 5,
            }
        );

        for (field, term, expected_doc_frequency, expected_collection_frequency) in [
            (1, "alpha", 2, 3),
            (1, "beta", 2, 2),
            (1, "gamma", 1, 1),
            (2, "blue", 1, 2),
            (2, "green", 1, 1),
            (2, "red", 2, 2),
        ] {
            let merged_term = merged
                .index()
                .resolve_term(FieldId::new(field), term)
                .expect("merged term exists");
            let oracle_term = oracle
                .resolve_term(FieldId::new(field), term)
                .expect("oracle term exists");
            let merged_postings = &merged.index().postings[&merged_term];
            let oracle_postings = &oracle.postings[&oracle_term];
            assert_eq!(merged_postings, oracle_postings);
            assert_eq!(merged_postings.len(), expected_doc_frequency);
            assert_eq!(
                merged_postings
                    .iter()
                    .map(|posting| posting.term_freq)
                    .sum::<u32>(),
                expected_collection_frequency
            );
        }
    }

    #[test]
    fn execute_supports_empty_and_singleton_inputs() {
        let empty = prepare_merge(vec![], analyzers(62, &[1]))
            .expect("empty merge prepares")
            .execute();
        let empty_oracle = InMemoryIndexBuilder::new(analyzers(62, &[1])).build_index();
        assert_logical_index_matches_oracle(empty.index(), &empty_oracle);
        assert_eq!(empty.document_remap(0), None);
        assert_eq!(empty.term_remap(0), None);

        let singleton_source = index_documents(62, &[(8, "beta alpha"), (2, "alpha")]);
        let source_terms = singleton_source.terms_to_ids.clone();
        let singleton = prepare_merge(vec![singleton_source], analyzers(62, &[1]))
            .expect("singleton merge prepares")
            .execute();
        let singleton_oracle = index_documents(62, &[(0, "alpha"), (1, "beta alpha")]);
        assert_logical_index_matches_oracle(singleton.index(), &singleton_oracle);
        assert_eq!(singleton.document_remap(0), Some(&[(2, 0), (8, 1)][..]));
        let term_remap = singleton.term_remap(0).expect("singleton term remap");
        for (canonical_term, source_term) in source_terms {
            let merged_term = singleton.index().terms_to_ids[&canonical_term];
            assert!(term_remap.contains(&(source_term, merged_term)));
        }
    }

    #[test]
    fn preparation_rejects_field_total_terms_that_cannot_execute_infallibly() {
        let mut source = index_documents(63, &[(1, "alpha"), (2, "beta")]);
        source
            .field_doc_lengths
            .insert((1, FieldId::new(1)), u32::MAX);
        source.field_doc_lengths.insert((2, FieldId::new(1)), 1);

        let rejected = prepare_merge(vec![source], analyzers(63, &[1]))
            .expect_err("unrepresentable merged field statistics must reject preparation");
        assert_eq!(
            rejected.error(),
            &MergeError::FieldTotalTermsOverflow {
                field: FieldId::new(1),
                total_terms: u64::from(u32::MAX) + 1,
            }
        );
        let (_, sources, _) = rejected.into_parts();
        assert_eq!(sources.len(), 1);
        assert!(sources[0].resolve_term(FieldId::new(1), "alpha").is_some());
    }

    #[test]
    fn preparation_rejects_inconsistent_field_and_posting_token_totals_atomically() {
        let mut source = index_documents(64, &[(1, "alpha alpha")]);
        let alpha = source
            .resolve_term(FieldId::new(1), "alpha")
            .expect("alpha exists");
        source.postings.get_mut(&alpha).expect("alpha postings")[0].term_freq = 1;

        let rejected = prepare_merge(vec![source], analyzers(64, &[1]))
            .expect_err("field token totals and summed posting TF must agree");
        assert_eq!(
            rejected.error(),
            &MergeError::FieldTokenFrequencyMismatch {
                field: FieldId::new(1),
                total_terms: 2,
                collection_frequency: 1,
            }
        );
        let (_, sources, _) = rejected.into_parts();
        assert_eq!(sources.len(), 1);
        assert!(sources[0].resolve_term(FieldId::new(1), "alpha").is_some());
    }

    #[test]
    fn preparation_rejects_orphaned_merge_references_atomically() {
        let mut orphan_length = index_documents(65, &[(1, "alpha")]);
        orphan_length
            .field_doc_lengths
            .insert((99, FieldId::new(1)), 1);
        let rejected = prepare_merge(
            vec![index_documents(65, &[(7, "prefix")]), orphan_length],
            analyzers(65, &[1]),
        )
        .expect_err("field lengths must reference a source document");
        assert_eq!(
            rejected.error(),
            &MergeError::FieldLengthDocumentMissing {
                source: 1,
                document: 99,
                field: FieldId::new(1),
            }
        );
        let (_, sources, _) = rejected.into_parts();
        assert_eq!(sources.len(), 2);
        assert!(sources[1].resolve_term(FieldId::new(1), "alpha").is_some());

        let mut unknown_term = index_documents(65, &[(1, "alpha")]);
        unknown_term
            .postings
            .insert(TermId::new(99), vec![PostingEntry::new(1, 1)]);
        let rejected = prepare_merge(
            vec![index_documents(65, &[(7, "prefix")]), unknown_term],
            analyzers(65, &[1]),
        )
        .expect_err("posting maps must reference a canonical term");
        assert_eq!(
            rejected.error(),
            &MergeError::PostingTermMissing {
                source: 1,
                term: TermId::new(99),
            }
        );
        let (_, sources, _) = rejected.into_parts();
        assert_eq!(sources.len(), 2);
        assert!(sources[1].resolve_term(FieldId::new(1), "alpha").is_some());

        let mut orphan_posting = index_documents(65, &[(1, "alpha")]);
        let alpha = orphan_posting
            .resolve_term(FieldId::new(1), "alpha")
            .expect("alpha exists");
        orphan_posting
            .postings
            .get_mut(&alpha)
            .expect("alpha postings")
            .push(PostingEntry::new(99, 1));
        let rejected = prepare_merge(
            vec![index_documents(65, &[(7, "prefix")]), orphan_posting],
            analyzers(65, &[1]),
        )
        .expect_err("postings must reference a source document");
        assert_eq!(
            rejected.error(),
            &MergeError::PostingDocumentMissing {
                source: 1,
                term: alpha,
                document: 99,
            }
        );
        let (_, sources, _) = rejected.into_parts();
        assert_eq!(sources.len(), 2);
        assert!(sources[1].resolve_term(FieldId::new(1), "alpha").is_some());

        let mut duplicate_term = index_documents(65, &[(1, "alpha")]);
        let alpha = duplicate_term
            .resolve_term(FieldId::new(1), "alpha")
            .expect("alpha exists");
        duplicate_term
            .terms_to_ids
            .insert((FieldId::new(1), "beta".into()), alpha);
        let rejected = prepare_merge(
            vec![index_documents(65, &[(7, "prefix")]), duplicate_term],
            analyzers(65, &[1]),
        )
        .expect_err("binary-search term remaps require unique source term IDs");
        assert_eq!(
            rejected.error(),
            &MergeError::DuplicateCanonicalTermId {
                source: 1,
                term: alpha,
            }
        );
        let (_, sources, _) = rejected.into_parts();
        assert_eq!(sources.len(), 2);
        assert!(sources[1].resolve_term(FieldId::new(1), "alpha").is_some());
    }
}
