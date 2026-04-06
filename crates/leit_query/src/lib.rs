// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

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

mod builder;
mod planner;
mod types;

pub use types::{
    BooleanOp, BooleanView, BoostView, ExecutionPlan, ExtractionError, FeatureSet, FieldRegistry,
    FilterPredicate, FilterValue, PhraseView, PlannerScratch, PlanningContext, QueryError,
    QueryNode, QueryProgram, TermDictionary, TermView, UserQueryNode, UserQueryProgram,
};

pub use builder::{QueryBuilder, phrase, phrase_with_slop, term, term_with_field};

pub use planner::Planner;

#[cfg(test)]
fn assert_f32_eq(actual: f32, expected: f32) {
    let delta = (actual - expected).abs();
    assert!(delta <= f32::EPSILON, "expected {expected}, got {actual}");
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use leit_core::QueryNodeId;

    use super::*;

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
        use crate::types::{QueryArena, UserQueryNode};
        let mut arena = QueryArena::default();
        let root = arena.push(UserQueryNode::Boolean {
            op: BooleanOp::Or,
            children: vec![QueryNodeId::new(7)],
        });
        let program = UserQueryProgram::new(arena, root);

        let nodes: Vec<QueryNodeId> = program.walk().collect();

        assert_eq!(nodes, vec![root]);
    }
}
