// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Leit integration test harness.
//!
//! This crate hosts canonical cross-crate integration suites that validate
//! readiness signals (e.g., Phase 1 seams) across the Leit workspace. The
//! library itself intentionally contains no APIs; the tests live under
//! `tests/` and use the public interfaces from other crates.

#![deny(missing_docs)]

/// Shared helpers for integration tests will live here once needed.
pub mod harness {}
