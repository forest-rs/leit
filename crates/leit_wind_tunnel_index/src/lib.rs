// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Criterion indexing-throughput benchmarks for the Leit wind tunnel.
//!
//! This crate intentionally has no library surface of its own — all logic lives
//! in its benchmark targets. It exists solely to isolate performance tooling
//! from the primary workspace crates. Run Criterion latency measurements with
//! `cargo bench -p leit_wind_tunnel_index --bench indexing` and whole-build
//! allocation measurements with
//! `cargo bench -p leit_wind_tunnel_index --bench index_allocations`.

#![warn(
    missing_debug_implementations,
    trivial_numeric_casts,
    unnameable_types,
    unreachable_pub,
    unused_import_braces,
    unused_lifetimes
)]
