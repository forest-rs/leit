#![no_std]

//! Query types and builders for the Leit search library.
//!
//! This crate provides:
//! - Query node types (term, phrase, boolean, boost)
//! - Arena-based query program storage
//! - Fluent construction DSL
//! - View types for query inspection

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use leit_core::QueryNodeId;

mod types;

use types::QueryArena;
pub use types::{
    BooleanOp, BooleanView, BoostView, ExecutionPlan, ExtractionError, FeatureSet, FieldRegistry,
    PhraseView, PlannedQueryNode, PlannedQueryProgram, PlannerScratch, PlanningContext, QueryError,
    QueryNode, QueryProgram, TermDictionary, TermView,
};

// ============================================================================
// Construction DSL
// ============================================================================

/// Builder for constructing query programs.
#[derive(Debug, Default)]
pub struct QueryBuilder {
    arena: QueryArena,
    root: Option<QueryNodeId>,
}

fn query_node_id(index: usize) -> QueryNodeId {
    QueryNodeId::new(u32::try_from(index).expect("query program exceeded u32 node IDs"))
}

const fn checked_len_plus_one(len: usize) -> usize {
    len.checked_add(1).expect("query node count overflow")
}

impl QueryBuilder {
    /// Create a new query builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the query program from the tracked root node.
    pub fn build(self) -> Option<QueryProgram> {
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
        QueryProgram::is_valid(&self.arena, root).then(|| QueryProgram::new(self.arena, root))
    }

    /// Set the root node for the query program.
    pub const fn set_root(&mut self, id: QueryNodeId) {
        self.root = Some(id);
    }

    /// Add a term query.
    pub fn term<S: Into<Arc<str>>>(&mut self, term: S) -> QueryNodeId {
        let id = self.arena.push(QueryNode::Term {
            term: term.into(),
            field: None,
        });
        self.root = Some(id);
        id
    }

    /// Add a term query with a field.
    pub fn term_with_field<S: Into<Arc<str>>>(&mut self, term: S, field: S) -> QueryNodeId {
        let id = self.arena.push(QueryNode::Term {
            term: term.into(),
            field: Some(field.into()),
        });
        self.root = Some(id);
        id
    }

    /// Add a phrase query with initial terms.
    pub fn phrase(&mut self, terms: Vec<Arc<str>>) -> QueryNodeId {
        let id = self.arena.push(QueryNode::Phrase { terms, slop: 0 });
        self.root = Some(id);
        id
    }

    /// Add a phrase query with terms and slop.
    pub fn phrase_with_slop(&mut self, terms: Vec<Arc<str>>, slop: u32) -> QueryNodeId {
        let id = self.arena.push(QueryNode::Phrase { terms, slop });
        self.root = Some(id);
        id
    }

    /// Add a boolean AND query with initial children.
    pub fn and(&mut self, children: Vec<QueryNodeId>) -> QueryNodeId {
        let id = self.arena.push(QueryNode::Boolean {
            op: BooleanOp::And,
            children,
        });
        self.root = Some(id);
        id
    }

    /// Add a boolean OR query with initial children.
    pub fn or(&mut self, children: Vec<QueryNodeId>) -> QueryNodeId {
        let id = self.arena.push(QueryNode::Boolean {
            op: BooleanOp::Or,
            children,
        });
        self.root = Some(id);
        id
    }

    /// Add a boolean NOT query with child.
    pub fn not(&mut self, child: QueryNodeId) -> QueryNodeId {
        let id = self.arena.push(QueryNode::Boolean {
            op: BooleanOp::Not,
            children: vec![child],
        });
        self.root = Some(id);
        id
    }

    /// Add a boost query.
    pub fn boost(&mut self, child: QueryNodeId, factor: f32) -> QueryNodeId {
        let id = self.arena.push(QueryNode::Boost { child, factor });
        self.root = Some(id);
        id
    }
}

// ============================================================================
// Fluent Functions
// ============================================================================

/// Create a term query in one call.
pub fn term<S: Into<Arc<str>>>(term: S) -> QueryProgram {
    let mut builder = QueryBuilder::new();
    builder.term(term);
    builder.build().expect("term should create valid program")
}

/// Create a term query with a field in one call.
pub fn term_with_field<S: Into<Arc<str>>>(term: S, field: S) -> QueryProgram {
    let mut builder = QueryBuilder::new();
    builder.term_with_field(term, field);
    builder
        .build()
        .expect("term_with_field should create valid program")
}

/// Create a phrase query in one call.
pub fn phrase(terms: &[&str]) -> QueryProgram {
    let mut builder = QueryBuilder::new();
    let terms: Vec<Arc<str>> = terms.iter().map(|t| (*t).into()).collect();
    builder.phrase(terms);
    builder.build().expect("phrase should create valid program")
}

/// Create a phrase query with slop in one call.
pub fn phrase_with_slop(terms: &[&str], slop: u32) -> QueryProgram {
    let mut builder = QueryBuilder::new();
    let terms: Vec<Arc<str>> = terms.iter().map(|t| (*t).into()).collect();
    builder.phrase_with_slop(terms, slop);
    builder
        .build()
        .expect("phrase_with_slop should create valid program")
}

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
    #[must_use]
    pub const fn with_max_nodes(mut self, count: usize) -> Self {
        self.max_nodes = count;
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
            program: PlannedQueryProgram::try_new(nodes, root, depth)?,
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

#[cfg(test)]
fn assert_f32_eq(actual: f32, expected: f32) {
    let delta = (actual - expected).abs();
    assert!(delta <= f32::EPSILON, "expected {expected}, got {actual}");
}

#[derive(Clone, Debug, PartialEq)]
enum Phase1Expr {
    Term {
        field: Option<alloc::string::String>,
        term: alloc::string::String,
        boost: f32,
    },
    And(Vec<Phase1Expr>),
    Or(Vec<Phase1Expr>),
    Not(Box<Phase1Expr>),
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

    if let Some((lhs, rhs)) = trimmed.split_once(" OR ") {
        return Ok(Phase1Expr::Or(vec![
            parse_phase1_query(lhs)?,
            parse_phase1_query(rhs)?,
        ]));
    }

    if let Some((lhs, rhs)) = trimmed.split_once(" AND ") {
        return Ok(Phase1Expr::And(vec![
            parse_phase1_query(lhs)?,
            parse_phase1_query(rhs)?,
        ]));
    }

    if let Some(rest) = trimmed.strip_prefix("NOT ") {
        return Ok(Phase1Expr::Not(Box::new(parse_phase1_query(rest)?)));
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
    nodes: &mut Vec<PlannedQueryNode>,
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
            let field_id = if let Some(field_name) = field {
                context.fields.resolve_field(field_name).ok_or_else(|| {
                    QueryError::UnknownField {
                        field: field_name.clone(),
                    }
                })?
            } else {
                context.default_field
            };

            let term_id = context
                .dictionary
                .resolve_term(field_id, term)
                .ok_or_else(|| QueryError::UnknownTerm {
                    field: field_id,
                    term: term.clone(),
                })?;

            PlannedQueryNode::Term {
                field: field_id,
                term: term_id,
                boost: *boost * context.default_boost,
            }
        }
        Phase1Expr::And(children) => {
            let mut child_ids = Vec::with_capacity(children.len());
            for child in children {
                child_ids.push(lower_phase1_expr(child, context, nodes, max_nodes)?);
            }
            PlannedQueryNode::And {
                children: child_ids,
                boost: 1.0,
            }
        }
        Phase1Expr::Or(children) => {
            let mut child_ids = Vec::with_capacity(children.len());
            for child in children {
                child_ids.push(lower_phase1_expr(child, context, nodes, max_nodes)?);
            }
            PlannedQueryNode::Or {
                children: child_ids,
                boost: 1.0,
            }
        }
        Phase1Expr::Not(child) => PlannedQueryNode::Not {
            child: lower_phase1_expr(child, context, nodes, max_nodes)?,
        },
    };

    let id = query_node_id(nodes.len());
    nodes.push(node);
    Ok(id)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // Construction Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_builder_term() {
        let mut builder = QueryBuilder::new();
        let _id = builder.term("test");
        let program = builder.build().expect("should build");

        assert_eq!(program.node_count(), 1);

        let view: TermView = (&program, program.root())
            .try_into()
            .expect("should be term");
        assert_eq!(view.term.as_ref(), "test");
        assert!(view.field.is_none());
    }

    #[test]
    fn test_builder_term_with_field() {
        let mut builder = QueryBuilder::new();
        let _id = builder.term_with_field("test", "title");
        let program = builder.build().expect("should build");

        assert_eq!(program.node_count(), 1);

        let view: TermView = (&program, program.root())
            .try_into()
            .expect("should be term");
        assert_eq!(view.term.as_ref(), "test");
        assert_eq!(view.field.as_deref(), Some("title"));
    }

    #[test]
    fn test_builder_phrase() {
        let mut builder = QueryBuilder::new();
        let terms = vec!["hello".into(), "world".into()];
        builder.phrase(terms);
        let program = builder.build().expect("should build");

        assert_eq!(program.node_count(), 1);

        let view: PhraseView = (&program, program.root())
            .try_into()
            .expect("should be phrase");
        assert_eq!(view.terms.len(), 2);
        assert_eq!(view.terms[0].as_ref(), "hello");
        assert_eq!(view.terms[1].as_ref(), "world");
        assert_eq!(view.slop, 0);
    }

    #[test]
    fn test_builder_phrase_with_slop() {
        let mut builder = QueryBuilder::new();
        let terms = vec!["quick".into(), "brown".into(), "fox".into()];
        builder.phrase_with_slop(terms, 2);
        let program = builder.build().expect("should build");

        assert_eq!(program.node_count(), 1);

        let view: PhraseView = (&program, program.root())
            .try_into()
            .expect("should be phrase");
        assert_eq!(view.terms.len(), 3);
        assert_eq!(view.slop, 2);
    }

    #[test]
    fn test_builder_and() {
        let mut builder = QueryBuilder::new();
        let t1 = builder.term("foo");
        let t2 = builder.term("bar");
        builder.and(vec![t1, t2]);
        let program = builder.build().expect("should build");

        assert_eq!(program.node_count(), 3);

        let view: BooleanView = (&program, program.root())
            .try_into()
            .expect("should be boolean");
        assert_eq!(view.op, BooleanOp::And);
        assert_eq!(view.children.len(), 2);
    }

    #[test]
    fn test_builder_or() {
        let mut builder = QueryBuilder::new();
        let t1 = builder.term("foo");
        let t2 = builder.term("bar");
        builder.or(vec![t1, t2]);
        let program = builder.build().expect("should build");

        assert_eq!(program.node_count(), 3);

        let view: BooleanView = (&program, program.root())
            .try_into()
            .expect("should be boolean");
        assert_eq!(view.op, BooleanOp::Or);
        assert_eq!(view.children.len(), 2);
    }

    #[test]
    fn test_builder_not() {
        let mut builder = QueryBuilder::new();
        let t1 = builder.term("unwanted");
        builder.not(t1);
        let program = builder.build().expect("should build");

        assert_eq!(program.node_count(), 2);

        let view: BooleanView = (&program, program.root())
            .try_into()
            .expect("should be boolean");
        assert_eq!(view.op, BooleanOp::Not);
        assert_eq!(view.children.len(), 1);
    }

    #[test]
    fn test_builder_boost() {
        let mut builder = QueryBuilder::new();
        let t1 = builder.term("important");
        builder.boost(t1, 2.0);
        let program = builder.build().expect("should build");

        assert_eq!(program.node_count(), 2);

        let view: BoostView = (&program, program.root())
            .try_into()
            .expect("should be boost");
        assert_f32_eq(view.factor, 2.0);
    }

    #[test]
    fn test_builder_complex_query() {
        let mut builder = QueryBuilder::new();

        // Build: (title:quick OR content:quick) AND (brown fox)
        let title_quick = builder.term_with_field("quick", "title");
        let content_quick = builder.term_with_field("quick", "content");
        let or_node = builder.or(vec![title_quick, content_quick]);

        let phrase = builder.phrase(vec!["brown".into(), "fox".into()]);

        builder.and(vec![or_node, phrase]);
        let program = builder.build().expect("should build");

        assert_eq!(program.node_count(), 5);

        let root = program.root();
        let root_view: BooleanView = (program, root).try_into().expect("should be boolean");
        assert_eq!(root_view.op, BooleanOp::And);
        assert_eq!(root_view.children.len(), 2);
    }

    // ------------------------------------------------------------------------
    // Fluent Function Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_fluent_term() {
        let program = term("hello");

        assert_eq!(program.node_count(), 1);

        let view: TermView = (&program, program.root())
            .try_into()
            .expect("should be term");
        assert_eq!(view.term.as_ref(), "hello");
    }

    #[test]
    fn test_fluent_term_with_field() {
        let program = term_with_field("search", "title");

        assert_eq!(program.node_count(), 1);

        let view: TermView = (&program, program.root())
            .try_into()
            .expect("should be term");
        assert_eq!(view.field.as_deref(), Some("title"));
    }

    #[test]
    fn test_fluent_phrase() {
        let terms = vec!["hello", "world"];
        let program = phrase(&terms);

        assert_eq!(program.node_count(), 1);

        let view: PhraseView = (&program, program.root())
            .try_into()
            .expect("should be phrase");
        assert_eq!(view.terms.len(), 2);
    }

    #[test]
    fn test_fluent_phrase_with_slop() {
        let terms = vec!["quick", "brown", "fox"];
        let program = phrase_with_slop(&terms, 2);

        assert_eq!(program.node_count(), 1);

        let view: PhraseView = (&program, program.root())
            .try_into()
            .expect("should be phrase");
        assert_eq!(view.terms.len(), 3);
        assert_eq!(view.slop, 2);
    }

    // ------------------------------------------------------------------------
    // Traversal Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_walk_single_node() {
        let program = term("singleton");

        let nodes: Vec<QueryNodeId> = program.walk().collect();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0], program.root());
    }

    #[test]
    fn test_walk_linear_chain() {
        let mut builder = QueryBuilder::new();
        let t1 = builder.term("a");
        let t2 = builder.term("b");
        let and1 = builder.and(vec![t1, t2]);
        let t3 = builder.term("c");
        let _root = builder.and(vec![and1, t3]);
        let program = builder.build().expect("should build");

        let nodes: Vec<QueryNodeId> = program.walk().collect();

        assert_eq!(nodes.len(), 5); // and_root, and1, t1, t2, t3
    }

    #[test]
    fn test_children_of_term() {
        let program = term("leaf");

        let children = program.children_of(program.root());
        assert_eq!(children.len(), 0);
    }

    #[test]
    fn test_children_of_boolean() {
        let mut builder = QueryBuilder::new();
        let t1 = builder.term("x");
        let t2 = builder.term("y");
        let _and_node = builder.and(vec![t1, t2]);
        let program = builder.build().expect("should build");

        let children = program.children_of(program.root());
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_children_of_boost() {
        let mut builder = QueryBuilder::new();
        let t1 = builder.term("child");
        let _boost_node = builder.boost(t1, 2.0);
        let program = builder.build().expect("should build");

        let children = program.children_of(program.root());
        assert_eq!(children.len(), 1);
    }

    // ------------------------------------------------------------------------
    // View Extraction Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_term_view_extraction() {
        let program = term_with_field("test", "field");

        let root = program.root();
        let view: TermView = (program, root)
            .try_into()
            .expect("should extract term view");

        assert_eq!(view.term.as_ref(), "test");
        assert_eq!(view.field.as_deref(), Some("field"));
    }

    #[test]
    fn test_phrase_view_extraction() {
        let mut builder = QueryBuilder::new();
        builder.phrase(vec!["a".into(), "b".into()]);
        let program = builder.build().expect("should build");

        let root = program.root();
        let view: PhraseView = (program, root)
            .try_into()
            .expect("should extract phrase view");

        assert_eq!(view.terms.len(), 2);
    }

    #[test]
    fn test_boolean_view_extraction() {
        let mut builder = QueryBuilder::new();
        let t1 = builder.term("x");
        let t2 = builder.term("y");
        builder.or(vec![t1, t2]);
        let program = builder.build().expect("should build");

        let root = program.root();
        let view: BooleanView = (program, root)
            .try_into()
            .expect("should extract boolean view");

        assert_eq!(view.op, BooleanOp::Or);
        assert_eq!(view.children.len(), 2);
    }

    #[test]
    fn test_boost_view_extraction() {
        let mut builder = QueryBuilder::new();
        let t1 = builder.term("boosted");
        builder.boost(t1, 3.0);
        let program = builder.build().expect("should build");

        let root = program.root();
        let view: BoostView = (program, root)
            .try_into()
            .expect("should extract boost view");

        assert_f32_eq(view.factor, 3.0);
    }

    #[test]
    fn test_wrong_node_type_error() {
        let program = term("test");

        let result: Result<PhraseView, _> = (&program, program.root()).try_into();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, ExtractionError::WrongNodeType { .. }));
    }

    #[test]
    fn test_invalid_node_id_error() {
        let program = term("test");
        let invalid_id = QueryNodeId::new(999);

        let result: Result<TermView, _> = (&program, invalid_id).try_into();
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, ExtractionError::InvalidNodeId(_)));
    }

    // ------------------------------------------------------------------------
    // Edge Cases
    // ------------------------------------------------------------------------

    #[test]
    fn test_empty_builder() {
        let builder = QueryBuilder::new();
        let program = builder.build();

        assert!(program.is_none());
    }

    #[test]
    fn test_empty_phrase() {
        let mut builder = QueryBuilder::new();
        builder.phrase(vec![]);
        let program = builder.build().expect("should build");

        let root = program.root();
        let view: PhraseView = (program, root)
            .try_into()
            .expect("should extract phrase view");

        assert_eq!(view.terms.len(), 0);
    }

    #[test]
    fn test_boolean_no_children() {
        let mut builder = QueryBuilder::new();
        builder.and(vec![]);
        let program = builder.build().expect("should build");

        let root = program.root();
        let view: BooleanView = (program, root)
            .try_into()
            .expect("should extract boolean view");

        assert_eq!(view.children.len(), 0);
    }

    #[test]
    fn test_deeply_nested_query() {
        let mut builder = QueryBuilder::new();

        // Build: AND(AND(AND(t1, t2), t3), t4)
        let t1 = builder.term("a");
        let t2 = builder.term("b");
        let and1 = builder.and(vec![t1, t2]);
        let t3 = builder.term("c");
        let and2 = builder.and(vec![and1, t3]);
        let t4 = builder.term("d");
        let _root = builder.and(vec![and2, t4]);

        let program = builder.build().expect("should build");

        assert_eq!(program.node_count(), 7);

        let nodes: Vec<_> = program.walk().collect();
        assert_eq!(nodes.len(), 7);
    }

    #[test]
    fn test_query_program_clone() {
        let mut builder = QueryBuilder::new();
        builder.term("test");
        let program1 = builder.build().expect("should build");
        let program2 = program1.clone();

        assert_eq!(program1.root(), program2.root());
        assert_eq!(program1.node_count(), program2.node_count());
    }

    #[test]
    fn test_builder_rejects_invalid_root_id() {
        let mut builder = QueryBuilder::new();
        builder.term("valid");
        builder.set_root(QueryNodeId::new(99));

        assert!(builder.build().is_none());
    }

    #[test]
    fn test_builder_rejects_invalid_child_id() {
        let mut builder = QueryBuilder::new();
        builder.term("valid");
        builder.and(vec![QueryNodeId::new(42)]);

        assert!(builder.build().is_none());
    }

    #[test]
    fn test_walk_skips_missing_child_nodes() {
        let mut arena = QueryArena::default();
        let root = arena.push(QueryNode::Boolean {
            op: BooleanOp::Or,
            children: vec![QueryNodeId::new(7)],
        });
        let program = QueryProgram::new(arena, root);

        let nodes: Vec<QueryNodeId> = program.walk().collect();

        assert_eq!(nodes, vec![root]);
    }
}
