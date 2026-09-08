// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{ffi::OsString, path::PathBuf};

pub(crate) const HELP: &str = "Search Beads JSONL snapshots with Leit.

Usage: leit-beads search [OPTIONS] QUERY...

  --repo PATH       Read PATH/.beads/issues.jsonl (repeatable; default: .)
  --export FILE     Read an explicit JSONL export (repeatable)
  --status VALUE    Keep this status (repeatable, OR)
  --type VALUE      Keep this issue type (repeatable, OR)
  --priority 0..4   Keep this priority (repeatable, OR)
  --label VALUE     Require this exact label (repeatable, AND)
  --any             Match any query term (default: all terms)
  --limit N         Maximum results, 1..100 (default: 10)
  --json            Emit one JSON object with sources, timings, and results
  --                Treat all remaining arguments as query text
  -h, --help        Show help

Queries are literal text, not boolean syntax. An exact issue ID performs a
filtered ID lookup. All sources are read-only snapshots, not live Beads databases.
";

#[derive(Debug, Default)]
pub(crate) struct Filters {
    pub statuses: Vec<String>,
    pub kinds: Vec<String>,
    pub priorities: Vec<u64>,
    pub labels: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct Options {
    pub sources: Vec<PathBuf>,
    pub filters: Filters,
    pub query: String,
    pub limit: usize,
    pub any: bool,
    pub json: bool,
}

impl Options {
    pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Self>, String> {
        let mut args = args.into_iter();
        match args.next().as_deref().and_then(|s| s.to_str()) {
            Some("--help" | "-h") | None => return Ok(None),
            Some("search") => {}
            _ => return Err("expected 'search'; use --help for usage".into()),
        }
        let mut result = Self {
            sources: Vec::new(),
            filters: Filters::default(),
            query: String::new(),
            limit: 10,
            any: false,
            json: false,
        };
        let mut words = Vec::new();
        let mut literal = false;
        while let Some(arg) = args.next() {
            let flag = arg.to_str().ok_or("query/options must be valid UTF-8")?;
            if literal {
                words.push(flag.to_owned());
                continue;
            }
            match flag {
                "--help" | "-h" => return Ok(None),
                "--" => literal = true,
                "--any" => result.any = true,
                "--json" => result.json = true,
                "--repo" | "--export" => {
                    let value = args
                        .next()
                        .ok_or_else(|| format!("{flag} requires a path"))?;
                    let path = PathBuf::from(value);
                    result.sources.push(if flag == "--repo" {
                        path.join(".beads/issues.jsonl")
                    } else {
                        path
                    });
                }
                "--status" | "--type" | "--priority" | "--label" | "--limit" => {
                    let value = args
                        .next()
                        .ok_or_else(|| format!("{flag} requires a value"))?;
                    let value = value
                        .into_string()
                        .map_err(|_| format!("{flag} requires UTF-8"))?;
                    match flag {
                        "--status" => result.filters.statuses.push(value),
                        "--type" => result.filters.kinds.push(value),
                        "--label" => result.filters.labels.push(value),
                        "--priority" => {
                            let priority =
                                value.parse::<u64>().map_err(|_| "priority must be 0..4")?;
                            if priority > 4 {
                                return Err("priority must be 0..4".into());
                            }
                            result.filters.priorities.push(priority);
                        }
                        _ => {
                            result.limit = value.parse().map_err(|_| "limit must be 1..100")?;
                            if !(1..=100).contains(&result.limit) {
                                return Err("limit must be 1..100".into());
                            }
                        }
                    }
                }
                _ if flag.starts_with('-') => {
                    return Err(format!(
                        "unknown option {flag}; use -- before a query beginning with '-'"
                    ));
                }
                _ => words.push(flag.to_owned()),
            }
        }
        result.query = words.join(" ").trim().to_owned();
        if result.query.is_empty() {
            return Err("provide query text or an exact issue ID".into());
        }
        if result.sources.is_empty() {
            result.sources.push(PathBuf::from(".beads/issues.jsonl"));
        }
        Ok(Some(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Option<Options>, String> {
        Options::parse(args.iter().map(OsString::from))
    }
    #[test]
    fn parses_multiple_sources_filters_and_literal_query() {
        let o = parse(&[
            "search",
            "--repo",
            "../one",
            "--export",
            "two.jsonl",
            "--status",
            "open",
            "--label",
            "api",
            "--json",
            "--",
            "--odd",
            "term",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(o.sources.len(), 2);
        assert_eq!(o.query, "--odd term");
        assert!(o.json);
        assert_eq!(o.filters.statuses, ["open"]);
    }
    #[test]
    fn rejects_bad_options_and_empty_queries() {
        for args in [
            vec!["search"],
            vec!["search", "--limit", "0", "x"],
            vec!["search", "--priority", "5", "x"],
            vec!["search", "--oops"],
            vec!["search", "--repo"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
}
