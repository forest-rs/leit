// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! CLI entry point for the Phase 1 Leit benchmark harness.

use leit_benchmark::run_phase1_smoke;

fn main() -> Result<(), String> {
    let report = run_phase1_smoke()?;

    println!("scenario: {}", report.scenario);
    println!("documents: {}", report.document_count);
    println!("queries: {}", report.query_count);
    println!("indexing_time_ms: {}", report.indexing_time.as_millis());
    for query in report.query_runs {
        println!(
            "query={} hits={} latency_us={} ids={:?}",
            query.name,
            query.hit_count,
            query.latency.as_micros(),
            query.hit_ids
        );
    }

    Ok(())
}
