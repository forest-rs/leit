//! Phase 1 planner contract tests.

use leit_core::{FieldId, QueryNodeId, TermId};
use leit_query::{
    FeatureSet, FieldRegistry, Planner, PlannerScratch, PlanningContext, QueryError, QueryNode,
    QueryProgram, TermDictionary,
};

#[derive(Debug, Default)]
struct TestFieldRegistry;

impl FieldRegistry for TestFieldRegistry {
    fn resolve_field(&self, field: &str) -> Option<FieldId> {
        match field {
            "title" => Some(Self::title()),
            "body" => Some(Self::body()),
            _ => None,
        }
    }
}

impl TestFieldRegistry {
    const fn title() -> FieldId {
        FieldId::new(1)
    }

    const fn body() -> FieldId {
        FieldId::new(2)
    }
}

#[derive(Debug, Default)]
struct TestDictionary;

impl TermDictionary for TestDictionary {
    fn resolve_term(&self, field: FieldId, term: &str) -> Option<TermId> {
        match (field, term) {
            (field, "rust") if field == FieldId::new(1) || field == FieldId::new(2) => {
                Some(Self::rust())
            }
            (field, "memory") if field == FieldId::new(2) => Some(Self::memory()),
            _ => None,
        }
    }
}

impl TestDictionary {
    const fn rust() -> TermId {
        TermId::new(10)
    }

    const fn memory() -> TermId {
        TermId::new(20)
    }
}

fn planner_context<'a>(
    dictionary: &'a TestDictionary,
    fields: &'a TestFieldRegistry,
) -> PlanningContext<'a> {
    PlanningContext::new(dictionary, fields)
        .with_default_field(TestFieldRegistry::body())
        .with_default_boost(1.0)
}

fn assert_f32_eq(actual: f32, expected: f32) {
    let delta = (actual - expected).abs();
    assert!(delta <= f32::EPSILON, "expected {expected}, got {actual}");
}

#[test]
fn test_planner_builds_default_field_term_program() {
    let planner = Planner::new();
    let dictionary = TestDictionary;
    let fields = TestFieldRegistry;
    let mut scratch = PlannerScratch::new();
    let context = planner_context(&dictionary, &fields);

    let plan = planner
        .plan("rust", &context, &mut scratch)
        .expect("plan term");

    assert_eq!(plan.program.root(), QueryNodeId::new(0));
    assert_eq!(plan.program.node_count(), 1);
    assert_eq!(plan.program.max_depth(), 1);
    assert_eq!(plan.required_features, FeatureSet::basic());
    assert_eq!(plan.cost, 1);
    assert!(plan.selectivity >= 0.0);

    match plan.program.get(plan.program.root()) {
        Some(QueryNode::Term { field, term, boost }) => {
            assert_eq!(*field, TestFieldRegistry::body());
            assert_eq!(*term, TestDictionary::rust());
            assert_f32_eq(*boost, 1.0);
        }
        other => panic!("expected term node, got {other:?}"),
    }
}

#[test]
fn test_execution_query_program_uses_canonical_term_handles() {
    let planner = Planner::new();
    let dictionary = TestDictionary;
    let fields = TestFieldRegistry;
    let mut scratch = PlannerScratch::new();
    let context = planner_context(&dictionary, &fields);

    let default_plan = planner
        .plan("rust", &context, &mut scratch)
        .expect("plan bare term");
    let default_term = match default_plan.program.get(default_plan.program.root()) {
        Some(QueryNode::Term { term, .. }) => *term,
        other => panic!("expected canonical term node, got {other:?}"),
    };

    scratch.reset();
    let explicit_plan = planner
        .plan("body:rust", &context, &mut scratch)
        .expect("plan explicit field term");
    let explicit_term = match explicit_plan.program.get(explicit_plan.program.root()) {
        Some(QueryNode::Term { term, .. }) => *term,
        other => panic!("expected canonical term node, got {other:?}"),
    };

    assert_eq!(default_term, TestDictionary::rust());
    assert_eq!(explicit_term, TestDictionary::rust());
    assert_eq!(default_term, explicit_term);
}

#[test]
fn test_planner_builds_field_qualified_boolean_program() {
    let planner = Planner::new();
    let dictionary = TestDictionary;
    let fields = TestFieldRegistry;
    let mut scratch = PlannerScratch::new();
    let context = planner_context(&dictionary, &fields);

    let plan = planner
        .plan("title:rust AND body:memory", &context, &mut scratch)
        .expect("plan boolean query");

    assert_eq!(plan.program.node_count(), 3);
    assert_eq!(plan.program.max_depth(), 2);

    match plan.program.get(plan.program.root()) {
        Some(QueryNode::And { children, boost }) => {
            assert_f32_eq(*boost, 1.0);
            assert_eq!(children.len(), 2);
        }
        other => panic!("expected and node, got {other:?}"),
    }
}

#[test]
fn test_planner_reports_unknown_field() {
    let planner = Planner::new();
    let dictionary = TestDictionary;
    let fields = TestFieldRegistry;
    let mut scratch = PlannerScratch::new();
    let context = planner_context(&dictionary, &fields);

    let error = planner
        .plan("summary:rust", &context, &mut scratch)
        .expect_err("unknown fields should fail");

    assert!(matches!(error, QueryError::UnknownField { .. }));
}

#[test]
fn test_planner_reports_unknown_term() {
    let planner = Planner::new();
    let dictionary = TestDictionary;
    let fields = TestFieldRegistry;
    let mut scratch = PlannerScratch::new();
    let context = planner_context(&dictionary, &fields);

    let error = planner
        .plan("body:unknown", &context, &mut scratch)
        .expect_err("unknown terms should fail");

    assert!(matches!(error, QueryError::UnknownTerm { .. }));
}

#[test]
fn test_planner_respects_depth_limit() {
    let planner = Planner::new().with_max_depth(1);
    let dictionary = TestDictionary;
    let fields = TestFieldRegistry;
    let mut scratch = PlannerScratch::new();
    let context = planner_context(&dictionary, &fields);

    let error = planner
        .plan("rust AND memory", &context, &mut scratch)
        .expect_err("depth limit should fail");

    assert!(matches!(error, QueryError::MaxDepthExceeded { .. }));
}

#[test]
fn test_planner_parses_and_tighter_than_or() {
    let planner = Planner::new();
    let dictionary = TestDictionary;
    let fields = TestFieldRegistry;
    let mut scratch = PlannerScratch::new();
    let context = planner_context(&dictionary, &fields);

    let plan = planner
        .plan("title:rust OR body:memory AND rust", &context, &mut scratch)
        .expect("mixed boolean query should plan");

    match plan.program.get(plan.program.root()) {
        Some(QueryNode::Or { children, .. }) => {
            assert_eq!(children.len(), 2);

            match plan.program.get(children[1]) {
                Some(QueryNode::And { children, .. }) => {
                    assert_eq!(children.len(), 2);
                }
                other => panic!("expected AND as OR rhs, got {other:?}"),
            }
        }
        other => panic!("expected OR root for mixed precedence, got {other:?}"),
    }
}

#[test]
fn test_planner_lowers_bare_multi_token_terms_to_and() {
    let planner = Planner::new();
    let dictionary = TestDictionary;
    let fields = TestFieldRegistry;
    let mut scratch = PlannerScratch::new();
    let context = planner_context(&dictionary, &fields);

    let plan = planner
        .plan("rust memory", &context, &mut scratch)
        .expect("whitespace-separated terms should plan");

    match plan.program.get(plan.program.root()) {
        Some(QueryNode::And { children, boost }) => {
            assert_f32_eq(*boost, 1.0);
            assert_eq!(children.len(), 2);
            for child in children {
                assert!(matches!(
                    plan.program.get(*child),
                    Some(QueryNode::Term { .. })
                ));
            }
        }
        other => panic!("expected AND root for implicit conjunction, got {other:?}"),
    }
}

#[test]
fn test_planner_rejects_field_qualified_multi_token_terms_without_grouping() {
    let planner = Planner::new();
    let dictionary = TestDictionary;
    let fields = TestFieldRegistry;
    let mut scratch = PlannerScratch::new();
    let context = planner_context(&dictionary, &fields);

    let error = planner
        .plan("title:rust memory", &context, &mut scratch)
        .expect_err("field-qualified multi-token terms should require grouping");

    assert!(matches!(error, QueryError::ParseError));
}

#[test]
#[should_panic(expected = "planned query program contains invalid references")]
fn test_planned_query_program_rejects_invalid_child_id() {
    let _ = QueryProgram::new(
        vec![QueryNode::Or {
            children: vec![QueryNodeId::new(1)],
            boost: 1.0,
        }],
        QueryNodeId::new(0),
        1,
    );
}
