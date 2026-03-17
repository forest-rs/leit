// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Property-based invariant tests for `leit_query`.

use leit_core::{FieldId, QueryNodeId, TermId};
use leit_query::{QueryBuilder, QueryError, QueryNode, QueryProgram};
use proptest::collection::vec;
use proptest::prelude::*;

fn term_strategy() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-z]{1,8}").expect("term regex should be valid")
}

proptest! {
    #[test]
    fn valid_builder_programs_only_walk_existing_unique_nodes(
        terms in vec(term_strategy(), 1..8),
        op in 0_u8..4,
        boost in 0.25_f32..4.0_f32,
    ) {
        let mut builder = QueryBuilder::new();
        let ids: Vec<_> = terms.iter().map(|term| builder.term(term.as_str())).collect();

        if ids.len() > 1 {
            match op % 4 {
                0 => {
                    builder.and(ids);
                }
                1 => {
                    builder.or(ids);
                }
                2 => {
                    let root = builder.and(ids);
                    builder.boost(root, boost);
                }
                _ => {
                    builder.not(ids[0]);
                }
            }
        }

        let program = builder.build().expect("generated builder program should be valid");
        let walked: Vec<_> = program.walk().collect();
        let node_count = u32::try_from(program.node_count())
            .expect("query program should fit within u32 node IDs");

        prop_assert!(!walked.is_empty());
        prop_assert!(walked.iter().all(|id| id.as_u32() < node_count));

        let mut unique = walked.clone();
        unique.sort_by_key(|id| id.as_u32());
        unique.dedup();
        prop_assert_eq!(unique.len(), walked.len());
    }

    #[test]
    fn invalid_roots_are_rejected(term in term_strategy(), invalid_root in 1_u32..1_000_u32) {
        let mut builder = QueryBuilder::new();
        builder.term(term.as_str());
        builder.set_root(QueryNodeId::new(invalid_root));

        prop_assert!(builder.build().is_none());
    }

    #[test]
    fn planned_program_rejects_invalid_references(invalid_child in 2_u32..1_000_u32) {
        let nodes = vec![
            QueryNode::Term {
                field: FieldId::new(0),
                term: TermId::new(0),
                boost: 1.0,
            },
            QueryNode::And {
                children: vec![QueryNodeId::new(invalid_child)],
                boost: 1.0,
            },
        ];

        let error = QueryProgram::try_new(nodes, QueryNodeId::new(1), 2)
            .expect_err("invalid child references must fail");

        let is_invalid_reference = matches!(error, QueryError::InvalidProgramReference { .. });
        prop_assert!(is_invalid_reference);
    }
}

#[test]
fn planned_program_rejects_self_cycles() {
    let nodes = vec![QueryNode::And {
        children: vec![QueryNodeId::new(0)],
        boost: 1.0,
    }];

    let error = QueryProgram::try_new(nodes, QueryNodeId::new(0), 1)
        .expect_err("self-referential planned programs must fail");

    assert!(matches!(error, QueryError::InvalidProgramCycle { .. }));
}

#[test]
fn planned_program_rejects_mutual_cycles() {
    let nodes = vec![
        QueryNode::Or {
            children: vec![QueryNodeId::new(1)],
            boost: 1.0,
        },
        QueryNode::And {
            children: vec![QueryNodeId::new(0)],
            boost: 1.0,
        },
    ];

    let error = QueryProgram::try_new(nodes, QueryNodeId::new(0), 2)
        .expect_err("cyclic planned programs must fail");

    assert!(matches!(error, QueryError::InvalidProgramCycle { .. }));
}

#[test]
fn planned_program_rejects_unreachable_nodes() {
    let nodes = vec![
        QueryNode::Term {
            field: FieldId::new(0),
            term: TermId::new(0),
            boost: 1.0,
        },
        QueryNode::Term {
            field: FieldId::new(0),
            term: TermId::new(1),
            boost: 1.0,
        },
    ];

    let error = QueryProgram::try_new(nodes, QueryNodeId::new(0), 1)
        .expect_err("unreachable planned nodes must fail");

    assert!(matches!(error, QueryError::UnreachableProgramNode { .. }));
}
