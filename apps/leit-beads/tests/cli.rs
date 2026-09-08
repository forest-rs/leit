// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! CLI contract tests using synthetic snapshots.

use serde_json::{Value, json};
use std::{fs, process::Command};

#[test]
fn multiple_sources_filters_exact_ids_and_read_only_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.jsonl");
    let b = dir.path().join("b.jsonl");
    let row = json!({"id":"same-1","title":"Atlas lifetime","status":"open","labels":["api","gpu"],"description":"Keep the atlas alive."}).to_string();
    fs::write(&a, &row).unwrap();
    fs::write(&b, &row).unwrap();
    for query in ["atlas lifetime", "SAME-1"] {
        let output = Command::new(env!("CARGO_BIN_EXE_leit-beads"))
            .args(["search", "--export"])
            .arg(&a)
            .arg("--export")
            .arg(&b)
            .args([
                "--json", "--status", "open", "--label", "api", "--label", "gpu", "--limit", "1",
                query,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["total"], 2);
        assert_eq!(value["results"].as_array().unwrap().len(), 1);
        assert_eq!(value["sources"].as_array().unwrap().len(), 2);
        if query == "SAME-1" {
            assert!(value["results"][0]["score"].is_null());
        }
    }
    assert_eq!(fs::read_to_string(a).unwrap(), row);
    assert_eq!(fs::read_to_string(b).unwrap(), row);
}

#[test]
fn malformed_input_has_location_and_no_partial_results() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.jsonl");
    fs::write(&path, "{\"id\":\"a\",\"title\":\"atlas\"}\nnot json\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_leit-beads"))
        .args(["search", "--export"])
        .arg(path)
        .args(["--json", "atlas"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("bad.jsonl:2:"));
}
