// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    cli::Options,
    model::{Issue, Source},
    search::{FIELDS, Outcome},
    text,
};
use serde_json::json;
use std::fmt::Write;

fn snippet<'a>(issue: &'a Issue, terms: &[String]) -> (&'a str, &'static str, Vec<&'static str>) {
    let fields = issue.fields();
    let counts: Vec<_> = fields
        .iter()
        .map(|value| {
            let found = text::terms(value);
            terms.iter().filter(|term| found.contains(term)).count()
        })
        .collect();
    let matched = FIELDS
        .iter()
        .zip(&counts)
        .filter(|(_, count)| **count > 0)
        .map(|((name, _), _)| *name)
        .collect();
    // Prefer prose to repeating the title, choosing the field with most query terms.
    let index = (2..fields.len())
        .max_by_key(|&i| (counts[i], std::cmp::Reverse(i)))
        .unwrap_or(2);
    (fields[index], FIELDS[index].0, matched)
}

pub(crate) fn render(
    sources: &[Source],
    issues: &[Issue],
    options: &Options,
    outcome: &Outcome,
    load_ms: f64,
) -> Result<String, String> {
    let mode = if outcome.exact_id {
        "exact_id"
    } else if options.any {
        "any"
    } else {
        "all"
    };
    if options.json {
        let results: Vec<_> = outcome.hits.iter().map(|hit| {
            let issue = &issues[hit.issue];
            let source = &sources[issue.source];
            let (prose, field, matched) = snippet(issue, &outcome.terms);
            json!({"id": issue.id, "repo": source.repo, "source": source.path,
                "title": issue.title, "status": issue.status, "type": issue.kind,
                "priority": issue.priority, "labels": issue.labels, "updated_at": issue.updated_at,
                "score": hit.score, "matched_fields": matched, "excerpt_field": field,
                "excerpt": text::excerpt(prose, &outcome.terms),
                "dependencies": issue.dependencies.iter().map(|d| json!({"id":d.id,"type":d.kind})).collect::<Vec<_>>()})
        }).collect();
        return serde_json::to_string_pretty(&json!({"query": options.query, "mode": mode,
            "terms": outcome.terms, "total": outcome.total, "results": results,
            "sources": sources.iter().map(|s| json!({"repo":s.repo,"path":s.path,
                "issues":s.issue_count,"bytes":s.bytes,"modified_unix_seconds":s.modified_unix_seconds,
                "issues_with_missing_comments":s.missing_comments})).collect::<Vec<_>>(),
            "timings_ms": {"load":load_ms,"index":outcome.index_ms,"search":outcome.search_ms},
            "execution": {"scored_postings":outcome.stats.scored_postings,
                "skipped_blocks":outcome.stats.skipped_blocks,"collected_hits":outcome.stats.collected_hits}
        })).map(|s| s + "\n").map_err(|e| e.to_string());
    }
    let mut out = String::new();
    for source in sources {
        writeln!(
            out,
            "Snapshot {}: {} issues, {} bytes; file mtime {}\n  {}",
            text::plain(&source.repo),
            source.issue_count,
            source.bytes,
            source
                .modified_unix_seconds
                .map_or_else(|| "unknown".into(), |t| format!("{t} (Unix seconds)")),
            text::plain(&source.path.display().to_string())
        )
        .unwrap();
        if source.missing_comments > 0 {
            writeln!(
                out,
                "  Warning: {} issues declare comments absent from this export.",
                source.missing_comments
            )
            .unwrap();
        }
    }
    writeln!(out, "\n{} matches; showing {} ({mode}). Load {load_ms:.1} ms, index {:.1} ms, search {:.1} ms.\n", outcome.total, outcome.hits.len(), outcome.index_ms, outcome.search_ms).unwrap();
    for hit in &outcome.hits {
        let issue = &issues[hit.issue];
        let (prose, field, _) = snippet(issue, &outcome.terms);
        writeln!(
            out,
            "{}/{}  {}",
            text::plain(&sources[issue.source].repo),
            text::plain(&issue.id),
            text::plain(&issue.title)
        )
        .unwrap();
        writeln!(
            out,
            "  {} · {} · priority {}{}",
            text::plain(&issue.status),
            text::plain(&issue.kind),
            issue
                .priority
                .map_or_else(|| "unset".into(), |p| p.to_string()),
            hit.score
                .map_or_else(String::new, |s| format!(" · score {s:.3}"))
        )
        .unwrap();
        if !prose.is_empty() {
            writeln!(out, "  {field}: {}", text::excerpt(prose, &outcome.terms)).unwrap();
        }
        if !issue.labels.is_empty() {
            writeln!(out, "  labels: {}", text::plain(&issue.labels.join(", "))).unwrap();
        }
        for dependency in &issue.dependencies {
            writeln!(
                out,
                "  {} → {}",
                text::plain(&dependency.kind),
                text::plain(&dependency.id)
            )
            .unwrap();
        }
        out.push('\n');
    }
    Ok(out)
}
