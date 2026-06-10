// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Criterion query-latency benchmarks for the Leit wind tunnel.
//!
//! This crate intentionally has no library surface of its own — all logic lives
//! in `benches/query_latency.rs`. It exists solely to isolate the Criterion
//! dev-dependency from the primary workspace crates (see STORY-0103). Run the
//! benchmarks with `cargo bench -p leit_wind_tunnel_query`.

#![warn(
    missing_debug_implementations,
    trivial_numeric_casts,
    unnameable_types,
    unreachable_pub,
    unused_import_braces,
    unused_lifetimes
)]
