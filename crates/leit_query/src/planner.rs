// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::boxed::Box;
use alloc::vec::Vec;

use leit_core::QueryNodeId;

use crate::builder::query_node_id;
use crate::types::{
    ExecutionPlan, FeatureSet, PlannerScratch, PlanningContext, QueryError, QueryNode, QueryProgram,
};

const fn checked_len_plus_one(len: usize) -> usize {
    len.checked_add(1).expect("query node count overflow")
}

/// An AND node with no children, which matches nothing at execution time.
const EMPTY_NODE: QueryNode = QueryNode::And {
    children: Vec::new(),
    boost: 1.0,
};

/// Phase 1 planner for execution-facing query programs.
#[derive(Clone, Debug)]
pub struct Planner {
    max_depth: usize,
    max_nodes: usize,
}

impl Planner {
    /// Create a planner with default limits.
    pub const fn new() -> Self {
        Self {
            max_depth: 32,
            max_nodes: 1024,
        }
    }

    /// Set the maximum planner depth.
    #[must_use]
    pub const fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set the maximum planner node count.
    ///
    /// Capped to `u16::MAX` to keep selectivity computations within supported
    /// precision.
    #[must_use]
    pub const fn with_max_nodes(mut self, count: usize) -> Self {
        self.max_nodes = if count > u16::MAX as usize {
            u16::MAX as usize
        } else {
            count
        };
        self
    }

    /// Plan a textual query into an execution-facing query program.
    pub fn plan(
        &self,
        query: &str,
        context: &PlanningContext<'_>,
        scratch: &mut PlannerScratch,
    ) -> Result<ExecutionPlan, QueryError> {
        scratch.reset();
        let parsed = parse_phase1_query(query)?;
        let depth = parsed.depth();
        if depth > self.max_depth {
            return Err(QueryError::MaxDepthExceeded {
                max_depth: self.max_depth,
                actual_depth: depth,
            });
        }

        let mut nodes = Vec::new();
        let root = lower_phase1_expr(&parsed, context, &mut nodes, self.max_nodes)?;
        let node_count = nodes.len();
        if node_count > self.max_nodes {
            return Err(QueryError::MaxNodesExceeded {
                max_nodes: self.max_nodes,
                actual_nodes: node_count,
            });
        }

        let selectivity = if node_count == 0 {
            1.0
        } else {
            let node_count_u16 = u16::try_from(node_count)
                .expect("planner node count exceeded supported selectivity precision");
            1.0 / f32::from(node_count_u16)
        };

        Ok(ExecutionPlan {
            program: QueryProgram::try_new(nodes, root, depth)?,
            selectivity,
            cost: u32::try_from(node_count).expect("planner node count exceeded u32 cost"),
            required_features: FeatureSet::basic(),
        })
    }
}

impl Default for Planner {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Phase1Expr {
    Term {
        field: Option<alloc::string::String>,
        term: alloc::string::String,
        boost: f32,
    },
    And(Vec<Self>),
    Or(Vec<Self>),
    Not(Box<Self>),
}

impl Phase1Expr {
    fn depth(&self) -> usize {
        match self {
            Self::Term { .. } => 1,
            Self::Not(child) => child.depth().checked_add(1).expect("query depth overflow"),
            Self::And(children) | Self::Or(children) => children
                .iter()
                .map(Self::depth)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .expect("query depth overflow"),
        }
    }
}

fn parse_phase1_query(query: &str) -> Result<Phase1Expr, QueryError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(QueryError::ParseError);
    }

    // Split on OR iteratively (lowest precedence)
    let or_parts: Vec<&str> = trimmed.split(" OR ").collect();
    if or_parts.len() > 1 {
        let children: Result<Vec<_>, _> = or_parts.into_iter().map(parse_and_expr).collect();
        return Ok(Phase1Expr::Or(children?));
    }

    parse_and_expr(trimmed)
}

fn parse_and_expr(query: &str) -> Result<Phase1Expr, QueryError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(QueryError::ParseError);
    }

    // Split on AND iteratively
    let and_parts: Vec<&str> = trimmed.split(" AND ").collect();
    if and_parts.len() > 1 {
        let children: Result<Vec<_>, _> = and_parts.into_iter().map(parse_unary_expr).collect();
        return Ok(Phase1Expr::And(children?));
    }

    parse_unary_expr(trimmed)
}

fn parse_unary_expr(query: &str) -> Result<Phase1Expr, QueryError> {
    let mut trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(QueryError::ParseError);
    }

    // Count NOT prefixes iteratively to avoid unbounded stack recursion.
    let mut not_count = 0_u32;
    while let Some(rest) = trimmed.strip_prefix("NOT ") {
        not_count += 1;
        trimmed = rest.trim();
        if trimmed.is_empty() {
            return Err(QueryError::ParseError);
        }
    }

    let mut expr = parse_term_expr(trimmed)?;
    for _ in 0..not_count {
        expr = Phase1Expr::Not(Box::new(expr));
    }
    Ok(expr)
}

fn parse_term_expr(query: &str) -> Result<Phase1Expr, QueryError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(QueryError::ParseError);
    }

    let (field, term) = if let Some((field, term)) = trimmed.split_once(':') {
        (
            Some(alloc::string::String::from(field)),
            alloc::string::String::from(term),
        )
    } else {
        (None, alloc::string::String::from(trimmed))
    };

    if term.is_empty() {
        return Err(QueryError::ParseError);
    }

    let tokens: Vec<_> = term.split_whitespace().collect();
    if tokens.len() > 1 {
        if field.is_some() {
            return Err(QueryError::ParseError);
        }
        return Ok(Phase1Expr::And(
            tokens
                .into_iter()
                .map(|token| Phase1Expr::Term {
                    field: field.clone(),
                    term: alloc::string::String::from(token),
                    boost: 1.0,
                })
                .collect(),
        ));
    }

    Ok(Phase1Expr::Term {
        field,
        term,
        boost: 1.0,
    })
}

fn lower_phase1_expr(
    expr: &Phase1Expr,
    context: &PlanningContext<'_>,
    nodes: &mut Vec<QueryNode>,
    max_nodes: usize,
) -> Result<QueryNodeId, QueryError> {
    if nodes.len() >= max_nodes {
        return Err(QueryError::MaxNodesExceeded {
            max_nodes,
            actual_nodes: checked_len_plus_one(nodes.len()),
        });
    }

    let node = match expr {
        Phase1Expr::Term { field, term, boost } => {
            if let Some(field_name) = field {
                // Explicit field: resolve directly
                let field_id = context.fields.resolve_field(field_name).ok_or_else(|| {
                    QueryError::UnknownField {
                        field: field_name.clone(),
                    }
                })?;
                match context.dictionary.resolve_term(field_id, term) {
                    Some(term_id) => QueryNode::Term {
                        field: field_id,
                        term: term_id,
                        boost: *boost * context.default_boost,
                    },
                    None => EMPTY_NODE,
                }
            } else if context.default_fields.len() == 1 {
                // Single default field
                let field_id = context.default_fields[0];
                match context.dictionary.resolve_term(field_id, term) {
                    Some(term_id) => QueryNode::Term {
                        field: field_id,
                        term: term_id,
                        boost: *boost * context.default_boost,
                    },
                    None => EMPTY_NODE,
                }
            } else if context.default_fields.is_empty() {
                return Err(QueryError::ParseError);
            } else {
                // Multiple default fields: expand to OR
                let mut child_ids = Vec::new();
                for &field_id in &context.default_fields {
                    if nodes.len() >= max_nodes {
                        return Err(QueryError::MaxNodesExceeded {
                            max_nodes,
                            actual_nodes: checked_len_plus_one(nodes.len()),
                        });
                    }
                    if let Some(term_id) = context.dictionary.resolve_term(field_id, term) {
                        let child_node = QueryNode::Term {
                            field: field_id,
                            term: term_id,
                            boost: *boost * context.default_boost,
                        };
                        let child_id = query_node_id(nodes.len());
                        nodes.push(child_node);
                        child_ids.push(child_id);
                    }
                }
                match child_ids.len() {
                    0 => EMPTY_NODE,
                    _ => QueryNode::TermExpansion {
                        children: child_ids,
                        fields: context.default_fields.clone(),
                        boost: 1.0,
                        field_weights: context.field_weights.clone(),
                    },
                }
            }
        }
        Phase1Expr::And(children) => {
            let mut child_ids = Vec::with_capacity(children.len());
            for child in children {
                child_ids.push(lower_phase1_expr(child, context, nodes, max_nodes)?);
            }
            QueryNode::And {
                children: child_ids,
                boost: 1.0,
            }
        }
        Phase1Expr::Or(children) => {
            let mut child_ids = Vec::with_capacity(children.len());
            for child in children {
                child_ids.push(lower_phase1_expr(child, context, nodes, max_nodes)?);
            }
            QueryNode::Or {
                children: child_ids,
                boost: 1.0,
            }
        }
        Phase1Expr::Not(child) => QueryNode::Not {
            child: lower_phase1_expr(child, context, nodes, max_nodes)?,
        },
    };

    let id = query_node_id(nodes.len());
    nodes.push(node);
    Ok(id)
}
