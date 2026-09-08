// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::cli::Filters;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct Source {
    pub path: PathBuf,
    pub repo: String,
    pub modified_unix_seconds: Option<u64>,
    pub issue_count: usize,
    pub bytes: usize,
    pub missing_comments: usize,
}

#[derive(Debug, Default)]
pub(crate) struct Issue {
    pub source: usize,
    pub id: String,
    pub title: String,
    pub status: String,
    pub kind: String,
    pub priority: Option<u64>,
    pub labels: Vec<String>,
    pub updated_at: String,
    pub description: String,
    pub design: String,
    pub acceptance: String,
    pub notes: String,
    pub comments: String,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug)]
pub(crate) struct Dependency {
    pub id: String,
    pub kind: String,
}

impl Issue {
    pub(crate) fn fields(&self) -> [&str; 7] {
        [
            &self.id,
            &self.title,
            &self.description,
            &self.design,
            &self.acceptance,
            &self.notes,
            &self.comments,
        ]
    }

    pub(crate) fn allowed(&self, filters: &Filters) -> bool {
        (filters.statuses.is_empty() || filters.statuses.contains(&self.status))
            && (filters.kinds.is_empty() || filters.kinds.contains(&self.kind))
            && (filters.priorities.is_empty()
                || self
                    .priority
                    .is_some_and(|p| filters.priorities.contains(&p)))
            && filters
                .labels
                .iter()
                .all(|label| self.labels.contains(label))
    }
}
