// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::boxed::Box;
use alloc::vec::Vec;

use leit_core::QueryNodeId;

use crate::builder::query_node_id;
use crate::types::{
    BooleanOp, ExecutionPlan, FeatureSet, PlannerScratch, PlanningContext, QueryError, QueryNode,
    QueryProgram, UserQueryNode, UserQueryProgram,
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

    /// Plan a typed [`UserQueryProgram`] into an execution-facing plan.
    ///
    /// Mirrors the lowering semantics of [`plan`](Self::plan) for the typed
    /// builder AST: identical field resolution, default-field term expansion,
    /// and the same `max_depth` / `max_nodes` guards. [`UserQueryNode::Boost`]
    /// nodes fold multiplicatively into descendant term boosts (matching how
    /// the textual path composes `boost * default_boost`) and add no plan
    /// node. [`UserQueryNode::Phrase`] lowers to the conjunction of its terms;
    /// Phase 1 execution has no positional data, so phrase slop is not
    /// enforced yet.
    ///
    /// Known Phase 1 phrase weakness: each phrase term expands independently
    /// over the default fields before the AND, so a phrase like `"A B"` can
    /// match a document where `A` appears only in one default field and `B`
    /// only in another. When positional data lands, phrase lowering must OR
    /// field-specific phrase nodes (one positional phrase per field) rather
    /// than merely swapping the AND node for a single phrase node.
    pub fn plan_program(
        &self,
        program: &UserQueryProgram,
        context: &PlanningContext<'_>,
        scratch: &mut PlannerScratch,
    ) -> Result<ExecutionPlan, QueryError> {
        scratch.reset();
        // Depth traversal enforces `max_depth` (and validates boost factors)
        // before lowering, so lowering recursion depth is bounded by
        // `max_depth` (Boost chains are folded iteratively).
        let depth = user_program_depth(program, program.root(), self.max_depth)?;

        let mut nodes = Vec::new();
        let root = lower_user_node(
            program,
            program.root(),
            context,
            &mut nodes,
            self.max_nodes,
            1.0,
        )?;
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
            lower_term_node(field.as_deref(), term, *boost, context, nodes, max_nodes)?
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

/// Lower a single (possibly fielded) term into an execution node.
///
/// Shared by the textual and typed planning paths so both compose boosts,
/// resolve fields, and expand default fields identically. May push
/// [`QueryNode::TermExpansion`] children onto `nodes`.
fn lower_term_node(
    field: Option<&str>,
    term: &str,
    boost: f32,
    context: &PlanningContext<'_>,
    nodes: &mut Vec<QueryNode>,
    max_nodes: usize,
) -> Result<QueryNode, QueryError> {
    if let Some(field_name) = field {
        // Explicit field: resolve directly
        let field_id =
            context
                .fields
                .resolve_field(field_name)
                .ok_or_else(|| QueryError::UnknownField {
                    field: alloc::string::String::from(field_name),
                })?;
        Ok(match context.dictionary.resolve_term(field_id, term) {
            Some(term_id) => QueryNode::Term {
                field: field_id,
                term: term_id,
                boost: boost * context.default_boost,
            },
            None => EMPTY_NODE,
        })
    } else if context.default_fields.len() == 1 {
        // Single default field
        let field_id = context.default_fields[0];
        Ok(match context.dictionary.resolve_term(field_id, term) {
            Some(term_id) => QueryNode::Term {
                field: field_id,
                term: term_id,
                boost: boost * context.default_boost,
            },
            None => EMPTY_NODE,
        })
    } else if context.default_fields.is_empty() {
        Err(QueryError::ParseError)
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
                    boost: boost * context.default_boost,
                };
                let child_id = query_node_id(nodes.len());
                nodes.push(child_node);
                child_ids.push(child_id);
            }
        }
        Ok(match child_ids.len() {
            0 => EMPTY_NODE,
            _ => QueryNode::TermExpansion {
                children: child_ids,
                fields: context.default_fields.clone(),
                boost: 1.0,
                field_weights: context.field_weights.clone(),
            },
        })
    }
}

/// Compute the lowered depth of a typed program from `root`.
///
/// Iterative tri-color depth-first traversal with per-node depth
/// memoization: shared DAG nodes are computed once, so diamond-shaped
/// sharing costs linear work in nodes plus edges instead of exponential
/// re-traversal. `max_depth` is enforced as soon as any node's computed
/// depth exceeds it, so overlong chains are rejected mid-traversal instead
/// of after a full (and possibly stack-overflowing) recursive walk. Arena
/// reference cycles (possible via hand-fed node identifiers) are rejected
/// when the traversal re-enters an in-progress (gray) node.
///
/// Boost factors are also validated here, at the `plan_program` boundary:
/// a factor that is not finite and non-negative yields
/// [`QueryError::InvalidBoost`].
fn user_program_depth(
    program: &UserQueryProgram,
    root: QueryNodeId,
    max_depth: usize,
) -> Result<usize, QueryError> {
    const WHITE: u8 = 0;
    const GRAY: u8 = 1;
    const BLACK: u8 = 2;

    enum Frame {
        Enter(QueryNodeId),
        Exit(QueryNodeId),
    }

    let node_count = program.node_count();
    let index_of = |id: QueryNodeId| -> Result<usize, QueryError> {
        let idx = id.as_u32() as usize;
        if idx < node_count {
            Ok(idx)
        } else {
            Err(QueryError::ParseError)
        }
    };

    let mut color = alloc::vec![WHITE; node_count];
    let mut depths = alloc::vec![0_usize; node_count];
    let mut stack = alloc::vec![Frame::Enter(root)];

    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Enter(id) => {
                let idx = index_of(id)?;
                match color[idx] {
                    BLACK => continue, // memoized: shared node already done
                    GRAY => return Err(QueryError::InvalidProgramCycle { node: id }),
                    _ => {}
                }
                color[idx] = GRAY;
                stack.push(Frame::Exit(id));
                match program.get(id).ok_or(QueryError::ParseError)? {
                    UserQueryNode::Term { .. } | UserQueryNode::Phrase { .. } => {}
                    UserQueryNode::Boolean { children, .. } => {
                        for child in children {
                            stack.push(Frame::Enter(*child));
                        }
                    }
                    UserQueryNode::Boost { child, factor } => {
                        if !factor.is_finite() || *factor < 0.0 {
                            return Err(QueryError::InvalidBoost { node: id });
                        }
                        stack.push(Frame::Enter(*child));
                    }
                }
            }
            Frame::Exit(id) => {
                let idx = index_of(id)?;
                let depth = match program.get(id).ok_or(QueryError::ParseError)? {
                    UserQueryNode::Term { .. } => 1,
                    // Multi-term phrases lower to AND-of-terms: two levels.
                    UserQueryNode::Phrase { terms, .. } => {
                        if terms.len() > 1 {
                            2
                        } else {
                            1
                        }
                    }
                    UserQueryNode::Boolean { children, .. } => {
                        let mut max_child = 0_usize;
                        for child in children {
                            max_child = max_child.max(depths[index_of(*child)?]);
                        }
                        max_child.checked_add(1).expect("query depth overflow")
                    }
                    // Boost folds into descendant boosts and adds no plan node.
                    UserQueryNode::Boost { child, .. } => depths[index_of(*child)?],
                };
                if depth > max_depth {
                    return Err(QueryError::MaxDepthExceeded {
                        max_depth,
                        actual_depth: depth,
                    });
                }
                depths[idx] = depth;
                color[idx] = BLACK;
            }
        }
    }

    Ok(depths[index_of(root)?])
}

/// Lower a typed query node into the execution node arena.
///
/// `boost` is the multiplicative product of enclosing
/// [`UserQueryNode::Boost`] factors, applied at term nodes exactly like the
/// textual path applies per-term boosts.
fn lower_user_node(
    program: &UserQueryProgram,
    id: QueryNodeId,
    context: &PlanningContext<'_>,
    nodes: &mut Vec<QueryNode>,
    max_nodes: usize,
    boost: f32,
) -> Result<QueryNodeId, QueryError> {
    if nodes.len() >= max_nodes {
        return Err(QueryError::MaxNodesExceeded {
            max_nodes,
            actual_nodes: checked_len_plus_one(nodes.len()),
        });
    }

    // Fold Boost chains iteratively (they add no plan node), validating the
    // composed product at every step: individually finite factors can still
    // compose to infinity.
    let mut id = id;
    let mut boost = boost;
    let user = loop {
        let user = program.get(id).ok_or(QueryError::ParseError)?;
        let UserQueryNode::Boost { child, factor } = user else {
            break user;
        };
        let composed = boost * factor;
        if !composed.is_finite() || composed < 0.0 {
            return Err(QueryError::InvalidBoost { node: id });
        }
        boost = composed;
        id = *child;
    };
    let node = match user {
        UserQueryNode::Term { term, field } => {
            lower_term_node(field.as_deref(), term, boost, context, nodes, max_nodes)?
        }
        UserQueryNode::Phrase { terms, slop: _ } => match terms.len() {
            0 => EMPTY_NODE,
            1 => lower_term_node(None, &terms[0], boost, context, nodes, max_nodes)?,
            _ => {
                // No positional execution in Phase 1: lower to conjunction.
                // Cross-field caveat: each term expands over the default
                // fields independently, so the AND can be satisfied across
                // *different* fields. Positional phrase support must replace
                // this with an OR of per-field phrase nodes, not just swap
                // the AND for a phrase node.
                let mut child_ids = Vec::with_capacity(terms.len());
                for term in terms {
                    if nodes.len() >= max_nodes {
                        return Err(QueryError::MaxNodesExceeded {
                            max_nodes,
                            actual_nodes: checked_len_plus_one(nodes.len()),
                        });
                    }
                    let child = lower_term_node(None, term, boost, context, nodes, max_nodes)?;
                    let child_id = query_node_id(nodes.len());
                    nodes.push(child);
                    child_ids.push(child_id);
                }
                QueryNode::And {
                    children: child_ids,
                    boost: 1.0,
                }
            }
        },
        UserQueryNode::Boolean { op, children } => match op {
            BooleanOp::And | BooleanOp::Or => {
                let mut child_ids = Vec::with_capacity(children.len());
                for child in children {
                    child_ids.push(lower_user_node(
                        program, *child, context, nodes, max_nodes, boost,
                    )?);
                }
                if *op == BooleanOp::And {
                    QueryNode::And {
                        children: child_ids,
                        boost: 1.0,
                    }
                } else {
                    QueryNode::Or {
                        children: child_ids,
                        boost: 1.0,
                    }
                }
            }
            BooleanOp::Not => {
                let [child] = children.as_slice() else {
                    return Err(QueryError::ParseError);
                };
                QueryNode::Not {
                    child: lower_user_node(program, *child, context, nodes, max_nodes, boost)?,
                }
            }
        },
        UserQueryNode::Boost { .. } => unreachable!("boost chains folded above"),
    };

    let id = query_node_id(nodes.len());
    nodes.push(node);
    Ok(id)
}
