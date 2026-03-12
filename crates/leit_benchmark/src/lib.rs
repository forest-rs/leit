//! Deterministic benchmark harnesses for the Phase 1 Leit stack.

use std::time::{Duration, Instant};

use leit_core::FieldId;
use leit_index::{ExecutionWorkspace, InMemoryIndexBuilder, SearchScorer};
use leit_text::{Analyzer, FieldAnalyzers, UnicodeNormalizer, WhitespaceTokenizer};

/// A fixed Phase 1 benchmark document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchmarkDocument {
    /// Stable document identifier.
    pub id: u32,
    /// Title field content.
    pub title: &'static str,
    /// Body field content.
    pub body: &'static str,
}

/// A fixed Phase 1 benchmark query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchmarkQuery {
    /// Stable query label.
    pub name: &'static str,
    /// Query text.
    pub text: &'static str,
    /// Maximum hits to retain.
    pub limit: usize,
}

/// A deterministic benchmark scenario.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchmarkScenario {
    /// Scenario name.
    pub name: &'static str,
    /// Documents to index.
    pub documents: Vec<BenchmarkDocument>,
    /// Queries to execute.
    pub queries: Vec<BenchmarkQuery>,
}

/// Result for a single benchmark query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryRunReport {
    /// Query label.
    pub name: &'static str,
    /// Number of hits returned.
    pub hit_count: usize,
    /// IDs of the returned hits, in rank order.
    pub hit_ids: Vec<u32>,
    /// Measured query latency.
    pub latency: Duration,
}

/// Result for one deterministic benchmark run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchmarkReport {
    /// Scenario name.
    pub scenario: &'static str,
    /// Indexed document count.
    pub document_count: usize,
    /// Number of executed queries.
    pub query_count: usize,
    /// Elapsed indexing time.
    pub indexing_time: Duration,
    /// Per-query results.
    pub query_runs: Vec<QueryRunReport>,
}

/// Return the fixed Phase 1 smoke scenario.
#[must_use]
pub fn phase1_smoke_scenario() -> BenchmarkScenario {
    BenchmarkScenario {
        name: "phase1-smoke",
        documents: vec![
            BenchmarkDocument {
                id: 1,
                title: "Rust Retrieval",
                body: "Rust retrieval systems use inverted indices",
            },
            BenchmarkDocument {
                id: 2,
                title: "Memory Safety",
                body: "Rust memory safety relies on ownership",
            },
            BenchmarkDocument {
                id: 3,
                title: "Search Engines",
                body: "Search engines rank retrieval results with bm25",
            },
            BenchmarkDocument {
                id: 4,
                title: "Unicode Text",
                body: "Unicode normalization and case folding improve search",
            },
        ],
        queries: vec![
            BenchmarkQuery {
                name: "rust",
                text: "rust",
                limit: 3,
            },
            BenchmarkQuery {
                name: "retrieval",
                text: "retrieval",
                limit: 3,
            },
            BenchmarkQuery {
                name: "unicode",
                text: "unicode",
                limit: 3,
            },
        ],
    }
}

/// Run the deterministic Phase 1 smoke benchmark.
pub fn run_phase1_smoke() -> Result<BenchmarkReport, String> {
    run_scenario(&phase1_smoke_scenario())
}

/// Run a benchmark scenario over the current in-memory stack.
pub fn run_scenario(scenario: &BenchmarkScenario) -> Result<BenchmarkReport, String> {
    let mut analyzers = FieldAnalyzers::new();
    analyzers.set(
        FieldId::new(1),
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );
    analyzers.set(
        FieldId::new(2),
        Analyzer::new(WhitespaceTokenizer::new()).with_normalizer(UnicodeNormalizer::new()),
    );

    let mut builder = InMemoryIndexBuilder::new(analyzers);
    builder.register_field_alias(FieldId::new(1), "title");
    builder.register_field_alias(FieldId::new(2), "body");

    let indexing_start = Instant::now();
    for document in &scenario.documents {
        builder
            .index_document(
                document.id,
                &[
                    (FieldId::new(1), document.title),
                    (FieldId::new(2), document.body),
                ],
            )
            .map_err(|error| format!("indexing document {} failed: {error:?}", document.id))?;
    }
    let index = builder.build_index();
    let indexing_time = indexing_start.elapsed();

    let mut query_runs = Vec::with_capacity(scenario.queries.len());
    let mut workspace = ExecutionWorkspace::new();
    for query in &scenario.queries {
        let query_start = Instant::now();
        let hits = workspace
            .search(&index, query.text, query.limit, SearchScorer::bm25())
            .map_err(|error| format!("benchmark query '{}' failed: {error:?}", query.name))?;
        let latency = query_start.elapsed();
        let hit_ids = hits.iter().map(|hit| hit.id).collect();
        query_runs.push(QueryRunReport {
            name: query.name,
            hit_count: hits.len(),
            hit_ids,
            latency,
        });
    }

    Ok(BenchmarkReport {
        scenario: scenario.name,
        document_count: scenario.documents.len(),
        query_count: scenario.queries.len(),
        indexing_time,
        query_runs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase1_smoke_scenario_is_stable() {
        let scenario = phase1_smoke_scenario();

        assert_eq!(scenario.name, "phase1-smoke");
        assert_eq!(scenario.documents.len(), 4);
        assert_eq!(scenario.queries.len(), 3);
        assert_eq!(scenario.documents[0].id, 1);
        assert_eq!(scenario.queries[0].text, "rust");
    }

    #[test]
    fn benchmark_run_produces_stable_hit_shapes() {
        let report = run_phase1_smoke().expect("benchmark run should succeed");

        assert_eq!(report.scenario, "phase1-smoke");
        assert_eq!(report.document_count, 4);
        assert_eq!(report.query_count, 3);
        assert_eq!(report.query_runs.len(), 3);
        assert_eq!(report.query_runs[0].name, "rust");
        assert_eq!(report.query_runs[0].hit_ids, vec![1]);
        assert_eq!(report.query_runs[1].name, "retrieval");
        assert_eq!(report.query_runs[1].hit_ids, vec![1]);
        assert_eq!(report.query_runs[2].name, "unicode");
        assert_eq!(report.query_runs[2].hit_ids, vec![4]);
    }
}
