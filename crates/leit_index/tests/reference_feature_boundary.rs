// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Cargo feature-boundary checks for the benchmark-only reference execution index.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Creates the scratch tree for one nested consumer crate.
///
/// The tests in this file run in parallel inside a single process, so the
/// directory must be unique per consumer. `TempDir` gets that from the
/// operating system — the directory is created exclusively, so uniqueness is
/// established by the create itself rather than predicted from the process id
/// and the clock. It is also removed on drop, including when a test panics.
fn consumer_tree() -> TempDir {
    TempDir::new().expect("temporary consumer tree")
}

fn manifest(index_path: &Path, feature: bool) -> String {
    let feature = if feature {
        ", features = [\"bench-internals\"]"
    } else {
        ""
    };
    format!(
        "[package]\nname = \"reference-feature-consumer\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\nleit_index = {{ path = {:?}{feature} }}\n",
        index_path
    )
}

fn check(consumer: &Path) -> std::process::Output {
    Command::new(env!("CARGO"))
        .args(["check", "--locked", "--offline", "--quiet"])
        .current_dir(consumer)
        .env("CARGO_TARGET_DIR", consumer.join("target"))
        .output()
        .expect("nested cargo should run")
}

fn prepare_lock(consumer: &Path) {
    let output = Command::new(env!("CARGO"))
        .args(["generate-lockfile", "--offline", "--quiet"])
        .current_dir(consumer)
        // Keep the nested cargo out of any ambient CARGO_TARGET_DIR: sharing the
        // outer target directory races the outer test run's own fingerprints.
        .env("CARGO_TARGET_DIR", consumer.join("target"))
        .output()
        .expect("nested cargo should prepare the copied workspace lock");
    assert!(
        output.status.success(),
        "lock preparation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn reference_type_requires_bench_internals_feature() {
    let index_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let consumer_dir = consumer_tree();
    let consumer = consumer_dir.path();
    fs::create_dir_all(consumer.join("src")).expect("consumer tree");
    let workspace_lock = index_path
        .parent()
        .and_then(Path::parent)
        .expect("index crate should be inside the workspace")
        .join("Cargo.lock");
    fs::copy(&workspace_lock, consumer.join("Cargo.lock"))
        .expect("workspace lock should prepare exact dependency versions");
    fs::write(
        consumer.join("src/main.rs"),
        "use leit_index::ReferenceExecutionIndex;\nfn main() { let _ = core::mem::size_of::<ReferenceExecutionIndex>(); }\n",
    )
    .expect("consumer source");
    fs::write(consumer.join("Cargo.toml"), manifest(&index_path, false))
        .expect("feature-off manifest");
    prepare_lock(consumer);
    let off = check(consumer);
    assert!(
        !off.status.success(),
        "feature-off import unexpectedly compiled"
    );
    let stderr = String::from_utf8_lossy(&off.stderr);
    assert!(
        stderr.contains("ReferenceExecutionIndex") && stderr.contains("unresolved import"),
        "unexpected feature-off failure: {stderr}"
    );
    fs::write(consumer.join("Cargo.toml"), manifest(&index_path, true))
        .expect("feature-on manifest");
    let on = check(consumer);
    assert!(
        on.status.success(),
        "feature-on import failed: {}",
        String::from_utf8_lossy(&on.stderr)
    );
}

#[test]
fn prepared_decode_requires_bench_internals_feature() {
    let index_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let consumer_dir = consumer_tree();
    let consumer = consumer_dir.path();
    fs::create_dir_all(consumer.join("src")).expect("consumer tree");
    let workspace_lock = index_path
        .parent()
        .and_then(Path::parent)
        .expect("index crate should be inside the workspace")
        .join("Cargo.lock");
    fs::copy(&workspace_lock, consumer.join("Cargo.lock"))
        .expect("workspace lock should prepare exact dependency versions");
    fs::write(
        consumer.join("src/main.rs"),
        "use leit_index::ExecutionWorkspace;\nfn main() { let mut workspace = ExecutionWorkspace::new(); let _ = workspace.decode_prepared_postings(loop {}); }\n",
    )
    .expect("consumer source");
    fs::write(consumer.join("Cargo.toml"), manifest(&index_path, false))
        .expect("feature-off manifest");
    prepare_lock(consumer);
    let off = check(consumer);
    assert!(
        !off.status.success(),
        "feature-off prepared decode unexpectedly compiled"
    );
    let stderr = String::from_utf8_lossy(&off.stderr);
    assert!(
        stderr.contains("decode_prepared_postings") && stderr.contains("no method named"),
        "unexpected feature-off failure: {stderr}"
    );
    fs::write(consumer.join("Cargo.toml"), manifest(&index_path, true))
        .expect("feature-on manifest");
    let on = check(consumer);
    assert!(
        on.status.success(),
        "feature-on prepared decode failed: {}",
        String::from_utf8_lossy(&on.stderr)
    );
}
