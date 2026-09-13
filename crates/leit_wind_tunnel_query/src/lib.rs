// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Criterion query-latency benchmarks for the Leit wind tunnel.
//!
//! This crate intentionally has no library surface of its own — all logic lives
//! in its benchmark targets. It exists solely to isolate performance tooling
//! from the primary workspace crates. Run Criterion latency measurements with
//! `cargo bench -p leit_wind_tunnel_query --bench query_latency` and allocation
//! measurements with
//! `cargo bench -p leit_wind_tunnel_query --bench query_allocations`.

#![warn(
    missing_debug_implementations,
    trivial_numeric_casts,
    unnameable_types,
    unreachable_pub,
    unused_import_braces,
    unused_lifetimes
)]
