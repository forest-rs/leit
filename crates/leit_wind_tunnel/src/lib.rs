// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Deterministic test harness and corpus generator for the Leit retrieval kernel.
//!
//! This crate provides:
//! - A deterministic corpus generator using seeded hashing
//! - Zipfian vocabulary distribution (s=1.0) over 500 common English words
//! - Query fixtures for benchmarking (single-term, multi-term AND/OR, fielded, cross-field)
//! - Integration testing utilities for Phase 2 evaluation scenarios

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
