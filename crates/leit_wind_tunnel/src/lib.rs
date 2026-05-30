// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Deterministic test harness and corpus generator for the Leit retrieval kernel.
//!
//! This crate provides:
//! - A deterministic corpus generator using seeded hashing
//! - Zipfian vocabulary distribution (s=1.0) over 500 common English words
//! - Query fixtures for benchmarking (single-term, multi-term AND/OR, fielded, cross-field)
//! - Integration testing utilities for Phase 2 evaluation scenarios
//!
//! # Relationship to `leit_benchmark`
//!
//! `leit_benchmark` and the wind tunnel are complementary, not redundant:
//!
//! - `leit_benchmark` is the Phase 1 **deterministic smoke test**. It runs a
//!   small, fixed, hand-authored corpus through the stack and asserts stable hit
//!   IDs, timing each phase with a single `Instant` measurement. Its job is
//!   *correctness/stability*, it pulls no heavy dependencies, and it runs in CI.
//! - The wind tunnel is the Phase 2 **performance measurement lab**. It generates
//!   seeded, scalable corpora (1K/10K/…) and feeds them to Criterion benchmarks
//!   (warmup, sampling, outlier analysis, HTML reports, saved regression
//!   baselines) across indexing and all five query execution paths. Its job is
//!   *performance baselines and regression detection*, and it is run manually or
//!   in a dedicated performance job — never in CI (too slow and noisy).
//!
//! Criterion lives only in the `leit_wind_tunnel_index` / `leit_wind_tunnel_query`
//! bench crates so the heavy dependency never reaches `leit_benchmark` or any
//! primary crate. Keep these distinct: do not add Criterion to `leit_benchmark`,
//! and do not duplicate the smoke test's correctness assertions here.

#![warn(
    missing_debug_implementations,
    missing_docs,
    trivial_numeric_casts,
    unnameable_types,
    unreachable_pub,
    unused_import_braces,
    unused_lifetimes
)]

pub mod corpus;
pub mod query_fixtures;
pub mod vocabulary;

pub use corpus::CorpusGenerator;
pub use query_fixtures::QueryFixtures;
pub use vocabulary::Vocabulary;
