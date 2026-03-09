# Query Planning Architecture

## Overview and Goals

The query planning architecture provides a composable, arena-based AST representation for queries with clear separation between logical structure, lowering, and physical planning.

### Core Design Principles

1. **Arena Allocation (Internal)** — All AST nodes are stored in a typed arena internally, enabling zero-copy cloning, cheap reference sharing, and stable node identities. **The arena is an implementation detail — the public API hides it completely.**
2. **Zero-Cost Composition** — Query subtrees can be composed, transformed, and extended without allocation or copying. Building a complex query from small pieces is free.
3. **AST Flexibility** — The AST is intentionally decoupled from any specific parser syntax or execution model. It can represent queries from multiple sources (text, structured, programmatic) with the same structures.
4. **Separated Concerns** — Lowering (parse tree → logical query) and physical planning (logical query → execution plan) are distinct phases with well-defined interfaces.
5. **Clean Public API** — Users interact with `QueryProgram` and `ExecutionPlan` through semantic methods like `walk()`, `node_count()`, and `validate()` rather than direct arena manipulation.

### API Design Philosophy

The architecture follows a **"public API first, implementation details hidden"** approach:

- **Public API**: `QueryProgram` and `ExecutionPlan` expose semantic methods for traversal, inspection, and validation
- **Internal Implementation**: Arena allocation provides efficient storage and cheap cloning under the hood
- **Opaque Handles**: `QueryNodeId` and `PhysicalOperatorId` are opaque handles from the user's perspective
- **Builder Pattern**: `QueryBuilder` and lowerers handle construction, users don't manipulate arenas directly

**Example of the public API:**
```rust
// Users interact with semantic methods, not arenas
let program = lowerer.lower(&parse_tree)?;

// Inspect the query structure
println!("Query has {} nodes", program.node_count());

// Walk the query tree
for node in program.walk() {
    println!("Node: {:?}", node);
}

// Get children of a specific node
for child_id in program.children_of(node_id) {
    // Process child
}

// Validate structural integrity
program.validate()?;
```

**Arena usage is internal:**
```rust
// Internal implementation (hidden from users)
pub struct QueryProgram {
    arena: Arc<QueryArena>,  // Internal implementation detail
    root: QueryNodeId,
}
```

## Memory Allocation Patterns

This architecture uses two distinct memory allocation patterns from leit_core, each serving different purposes. **These patterns must not be mixed** — each has a specific role.

### Pattern 1: Shared Immutable Arena (for QueryProgram, ExecutionPlan)

Used for query structure that is built once and shared immutably:

```rust
use std::sync::Arc;

/// Arena storing all nodes for a query structure
pub struct QueryArena {
    nodes: Vec<QueryNode>,
    strings: Arc<strint::InternPool>,
}

/// Complete query representation — cloning is cheap (Arc bump)
#[derive(Debug, Clone)]
pub struct QueryProgram {
    arena: Arc<QueryArena>,  // Shared immutable storage
    root: QueryNodeId,
}
```

**Characteristics:**
- **Arc-wrapped**: `Arc<Arena>` enables cheap cloning and sharing across threads
- **Stable indices**: Nodes are indexed by typed IDs (`QueryNodeId`, `PhysicalOperatorId`)
- **Immutable after construction**: Once built, the arena is not modified
- **Zero-cost cloning**: `clone()` on QueryProgram just bumps the Arc reference count

**Used for:**
- Query AST (`QueryArena` + `QueryNode`)
- Query program (`QueryProgram` wrapping `Arc<QueryArena>`)
- Execution plan (`ExecutionPlan` wrapping `Arc<PhysicalOperatorArena>`)

### Pattern 2: ScratchSpace/Workspace (for Execution)

Used for mutable, temporary buffers during execution:

```rust
use leit_core::ScratchSpace;

/// Execute a physical operator using scratch space for temps
pub trait PhysicalOperatorExecutor {
    fn execute(
        &self,
        scratch: &mut dyn ScratchSpace,  // Mutable temp buffers
    ) -> Result<Cursor, ExecutionError>;
}
```

**Characteristics:**
- **Mutable**: Passed as `&mut dyn ScratchSpace`
- **Temporary**: Reset/reused between operations
- **No Arc**: Direct mutable reference, not shared
- **trait object safe**: Can be used as `dyn ScratchSpace`

**Used for:**
- Score accumulation buffers
- Document ID collector scratch
- Temporary vectors for intersection/union operations
- Any allocation that only lives during one operation

**ScratchSpace API (from leit_core):**
```rust
pub trait ScratchSpace {
    type Error: Into<CoreError>;
    
    fn alloc_vec<T>(&mut self, capacity: usize) -> Result<Vec<T>, Self::Error>
    where T: Default;
    fn alloc_string(&mut self, capacity: usize) -> Result<String, Self::Error>;
    fn alloc_bytes(&mut self, capacity: usize) -> Result<Vec<u8>, Self::Error>;
    fn reset(&mut self);  // Clears allocations, preserves capacity
}
```

### Why Two Patterns?

| Aspect | Arc<Arena> | ScratchSpace |
|--------|-----------|--------------|
| Mutability | Immutable after build | Mutable during execution |
| Sharing | Arc enables cheap clone | No sharing, one owner |
| Lifetime | Query/plan lifetime | Single operation |
| Use case | Query structure | Execution temps |
| Reset | Never (drop whole arena) | `reset()` between ops |

**Anti-pattern to avoid:**
```rust
// ❌ WRONG: Using ScratchSpace for query structure
fn build_query(scratch: &mut ScratchSpace) -> QueryProgram {
    // This conflates two different allocation patterns
}

// ✅ CORRECT: Use Arc<Arena> for structure, ScratchSpace for execution
fn build_query() -> QueryProgram {
    let arena = Arc::new(QueryArena::new());
    QueryProgram::new(arena, root)
}

fn execute_query(plan: &ExecutionPlan, scratch: &mut dyn ScratchSpace) -> Results {
    plan.root_executor().execute(scratch)
}
```

### Why This Architecture Matters

Traditional query engines often blur these concerns:

- Parsers directly produce execution-ready structures
- Query nodes own their children (making transformations expensive)
- Logical structure is tied to a specific physical execution strategy
- Implementation details leak into the public API

This leads to:

- Expensive query transformations (requiring deep copies)
- Difficulty supporting multiple query syntaxes
- Inability to reason about logical queries independently of execution
- Hard-to-extend optimizer pipelines
- Fragile APIs that break when internal implementation changes

The Leit architecture separates these concerns while keeping the implementation hidden:

```text
Parse Tree (syntax-specific)
        ↓
   Lowering (syntax → logical)
        ↓
QueryProgram (logical AST, clean public API)
        ↓
  Optimization (logical rewrites via internal arena)
        ↓
Physical Planning (logical → physical)
        ↓
ExecutionPlan (operator graph, clean public API)
```

Each phase can be extended, replaced, or composed independently, and users interact with clean, stable APIs that don't expose the arena implementation.

## Core Types

### Arena and Typed Indices (Internal Implementation)

**Note: The arena is an internal implementation detail. Users interact with the public API (`QueryProgram`, `ExecutionPlan`) and never see the arena directly.**

The foundation is a typed arena that stores all AST nodes contiguously with stable indices. This follows **Pattern 1: Shared Immutable Arena** from the Memory Allocation Patterns section above.

```rust
use std::sync::Arc;

/// Arena storing all query nodes for a single logical query
#[derive(Debug, Clone)]
pub struct QueryArena {
    /// Raw node storage
    nodes: Vec<QueryNode>,
    /// Interned strings for terms and phrases
    strings: Arc<strint::InternPool>,
    /// Cached metadata
    metadata: ArenaMetadata,
}

impl QueryArena {
    /// Allocate a new node and return its stable index
    pub fn alloc(&mut self, node: QueryNode) -> QueryNodeId {
        let id = QueryNodeId::new(self.nodes.len());
        self.nodes.push(node);
        id
    }

    /// Access a node by ID
    pub fn get(&self, id: QueryNodeId) -> Option<&QueryNode> {
        self.nodes.get(id.index()).filter(|_| !id.is_none())
    }

    /// Access a node mutably by ID
    pub fn get_mut(&mut self, id: QueryNodeId) -> Option<&mut QueryNode> {
        self.nodes.get_mut(id.index()).filter(|_| !id.is_none())
    }

    /// Get the total number of nodes in the arena
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Validate structural integrity of the arena
    pub fn validate(&self) -> Result<(), ValidationError> {
        // Check for cycles, invalid references, etc.
        // This is a placeholder for actual validation logic
        if self.nodes.is_empty() {
            return Err(ValidationError::EmptyQuery);
        }
        Ok(())
    }
}

/// Stable identifier for a node in the arena
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueryNodeId {
    index: u32,
}

impl QueryNodeId {
    const NONE: u32 = u32::MAX;

    pub fn new(index: usize) -> Self {
        Self {
            index: index as u32,
        }
    }

    pub fn index(self) -> usize {
        self.index as usize
    }

    pub fn is_none(self) -> bool {
        self.index == Self::NONE
    }

    pub fn none() -> Self {
        Self { index: Self::NONE }
    }
}
```

### Arena Benefits

These benefits apply to **Pattern 1: Shared Immutable Arena** — the arena-based query structure:

- **Zero-copy cloning** — Cloning a `QueryProgram` copies only the arena reference and root node ID
- **Stable identities** — Node indices never change, enabling memoization and caching
- **Contiguous storage** — Better cache locality during traversal
- **Cheap transformations** — Rewrites allocate new nodes but keep the arena compact

**Note:** For execution-time temporary allocations (score buffers, document ID collectors, etc.), see **Pattern 2: ScratchSpace/Workspace** in the Memory Allocation Patterns section.

## Token System

Tokens represent the atomic units that queries reference — terms, phrases, or vectors. They're resolved to concrete `TermId` values during lowering or planning.

### Token Enum

```rust
/// Atomic query unit that can be resolved to a TermId
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token {
    /// Simple text term (e.g., "rust")
    Text(String),
    
    /// Phrase query (e.g., "query planning")
    Phrase(String),
    
    /// Dense vector embedding
    Vector(Vec<f32>),
    
    /// Sparse vector (e.g., from BM25)
    Sparse { dimensions: Vec<(u32, f32)> },
    
    /// Term with field specification
    Field { field: String, token: Box<Token> },
    
    /// Term with boost
    Boosted { token: Box<Token>, boost: f32 },
}

impl Token {
    /// Create a text token
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    /// Create a phrase token
    pub fn phrase(phrase: impl Into<String>) -> Self {
        Self::Phrase(phrase.into())
    }

    /// Create a vector token
    pub fn vector(vec: Vec<f32>) -> Self {
        Self::Vector(vec)
    }

    /// Create a field-scoped token
    pub fn field(field: impl Into<String>, token: Token) -> Self {
        Self::Field {
            field: field.into(),
            token: Box::new(token),
        }
    }

    /// Create a boosted token
    pub fn boosted(token: Token, boost: f32) -> Self {
        Self::Boosted {
            token: Box::new(token),
            boost,
        }
    }

    /// Access the underlying text if this is a Text token
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Token::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Access the underlying phrase if this is a Phrase token
    pub fn as_phrase(&self) -> Option<&str> {
        match self {
            Token::Phrase(s) => Some(s),
            _ => None,
        }
    }
}
```

### Token Resolution

```rust
/// Error during token resolution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionError {
    /// Token not found in the index
    NotFound,
    
    /// Invalid token format
    InvalidToken,
    
    /// Resolution not supported for this token type
    UnsupportedType,
}

/// Trait for resolving tokens to TermIds
pub trait TokenResolver {
    /// Resolve a single token to a TermId
    fn resolve_token(&self, token: &Token) -> Result<TermId, ResolutionError>;

    /// Resolve multiple tokens in bulk
    fn resolve_tokens(&self, tokens: &[Token]) -> Result<Vec<TermId>, ResolutionError> {
        tokens
            .iter()
            .map(|t| self.resolve_token(t))
            .collect()
    }

    /// Resolve a field-scoped token
    fn resolve_field_token(
        &self,
        field: &str,
        token: &Token,
    ) -> Result<TermId, ResolutionError>;
}

/// Example resolver backed by a term dictionary
pub struct DictionaryTokenResolver<'a> {
    dict: &'a Dictionary,
}

impl<'a> TokenResolver for DictionaryTokenResolver<'a> {
    fn resolve_token(&self, token: &Token) -> Result<TermId, ResolutionError> {
        match token {
            Token::Text(term) => self.dict.lookup(term).ok_or(ResolutionError::NotFound),
            Token::Phrase(phrase) => {
                // Phrase tokens may resolve to a synthetic term ID
                self.dict.lookup_phrase(phrase).ok_or(ResolutionError::NotFound)
            }
            Token::Field { field, token } => {
                self.resolve_field_token(field, token)
            }
            Token::Vector(_) | Token::Sparse { .. } => {
                // Vector tokens are handled differently
                Err(ResolutionError::UnsupportedType)
            }
            Token::Boosted { token, .. } => self.resolve_token(token),
        }
    }

    fn resolve_field_token(
        &self,
        field: &str,
        token: &Token,
    ) -> Result<TermId, ResolutionError> {
        // Field-scoped terms have their own namespace
        match token {
            Token::Text(term) => {
                let field_term = format!("{}:{}", field, term);
                self.dict.lookup(&field_term)
                    .ok_or(ResolutionError::NotFound)
            }
            _ => self.resolve_token(token),
        }
    }
}
```

## QueryNode AST

All query nodes are stored in the arena and reference each other by `QueryNodeId`. This creates a directed acyclic graph (DAG) structure.

### Node Types

```rust
/// Logical query node stored in the arena
#[derive(Debug, Clone, PartialEq)]
pub enum QueryNode {
    /// Leaf node referencing a single term
    Term {
        /// The token to resolve
        token: Token,
        /// Optional ID if pre-resolved (cached from previous resolution)
        term_id: Option<TermId>,
    },

    /// Boolean AND — all children must match
    And {
        /// Child nodes
        children: Vec<QueryNodeId>,
    },

    /// Boolean OR — any child may match
    Or {
        /// Child nodes
        children: Vec<QueryNodeId>,
    },

    /// Boolean NOT — child must not match
    Not {
        /// Child node to negate
        child: QueryNodeId,
    },

    /// Phrase query — exact sequence match
    Phrase {
        /// Phrase terms
        terms: Vec<String>,
        /// Maximum distance for sloppy phrase (0 = exact)
        slop: u32,
    },

    /// Range query — term within bounds
    Range {
        /// Field name
        field: String,
        /// Lower bound (inclusive)
        min: Bound<String>,
        /// Upper bound (inclusive)
        max: Bound<String>,
    },

    /// Boosted query — adjust score
    Boost {
        /// Child node
        child: QueryNodeId,
        /// Boost multiplier
        boost: f32,
    },

    /// Constant score — ignore child's score
    ConstantScore {
        /// Child node
        child: QueryNodeId,
        /// Constant score to return
        score: f32,
    },

    /// Proximity query — terms within distance
    Proximity {
        /// Terms that must appear near each other
        terms: Vec<String>,
        /// Maximum distance between terms
        distance: u32,
        /// Whether order matters
        ordered: bool,
    },

    /// Fuzzy query — term with edit distance
    Fuzzy {
        /// Base term
        term: String,
        /// Maximum edit distance
        distance: u32,
        /// Optional prefix length (exact match)
        prefix_length: u32,
    },

    /// Wildcard query
    Wildcard {
        /// Pattern (e.g., "rus*", "*u?t")
        pattern: String,
    },

    /// Prefix query
    Prefix {
        /// Prefix string
        prefix: String,
    },
}

/// Range bound
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound<T> {
    Unbounded,
    Included(T),
    Excluded(T),
}

impl QueryNode {
    /// Create a term node
    pub fn term(token: Token) -> Self {
        Self::Term {
            token,
            term_id: None,
        }
    }

    /// Create an AND node
    pub fn and(children: Vec<QueryNodeId>) -> Self {
        Self::And { children }
    }

    /// Create an OR node
    pub fn or(children: Vec<QueryNodeId>) -> Self {
        Self::Or { children }
    }

    /// Create a NOT node
    pub fn not(child: QueryNodeId) -> Self {
        Self::Not { child }
    }

    /// Create a phrase node
    pub fn phrase(terms: Vec<String>) -> Self {
        Self::Phrase {
            terms,
            slop: 0,
        }
    }

    /// Create a range node
    pub fn range(field: String, min: Bound<String>, max: Bound<String>) -> Self {
        Self::Range { field, min, max }
    }

    /// Check if this is a leaf node (no children)
    pub fn is_leaf(&self) -> bool {
        matches!(self, Self::Term { .. } | Self::Range { .. } | Self::Phrase { .. })
    }

    /// Get direct children of this node as a slice
    pub fn children_slice(&self) -> &[QueryNodeId] {
        match self {
            Self::And { children } | Self::Or { children } => children.as_slice(),
            Self::Not { child }
            | Self::Boost { child, .. }
            | Self::ConstantScore { child, .. } => std::slice::from_ref(child),
            Self::Term { .. } | Self::Range { .. } | Self::Phrase { .. } => &[],
            Self::Proximity { .. } | Self::Fuzzy { .. } | Self::Wildcard { .. } | Self::Prefix { .. } => &[],
        }
    }

    /// Get direct children of this node (for internal use)
    pub(crate) fn children(&self) -> Vec<QueryNodeId> {
        self.children_slice().to_vec()
    }
}
```

### Node Builders

Convenience builders for common patterns:

```rust
/// Builder for constructing query nodes
///
/// The builder hides arena allocation details while providing
/// a fluent API for query construction.
pub struct QueryBuilder {
    arena: QueryArena,
}

impl QueryBuilder {
    pub fn new() -> Self {
        Self {
            arena: QueryArena::new(),
        }
    }

    // === Node construction methods ===

    /// Build a term query
    pub fn term(&mut self, token: Token) -> QueryNodeId {
        self.node(QueryNode::term(token))
    }

    /// Build a text term query
    pub fn text(&mut self, text: impl Into<String>) -> QueryNodeId {
        self.term(Token::text(text))
    }

    /// Build an AND query
    pub fn and(&mut self, children: Vec<QueryNodeId>) -> QueryNodeId {
        self.node(QueryNode::and(children))
    }

    /// Build an OR query
    pub fn or(&mut self, children: Vec<QueryNodeId>) -> QueryNodeId {
        self.node(QueryNode::or(children))
    }

    /// Build a NOT query
    pub fn not(&mut self, child: QueryNodeId) -> QueryNodeId {
        self.node(QueryNode::not(child))
    }

    /// Build a phrase query
    pub fn phrase(&mut self, terms: Vec<String>) -> QueryNodeId {
        self.node(QueryNode::phrase(terms))
    }

    /// Build a boosted query
    pub fn boost(&mut self, child: QueryNodeId, boost: f32) -> QueryNodeId {
        self.node(QueryNode::Boost { child, boost })
    }

    /// Allocate a custom node in the arena
    pub fn node(&mut self, node: QueryNode) -> QueryNodeId {
        self.arena.alloc(node)
    }

    // === Build methods ===

    /// Build a QueryProgram with the given root node
    pub fn build(self, root: QueryNodeId) -> QueryProgram {
        QueryProgram::new(self.arena, root)
    }

    /// Consume the builder and return the arena (for advanced use cases)
    pub fn into_arena(self) -> QueryArena {
        self.arena
    }
}

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}
```

## Node Extraction Traits

### Introduction

The query tree uses Rust's standard `From` and `TryFrom` traits with newtype wrapper types to provide type-safe, zero-copy extraction of node data. Instead of adding custom methods to `QueryNode` for every possible extraction, we define lightweight view structs that borrow from the underlying node and implement standard conversion traits.

This approach provides:
- **Type safety**: Each view type represents a specific node variant
- **Zero-copy**: Views borrow from the source node without allocation
- **Ecosystem compatibility**: Works with `?` operator, standard combinators, and third-party libraries
- **Clear error handling**: Custom `ExtractionError` indicates expected vs actual variant

### View Types

Each extractable node kind has a corresponding view struct:

```rust
/// Error type for failed node extraction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionError {
    /// Expected a specific variant but found another
    ExpectedVariant {
        expected: &'static str,
        found: &'static str,
    },
    /// Missing required field
    MissingField(&'static str),
}

impl std::fmt::Display for ExtractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractionError::ExpectedVariant { expected, found } => {
                write!(f, "expected {} but found {}", expected, found)
            }
            ExtractionError::MissingField(field) => {
                write!(f, "missing required field: {}", field)
            }
        }
    }
}

impl std::error::Error for ExtractionError {}

/// View of a phrase query node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhraseView<'a> {
    /// The phrase terms in order
    pub terms: &'a [String],
    /// The field to search (defaults to text field)
    pub field: Option<&'a str>,
    /// Slop factor for phrase proximity (0 = exact phrase)
    pub slop: u32,
}

/// View of a term query node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermView<'a> {
    /// The term text
    pub term: &'a str,
    /// The field to search (None = default field)
    pub field: Option<&'a str>,
}

/// Boolean operator for combining queries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOp {
    And,
    Or,
}

/// View of a boolean (AND/OR) query node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanView<'a> {
    /// Child node IDs
    pub children: &'a [QueryNodeId],
    /// The boolean operator
    pub op: BooleanOp,
}

/// View of a range query node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeView<'a> {
    /// Field to search
    pub field: &'a str,
    /// Lower bound (inclusive)
    pub min: Option<&'a str>,
    /// Upper bound (inclusive)
    pub max: Option<&'a str>,
    /// Whether bounds are inclusive
    pub include_min: bool,
    pub include_max: bool,
}

/// View of a boosted or constant-score query node
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoostView<'a> {
    /// The child node being boosted
    pub child: QueryNodeId,
    /// Boost multiplier (or constant score)
    pub score: f32,
}

/// View of a node with children (AND, OR, NOT, Boost, ConstantScore)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildrenView<'a> {
    /// Child node IDs
    pub children: &'a [QueryNodeId],
}
```

### From/TryFrom Implementations

Each view type implements `TryFrom<&QueryNode>` with clear error messages:

```rust
impl<'a> TryFrom<&'a QueryNode> for PhraseView<'a> {
    type Error = ExtractionError;

    fn try_from(node: &'a QueryNode) -> Result<Self, Self::Error> {
        match node {
            QueryNode::Phrase { terms, field, slop } => Ok(PhraseView {
                terms,
                field: field.as_deref(),
                slop: *slop,
            }),
            other => Err(ExtractionError::ExpectedVariant {
                expected: "Phrase",
                found: other.variant_name(),
            }),
        }
    }
}

impl<'a> TryFrom<&'a QueryNode> for TermView<'a> {
    type Error = ExtractionError;

    fn try_from(node: &'a QueryNode) -> Result<Self, Self::Error> {
        match node {
            QueryNode::Term { token } => Ok(TermView {
                term: token.text(),
                field: token.field(),
            }),
            other => Err(ExtractionError::ExpectedVariant {
                expected: "Term",
                found: other.variant_name(),
            }),
        }
    }
}

impl<'a> TryFrom<&'a QueryNode> for BooleanView<'a> {
    type Error = ExtractionError;

    fn try_from(node: &'a QueryNode) -> Result<Self, Self::Error> {
        match node {
            QueryNode::And { children } => Ok(BooleanView {
                children,
                op: BooleanOp::And,
            }),
            QueryNode::Or { children } => Ok(BooleanView {
                children,
                op: BooleanOp::Or,
            }),
            other => Err(ExtractionError::ExpectedVariant {
                expected: "Boolean (And/Or)",
                found: other.variant_name(),
            }),
        }
    }
}

impl<'a> TryFrom<&'a QueryNode> for RangeView<'a> {
    type Error = ExtractionError;

    fn try_from(node: &'a QueryNode) -> Result<Self, Self::Error> {
        match node {
            QueryNode::Range {
                field,
                min,
                max,
                include_min,
                include_max,
            } => Ok(RangeView {
                field,
                min: min.as_deref(),
                max: max.as_deref(),
                include_min: *include_min,
                include_max: *include_max,
            }),
            other => Err(ExtractionError::ExpectedVariant {
                expected: "Range",
                found: other.variant_name(),
            }),
        }
    }
}

impl<'a> TryFrom<&'a QueryNode> for BoostView<'a> {
    type Error = ExtractionError;

    fn try_from(node: &'a QueryNode) -> Result<Self, Self::Error> {
        match node {
            QueryNode::Boost { child, boost } => Ok(BoostView {
                child: *child,
                score: *boost,
            }),
            QueryNode::ConstantScore { child, score } => Ok(BoostView {
                child: *child,
                score: *score,
            }),
            other => Err(ExtractionError::ExpectedVariant {
                expected: "Boost/ConstantScore",
                found: other.variant_name(),
            }),
        }
    }
}

impl<'a> TryFrom<&'a QueryNode> for ChildrenView<'a> {
    type Error = ExtractionError;

    fn try_from(node: &'a QueryNode) -> Result<Self, Self::Error> {
        match node {
            QueryNode::And { children } | QueryNode::Or { children } => {
                Ok(ChildrenView { children })
            }
            QueryNode::Not { child } => Ok(ChildrenView {
                children: std::slice::from_ref(child),
            }),
            QueryNode::Boost { child, .. } | QueryNode::ConstantScore { child, .. } => {
                Ok(ChildrenView {
                    children: std::slice::from_ref(child),
                })
            }
            other => Err(ExtractionError::ExpectedVariant {
                expected: "Node with children",
                found: other.variant_name(),
            }),
        }
    }
}

// Helper method on QueryNode to get variant name for error messages
impl QueryNode {
    fn variant_name(&self) -> &'static str {
        match self {
            QueryNode::Term { .. } => "Term",
            QueryNode::Phrase { .. } => "Phrase",
            QueryNode::And { .. } => "And",
            QueryNode::Or { .. } => "Or",
            QueryNode::Not { .. } => "Not",
            QueryNode::Boost { .. } => "Boost",
            QueryNode::ConstantScore { .. } => "ConstantScore",
            QueryNode::Range { .. } => "Range",
        }
    }
}
```

### Usage Examples

The `?` operator makes extraction clean and idiomatic:

```rust
// Direct extraction with ? operator
fn process_phrase(node: &QueryNode) -> Result<Vec<String>, ExtractionError> {
    let phrase: PhraseView = node.try_into()?;
    Ok(phrase.terms.to_vec())
}

// Pattern matching with if-let
fn analyze_node(node: &QueryNode) {
    if let Ok(term) = TermView::try_from(node) {
        println!("Term: {} (field: {:?})", term.term, term.field);
    } else if Ok(bool) = BooleanView::try_from(node) {
        println!("Boolean {:?} with {} children", bool.op, bool.children.len());
    }
}

// Chaining extractions with ?
fn extract_phrase_text(node: &QueryNode) -> Result<String, ExtractionError> {
    let phrase: PhraseView = node.try_into()?;
    let field = phrase.field.unwrap_or("_text");
    Ok(format!("{}:{}", field, phrase.terms.join(" ")))
}

// Iterating over children
fn visit_bool_children(node: &QueryNode, arena: &QueryArena) -> Result<usize, ExtractionError> {
    let bool_view: BooleanView = node.try_into()?;
    let mut count = 0;
    for &child_id in bool_view.children {
        let child = arena.get(child_id);
        count += visit_children(child, arena)?;
    }
    Ok(count)
}
```

### Generic Processing with TryFrom Bounds

Generic algorithms can use `TryFrom` bounds to work with any node-like type:

```rust
/// Extract all terms from a query tree using TryFrom bounds
fn extract_terms<N>(root: &N, arena: &QueryArena) -> Vec<String>
where
    for<'a> &'a N: TryInto<TermView<'a>, Error = ExtractionError>
        + TryInto<ChildrenView<'a>, Error = ExtractionError>,
{
    let mut terms = Vec::new();

    // Try to extract as a term
    if let Ok(term) = TermView::try_from(root) {
        terms.push(term.term.to_string());
    }

    // Try to extract children and recurse
    if let Ok(children) = ChildrenView::try_from(root) {
        for &child_id in children.children {
            let child = arena.get(child_id);
            terms.extend(extract_terms(child, arena));
        }
    }

    terms
}

/// Count phrase nodes in a tree
fn count_phrases<N>(root: &N, arena: &QueryArena) -> usize
where
    for<'a> &'a N: TryInto<PhraseView<'a>, Error = ExtractionError>
        + TryInto<ChildrenView<'a>, Error = ExtractionError>,
{
    let mut count = 0;

    if PhraseView::try_from(root).is_ok() {
        count += 1;
    }

    if let Ok(children) = ChildrenView::try_from(root) {
        for &child_id in children.children {
            let child = arena.get(child_id);
            count += count_phrases(child, arena);
        }
    }

    count
}

/// Validate that all range queries have both bounds
fn validate_ranges<N>(root: &N, arena: &QueryArena) -> Result<(), ExtractionError>
where
    for<'a> &'a N: TryInto<RangeView<'a>, Error = ExtractionError>
        + TryInto<ChildrenView<'a>, Error = ExtractionError>,
{
    // Check if this is a range node
    if let Ok(range) = RangeView::try_from(root) {
        if range.min.is_none() && range.max.is_none() {
            return Err(ExtractionError::MissingField("range bounds"));
        }
    }

    // Recurse into children
    if let Ok(children) = ChildrenView::try_from(root) {
        for &child_id in children.children {
            let child = arena.get(child_id);
            validate_ranges(child, arena)?;
        }
    }

    Ok(())
}
```

### Benefits of Standard Traits

Using `From` and `TryFrom` provides several advantages over custom traits:

1. **Ecosystem Compatibility**: Works seamlessly with standard library combinators (`Result::map`, `Option::ok_or`, etc.) and third-party libraries that expect standard traits.

2. **`?` Operator Support**: The `?` operator automatically uses `From::from` for error conversion, making error propagation natural.

3. **Clear Error Types**: `ExtractionError` provides detailed information about what went wrong, making debugging easier than unit-type errors.

4. **Zero-Cost Abstraction**: View types are newtype wrappers over borrowed references - they compile away and incur no runtime overhead.

5. **Discoverability**: Standard traits are well-documented and familiar to Rust developers, reducing learning curve compared to custom extraction traits.

6. **Extensibility**: External crates can implement `TryFrom` for their own types to work with your query nodes, enabling plugin architectures.

7. **Testability**: Extraction logic is isolated in trait implementations, making unit testing straightforward.

8. **Generic Constraints**: `TryFrom` bounds work naturally with Rust's trait system, enabling generic algorithms that operate on any extractable type.

## QueryProgram

The `QueryProgram` represents a complete logical query with its arena and root node.

```rust
/// Complete logical query representation
#[derive(Debug, Clone)]
pub struct QueryProgram {
    /// Arena storing all nodes
    arena: Arc<QueryArena>,
    /// Root node of the query
    root: QueryNodeId,
    /// Optional metadata
    metadata: QueryMetadata,
}

/// Metadata about a query program
#[derive(Debug, Clone, Default)]
pub struct QueryMetadata {
    /// Original query string (if from text)
    pub source: Option<String>,
    /// Query creation timestamp
    pub created_at: std::time::SystemTime,
    /// Hints for planning
    pub hints: QueryHints,
}

/// Hints to guide physical planning
#[derive(Debug, Clone, Default)]
pub struct QueryHints {
    /// Expected result set size
    pub expected_cardinality: Option<usize>,
    /// Timeout hint
    pub timeout_hint: Option<std::time::Duration>,
    /// Priority for this query
    pub priority: QueryPriority,
    /// User-provided scoring preferences
    pub scoring: ScoringHints,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueryPriority {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Debug, Clone, Default)]
pub struct ScoringHints {
    /// Prefer BM25 over TF-IDF
    pub prefer_bm25: bool,
    /// TF normalization parameter
    pub tf_normalization: Option<f32>,
}

impl QueryProgram {
    /// Create a new query program (typically via QueryBuilder or lowerer)
    pub fn new(arena: QueryArena, root: QueryNodeId) -> Self {
        Self {
            arena: Arc::new(arena),
            root,
            metadata: QueryMetadata::default(),
        }
    }

    // === Public API: Arena is an implementation detail ===

    /// Get the root node of the query
    pub fn root(&self) -> &QueryNode {
        self.arena.get(self.root).expect("invalid root node")
    }

    /// Iterate over all nodes in the query (post-order traversal)
    pub fn walk(&self) -> impl Iterator<Item = &QueryNode> {
        WalkIter::new(self)
    }

    /// Get children of a node by ID
    pub fn children_of(&self, id: QueryNodeId) -> &[QueryNodeId] {
        self.arena.get(id)
            .map(|node| node.children_slice())
            .unwrap_or(&[])
    }

    /// Get the total number of nodes in the query
    pub fn node_count(&self) -> usize {
        self.arena.node_count()
    }

    /// Validate structural integrity of the query
    pub fn validate(&self) -> Result<(), ValidationError> {
        // Check for cycles, invalid node references, etc.
        self.arena.validate()
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: QueryMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Get metadata
    pub fn metadata(&self) -> &QueryMetadata {
        &self.metadata
    }

    // === Internal API (for construction/lowering) ===

    /// Clone with a different root (for partial queries or transformations)
    pub(crate) fn with_root(&self, root: QueryNodeId) -> Self {
        Self {
            arena: Arc::clone(&self.arena),
            root,
            metadata: self.metadata.clone(),
        }
    }

    /// Access the arena (internal use only - for lowerers/planners)
    pub(crate) fn arena(&self) -> &QueryArena {
        &self.arena
    }
}

/// Iterator for walking query nodes
pub struct WalkIter<'a> {
    program: &'a QueryProgram,
    stack: Vec<QueryNodeId>,
    visited: std::collections::HashSet<QueryNodeId>,
}

impl<'a> WalkIter<'a> {
    fn new(program: &'a QueryProgram) -> Self {
        Self {
            program,
            stack: vec![program.root],
            visited: std::collections::HashSet::new(),
        }
    }
}

impl<'a> Iterator for WalkIter<'a> {
    type Item = &'a QueryNode;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(id) = self.stack.pop() {
            if self.visited.contains(&id) {
                continue;
            }

            let node = self.program.arena.get(id)?;
            self.visited.insert(id);

            // Push children first
            for child in node.children_slice() {
                if !self.visited.contains(child) {
                    self.stack.push(*child);
                }
            }

            // Then process this node
            return Some(node);
        }
        None
    }
}

/// Validation error type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Cycle detected in query graph
    Cycle { node: QueryNodeId },
    /// Invalid node reference
    InvalidReference { node: QueryNodeId },
    /// Empty query
    EmptyQuery,
}
```

### Zero-Cost Composition

Because `QueryProgram` uses `Arc` for the arena, creating variations is cheap:

```rust
// Base query: "rust programming"
let base = QueryProgram::new(arena, rust_and_programming);

// Enhanced: "rust programming" AND ("language" OR "system")
let enhanced = QueryProgram::new(
    Arc::clone(&base.arena),
    and(vec![base.root, language_or_system]),
);

// Partial: Just the "programming" part
let partial = base.with_root(programming_node_id);

// All share the same arena allocation
```

## Lowering Pipeline

Lowering converts syntax-specific parse trees into the logical `QueryProgram`. This is where grammar-level constructs are mapped to the generic AST.

### Lowering Context

```rust
/// Context for lowering parse trees to QueryProgram
pub struct LoweringContext<'a> {
    /// Builder for the output arena
    builder: QueryBuilder,
    /// Token resolver for term resolution
    resolver: &'a dyn TokenResolver,
    /// Configuration options
    options: LoweringOptions,
    /// Error collection
    errors: Vec<LoweringError>,
}

/// Options for the lowering process
#[derive(Debug, Clone, Default)]
pub struct LoweringOptions {
    /// Whether to eagerly resolve tokens during lowering
    pub eager_resolution: bool,
    /// Whether to preserve original parse tree metadata
    pub preserve_metadata: bool,
    /// Maximum depth for nested queries
    pub max_depth: usize,
    /// How to handle unsupported constructs
    pub unsupported_mode: UnsupportedMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedMode {
    /// Return an error
    Error,
    /// Skip the unsupported node
    Skip,
    /// Replace with a MatchAll node
    MatchAll,
}

/// Errors during lowering
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweringError {
    /// Syntax error in the input
    Syntax { message: String, offset: usize },
    
    /// Token resolution failed
    Resolution { token: Token, error: ResolutionError },
    
    /// Query depth exceeded
    DepthExceeded { max: usize, actual: usize },
    
    /// Unsupported construct
    Unsupported { construct: String, suggestion: Option<String> },
}

impl<'a> LoweringContext<'a> {
    /// Create a new lowering context
    pub fn new(resolver: &'a dyn TokenResolver) -> Self {
        Self {
            builder: QueryBuilder::new(),
            resolver,
            options: LoweringOptions::default(),
            errors: Vec::new(),
        }
    }

    /// Set options
    pub fn with_options(mut self, options: LoweringOptions) -> Self {
        self.options = options;
        self
    }

    /// Lower a parse tree to a QueryProgram
    pub fn lower(&mut self, parse: &ParseTree) -> Result<QueryProgram, LoweringError> {
        let root = self.lower_node(parse)?;
        let arena = self.builder.clone().into_arena();
        
        Ok(QueryProgram::new(arena, root))
    }

    /// Lower a single parse node
    fn lower_node(&mut self, node: &ParseNode) -> Result<QueryNodeId, LoweringError> {
        match node {
            ParseNode::Term { text } => {
                let token = Token::text(text);
                self.lower_token(token)
            }
            
            ParseNode::Phrase { terms } => {
                let token = Token::phrase(terms.join(" "));
                self.lower_token(token)
            }
            
            ParseNode::And { children } => {
                let lowered: Result<Vec<_>, _> = children
                    .iter()
                    .map(|c| self.lower_node(c))
                    .collect();
                Ok(self.builder.and(lowered?))
            }
            
            ParseNode::Or { children } => {
                let lowered: Result<Vec<_>, _> = children
                    .iter()
                    .map(|c| self.lower_node(c))
                    .collect();
                Ok(self.builder.or(lowered?))
            }
            
            ParseNode::Not { child } => {
                let lowered = self.lower_node(child)?;
                Ok(self.builder.not(lowered))
            }
            
            ParseNode::Boost { child, factor } => {
                let lowered = self.lower_node(child)?;
                Ok(self.builder.boost(lowered, *factor))
            }
            
            ParseNode::Field { field, value } => {
                self.lower_field(field, value)
            }
            
            ParseNode::Range { field, min, max } => {
                Ok(self.builder.node(QueryNode::range(
                    field.clone(),
                    min.clone(),
                    max.clone(),
                )))
            }
            
            _ => Err(LoweringError::Unsupported {
                construct: format!("{:?}", node),
                suggestion: None,
            }),
        }
    }

    /// Lower a token, optionally resolving it
    fn lower_token(&mut self, token: Token) -> Result<QueryNodeId, LoweringError> {
        let term_id = if self.options.eager_resolution {
            Some(self.resolve_token(&token)?)
        } else {
            None
        };

        Ok(self.builder.node(QueryNode::Term { token, term_id }))
    }

    /// Lower a field-scoped query
    fn lower_field(&mut self, field: &str, value: &ParseNode) -> Result<QueryNodeId, LoweringError> {
        match value {
            ParseNode::Term { text } => {
                let token = Token::field(field, Token::text(text));
                self.lower_token(token)
            }
            
            ParseNode::Phrase { terms } => {
                let token = Token::field(field, Token::phrase(terms.join(" ")));
                self.lower_token(token)
            }
            
            // For complex field queries, wrap the child
            child => {
                let lowered = self.lower_node(child)?;
                Ok(self.builder.node(QueryNode::Boost {
                    child: lowered,
                    boost: 1.0, // Could use field-specific boost
                }))
            }
        }
    }

    /// Resolve a token to a TermId
    fn resolve_token(&self, token: &Token) -> Result<TermId, LoweringError> {
        self.resolver
            .resolve_token(token)
            .map_err(|e| LoweringError::Resolution {
                token: token.clone(),
                error: e,
            })
    }

    /// Check for errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get collected errors
    pub fn errors(&self) -> &[LoweringError] {
        &self.errors
    }
}
```

### Parse Tree Representation

```rust
/// Syntax-independent parse tree node
#[derive(Debug, Clone, PartialEq)]
pub enum ParseNode {
    Term { text: String },
    Phrase { terms: Vec<String> },
    And { children: Vec<ParseNode> },
    Or { children: Vec<ParseNode> },
    Not { child: Box<ParseNode> },
    Boost { child: Box<ParseNode>, factor: f32 },
    Field { field: String, value: Box<ParseNode> },
    Range { field: String, min: Bound<String>, max: Bound<String> },
    Wildcard { pattern: String },
    Prefix { prefix: String },
    Fuzzy { term: String, distance: u32 },
    Group { child: Box<ParseNode> },
}

/// Complete parse tree
#[derive(Debug, Clone)]
pub struct ParseTree {
    pub root: ParseNode,
    pub source: Option<String>,
}
```

### Lowering Examples

```rust
// Example: Lowering a simple query
let resolver = DictionaryTokenResolver { dict: &dictionary };
let mut ctx = LoweringContext::new(&resolver);

// Parse tree for: "rust AND programming"
let parse = ParseTree {
    root: ParseNode::And {
        children: vec![
            ParseNode::Term { text: "rust".into() },
            ParseNode::Term { text: "programming".into() },
        ],
    },
    source: Some("rust AND programming".into()),
};

let program = ctx.lower(&parse)?;
// program.root is an And node with two Term children

// Example: Lowering a field-scoped query
let parse = ParseTree {
    root: ParseNode::Field {
        field: "title".into(),
        value: Box::new(ParseNode::Term {
            text: "rust".into(),
        }),
    },
    source: Some("title:rust".into()),
};

let program = ctx.lower(&parse)?;
// program.root is a Term node with token Token::Field { field: "title", token: Text("rust") }

// Example: Lowering a boosted phrase
let parse = ParseTree {
    root: ParseNode::Boost {
        child: Box::new(ParseNode::Phrase {
            terms: vec!["query".into(), "planning".into()],
        }),
        factor: 2.0,
    },
    source: Some("\"query planning\"^2".into()),
};

let program = ctx.lower(&parse)?;
// program.root is a Boost node with child Phrase and boost 2.0
```

## ExecutionPlan

The execution plan represents the physical operator graph that will be executed. It's separate from the logical query to allow for algorithm selection and optimization.

### Physical Operator Nodes

```rust
/// Physical operator for query execution
#[derive(Debug, Clone)]
pub enum PhysicalOperator {
    /// Leaf operator scanning a term's postings
    TermScan {
        /// Resolved term ID
        term_id: TermId,
        /// Scoring configuration
        scorer: ScorerConfig,
    },

    /// Leaf operator scanning a phrase
    PhraseScan {
        /// Phrase terms
        terms: Vec<TermId>,
        /// Sloppy phrase distance
        slop: u32,
        /// Scoring configuration
        scorer: ScorerConfig,
    },

    /// Leaf operator scanning a range
    RangeScan {
        /// Field and bounds
        field: String,
        min: Bound<String>,
        max: Bound<String>,
        /// Scoring configuration
        scorer: ScorerConfig,
    },

    /// Boolean AND intersection
    Intersect {
        /// Child operators
        children: Vec<PhysicalOperatorId>,
        /// Intersection algorithm
        algorithm: IntersectionAlgorithm,
    },

    /// Boolean OR union
    Union {
        /// Child operators
        children: Vec<PhysicalOperatorId>,
        /// Union algorithm
        algorithm: UnionAlgorithm,
    },

    /// Boolean NOT exclusion
    Exclude {
        /// Positive set
        include: PhysicalOperatorId,
        /// Negative set
        exclude: PhysicalOperatorId,
    },

    /// Constant score assignment
    ConstantScore {
        /// Child operator
        child: PhysicalOperatorId,
        /// Score to assign
        score: f32,
    },

    /// Score boost
    Boost {
        /// Child operator
        child: PhysicalOperatorId,
        /// Boost multiplier
        boost: f32,
    },

    /// Top-K collector
    TopK {
        /// Child operator
        child: PhysicalOperatorId,
        /// Number of results to collect
        k: usize,
    },
}

/// Algorithms for boolean intersection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntersectionAlgorithm {
    /// Skip-pointer intersection (good for sparse sets)
    SkipPointers,
    /// Bitmap intersection (good for dense sets)
    Bitmap,
    /// Galloping intersection (good for very unbalanced sets)
    Galloping,
    /// Adaptive (choose based on set characteristics)
    Adaptive,
}

/// Algorithms for boolean union
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnionAlgorithm {
    /// Heap-based merge
    Heap,
    /// Simple append + dedupe (good for disjoint sets)
    Append,
    /// Adaptive (choose based on set characteristics)
    Adaptive,
}

/// Scoring configuration
#[derive(Debug, Clone, PartialEq)]
pub struct ScorerConfig {
    /// Scoring algorithm
    pub algorithm: ScoringAlgorithm,
    /// Optional term boost
    pub boost: f32,
}

/// Scoring algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoringAlgorithm {
    /// TF-IDF scoring
    TfIdf,
    /// BM25 scoring
    BM25 { k1: f32, b: f32 },
    /// Constant score
    Constant,
}

impl PhysicalOperator {
    /// Create a term scan operator
    pub fn term_scan(term_id: TermId) -> Self {
        Self::TermScan {
            term_id,
            scorer: ScorerConfig::default(),
        }
    }

    /// Check if this is a leaf operator
    pub fn is_leaf(&self) -> bool {
        matches!(
            self,
            Self::TermScan { .. } | Self::PhraseScan { .. } | Self::RangeScan { .. }
        )
    }

    /// Get child operators as a slice
    pub fn children_slice(&self) -> &[PhysicalOperatorId] {
        match self {
            Self::Intersect { children, .. } | Self::Union { children, .. } => children.as_slice(),
            Self::Exclude { include, exclude } => {
                // Note: exclude is a special case with two children
                // We return include as the primary child
                std::slice::from_ref(include)
            }
            Self::ConstantScore { child, .. } | Self::Boost { child, .. } | Self::TopK { child, .. } => {
                std::slice::from_ref(child)
            }
            Self::TermScan { .. } | Self::PhraseScan { .. } | Self::RangeScan { .. } => &[],
        }
    }

    /// Get child operators (for internal use)
    pub(crate) fn children(&self) -> Vec<PhysicalOperatorId> {
        self.children_slice().to_vec()
    }
}
```

### Execution with ScratchSpace

Physical operators are executed using **Pattern 2: ScratchSpace/Workspace** for temporary allocations. This keeps the arena-based plan structure immutable while providing efficient mutable buffers during execution.

```rust
use leit_core::ScratchSpace;

/// Executor for physical operators
pub trait PhysicalOperatorExecutor {
    /// Execute the operator using scratch space for temporary allocations
    fn execute(
        &self,
        scratch: &mut dyn ScratchSpace,
    ) -> Result<Cursor, ExecutionError>;
}

/// Result cursor from executing a physical operator
pub struct Cursor {
    /// Matching document IDs
    pub doc_ids: Vec<u64>,
    /// Scores for each document
    pub scores: Vec<f32>,
}

/// Execution errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionError {
    /// Scratch space allocation failed
    AllocationFailed,
    /// Postings list not found
    PostingsNotFound { term_id: TermId },
    /// Invalid operator state
    InvalidState,
}

/// Example: Term scan executor
pub struct TermScanExecutor {
    term_id: TermId,
    scorer: ScorerConfig,
}

impl PhysicalOperatorExecutor for TermScanExecutor {
    fn execute(&self, scratch: &mut dyn ScratchSpace) -> Result<Cursor, ExecutionError> {
        // Use scratch space for temporary score buffer
        let mut scores = scratch.alloc_vec::<f32>(1024)
            .map_err(|_| ExecutionError::AllocationFailed)?;

        // Simulate fetching postings and computing scores
        // In real implementation, this would read from index
        scores.push(0.85);
        scores.push(0.72);

        let doc_ids = vec![1, 5];

        // Scratch space will be reset after this operation
        Ok(Cursor { doc_ids, scores })
    }
}

/// Example: Intersection executor
pub struct IntersectExecutor {
    children: Vec<Box<dyn PhysicalOperatorExecutor>>,
    algorithm: IntersectionAlgorithm,
}

impl PhysicalOperatorExecutor for IntersectExecutor {
    fn execute(&self, scratch: &mut dyn ScratchSpace) -> Result<Cursor, ExecutionError> {
        // Execute all children
        let mut child_results = Vec::with_capacity(self.children.len());
        for child in &self.children {
            let result = child.execute(scratch)?;
            child_results.push(result);
        }

        // Use scratch space for intersection buffers
        let mut result_buffer = scratch.alloc_vec::<u64>(1024)
            .map_err(|_| ExecutionError::AllocationFailed)?;

        match self.algorithm {
            IntersectionAlgorithm::SkipPointers => {
                // Use result_buffer for skip-pointer intersection
                // Scratch space gets reused for each intersection operation
                result_buffer.push(42);
            }
            IntersectionAlgorithm::Bitmap => {
                // For bitmap intersection, might alloc bytes instead
                let mut bitmap = scratch.alloc_bytes(4096)
                    .map_err(|_| ExecutionError::AllocationFailed)?;
                // Perform bitmap intersection...
            }
            _ => unimplemented!(),
        }

        Ok(Cursor {
            doc_ids: result_buffer,
            scores: vec![1.0],
        })
    }
}
```

**Key points about ScratchSpace execution:**
- The `ExecutionPlan` (Arc<PhysicalOperatorArena>) remains immutable and can be shared
- Each execution gets its own `&mut dyn ScratchSpace` for temporary buffers
- ScratchSpace is `reset()` between operations, preserving capacity for reuse
- No Arc or sharing needed during execution — single mutable reference

### Physical Plan Structure

```rust
/// Complete execution plan
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Arena storing all operators
    operators: Arc<PhysicalOperatorArena>,
    /// Root operator
    root: PhysicalOperatorId,
    /// Planning metadata
    metadata: ExecutionMetadata,
}

/// Arena for physical operators
#[derive(Debug, Clone)]
pub struct PhysicalOperatorArena {
    operators: Vec<PhysicalOperator>,
}

impl PhysicalOperatorArena {
    pub fn new() -> Self {
        Self {
            operators: Vec::new(),
        }
    }

    pub fn alloc(&mut self, op: PhysicalOperator) -> PhysicalOperatorId {
        let id = PhysicalOperatorId::new(self.operators.len());
        self.operators.push(op);
        id
    }

    pub fn get(&self, id: PhysicalOperatorId) -> Option<&PhysicalOperator> {
        self.operators.get(id.index())
    }

    pub fn get_mut(&mut self, id: PhysicalOperatorId) -> Option<&mut PhysicalOperator> {
        self.operators.get_mut(id.index())
    }

    pub fn operator_count(&self) -> usize {
        self.operators.len()
    }
}

/// Stable identifier for a physical operator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhysicalOperatorId {
    index: u32,
}

impl PhysicalOperatorId {
    pub fn new(index: usize) -> Self {
        Self {
            index: index as u32,
        }
    }

    pub fn index(self) -> usize {
        self.index as usize
    }
}

/// Execution metadata
#[derive(Debug, Clone)]
pub struct ExecutionMetadata {
    /// Estimated cost
    pub estimated_cost: f64,
    /// Estimated cardinality
    pub estimated_cardinality: usize,
    /// Planning algorithm used
    pub planner: String,
    /// Planning duration
    pub planning_duration: std::time::Duration,
}

impl ExecutionPlan {
    /// Create a new execution plan (typically via PhysicalPlanner)
    pub fn new(operators: PhysicalOperatorArena, root: PhysicalOperatorId) -> Self {
        Self {
            operators: Arc::new(operators),
            root,
            metadata: ExecutionMetadata {
                estimated_cost: 0.0,
                estimated_cardinality: 0,
                planner: "unknown".into(),
                planning_duration: std::time::Duration::ZERO,
            },
        }
    }

    // === Public API: Arena is an implementation detail ===

    /// Get the root operator of the execution plan
    pub fn root(&self) -> &PhysicalOperator {
        self.operators.get(self.root).expect("invalid root operator")
    }

    /// Iterate over all operators in the plan
    pub fn walk(&self) -> impl Iterator<Item = &PhysicalOperator> {
        OperatorWalkIter::new(self)
    }

    /// Get children of an operator by ID
    pub fn children_of(&self, id: PhysicalOperatorId) -> &[PhysicalOperatorId] {
        self.operators.get(id)
            .map(|op| op.children_slice())
            .unwrap_or(&[])
    }

    /// Get the total number of operators in the plan
    pub fn operator_count(&self) -> usize {
        self.operators.operator_count()
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: ExecutionMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Get metadata
    pub fn metadata(&self) -> &ExecutionMetadata {
        &self.metadata
    }

    /// Explain the plan
    pub fn explain(&self) -> ExecutionPlanExplanation {
        ExecutionPlanExplanation::from_plan(self)
    }

    // === Internal API (for physical planners/executors) ===

    /// Access the operator arena (internal use only)
    pub(crate) fn operators(&self) -> &PhysicalOperatorArena {
        &self.operators
    }
}

/// Iterator for walking physical operators
pub struct OperatorWalkIter<'a> {
    plan: &'a ExecutionPlan,
    stack: Vec<PhysicalOperatorId>,
    visited: std::collections::HashSet<PhysicalOperatorId>,
}

impl<'a> OperatorWalkIter<'a> {
    fn new(plan: &'a ExecutionPlan) -> Self {
        Self {
            plan,
            stack: vec![plan.root],
            visited: std::collections::HashSet::new(),
        }
    }
}

impl<'a> Iterator for OperatorWalkIter<'a> {
    type Item = &'a PhysicalOperator;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(id) = self.stack.pop() {
            if self.visited.contains(&id) {
                continue;
            }

            let op = self.plan.operators.get(id)?;
            self.visited.insert(id);

            // Push children first
            for child in op.children_slice() {
                if !self.visited.contains(child) {
                    self.stack.push(*child);
                }
            }

            // Then process this operator
            return Some(op);
        }
        None
    }
}

/// Human-readable explanation of an execution plan
#[derive(Debug, Clone)]
pub struct ExecutionPlanExplanation {
    pub text: String,
}

impl ExecutionPlanExplanation {
    pub fn from_plan(plan: &ExecutionPlan) -> Self {
        let mut text = String::new();
        Self::explain_operator(plan, plan.root, 0, &mut text);
        Self { text }
    }

    fn explain_operator(
        plan: &ExecutionPlan,
        op_id: PhysicalOperatorId,
        indent: usize,
        text: &mut String,
    ) {
        let indent_str = "  ".repeat(indent);
        let op = plan.operators.get(op_id).unwrap();

        match op {
            PhysicalOperator::TermScan { term_id, .. } => {
                writeln!(text, "{}TermScan({})", indent_str, term_id).unwrap();
            }
            PhysicalOperator::Intersect {
                children,
                algorithm,
            } => {
                writeln!(text, "{}Intersect({:?})", indent_str, algorithm).unwrap();
                for child in children {
                    Self::explain_operator(plan, *child, indent + 1, text);
                }
            }
            PhysicalOperator::Union { children, algorithm } => {
                writeln!(text, "{}Union({:?})", indent_str, algorithm).unwrap();
                for child in children {
                    Self::explain_operator(plan, *child, indent + 1, text);
                }
            }
            _ => {
                writeln!(text, "{}{:?}", indent_str, op).unwrap();
            }
        }
    }
}

impl std::fmt::Display for ExecutionPlanExplanation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}
```

## Physical Planning

Physical planning converts a `QueryProgram` into an `ExecutionPlan` by selecting algorithms and binding operators.

### Physical Planner Trait

```rust
/// Trait for converting logical queries to execution plans
pub trait PhysicalPlanner {
    /// Plan a query program
    fn plan(&self, program: &QueryProgram) -> Result<ExecutionPlan, PlanningError>;

    /// Plan with additional context
    fn plan_with_context(
        &self,
        program: &QueryProgram,
        context: &PlanningContext,
    ) -> Result<ExecutionPlan, PlanningError>;
}

/// Context for physical planning
pub struct PlanningContext<'a> {
    /// Segment statistics
    pub segment_stats: &'a SegmentStats,
    /// Index statistics
    pub index_stats: &'a IndexStats,
    /// Runtime configuration
    pub config: &'a PlannerConfig,
}

/// Statistics about a segment
#[derive(Debug, Clone)]
pub struct SegmentStats {
    pub document_count: u64,
    pub total_terms: u64,
    pub avg_doc_length: f64,
}

/// Statistics about the index
#[derive(Debug, Clone)]
pub struct IndexStats {
    pub term_counts: std::collections::HashMap<String, u64>,
    pub field_counts: std::collections::HashMap<String, u64>,
}

/// Configuration for planning
#[derive(Debug, Clone)]
pub struct PlannerConfig {
    /// Default intersection algorithm
    pub default_intersection: IntersectionAlgorithm,
    /// Default union algorithm
    pub default_union: UnionAlgorithm,
    /// Whether to use cost-based optimization
    pub use_cost_based: bool,
    /// Maximum branches for adaptive algorithms
    pub max_adaptive_branches: usize,
}

/// Errors during planning
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanningError {
    /// Unresolved token
    UnresolvedToken { token: Token },
    
    /// Unsupported query node
    UnsupportedNode { node: QueryNode },
    
    /// Cycle in the query graph
    Cycle { node: QueryNodeId },
    
    /// Planning timeout
    Timeout,
}
```

### Default Physical Planner

```rust
/// Default physical planner implementation
pub struct DefaultPhysicalPlanner {
    config: PlannerConfig,
    resolver: Box<dyn TokenResolver>,
}

impl DefaultPhysicalPlanner {
    /// Create a new planner
    pub fn new(resolver: Box<dyn TokenResolver>) -> Self {
        Self {
            config: PlannerConfig::default(),
            resolver,
        }
    }

    /// Set configuration
    pub fn with_config(mut self, config: PlannerConfig) -> Self {
        self.config = config;
        self
    }

    /// Plan a query program
    pub fn plan_program(
        &self,
        program: &QueryProgram,
    ) -> Result<ExecutionPlan, PlanningError> {
        let mut arena = PhysicalOperatorArena::new();
        let root = self.plan_node(program, program.root(), &mut arena)?;
        
        Ok(ExecutionPlan::new(arena, root))
    }

    /// Plan a single query node
    fn plan_node(
        &self,
        program: &QueryProgram,
        node_id: QueryNodeId,
        arena: &mut PhysicalOperatorArena,
    ) -> Result<PhysicalOperatorId, PlanningError> {
        // Use internal API to access arena for planning
        let node = program
            .arena()
            .get(node_id)
            .ok_or_else(|| PlanningError::Cycle { node: node_id })?;

        match node {
            QueryNode::Term { token, .. } => {
                let term_id = self.resolve_token(token)?;
                Ok(arena.alloc(PhysicalOperator::term_scan(term_id)))
            }

            QueryNode::And { children } => {
                let planned: Result<Vec<_>, _> = children
                    .iter()
                    .map(|&child| self.plan_node(program, child, arena))
                    .collect();

                let algorithm = self.select_intersection_algorithm(program, children);
                Ok(arena.alloc(PhysicalOperator::Intersect {
                    children: planned?,
                    algorithm,
                }))
            }

            QueryNode::Or { children } => {
                let planned: Result<Vec<_>, _> = children
                    .iter()
                    .map(|&child| self.plan_node(program, child, arena))
                    .collect();

                let algorithm = self.select_union_algorithm(program, children);
                Ok(arena.alloc(PhysicalOperator::Union {
                    children: planned?,
                    algorithm,
                }))
            }

            QueryNode::Not { child } => {
                let planned = self.plan_node(program, *child, arena)?;
                // For NOT, we need to handle it at execution time
                // This is a simplified version
                Ok(arena.alloc(PhysicalOperator::Exclude {
                    include: arena.alloc(PhysicalOperator::TermScan {
                        term_id: TermId::match_all(),
                        scorer: ScorerConfig::default(),
                    }),
                    exclude: planned,
                }))
            }

            QueryNode::Boost { child, boost } => {
                let planned = self.plan_node(program, *child, arena)?;
                Ok(arena.alloc(PhysicalOperator::Boost {
                    child: planned,
                    boost: *boost,
                }))
            }

            QueryNode::ConstantScore { child, score } => {
                let planned = self.plan_node(program, *child, arena)?;
                Ok(arena.alloc(PhysicalOperator::ConstantScore {
                    child: planned,
                    score: *score,
                }))
            }

            QueryNode::Phrase { terms, .. } => {
                let term_ids: Result<Vec<_>, _> = terms
                    .iter()
                    .map(|t| self.resolve_token(&Token::text(t)))
                    .collect();
                
                Ok(arena.alloc(PhysicalOperator::PhraseScan {
                    terms: term_ids?,
                    slop: 0,
                    scorer: ScorerConfig::default(),
                }))
            }

            QueryNode::Range { field, min, max } => {
                Ok(arena.alloc(PhysicalOperator::RangeScan {
                    field: field.clone(),
                    min: min.clone(),
                    max: max.clone(),
                    scorer: ScorerConfig::default(),
                }))
            }

            _ => Err(PlanningError::UnsupportedNode {
                node: node.clone(),
            }),
        }
    }

    /// Resolve a token to a TermId
    fn resolve_token(&self, token: &Token) -> Result<TermId, PlanningError> {
        self.resolver
            .resolve_token(token)
            .map_err(|_| PlanningError::UnresolvedToken { token: token.clone() })
    }

    /// Select intersection algorithm based on query characteristics
    fn select_intersection_algorithm(
        &self,
        program: &QueryProgram,
        children: &[QueryNodeId],
    ) -> IntersectionAlgorithm {
        if !self.config.use_cost_based {
            return self.config.default_intersection;
        }

        // Simple heuristic: use adaptive for 3+ children
        if children.len() >= 3 {
            IntersectionAlgorithm::Adaptive
        } else {
            self.config.default_intersection
        }
    }

    /// Select union algorithm based on query characteristics
    fn select_union_algorithm(
        &self,
        program: &QueryProgram,
        children: &[QueryNodeId],
    ) -> UnionAlgorithm {
        if !self.config.use_cost_based {
            return self.config.default_union;
        }

        // Simple heuristic: use heap for 2+ children
        if children.len() >= 2 {
            UnionAlgorithm::Heap
        } else {
            self.config.default_union
        }
    }
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            default_intersection: IntersectionAlgorithm::SkipPointers,
            default_union: UnionAlgorithm::Heap,
            use_cost_based: true,
            max_adaptive_branches: 10,
        }
    }
}

impl Default for ScorerConfig {
    fn default() -> Self {
        Self {
            algorithm: ScoringAlgorithm::BM25 { k1: 1.2, b: 0.75 },
            boost: 1.0,
        }
    }
}
```

## Extension Points

The architecture provides several extension points for customization:

### Custom Optimizers

```rust
/// Trait for query optimization passes
pub trait QueryOptimizer {
    /// Optimize a query program in-place
    fn optimize(&self, program: &mut QueryProgram) -> Result<(), OptimizationError>;
}

/// Example: Constant folding optimizer
pub struct ConstantFolder;

impl QueryOptimizer for ConstantFolder {
    fn optimize(&self, program: &mut QueryProgram) -> Result<(), OptimizationError> {
        // Simplify constant expressions
        // e.g., (AND A A) -> A
        Ok(())
    }
}

/// Example: Query simplification
pub struct QuerySimplifier;

impl QueryOptimizer for QuerySimplifier {
    fn optimize(&self, program: &mut QueryProgram) -> Result<(), OptimizationError> {
        // Remove redundant NOT nodes
        // Flatten nested AND/OR nodes
        Ok(())
    }
}

/// Optimizer pipeline
pub struct OptimizerPipeline {
    optimizers: Vec<Box<dyn QueryOptimizer>>,
}

impl OptimizerPipeline {
    pub fn new() -> Self {
        Self {
            optimizers: Vec::new(),
        }
    }

    pub fn add(mut self, optimizer: Box<dyn QueryOptimizer>) -> Self {
        self.optimizers.push(optimizer);
        self
    }

    pub fn optimize(&self, program: &mut QueryProgram) -> Result<(), OptimizationError> {
        for optimizer in &self.optimizers {
            optimizer.optimize(program)?;
        }
        Ok(())
    }
}

impl Default for OptimizerPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Optimization errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizationError {
    /// Cycle detected during optimization
    Cycle { node: QueryNodeId },
    /// Optimization pass failed
    PassFailed { pass: String, reason: String },
}
```

### Custom Lowerers

```rust
/// Trait for syntax-specific lowering
pub trait QueryLowerer {
    /// Lower a parse tree to a query program
    fn lower(&self, parse: &ParseTree) -> Result<QueryProgram, LoweringError>;
}

/// Example: Lucene-style query lowerer
pub struct LuceneQueryLowerer<'a> {
    resolver: &'a dyn TokenResolver,
    options: LoweringOptions,
}

impl<'a> QueryLowerer for LuceneQueryLowerer<'a> {
    fn lower(&self, parse: &ParseTree) -> Result<QueryProgram, LoweringError> {
        let mut ctx = LoweringContext::new(self.resolver).with_options(self.options.clone());
        ctx.lower(parse)
    }
}

/// Example: Custom DSL lowerer
pub struct CustomDslLowerer<'a> {
    resolver: &'a dyn TokenResolver,
}

impl<'a> QueryLowerer for CustomDslLowerer<'a> {
    fn lower(&self, parse: &ParseTree) -> Result<QueryProgram, LoweringError> {
        // Custom lowering logic
        let mut ctx = LoweringContext::new(self.resolver);
        ctx.lower(parse)
    }
}
```

### Custom Physical Planners

```rust
/// Example: Cost-based planner
pub struct CostBasedPlanner {
    resolver: Box<dyn TokenResolver>,
    cost_model: Box<dyn CostModel>,
}

impl CostBasedPlanner {
    pub fn new(resolver: Box<dyn TokenResolver>, cost_model: Box<dyn CostModel>) -> Self {
        Self {
            resolver,
            cost_model,
        }
    }
}

impl PhysicalPlanner for CostBasedPlanner {
    fn plan(&self, program: &QueryProgram) -> Result<ExecutionPlan, PlanningError> {
        // Use cost model to select algorithms
        Ok(ExecutionPlan::new(PhysicalOperatorArena::new(), PhysicalOperatorId::new(0)))
    }

    fn plan_with_context(
        &self,
        program: &QueryProgram,
        context: &PlanningContext,
    ) -> Result<ExecutionPlan, PlanningError> {
        // Use context statistics for cost estimation
        Ok(ExecutionPlan::new(PhysicalOperatorArena::new(), PhysicalOperatorId::new(0)))
    }
}

/// Cost model for planning
pub trait CostModel {
    /// Estimate cost of an operator
    fn estimate_cost(&self, op: &PhysicalOperator) -> f64;
    
    /// Estimate cardinality of an operator
    fn estimate_cardinality(&self, op: &PhysicalOperator) -> usize;
}

/// Example: Simple cost model
pub struct SimpleCostModel;

impl CostModel for SimpleCostModel {
    fn estimate_cost(&self, op: &PhysicalOperator) -> f64 {
        match op {
            PhysicalOperator::TermScan { .. } => 1.0,
            PhysicalOperator::Intersect { children, .. } => {
                children.len() as f64 * 0.5
            }
            PhysicalOperator::Union { children, .. } => {
                children.len() as f64 * 0.3
            }
            _ => 1.0,
        }
    }

    fn estimate_cardinality(&self, op: &PhysicalOperator) -> usize {
        // Simple heuristic
        100
    }
}
```

## Usage Examples

### End-to-End Example

This example shows both allocation patterns in action:

```rust
use leit_query::*;
use leit_core::ScratchSpace;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // === PATTERN 1: Shared Immutable Arena (query structure) ===

    // 1. Parse the query
    let parse_tree = parse_query("rust AND programming")?;

    // 2. Lower to logical query (arena is internal implementation detail)
    let resolver = DictionaryTokenResolver { dict: &dictionary };
    let lowerer = LuceneQueryLowerer::new(&resolver);
    let program = lowerer.lower(&parse_tree)?;
    // program uses Arc<QueryArena> internally — cheap to clone/share

    // 3. Use the public API to inspect the query
    println!("Query has {} nodes", program.node_count());
    for node in program.walk() {
        // Process each node without touching the arena directly
        println!("Node: {:?}", node);
    }

    // 4. Validate query structure
    program.validate()?;

    // 5. Optimize (optional) — creates new arena nodes internally
    let optimizer = OptimizerPipeline::default()
        .add(Box::new(ConstantFolder))
        .add(Box::new(QuerySimplifier));
    let mut optimized_program = program.clone();  // Cheap Arc clone
    optimizer.optimize(&mut optimized_program)?;

    // 6. Physical planning (creates PhysicalOperatorArena internally)
    let planner = DefaultPhysicalPlanner::new(Box::new(resolver));
    let execution_plan = planner.plan(&optimized_program)?;
    // execution_plan uses Arc<PhysicalOperatorArena> internally — immutable, shareable

    // 7. Use the public API to inspect the execution plan
    println!("Execution plan has {} operators", execution_plan.operator_count());
    for op in execution_plan.walk() {
        println!("Operator: {:?}", op);
    }

    // === PATTERN 2: ScratchSpace/Workspace (execution temps) ===

    // 8. Execute using scratch space for temporary allocations
    let mut scratch = leit_core::DefaultScratchSpace::new();
    let results = execution_plan.root_executor().execute(&mut scratch)?;

    // Scratch space is reset/reused for next query
    scratch.reset();

    println!("Found {} results", results.len());
    Ok(())
}
```

**Key distinctions in this example:**
- `program` and `execution_plan` use Arc-wrapped arenas internally — immutable, cheap to clone, shared across threads
- Public API methods (`walk()`, `node_count()`, `validate()`, etc.) hide the arena implementation
- `scratch` is mutable, temporary, single-owner — used only during execution, reset between queries

### Complex Query Example

```rust
// Query: (title:"query planning" OR body:architecture) AND language:rust
fn build_complex_query() -> Result<QueryProgram, Box<dyn std::error::Error>> {
    let mut builder = QueryBuilder::new();
    let resolver = DictionaryTokenResolver { dict: &dictionary };

    // title:"query planning"
    let title_phrase = builder.node(QueryNode::Term {
        token: Token::field("title", Token::phrase("query planning")),
        term_id: None,
    });

    // body:architecture
    let body_arch = builder.node(QueryNode::Term {
        token: Token::field("body", Token::text("architecture")),
        term_id: None,
    });

    // title:"query planning" OR body:architecture
    let or_group = builder.or(vec![title_phrase, body_arch]);

    // language:rust
    let lang_rust = builder.node(QueryNode::Term {
        token: Token::field("language", Token::text("rust")),
        term_id: None,
    });

    // (title:"query planning" OR body:architecture) AND language:rust
    let root = builder.and(vec![or_group, lang_rust]);

    // Build the query program using the builder
    let program = builder.build(root);

    // Optimize and plan
    let planner = DefaultPhysicalPlanner::new(Box::new(resolver));
    let execution_plan = planner.plan(&program)?;

    Ok(program)
}
```

### Zero-Cost Composition Example

```rust
// Build reusable query fragments
fn build_query_fragments() -> (QueryNodeId, QueryNodeId, QueryNodeId) {
    let mut builder = QueryBuilder::new();
    
    // Fragment 1: rust programming
    let rust = builder.text("rust");
    let programming = builder.text("programming");
    let rust_and_programming = builder.and(vec![rust, programming]);
    
    // Fragment 2: language OR system
    let language = builder.text("language");
    let system = builder.text("system");
    let language_or_system = builder.or(vec![language, system]);
    
    // Fragment 3: architecture
    let architecture = builder.text("architecture");
    
    (rust_and_programming, language_or_system, architecture)
}

// Compose fragments into full query
fn compose_query(
    base: QueryNodeId,
    addition: QueryNodeId,
    optional: QueryNodeId,
) -> QueryProgram {
    let mut builder = QueryBuilder::new();
    
    // (base AND addition) OR optional
    let base_and_addition = builder.and(vec![base, addition]);
    let root = builder.or(vec![base_and_addition, optional]);
    
    QueryProgram::new(builder.into_arena(), root)
}

// Usage
let (base, addition, optional) = build_query_fragments();
let program = compose_query(base, addition, optional);
```

### Custom Lowerer Example

```rust
// Custom DSL with special operators
struct CustomDslNode {
    // Custom syntax
}

struct CustomDslLowerer<'a> {
    resolver: &'a dyn TokenResolver,
}

impl<'a> CustomDslLowerer<'a> {
    fn lower_custom(&self, node: &CustomDslNode) -> Result<QueryProgram, LoweringError> {
        let mut builder = QueryBuilder::new();
        
        // Translate custom DSL to QueryNodes
        let root = match node {
            // Custom logic here
            _ => return Err(LoweringError::Unsupported {
                construct: "custom".into(),
                suggestion: None,
            }),
        };
        
        Ok(QueryProgram::new(builder.into_arena(), root))
    }
}
```

### Custom Planner Example

```rust
// Planner with custom algorithm selection
struct AdaptivePlanner {
    resolver: Box<dyn TokenResolver>,
    statistics: IndexStatistics,
}

impl AdaptivePlanner {
    fn select_intersection_algorithm(
        &self,
        children: &[QueryNodeId],
        program: &QueryProgram,
    ) -> IntersectionAlgorithm {
        // Use statistics to choose algorithm
        if children.len() > 5 {
            IntersectionAlgorithm::Adaptive
        } else if self.should_use_bitmap(children, program) {
            IntersectionAlgorithm::Bitmap
        } else {
            IntersectionAlgorithm::SkipPointers
        }
    }
    
    fn should_use_bitmap(&self, children: &[QueryNodeId], program: &QueryProgram) -> bool {
        // Check if all terms are dense
        // Uses index statistics
        true // Placeholder
    }
}

impl PhysicalPlanner for AdaptivePlanner {
    fn plan(&self, program: &QueryProgram) -> Result<ExecutionPlan, PlanningError> {
        // Custom planning logic
        Ok(ExecutionPlan::new(PhysicalOperatorArena::new(), PhysicalOperatorId::new(0)))
    }
}
```

## Summary

This query planning architecture provides:

1. **Arena-based AST** — Zero-copy cloning, stable node identities, cheap transformations
2. **Clear separation of concerns** — Parsing, lowering, logical planning, physical planning
3. **Composable queries** — Build complex queries from reusable fragments
4. **Extensible design** — Custom parsers, optimizers, and planners via traits
5. **Type-safe** — Leverages Rust's type system for correctness
6. **Performance-focused** — Arena allocation, minimal indirection, algorithm selection

The architecture enables building sophisticated query systems while maintaining clarity and extensibility. The separation between logical and physical planning allows for experimentation with optimization strategies without changing the core query representation.
