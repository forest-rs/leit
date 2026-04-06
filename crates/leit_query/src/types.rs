// Copyright 2026 the Leit Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use core::convert::TryFrom;
use core::fmt;
use core::iter::Iterator;

use leit_core::QueryNodeId;

/// Internal arena storage for query nodes.
#[derive(Debug, Default)]
pub(crate) struct QueryArena {
    nodes: Vec<UserQueryNode>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Visited,
}

impl QueryArena {
    /// Create a new empty arena.
    #[expect(dead_code, reason = "reserved for future use")]
    pub(crate) const fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Push a node and return its ID.
    pub(crate) fn push(&mut self, node: UserQueryNode) -> QueryNodeId {
        let id = QueryNodeId::new(
            u32::try_from(self.nodes.len()).expect("query arena exceeded u32 node IDs"),
        );
        self.nodes.push(node);
        id
    }

    /// Get a node by ID.
    pub(crate) fn get(&self, id: QueryNodeId) -> Option<&UserQueryNode> {
        self.nodes.get(id.as_u32() as usize)
    }

    /// Get the number of nodes.
    pub(crate) const fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the arena is empty.
    pub(crate) const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Check whether an identifier exists in the arena.
    pub(crate) const fn contains(&self, id: QueryNodeId) -> bool {
        (id.as_u32() as usize) < self.nodes.len()
    }
}

/// A node in a query program.
#[derive(Clone, Debug)]
pub enum UserQueryNode {
    /// A single term query.
    Term {
        /// The term text.
        term: Arc<str>,
        /// Optional field specifier.
        field: Option<Arc<str>>,
    },
    /// A phrase query (multiple terms in order).
    Phrase {
        /// The terms in the phrase.
        terms: Vec<Arc<str>>,
        /// Maximum distance between terms (slop).
        slop: u32,
    },
    /// A boolean query combining multiple children.
    Boolean {
        /// The boolean operator.
        op: BooleanOp,
        /// Child node IDs.
        children: Vec<QueryNodeId>,
    },
    /// A boost query modifying a child's score.
    Boost {
        /// The child node ID.
        child: QueryNodeId,
        /// Boost multiplier.
        factor: f32,
    },
}

/// Boolean operators for combining queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOp {
    /// Logical AND - all children must match.
    And,
    /// Logical OR - any child may match.
    Or,
    /// Logical NOT - child must not match.
    Not,
}

/// A compiled query program with arena storage.
#[derive(Clone, Debug)]
pub struct UserQueryProgram {
    arena: Arc<QueryArena>,
    root: QueryNodeId,
}

impl UserQueryProgram {
    /// Create a new query program.
    pub(crate) fn new(arena: QueryArena, root: QueryNodeId) -> Self {
        Self {
            arena: Arc::new(arena),
            root,
        }
    }

    pub(crate) fn is_valid(arena: &QueryArena, root: QueryNodeId) -> bool {
        if !arena.contains(root) {
            return false;
        }

        arena.nodes.iter().all(|node| match node {
            UserQueryNode::Boolean { children, .. } => {
                children.iter().all(|child| arena.contains(*child))
            }
            UserQueryNode::Boost { child, .. } => arena.contains(*child),
            UserQueryNode::Term { .. } | UserQueryNode::Phrase { .. } => true,
        })
    }

    /// Get the root node ID.
    pub const fn root(&self) -> QueryNodeId {
        self.root
    }

    /// Get the number of nodes in the query.
    pub fn node_count(&self) -> usize {
        self.arena.len()
    }

    /// Get a reference to a node by ID.
    pub fn get(&self, id: QueryNodeId) -> Option<&UserQueryNode> {
        self.arena.get(id)
    }

    /// Get the children of a boolean node.
    pub fn children_of(&self, id: QueryNodeId) -> &[QueryNodeId] {
        match self.get(id) {
            Some(UserQueryNode::Boolean { children, .. }) => children.as_slice(),
            Some(UserQueryNode::Boost { child, .. }) => core::slice::from_ref(child),
            _ => &[],
        }
    }

    /// Walk the query tree in pre-order traversal.
    pub fn walk(&self) -> impl Iterator<Item = QueryNodeId> + '_ {
        WalkIter {
            program: self,
            stack: vec![self.root],
            visited: vec![false; self.arena.len()],
        }
    }
}

/// Iterator for pre-order traversal of query nodes.
#[derive(Clone)]
struct WalkIter<'a> {
    program: &'a UserQueryProgram,
    stack: Vec<QueryNodeId>,
    visited: Vec<bool>,
}

impl Iterator for WalkIter<'_> {
    type Item = QueryNodeId;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(id) = self.stack.pop() {
            let idx = id.as_u32() as usize;
            if idx < self.visited.len() && self.visited[idx] {
                continue;
            }

            if let Some(node) = self.program.get(id) {
                if idx < self.visited.len() {
                    self.visited[idx] = true;
                }
                let children: Vec<QueryNodeId> = match node {
                    UserQueryNode::Boolean { children, .. } => children.clone(),
                    UserQueryNode::Boost { child, .. } => vec![*child],
                    _ => vec![],
                };
                for child in children.into_iter().rev() {
                    self.stack.push(child);
                }
                return Some(id);
            }
        }
        None
    }
}

/// Error type for view extraction failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtractionError {
    /// The node ID does not exist in the arena.
    InvalidNodeId(QueryNodeId),
    /// The node exists but is not of the expected type.
    WrongNodeType {
        /// The ID that was accessed.
        id: QueryNodeId,
        /// The expected type name.
        expected: &'static str,
        /// The actual type name.
        actual: &'static str,
    },
}

impl core::error::Error for ExtractionError {}

impl fmt::Display for ExtractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNodeId(id) => write!(f, "invalid node ID: {}", id.as_u32()),
            Self::WrongNodeType {
                id,
                expected,
                actual,
            } => write!(
                f,
                "wrong node type for ID {}: expected {}, got {}",
                id.as_u32(),
                expected,
                actual
            ),
        }
    }
}

/// A view of a term query node.
#[derive(Clone, Debug)]
pub struct TermView {
    /// The term text.
    pub term: Arc<str>,
    /// Optional field specifier.
    pub field: Option<Arc<str>>,
}

impl TryFrom<(&UserQueryProgram, QueryNodeId)> for TermView {
    type Error = ExtractionError;

    fn try_from((program, id): (&UserQueryProgram, QueryNodeId)) -> Result<Self, Self::Error> {
        match program.get(id) {
            Some(UserQueryNode::Term { term, field }) => Ok(Self {
                term: term.clone(),
                field: field.clone(),
            }),
            Some(node) => Err(ExtractionError::WrongNodeType {
                id,
                expected: "Term",
                actual: node.type_name(),
            }),
            None => Err(ExtractionError::InvalidNodeId(id)),
        }
    }
}

impl TryFrom<(UserQueryProgram, QueryNodeId)> for TermView {
    type Error = ExtractionError;

    fn try_from((program, id): (UserQueryProgram, QueryNodeId)) -> Result<Self, Self::Error> {
        Self::try_from((&program, id))
    }
}

/// A view of a phrase query node.
#[derive(Clone, Debug)]
pub struct PhraseView {
    /// The terms in the phrase.
    pub terms: Vec<Arc<str>>,
    /// Maximum distance between terms (slop).
    pub slop: u32,
}

impl TryFrom<(&UserQueryProgram, QueryNodeId)> for PhraseView {
    type Error = ExtractionError;

    fn try_from((program, id): (&UserQueryProgram, QueryNodeId)) -> Result<Self, Self::Error> {
        match program.get(id) {
            Some(UserQueryNode::Phrase { terms, slop }) => Ok(Self {
                terms: terms.clone(),
                slop: *slop,
            }),
            Some(node) => Err(ExtractionError::WrongNodeType {
                id,
                expected: "Phrase",
                actual: node.type_name(),
            }),
            None => Err(ExtractionError::InvalidNodeId(id)),
        }
    }
}

impl TryFrom<(UserQueryProgram, QueryNodeId)> for PhraseView {
    type Error = ExtractionError;

    fn try_from((program, id): (UserQueryProgram, QueryNodeId)) -> Result<Self, Self::Error> {
        Self::try_from((&program, id))
    }
}

/// A view of a boolean query node.
#[derive(Clone, Debug)]
pub struct BooleanView {
    /// The boolean operator.
    pub op: BooleanOp,
    /// Child node IDs.
    pub children: Vec<QueryNodeId>,
}

impl TryFrom<(&UserQueryProgram, QueryNodeId)> for BooleanView {
    type Error = ExtractionError;

    fn try_from((program, id): (&UserQueryProgram, QueryNodeId)) -> Result<Self, Self::Error> {
        match program.get(id) {
            Some(UserQueryNode::Boolean { op, children }) => Ok(Self {
                op: *op,
                children: children.clone(),
            }),
            Some(node) => Err(ExtractionError::WrongNodeType {
                id,
                expected: "Boolean",
                actual: node.type_name(),
            }),
            None => Err(ExtractionError::InvalidNodeId(id)),
        }
    }
}

impl TryFrom<(UserQueryProgram, QueryNodeId)> for BooleanView {
    type Error = ExtractionError;

    fn try_from((program, id): (UserQueryProgram, QueryNodeId)) -> Result<Self, Self::Error> {
        Self::try_from((&program, id))
    }
}

/// A view of a boost query node.
#[derive(Clone, Debug)]
pub struct BoostView {
    /// The child node ID.
    pub child: QueryNodeId,
    /// Boost multiplier.
    pub factor: f32,
}

impl TryFrom<(&UserQueryProgram, QueryNodeId)> for BoostView {
    type Error = ExtractionError;

    fn try_from((program, id): (&UserQueryProgram, QueryNodeId)) -> Result<Self, Self::Error> {
        match program.get(id) {
            Some(UserQueryNode::Boost { child, factor }) => Ok(Self {
                child: *child,
                factor: *factor,
            }),
            Some(node) => Err(ExtractionError::WrongNodeType {
                id,
                expected: "Boost",
                actual: node.type_name(),
            }),
            None => Err(ExtractionError::InvalidNodeId(id)),
        }
    }
}

impl TryFrom<(UserQueryProgram, QueryNodeId)> for BoostView {
    type Error = ExtractionError;

    fn try_from((program, id): (UserQueryProgram, QueryNodeId)) -> Result<Self, Self::Error> {
        Self::try_from((&program, id))
    }
}

impl UserQueryNode {
    /// Get the type name of this node for error messages.
    pub(crate) const fn type_name(&self) -> &'static str {
        match self {
            Self::Term { .. } => "Term",
            Self::Phrase { .. } => "Phrase",
            Self::Boolean { .. } => "Boolean",
            Self::Boost { .. } => "Boost",
        }
    }
}

/// Execution-facing query program produced by the Phase 1 planner.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryProgram {
    pub(crate) nodes: Vec<QueryNode>,
    pub(crate) root: QueryNodeId,
    pub(crate) max_depth: usize,
}

impl QueryProgram {
    /// Create a new planned query program.
    pub fn new(nodes: Vec<QueryNode>, root: QueryNodeId, max_depth: usize) -> Self {
        Self::try_new(nodes, root, max_depth)
            .expect("planned query program contains invalid references")
    }

    /// Create a new planned query program, returning an error for invalid references.
    pub fn try_new(
        nodes: Vec<QueryNode>,
        root: QueryNodeId,
        max_depth: usize,
    ) -> Result<Self, QueryError> {
        validate_planned_program(&nodes, root)?;

        Ok(Self {
            nodes,
            root,
            max_depth,
        })
    }

    /// Root node identifier.
    pub const fn root(&self) -> QueryNodeId {
        self.root
    }

    /// Number of nodes in the program.
    pub const fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Maximum nesting depth in the program.
    pub const fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Get a node by identifier.
    pub fn get(&self, id: QueryNodeId) -> Option<&QueryNode> {
        self.nodes.get(id.as_u32() as usize)
    }
}

fn validate_planned_program(nodes: &[QueryNode], root: QueryNodeId) -> Result<(), QueryError> {
    let contains = |id: QueryNodeId| (id.as_u32() as usize) < nodes.len();

    if !contains(root) {
        return Err(QueryError::InvalidProgramRoot { root });
    }

    for (index, node) in nodes.iter().enumerate() {
        let parent = QueryNodeId::new(
            u32::try_from(index).expect("planned query program exceeded u32 node IDs"),
        );
        match node {
            QueryNode::And { children, .. } | QueryNode::Or { children, .. } => {
                for child in children {
                    if !contains(*child) {
                        return Err(QueryError::InvalidProgramReference {
                            parent,
                            child: *child,
                        });
                    }
                }
            }
            QueryNode::Not { child }
            | QueryNode::ConstantScore { child, .. }
            | QueryNode::Filter { input: child, .. }
            | QueryNode::ExternalFilter { input: child, .. } => {
                if !contains(*child) {
                    return Err(QueryError::InvalidProgramReference {
                        parent,
                        child: *child,
                    });
                }
            }
            QueryNode::Term { .. } => {}
        }
    }

    let mut states = vec![None; nodes.len()];
    visit_planned_program(root, nodes, &mut states)?;

    if let Some((index, _)) = states.iter().enumerate().find(|(_, state)| state.is_none()) {
        return Err(QueryError::UnreachableProgramNode {
            node: QueryNodeId::new(
                u32::try_from(index).expect("planned query program exceeded u32 node IDs"),
            ),
        });
    }

    Ok(())
}

fn visit_planned_program(
    node_id: QueryNodeId,
    nodes: &[QueryNode],
    states: &mut [Option<VisitState>],
) -> Result<(), QueryError> {
    let index = node_id.as_u32() as usize;
    match states[index] {
        Some(VisitState::Visiting) => {
            return Err(QueryError::InvalidProgramCycle { node: node_id });
        }
        Some(VisitState::Visited) => return Ok(()),
        None => {}
    }

    states[index] = Some(VisitState::Visiting);
    for child in nodes[index].children() {
        visit_planned_program(*child, nodes, states)?;
    }
    states[index] = Some(VisitState::Visited);
    Ok(())
}

/// Execution-facing query node variants for Phase 1 planning.
#[derive(Clone, Debug, PartialEq)]
pub enum QueryNode {
    /// Resolved term lookup.
    Term {
        /// Canonical field identifier.
        field: leit_core::FieldId,
        /// Canonical term identifier.
        term: leit_core::TermId,
        /// Score multiplier for the term.
        boost: f32,
    },
    /// Logical conjunction.
    And {
        /// Child node identifiers.
        children: Vec<QueryNodeId>,
        /// Score multiplier for the node.
        boost: f32,
    },
    /// Logical disjunction.
    Or {
        /// Child node identifiers.
        children: Vec<QueryNodeId>,
        /// Score multiplier for the node.
        boost: f32,
    },
    /// Logical negation.
    Not {
        /// Child node identifier.
        child: QueryNodeId,
    },
    /// Constant score wrapper.
    ConstantScore {
        /// Child node identifier.
        child: QueryNodeId,
        /// Score multiplier.
        score: f32,
    },
    /// Structured predicate filter. Evaluates `input` for scoring, then
    /// accepts/rejects candidates based on `predicate`.
    ///
    /// Does not contribute to score. Native evaluation deferred until
    /// columnar storage lands (Phase 3).
    Filter {
        /// The scoring subquery.
        input: QueryNodeId,
        /// The structured predicate.
        predicate: FilterPredicate,
    },
    /// Application-provided filter resolved at execution time via
    /// [`FilterEvaluator`](leit_core::FilterEvaluator). Evaluates `input` for scoring, then calls
    /// the evaluator with `slot` to accept/reject candidates.
    ///
    /// Does not contribute to score.
    ExternalFilter {
        /// The scoring subquery.
        input: QueryNodeId,
        /// The filter slot identifier.
        slot: leit_core::FilterSlotId,
    },
}

impl QueryNode {
    fn children(&self) -> &[QueryNodeId] {
        match self {
            Self::And { children, .. } | Self::Or { children, .. } => children,
            Self::Not { child }
            | Self::ConstantScore { child, .. }
            | Self::Filter { input: child, .. }
            | Self::ExternalFilter { input: child, .. } => core::slice::from_ref(child),
            Self::Term { .. } => &[],
        }
    }
}

/// Trait for resolving field names during planning.
pub trait FieldRegistry {
    /// Resolve a field name to a canonical field identifier.
    fn resolve_field(&self, field: &str) -> Option<leit_core::FieldId>;
}

/// Trait for resolving terms during planning.
pub trait TermDictionary {
    /// Resolve a term for a field to a canonical term identifier.
    fn resolve_term(&self, field: leit_core::FieldId, term: &str) -> Option<leit_core::TermId>;
}

/// Planning context for Phase 1 query planning.
#[derive(Clone)]
pub struct PlanningContext<'a> {
    pub(crate) dictionary: &'a dyn TermDictionary,
    pub(crate) fields: &'a dyn FieldRegistry,
    pub(crate) default_fields: Vec<leit_core::FieldId>,
    pub(crate) default_boost: f32,
}

impl<'a> PlanningContext<'a> {
    /// Create a new planning context.
    pub fn new(dictionary: &'a dyn TermDictionary, fields: &'a dyn FieldRegistry) -> Self {
        Self {
            dictionary,
            fields,
            default_fields: Vec::new(),
            default_boost: 1.0,
        }
    }

    /// Set the default fields for unfielded term expansion.
    #[must_use]
    pub fn with_default_fields(mut self, fields: Vec<leit_core::FieldId>) -> Self {
        self.default_fields = fields;
        self
    }

    /// Set a single default field (backward compatible).
    #[must_use]
    pub fn with_default_field(mut self, field: leit_core::FieldId) -> Self {
        self.default_fields = alloc::vec![field];
        self
    }

    /// Set the default boost.
    #[must_use]
    pub fn with_default_boost(mut self, boost: f32) -> Self {
        self.default_boost = boost;
        self
    }
}

impl fmt::Debug for PlanningContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlanningContext")
            .field("default_fields", &self.default_fields)
            .field("default_boost", &self.default_boost)
            .finish_non_exhaustive()
    }
}

/// Planner scratch space reused across planning operations.
#[derive(Clone, Debug, Default)]
pub struct PlannerScratch {}

impl PlannerScratch {
    /// Create a new empty planner scratch buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset the scratch state for reuse.
    pub fn reset(&mut self) {}
}

/// Planning errors for Phase 1 query planning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryError {
    /// The query string could not be parsed.
    ParseError,
    /// The field name was not known to the planning context.
    UnknownField {
        /// The unresolved field name.
        field: alloc::string::String,
    },
    /// The term could not be resolved.
    UnknownTerm {
        /// The field whose dictionary lookup failed.
        field: leit_core::FieldId,
        /// The unresolved term text.
        term: alloc::string::String,
    },
    /// The maximum planner depth was exceeded.
    MaxDepthExceeded {
        /// The configured planner depth limit.
        max_depth: usize,
        /// The depth required by the parsed query.
        actual_depth: usize,
    },
    /// The maximum planner node count was exceeded.
    MaxNodesExceeded {
        /// The configured planner node limit.
        max_nodes: usize,
        /// The node count required by the parsed query.
        actual_nodes: usize,
    },
    /// The planned program root is invalid.
    InvalidProgramRoot {
        /// The root node identifier.
        root: QueryNodeId,
    },
    /// The planned program references a child node that does not exist.
    InvalidProgramReference {
        /// The parent node identifier.
        parent: QueryNodeId,
        /// The missing child node identifier.
        child: QueryNodeId,
    },
    /// The planned program contains a cycle.
    InvalidProgramCycle {
        /// The node where cycle detection re-entered the graph.
        node: QueryNodeId,
    },
    /// The planned program contains a node unreachable from the root.
    UnreachableProgramNode {
        /// The unreachable node identifier.
        node: QueryNodeId,
    },
}

impl core::error::Error for QueryError {}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseError => write!(f, "failed to parse query"),
            Self::UnknownField { field } => write!(f, "unknown field: {field}"),
            Self::UnknownTerm { field, term } => {
                write!(f, "unknown term for field {}: {term}", field.as_u32())
            }
            Self::MaxDepthExceeded {
                max_depth,
                actual_depth,
            } => write!(
                f,
                "query depth exceeded: max {max_depth}, actual {actual_depth}"
            ),
            Self::MaxNodesExceeded {
                max_nodes,
                actual_nodes,
            } => write!(
                f,
                "query node count exceeded: max {max_nodes}, actual {actual_nodes}"
            ),
            Self::InvalidProgramRoot { root } => {
                write!(
                    f,
                    "planned query program has invalid root {}",
                    root.as_u32()
                )
            }
            Self::InvalidProgramReference { parent, child } => write!(
                f,
                "planned query program references missing child {} from parent {}",
                child.as_u32(),
                parent.as_u32()
            ),
            Self::InvalidProgramCycle { node } => write!(
                f,
                "planned query program contains a cycle at node {}",
                node.as_u32()
            ),
            Self::UnreachableProgramNode { node } => write!(
                f,
                "planned query program contains unreachable node {}",
                node.as_u32()
            ),
        }
    }
}

/// Required execution features for a planned query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureSet {
    /// Whether term frequency data is required.
    pub needs_term_frequency: bool,
    /// Whether positions are required.
    pub needs_positions: bool,
    /// Whether block-max data is required.
    pub needs_block_max: bool,
}

impl FeatureSet {
    /// No special execution features required.
    pub const NONE: Self = Self {
        needs_term_frequency: false,
        needs_positions: false,
        needs_block_max: false,
    };

    /// Basic lexical execution requirements.
    pub const fn basic() -> Self {
        Self {
            needs_term_frequency: true,
            needs_positions: false,
            needs_block_max: false,
        }
    }
}

/// Planned query plus execution metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionPlan {
    /// The planned query program.
    pub program: QueryProgram,
    /// Estimated selectivity.
    pub selectivity: f32,
    /// Estimated cost.
    pub cost: u32,
    /// Required execution features.
    pub required_features: FeatureSet,
}

impl ExecutionPlan {
    /// Wrap the current plan root in an [`ExternalFilter`](QueryNode::ExternalFilter) node.
    ///
    /// This is a post-planning transformation. It appends an `ExternalFilter`
    /// node to the program, updates the root, and recomputes metadata:
    ///
    /// - `program.root` — replaced with the new node ID
    /// - `program.max_depth` — incremented by 1
    /// - `cost` — recomputed as `program.nodes.len()`
    /// - `selectivity` — recomputed as `1.0 / node_count`
    /// - `required_features` — preserved
    pub fn wrap_external_filter(&mut self, slot: leit_core::FilterSlotId) -> &mut Self {
        let old_root = self.program.root;
        let new_id = QueryNodeId::new(
            u32::try_from(self.program.nodes.len()).expect("query program exceeded u32 node IDs"),
        );
        self.program.nodes.push(QueryNode::ExternalFilter {
            input: old_root,
            slot,
        });
        self.program.root = new_id;
        self.program.max_depth += 1;

        let node_count = self.program.nodes.len();
        self.cost = u32::try_from(node_count).expect("node count exceeded u32");
        self.selectivity = if node_count == 0 {
            1.0
        } else {
            1.0 / node_count as f32
        };
        self
    }
}

/// A typed filter value for structured predicates.
///
/// `F64` values must be finite. Non-finite values (NaN, infinity) are not
/// meaningful as filter predicates and may produce unexpected equality/ordering
/// behavior.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterValue {
    /// Unsigned 64-bit integer.
    U64(u64),
    /// Signed 64-bit integer.
    I64(i64),
    /// 64-bit floating point (must be finite).
    F64(f64),
    /// String value.
    Str(alloc::string::String),
}

/// A structured field predicate for columnar data.
///
/// These predicates define the filter IR. Native evaluation requires
/// columnar storage (Phase 3). Until then, constructing a [`QueryNode::Filter`]
/// is valid, but executing it returns an error.
///
/// Boolean combinators (`And`, `Or`, `Not`) compose predicates within a
/// single `Filter` node, avoiding double-evaluation of the input subquery.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterPredicate {
    /// Exact equality on a field value.
    Eq {
        /// The field to filter on.
        field: leit_core::FieldId,
        /// The value to compare against.
        value: FilterValue,
    },
    /// Inclusive range predicate. Either bound may be omitted for open-ended ranges.
    Range {
        /// The field to filter on.
        field: leit_core::FieldId,
        /// Lower bound (inclusive), or `None` for unbounded.
        low: Option<FilterValue>,
        /// Upper bound (inclusive), or `None` for unbounded.
        high: Option<FilterValue>,
    },
    /// Set membership predicate.
    In {
        /// The field to filter on.
        field: leit_core::FieldId,
        /// The set of values to match against.
        values: Vec<FilterValue>,
    },
    /// Boolean conjunction — all child predicates must match.
    And(Vec<Self>),
    /// Boolean disjunction — at least one child predicate must match.
    Or(Vec<Self>),
    /// Boolean negation.
    Not(alloc::boxed::Box<Self>),
}

#[cfg(test)]
mod execution_plan_tests {
    use super::*;
    use leit_core::{FieldId, FilterSlotId, TermId};

    fn simple_term_plan() -> ExecutionPlan {
        let nodes = vec![QueryNode::Term {
            field: FieldId::new(0),
            term: TermId::new(0),
            boost: 1.0,
        }];
        ExecutionPlan {
            program: QueryProgram::new(nodes, QueryNodeId::new(0), 1),
            selectivity: 1.0,
            cost: 1,
            required_features: FeatureSet::basic(),
        }
    }

    #[test]
    fn wrap_external_filter_updates_root() {
        let mut plan = simple_term_plan();
        let old_root = plan.program.root();
        plan.wrap_external_filter(FilterSlotId::new(0));
        assert_ne!(plan.program.root(), old_root);
    }

    #[test]
    fn wrap_external_filter_increments_depth() {
        let mut plan = simple_term_plan();
        let old_depth = plan.program.max_depth();
        plan.wrap_external_filter(FilterSlotId::new(0));
        assert_eq!(plan.program.max_depth(), old_depth + 1);
    }

    #[test]
    fn wrap_external_filter_recomputes_cost() {
        let mut plan = simple_term_plan();
        assert_eq!(plan.cost, 1);
        plan.wrap_external_filter(FilterSlotId::new(0));
        assert_eq!(plan.cost, 2);
        plan.wrap_external_filter(FilterSlotId::new(1));
        assert_eq!(plan.cost, 3);
    }

    #[test]
    fn wrap_external_filter_recomputes_selectivity() {
        let mut plan = simple_term_plan();
        plan.wrap_external_filter(FilterSlotId::new(0));
        let expected = 1.0 / plan.program.node_count() as f32;
        assert!((plan.selectivity - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn wrap_external_filter_preserves_required_features() {
        let mut plan = simple_term_plan();
        let features_before = plan.required_features;
        plan.wrap_external_filter(FilterSlotId::new(0));
        assert_eq!(plan.required_features, features_before);
    }

    #[test]
    fn wrapped_plan_passes_validation() {
        let mut plan = simple_term_plan();
        plan.wrap_external_filter(FilterSlotId::new(0));
        plan.wrap_external_filter(FilterSlotId::new(1));
        assert!(plan.program.get(plan.program.root()).is_some());
    }
}
