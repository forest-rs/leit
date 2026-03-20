// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use leit_core::QueryNodeId;

use crate::types::QueryArena;
use crate::types::{BooleanOp, UserQueryNode, UserQueryProgram};

pub(crate) fn query_node_id(index: usize) -> QueryNodeId {
    QueryNodeId::new(u32::try_from(index).expect("query program exceeded u32 node IDs"))
}

/// Builder for constructing query programs.
#[derive(Debug, Default)]
pub struct QueryBuilder {
    arena: QueryArena,
    root: Option<QueryNodeId>,
}

impl QueryBuilder {
    /// Create a new query builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the query program from the tracked root node.
    pub fn build(self) -> Option<UserQueryProgram> {
        if self.arena.is_empty() {
            return None;
        }
        let root = self.root.unwrap_or_else(|| {
            query_node_id(
                self.arena
                    .len()
                    .checked_sub(1)
                    .expect("query arena cannot be empty"),
            )
        });
        UserQueryProgram::is_valid(&self.arena, root)
            .then(|| UserQueryProgram::new(self.arena, root))
    }

    /// Set the root node for the query program.
    pub const fn set_root(&mut self, id: QueryNodeId) {
        self.root = Some(id);
    }

    /// Add a term query.
    pub fn term<S: Into<Arc<str>>>(&mut self, term: S) -> QueryNodeId {
        let id = self.arena.push(UserQueryNode::Term {
            term: term.into(),
            field: None,
        });
        self.root = Some(id);
        id
    }

    /// Add a term query with a field.
    pub fn term_with_field<S: Into<Arc<str>>>(&mut self, term: S, field: S) -> QueryNodeId {
        let id = self.arena.push(UserQueryNode::Term {
            term: term.into(),
            field: Some(field.into()),
        });
        self.root = Some(id);
        id
    }

    /// Add a phrase query with initial terms.
    ///
    /// Note: phrase execution is not yet implemented in the Phase 1 planner.
    /// This node type is available for query representation and inspection.
    pub fn phrase(&mut self, terms: Vec<Arc<str>>) -> QueryNodeId {
        let id = self.arena.push(UserQueryNode::Phrase { terms, slop: 0 });
        self.root = Some(id);
        id
    }

    /// Add a phrase query with terms and slop.
    ///
    /// Note: phrase execution is not yet implemented in the Phase 1 planner.
    /// This node type is available for query representation and inspection.
    pub fn phrase_with_slop(&mut self, terms: Vec<Arc<str>>, slop: u32) -> QueryNodeId {
        let id = self.arena.push(UserQueryNode::Phrase { terms, slop });
        self.root = Some(id);
        id
    }

    /// Add a boolean AND query with initial children.
    pub fn and(&mut self, children: Vec<QueryNodeId>) -> QueryNodeId {
        let id = self.arena.push(UserQueryNode::Boolean {
            op: BooleanOp::And,
            children,
        });
        self.root = Some(id);
        id
    }

    /// Add a boolean OR query with initial children.
    pub fn or(&mut self, children: Vec<QueryNodeId>) -> QueryNodeId {
        let id = self.arena.push(UserQueryNode::Boolean {
            op: BooleanOp::Or,
            children,
        });
        self.root = Some(id);
        id
    }

    /// Add a boolean NOT query with child.
    pub fn not(&mut self, child: QueryNodeId) -> QueryNodeId {
        let id = self.arena.push(UserQueryNode::Boolean {
            op: BooleanOp::Not,
            children: vec![child],
        });
        self.root = Some(id);
        id
    }

    /// Add a boost query.
    pub fn boost(&mut self, child: QueryNodeId, factor: f32) -> QueryNodeId {
        let id = self.arena.push(UserQueryNode::Boost { child, factor });
        self.root = Some(id);
        id
    }
}

/// Create a term query in one call.
pub fn term<S: Into<Arc<str>>>(term: S) -> UserQueryProgram {
    let mut builder = QueryBuilder::new();
    builder.term(term);
    builder.build().expect("term should create valid program")
}

/// Create a term query with a field in one call.
pub fn term_with_field<S: Into<Arc<str>>>(term: S, field: S) -> UserQueryProgram {
    let mut builder = QueryBuilder::new();
    builder.term_with_field(term, field);
    builder
        .build()
        .expect("term_with_field should create valid program")
}

/// Create a phrase query in one call.
pub fn phrase(terms: &[&str]) -> UserQueryProgram {
    let mut builder = QueryBuilder::new();
    let terms: Vec<Arc<str>> = terms.iter().map(|t| (*t).into()).collect();
    builder.phrase(terms);
    builder.build().expect("phrase should create valid program")
}

/// Create a phrase query with slop in one call.
pub fn phrase_with_slop(terms: &[&str], slop: u32) -> UserQueryProgram {
    let mut builder = QueryBuilder::new();
    let terms: Vec<Arc<str>> = terms.iter().map(|t| (*t).into()).collect();
    builder.phrase_with_slop(terms, slop);
    builder
        .build()
        .expect("phrase_with_slop should create valid program")
}
