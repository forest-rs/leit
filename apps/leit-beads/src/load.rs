// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::model::{Dependency, Issue, Source};
use serde_json::Value;
use std::{collections::BTreeSet, fs::File, io::Read, path::PathBuf, time::UNIX_EPOCH};

const MAX_EXPORT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 1024 * 1024;

pub(crate) fn load(paths: &[PathBuf]) -> Result<(Vec<Source>, Vec<Issue>), String> {
    let paths: BTreeSet<_> = paths.iter().map(|path| path.canonicalize().map_err(|e| {
        format!("{}: {e}. Supply an existing Beads issues.jsonl snapshot with --repo or --export; live databases are not read", path.display())
    })).collect::<Result<_, _>>()?;
    let mut sources = Vec::new();
    let mut issues = Vec::new();
    for path in paths {
        let file = File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err(format!("{} is not a regular file", path.display()));
        }
        let mut content = String::new();
        file.take(MAX_EXPORT_BYTES + 1)
            .read_to_string(&mut content)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if content.len() as u64 > MAX_EXPORT_BYTES {
            return Err(format!(
                "{} exceeds the 64 MiB snapshot limit",
                path.display()
            ));
        }
        let source_index = sources.len();
        let mut parsed = Vec::new();
        let mut ids = BTreeSet::new();
        let mut missing_comments = 0;
        for (line_index, line) in content.trim_start_matches('\u{feff}').lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let result = (|| {
                if line.len() > MAX_RECORD_BYTES {
                    return Err("record exceeds 1 MiB".into());
                }
                let value: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
                let issue = parse_issue(&value, source_index)?;
                if !ids.insert(issue.id.clone()) {
                    return Err(format!("duplicate issue ID {} in one snapshot", issue.id));
                }
                let actual_comments = array(&value, "comments")?.len();
                let declared_comments = match value.get("comment_count") {
                    None | Some(Value::Null) => None,
                    Some(value) => Some(
                        value
                            .as_u64()
                            .ok_or("comment_count must be a nonnegative integer")?,
                    ),
                };
                if declared_comments.is_some_and(|n| n > actual_comments as u64) {
                    missing_comments += 1;
                }
                Ok(issue)
            })();
            parsed.push(
                result
                    .map_err(|e: String| format!("{}:{}: {e}", path.display(), line_index + 1))?,
            );
        }
        parsed.sort_by(|a, b| a.id.cmp(&b.id));
        let repo = if path
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|s| s == ".beads")
        {
            path.parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.file_name())
        } else {
            path.file_stem()
        }
        .map_or_else(|| "export".into(), |s| s.to_string_lossy().into_owned());
        sources.push(Source {
            path,
            repo,
            bytes: content.len(),
            issue_count: parsed.len(),
            missing_comments,
            modified_unix_seconds: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|t| t.as_secs()),
        });
        issues.extend(parsed);
    }
    Ok((sources, issues))
}

fn string(value: &Value, key: &str) -> Result<String, String> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err(format!("{key} must be a string or null")),
    }
}
fn required(value: &Value, key: &str) -> Result<String, String> {
    let result = string(value, key)?;
    if result.trim().is_empty() {
        Err(format!("missing nonempty {key}"))
    } else {
        Ok(result)
    }
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value], String> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        _ => Err(format!("{key} must be an array or null")),
    }
}
fn parse_issue(value: &Value, source: usize) -> Result<Issue, String> {
    if !value.is_object() {
        return Err("expected an issue object".into());
    }
    let record_type = string(value, "_type")?;
    if !record_type.is_empty() && record_type != "issue" {
        return Err(format!("unsupported record type {record_type}"));
    }
    let priority = match value.get("priority") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_u64()
                .filter(|v| *v <= 4)
                .ok_or("priority must be an integer from 0 to 4")?,
        ),
    };
    let labels = array(value, "labels")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "labels must contain strings".into())
        })
        .collect::<Result<_, String>>()?;
    let comments = array(value, "comments")?
        .iter()
        .map(|v| required(v, "text"))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n\n");
    let dependencies = array(value, "dependencies")?
        .iter()
        .map(|v| {
            Ok(Dependency {
                id: required(v, "depends_on_id")?,
                kind: string(v, "type")?,
            })
        })
        .collect::<Result<_, String>>()?;
    let mut notes = string(value, "notes")?;
    let closed = string(value, "close_reason")?;
    if !closed.is_empty() {
        notes.push('\n');
        notes.push_str(&closed);
    }
    Ok(Issue {
        source,
        id: required(value, "id")?,
        title: required(value, "title")?,
        status: string(value, "status")?,
        kind: string(value, "issue_type")?,
        priority,
        labels,
        updated_at: string(value, "updated_at")?,
        description: string(value, "description")?,
        design: string(value, "design")?,
        acceptance: string(value, "acceptance_criteria")?,
        notes,
        comments,
        dependencies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn optional_fields_and_rich_records_parse() {
        let i = parse_issue(&json!({"id":"x-1","title":"hello","description":null,"priority":2,"labels":["api"],"comments":[{"text":"a useful comment"}],"dependencies":[{"depends_on_id":"x-2","type":"blocks"}]}), 3).unwrap();
        assert_eq!(i.source, 3);
        assert_eq!(i.comments, "a useful comment");
        assert_eq!(i.dependencies[0].id, "x-2");
        for bad in [
            json!({"id":"x"}),
            json!({"id":"x","title":"t","labels":"api"}),
            json!({"id":"x","title":"t","priority":9}),
        ] {
            assert!(parse_issue(&bad, 0).is_err());
        }
    }
    #[test]
    fn source_deduplication_duplicate_ids_and_line_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("issues.jsonl");
        let row = "{\"id\":\"x-1\",\"title\":\"hello\",\"comment_count\":2}\n";
        std::fs::write(&path, row).unwrap();
        let (sources, issues) = load(&[path.clone(), path.clone()]).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(issues.len(), 1);
        assert_eq!(sources[0].missing_comments, 1);
        std::fs::write(&path, format!("{row}{row}")).unwrap();
        assert!(
            load(std::slice::from_ref(&path))
                .unwrap_err()
                .contains(":2: duplicate")
        );
        std::fs::write(&path, format!("{row}bad json\n")).unwrap();
        assert!(load(&[path]).unwrap_err().contains(":2:"));
    }
}
