# leit_query Crate Specification

**Status:** 📋 Specification  
**Phase:** 1  
**Component:** Query Language and Planning  
**Dependencies:** leit_core, leit_text  

---

## 1. Overview and Purpose

The `leit_query` crate provides the query language parsing, representation, and planning infrastructure for Leit. It translates human-readable query strings into compact, executable query programs that operate on canonicalized term identifiers.

### Core Responsibilities

- **Query Parsing:** Convert query strings into structured query ASTs
- **Query Representation:** Provide compact arena-based query programs for execution
- **Query Planning:** Transform parsed queries into optimized execution plans
- **Term Resolution:** Map query terms to canonicalized TermId handles
- **Syntax Support:** Implement a practical query language with terms, phrases, and operators

### Design Philosophy

- **no_std + alloc:** Kernel crate portable to embedded environments
- **Arena allocation:** Query programs use index-based arenas to eliminate recursive allocations
- **Explicit planning:** Separate parsing from planning to enable optimization passes
- **Compact representation:** Execution operates on TermId handles, not strings
- **Minimal allocations:** Hot-path execution reuses pre-allocated structures

### Non-Goals

- Query execution (handled by `leit_postings` and `leit_score`)
- Index management (handled by `leit_index`)
- Storage and persistence (handled by higher-level crates)
- Distributed query routing (out of scope for kernel)

---

## 2. Dependencies

### Required Dependencies

**leit_core:**
- `TermId` — Canonicalized term identifier
- `FieldId` — Field identifier for multi-field queries
- `EntityId` — Generic entity identifier trait
- Error types for common vocabulary

**leit_text:**
- Term extraction utilities
- Text normalization helpers for query terms
- Phrase detection support

### Optional Dependencies

**alloc (for no_std):**
- `Vec`, `String`, `Box` for arena allocation
- `collections::BTreeMap` for planning metadata

---

## 3. Target: no_std + alloc

### Platform Requirements

```toml
[dependencies]
alloc = "version"  # For Vec, String, BTreeMap
```

### Crate Attributes

```rust
#![no_std]
extern crate alloc;
```

### Constraints

- No use of `std::fs`, `std::io`, `std::thread`, `std::net`
- All collections use `alloc::vec`, `alloc::string`, `alloc::collections`
- Error handling uses concrete enums, not `anyhow` or `Box<dyn Error>`
- APIs work with borrowed data where practical

### Testing Strategy

- Unit tests use `std` in test configuration only
- Integration tests may use std for test fixtures
- no_std compatibility verified in CI

---

## 4. Public API Specification

### 4.1 Query AST (Parsed Form)

The query AST represents the immediate output of parsing, before planning and term resolution.

```rust
/// A node in the parsed query AST
#[derive(Debug, Clone, PartialEq)]
pub enum QueryAst {
    /// Single term query
    Term {
        field: Option<String>,
        value: String,
        boost: f32,
    },
    
    /// Phrase query (multiple terms in sequence)
    Phrase {
        field: Option<String>,
        terms: Vec<String>,
        slop: u32,  // Edit distance for proximity
        boost: f32,
    },
    
    /// Logical AND (intersection)
    And {
        children: Vec<QueryAst>,
        boost: f32,
    },
    
    /// Logical OR (union)
    Or {
        children: Vec<QueryAst>,
        boost: f32,
    },
    
    /// Logical NOT (exclusion)
    Not {
        child: Box<QueryAst>,
    },
    
    /// Boost wrapper (applies to any query)
    Boost {
        child: Box<QueryAst>,
        factor: f32,
    },
}
```

### 4.2 ParsedQuery

Container for parsed query with metadata.

```rust
/// Result of parsing a query string
#[derive(Debug, Clone)]
pub struct ParsedQuery {
    /// The root AST node
    pub root: QueryAst,
    
    /// Original query string (for debugging)
    pub source: String,
    
    /// Offset mapping for error reporting
    /// Maps byte offset in source to AST node positions
    pub offset_map: Vec<(usize, AstLocation)>,
}

/// Location information for error reporting
#[derive(Debug, Clone, Copy)]
pub struct AstLocation {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}
```

### 4.3 parse_query

Main parsing entry point.

```rust
/// Parse a query string into a structured AST
///
/// # Arguments
/// * `query` - Query string to parse
///
/// # Returns
/// A `ParsedQuery` containing the AST and metadata
///
/// # Errors
/// Returns `QueryError::InvalidSyntax` if the query cannot be parsed
pub fn parse_query(query: &str) -> Result<ParsedQuery, QueryError> {
    // Implementation parses query string into QueryAst
}
```

### 4.4 QueryError

Concrete error type for query operations.

```rust
/// Errors that can occur during query parsing or planning
#[derive(Debug, Clone, PartialEq)]
pub enum QueryError {
    /// Syntax error in query string
    InvalidSyntax {
        position: usize,
        message: String,
    },
    
    /// Unknown field name referenced
    UnknownField {
        field: String,
    },
    
    /// Term not found in dictionary
    TermNotFound {
        term: String,
    },
    
    /// Invalid boost value
    InvalidBoost {
        value: String,
    },
    
    /// Query is empty or contains only whitespace
    EmptyQuery,
    
    /// Query exceeds maximum complexity
    QueryTooComplex {
        node_count: usize,
        max_allowed: usize,
    },
    
    /// Nesting depth exceeded
    NestingTooDeep {
        depth: usize,
        max_allowed: usize,
    },
}
```

### 4.5 QueryProgram (Arena-Based Execution Form)

Compact arena-based representation for query execution. Uses index-based relationships to avoid pointer overhead.

```rust
/// Arena-based query program for execution
#[derive(Debug, Clone)]
pub struct QueryProgram {
    /// Arena of query nodes
    pub nodes: Vec<QueryNode>,
    
    /// Shared buffer for child node indices
    /// Each QueryNode references a range in this buffer
    pub children: Vec<QueryNodeId>,
    
    /// Root node of the query
    pub root: QueryNodeId,
    
    /// Maximum nesting depth (for execution planning)
    pub max_depth: usize,
    
    /// Total node count (for complexity analysis)
    pub node_count: usize,
}

/// Index-based identifier for a query node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QueryNodeId(u32);

impl QueryNodeId {
    pub const INVALID: Self = Self(u32::MAX);
    
    pub fn new(index: u32) -> Self {
        Self(index)
    }
    
    pub fn get(self) -> Option<u32> {
        if self.0 == u32::MAX {
            None
        } else {
            Some(self.0)
        }
    }
}
```

### 4.6 QueryNode (Arena Node Types)

Node types stored in the QueryProgram arena.

```rust
/// Query node variants in the arena
#[derive(Debug, Clone)]
pub enum QueryNode {
    /// Single term lookup
    Term {
        field: FieldId,
        term: TermId,
        boost: f32,
    },
    
    /// Logical AND (intersection)
    And {
        /// Range into QueryProgram.children buffer
        children: core::ops::Range<usize>,
        boost: f32,
    },
    
    /// Logical OR (union)
    Or {
        /// Range into QueryProgram.children buffer
        children: core::ops::Range<usize>,
        boost: f32,
    },
    
    /// Logical NOT (exclusion)
    Not {
        /// Single child node
        child: QueryNodeId,
    },
    
    /// Constant score wrapper
    ConstantScore {
        child: QueryNodeId,
        score: f32,
    },
}
```

### 4.7 QueryNodeKind

Lightweight enum for node classification (used in planning).

```rust
/// Kind of query node (for planning and optimization)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryNodeKind {
    /// Leaf node (term or phrase)
    Leaf,
    
    /// Conjunction (AND)
    Conjunction,
    
    /// Disjunction (OR)
    Disjunction,
    
    /// Negation (NOT)
    Negation,
    
    /// Score modifier
    ScoreModifier,
}
```

### 4.8 Planner

Query planning engine that transforms ASTs into optimized QueryPrograms.

```rust
/// Query planning engine
pub struct Planner {
    /// Maximum allowed query depth
    max_depth: usize,
    
    /// Maximum allowed node count
    max_nodes: usize,
    
    /// Field resolver (optional)
    field_resolver: Option<Box<dyn FieldResolver>>,
}

impl Planner {
    /// Create a new planner with default limits
    pub fn new() -> Self {
        Self {
            max_depth: 32,
            max_nodes: 1024,
            field_resolver: None,
        }
    }
    
    /// Set maximum query depth
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }
    
    /// Set maximum node count
    pub fn with_max_nodes(mut self, count: usize) -> Self {
        self.max_nodes = count;
        self
    }
    
    /// Set field resolver
    pub fn with_field_resolver(mut self, resolver: Box<dyn FieldResolver>) -> Self {
        self.field_resolver = Some(resolver);
        self
    }
    
    /// Plan a parsed query into an executable QueryProgram
    pub fn plan(
        &self,
        parsed: &ParsedQuery,
        ctx: &PlanningContext,
        scratch: &mut PlannerScratch,
    ) -> Result<QueryProgram, QueryError> {
        // Implementation transforms AST into QueryProgram
    }
    
    /// Optimize an existing QueryProgram
    pub fn optimize(
        &self,
        program: QueryProgram,
        ctx: &PlanningContext,
        scratch: &mut PlannerScratch,
    ) -> Result<QueryProgram, QueryError> {
        // Implementation applies optimizations:
        // - Flatten nested AND/OR
        // - Pull up constant boosts
        // - Eliminate redundant NOTs
        // - Reorder terms by selectivity
    }
}

impl Default for Planner {
    fn default() -> Self {
        Self::new()
    }
}
```

### 4.9 PlanningContext

Context provided to the planner for term resolution and field mapping.

```rust
/// Context for query planning
pub struct PlanningContext<'a> {
    /// Term dictionary for resolving term strings to TermIds
    pub dictionary: &'a dyn TermDictionary,
    
    /// Field registry for resolving field names to FieldIds
    pub fields: &'a dyn FieldRegistry,
    
    /// Default boost for terms without explicit boost
    pub default_boost: f32,
}

impl<'a> PlanningContext<'a> {
    pub fn new(
        dictionary: &'a dyn TermDictionary,
        fields: &'a dyn FieldRegistry,
    ) -> Self {
        Self {
            dictionary,
            fields,
            default_boost: 1.0,
        }
    }
    
    pub fn with_default_boost(mut self, boost: f32) -> Self {
        self.default_boost = boost;
        self
    }
}
```

### 4.10 PlannerScratch

Reusable scratch space for query planning (allocation discipline).

```rust
/// Reusable scratch space for query planning
pub struct PlannerScratch {
    /// Temporary buffer for AST traversal
    node_stack: Vec<QueryNodeId>,
    
    /// Temporary buffer for term resolution
    term_buffer: Vec<TermId>,
    
    /// Temporary buffer for field resolution
    field_buffer: Vec<FieldId>,
    
    /// Reserved capacity for planning operations
    reserved_capacity: usize,
}

impl PlannerScratch {
    /// Create new scratch space with default capacity
    pub fn new() -> Self {
        Self {
            node_stack: Vec::with_capacity(64),
            term_buffer: Vec::with_capacity(128),
            field_buffer: Vec::with_capacity(16),
            reserved_capacity: 1024,
        }
    }
    
    /// Create scratch space with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            node_stack: Vec::with_capacity(capacity / 16),
            term_buffer: Vec::with_capacity(capacity),
            field_buffer: Vec::with_capacity(capacity / 8),
            reserved_capacity: capacity,
        }
    }
    
    /// Clear all buffers (retains allocated capacity)
    pub fn clear(&mut self) {
        self.node_stack.clear();
        self.term_buffer.clear();
        self.field_buffer.clear();
    }
    
    /// Reset to initial state
    pub fn reset(&mut self) {
        self.clear();
    }
}

impl Default for PlannerScratch {
    fn default() -> Self {
        Self::new()
    }
}
```

### 4.11 ExecutionPlan

Planned query ready for execution with metadata.

```rust
/// Planned query with execution metadata
pub struct ExecutionPlan {
    /// The query program
    pub program: QueryProgram,
    
    /// Estimated selectivity (0.0 = selective, 1.0 = unselective)
    pub selectivity: f32,
    
    /// Estimated cost (higher = more expensive)
    pub cost: u32,
    
    /// Required features for execution
    pub required_features: FeatureSet,
}

/// Set of features required for query execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureSet {
    pub needs_term_frequency: bool,
    pub needs_positions: bool,
    pub needs_block_max: bool,
}

impl FeatureSet {
    pub const NONE: Self = Self {
        needs_term_frequency: false,
        needs_positions: false,
        needs_block_max: false,
    };
    
    pub fn basic() -> Self {
        Self {
            needs_term_frequency: true,
            needs_positions: false,
            needs_block_max: false,
        }
    }
    
    pub fn with_positions() -> Self {
        Self {
            needs_term_frequency: true,
            needs_positions: true,
            needs_block_max: false,
        }
    }
    
    pub fn with_block_max() -> Self {
        Self {
            needs_term_frequency: true,
            needs_positions: true,
            needs_block_max: true,
        }
    }
}
```

---

## 5. Query Parser Syntax

### Supported Query Language

The parser supports a practical query language with common operators:

#### 5.1 Term Queries

Single terms match individual tokens.

```
title:rust              // Field-specific term
rust                    // Default field term
"rust"                  // Quoted term (same as unquoted)
```

#### 5.2 Phrase Queries

Phrases match multiple terms in sequence.

```
"search engine"         // Exact phrase
title:"memory safety"   // Field-specific phrase
"search engine"~2       // Phrase with slop (edit distance 2)
```

#### 5.3 Boolean Operators

Supported operators: `AND`, `OR`, `NOT` (case-sensitive).

```
rust AND safety                    // Both terms must match
rust OR safety                     // Either term must match
rust AND NOT safety                // Must match rust, exclude safety
(rust OR cpp) AND safety           // Nested expressions
```

#### 5.4 Boosting

Boost factors adjust relevance scores.

```
rust^2.0                          // Double weight
title:rust^1.5                    // Field-specific boost
(rust OR cpp)^1.5 AND safety      // Boost on sub-expression
```

#### 5.5 Grouping

Parentheses control operator precedence.

```
(rust OR cpp) AND (safety OR memory)
title:(rust OR cpp)               // Field-scoped group
NOT (rust OR cpp)                 // Negate entire group
```

### Operator Precedence

1. NOT (highest precedence)
2. AND
3. OR (lowest precedence)

Boost operators (`^`) apply to the immediately preceding term or group.

### Syntax Examples

| Query | Meaning |
|-------|---------|
| `rust` | Match "rust" in default field |
| `title:rust` | Match "rust" in title field |
| `"search engine"` | Match phrase "search engine" |
| `rust AND safety` | Match both "rust" and "safety" |
| `rust OR cpp` | Match "rust" or "cpp" |
| `rust AND NOT safety` | Match "rust", exclude "safety" |
| `(rust OR cpp) AND safety` | Match safety and (rust or cpp) |
| `rust^2.0 safety` | Match both, rust weighted 2x |
| `"memory safety"~1` | Match phrase with 1 edit distance |

### Unsupported Syntax (Future Work)

- Wildcards: `rust*`
- Fuzzy matching: `rust~1`
- Range queries: `date:[2020-01-01 TO 2020-12-31]`
- Regex: `/.*pattern.*/`
- Proximity without phrases: `rust NEAR/2 safety`

---

## 6. Arena Allocation Strategy

### Memory Layout

The `QueryProgram` uses a two-arena strategy:

1. **Nodes Arena (`Vec<QueryNode>`):** Stores all query nodes
2. **Children Buffer (`Vec<QueryNodeId>`):** Stores child index lists

### Child Relationship Encoding

Instead of storing child indices directly in nodes (which would require recursion or boxing), we store ranges into a shared buffer:

```rust
// Example: AND(A, B, C)
// nodes arena: [And { children: 0..3, boost: 1.0 }]
// children buffer: [id_A, id_B, id_C]

// Example: OR(AND(A, B), C)
// nodes arena: [Or { children: 0..2, boost: 1.0 }, And { children: 2..4, boost: 1.0 }]
// children buffer: [and_id, c_id, a_id, b_id]
```

### Benefits

1. **No recursion:** Traversal uses iteration, not recursive function calls
2. **No boxes:** All nodes stored inline in contiguous memory
3. **Better cache behavior:** Sequential memory access patterns
4. **Easy rewriting:** Optimizations modify arena contents without restructuring
5. **Stable IDs:** Node indices remain valid after modifications

### Allocation Pattern

```rust
// Planning phase: pre-allocate arenas
let mut scratch = PlannerScratch::with_capacity(estimated_node_count);
let mut program = QueryProgram {
    nodes: Vec::with_capacity(estimated_node_count),
    children: Vec::with_capacity(estimated_node_count * 2),
    root: QueryNodeId::INVALID,
    max_depth: 0,
    node_count: 0,
};

// Build program by pushing to arenas
let term_id = program.nodes.len();
program.nodes.push(QueryNode::Term { field, term, boost: 1.0 });
program.children.push(term_id); // Add to parent's children
```

### Reuse Strategy

Scratch space is allocated once and reused across multiple planning operations:

```rust
let mut scratch = PlannerScratch::new();
let planner = Planner::new();

for query in queries {
    scratch.reset();
    let program = planner.plan(&parsed, &ctx, &mut scratch)?;
    // Execute program...
}
```

---

## 7. Acceptance Criteria Checklist

### Core Functionality

- [ ] Parse term queries with optional field specification
- [ ] Parse phrase queries with optional slop
- [ ] Parse AND, OR, NOT operators with correct precedence
- [ ] Parse grouping with parentheses
- [ ] Parse boost factors on terms and expressions
- [ ] Generate compact `QueryProgram` from parsed AST
- [ ] Resolve terms to `TermId` via dictionary
- [ ] Resolve fields to `FieldId` via field registry
- [ ] Apply query optimization passes (flattening, reordering)
- [ ] Generate execution metadata (selectivity, cost, features)

### Error Handling

- [ ] Report syntax errors with accurate position information
- [ ] Detect and report unknown field names
- [ ] Detect and report terms not in dictionary
- [ ] Detect and reject queries exceeding depth limits
- [ ] Detect and reject queries exceeding node count limits
- [ ] Provide clear error messages for invalid syntax

### no_std Compatibility

- [ ] Crate compiles with `#![no_std]`
- [ ] All collections use `alloc::vec`, `alloc::string`, etc.
- [ ] No use of `std` APIs in library code
- [ ] Tests verify no_std compatibility in CI

### Performance and Allocation

- [ ] Query planning does not allocate during normal operation (uses scratch)
- [ ] QueryProgram uses compact arena layout
- [ ] Scratch space is reusable across multiple planning operations
- [ ] Planning time is O(n) in query size
- [ ] Memory usage is bounded and predictable

### Testing

- [ ] Unit tests for all parser syntax cases
- [ ] Unit tests for all error conditions
- [ ] Unit tests for arena allocation correctness
- [ ] Integration tests for end-to-end query planning
- [ ] Property tests for query round-tripping
- [ ] Performance benchmarks for planning time

### Documentation

- [ ] All public types have rustdoc comments
- [ ] All public functions have rustdoc with examples
- [ ] Module-level documentation explains architecture
- [ ] Syntax documentation covers supported query language
- [ ] Error handling documentation covers all error cases

---

## 8. Test Plan

### Unit Tests

#### Parser Tests

```rust
#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn test_parse_term() {
        let result = parse_query("rust").unwrap();
        assert!(matches!(result.root, QueryAst::Term { value, .. } if value == "rust"));
    }

    #[test]
    fn test_parse_fielded_term() {
        let result = parse_query("title:rust").unwrap();
        assert!(matches!(result.root, QueryAst::Term { field, value, .. } 
            if field == Some("title".to_string()) && value == "rust"));
    }

    #[test]
    fn test_parse_phrase() {
        let result = parse_query("\"search engine\"").unwrap();
        assert!(matches!(result.root, QueryAst::Phrase { terms, .. } 
            if terms == vec!["search", "engine"]));
    }

    #[test]
    fn test_parse_and() {
        let result = parse_query("rust AND safety").unwrap();
        assert!(matches!(result.root, QueryAst::And { .. }));
    }

    #[test]
    fn test_parse_or() {
        let result = parse_query("rust OR cpp").unwrap();
        assert!(matches!(result.root, QueryAst::Or { .. }));
    }

    #[test]
    fn test_parse_not() {
        let result = parse_query("NOT rust").unwrap();
        assert!(matches!(result.root, QueryAst::Not { .. }));
    }

    #[test]
    fn test_parse_grouping() {
        let result = parse_query("(rust OR cpp) AND safety").unwrap();
        // Verify structure matches expected precedence
    }

    #[test]
    fn test_parse_boost() {
        let result = parse_query("rust^2.0").unwrap();
        assert!(matches!(result.root, QueryAst::Term { boost, .. } if boost == 2.0));
    }

    #[test]
    fn test_syntax_error() {
        let result = parse_query("rust AND AND safety");
        assert!(matches!(result, Err(QueryError::InvalidSyntax { .. })));
    }

    #[test]
    fn test_empty_query() {
        let result = parse_query("   ");
        assert!(matches!(result, Err(QueryError::EmptyQuery)));
    }
}
```

#### Arena Tests

```rust
#[cfg(test)]
mod arena_tests {
    use super::*;

    #[test]
    fn test_arena_allocation() {
        let program = QueryProgram {
            nodes: vec![
                QueryNode::Term { field: FieldId(0), term: TermId(0), boost: 1.0 },
            ],
            children: vec![],
            root: QueryNodeId::new(0),
            max_depth: 1,
            node_count: 1,
        };
        
        assert_eq!(program.root.get(), Some(0));
    }

    #[test]
    fn test_child_ranges() {
        let program = QueryProgram {
            nodes: vec![
                QueryNode::And { children: 0..2, boost: 1.0 },
                QueryNode::Term { field: FieldId(0), term: TermId(0), boost: 1.0 },
                QueryNode::Term { field: FieldId(1), term: TermId(1), boost: 1.0 },
            ],
            children: vec![QueryNodeId::new(1), QueryNodeId::new(2)],
            root: QueryNodeId::new(0),
            max_depth: 2,
            node_count: 3,
        };
        
        // Verify child range references correct nodes
    }
}
```

### Integration Tests

#### End-to-End Planning Tests

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::test_helpers::*;

    #[test]
    fn test_plan_simple_query() {
        let dictionary = TestDictionary::new();
        let fields = TestFields::new();
        let ctx = PlanningContext::new(&dictionary, &fields);
        let mut scratch = PlannerScratch::new();
        let planner = Planner::new();

        let parsed = parse_query("rust").unwrap();
        let program = planner.plan(&parsed, &ctx, &mut scratch).unwrap();
        
        assert!(matches!(program.nodes[0], QueryNode::Term { .. }));
    }

    #[test]
    fn test_plan_complex_query() {
        let dictionary = TestDictionary::new();
        let fields = TestFields::new();
        let ctx = PlanningContext::new(&dictionary, &fields);
        let mut scratch = PlannerScratch::new();
        let planner = Planner::new();

        let parsed = parse_query("(rust OR cpp) AND safety^2.0").unwrap();
        let program = planner.plan(&parsed, &ctx, &mut scratch).unwrap();
        
        // Verify structure and boost application
    }

    #[test]
    fn test_optimization_flattening() {
        // Verify that nested AND/OR are flattened
        // AND(AND(A, B), C) -> AND(A, B, C)
    }

    #[test]
    fn test_term_resolution() {
        let dictionary = TestDictionary::with_terms(vec!["rust", "safety"]);
        let fields = TestFields::new();
        let ctx = PlanningContext::new(&dictionary, &fields);
        let mut scratch = PlannerScratch::new();
        let planner = Planner::new();

        let parsed = parse_query("rust AND safety").unwrap();
        let program = planner.plan(&parsed, &ctx, &mut scratch).unwrap();
        
        // Verify terms resolved to TermIds
    }
}
```

### Example Queries for Testing

| Query | Expected Behavior |
|-------|-------------------|
| `rust` | Single term query |
| `title:rust` | Field-specific term |
| `"search engine"` | Phrase query |
| `"search engine"~2` | Phrase with slop |
| `rust AND safety` | Both terms required |
| `rust OR cpp` | Either term required |
| `NOT rust` | Excludes rust |
| `rust AND NOT safety` | Includes rust, excludes safety |
| `(rust OR cpp) AND safety` | Nested grouping |
| `rust^2.0` | Boosted term |
| `title:(rust OR cpp)` | Field-scoped group |
| `(rust AND safety)^1.5 OR cpp` | Boosted group |

### Property Tests

```rust
#[cfg(test)]
mod property_tests {
    use super::*;

    #[test]
    fn test_parse_roundtrip() {
        // Parse a query, generate string representation, parse again
        // Verify equivalence
    }

    #[test]
    fn test_query_invariants() {
        // Verify invariants:
        // - Node count matches arena length
        // - All child indices are valid
        // - Root is valid
        // - Depth is correctly calculated
    }

    #[test]
    fn test_optimization_preserves_semantics() {
        // Verify that optimizations don't change query semantics
        // Compare execution results before/after optimization
    }
}
```

### Performance Benchmarks

```rust
#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    #[test]
    fn benchmark_simple_query_parsing() {
        let start = Instant::now();
        for _ in 0..10_000 {
            let _ = parse_query("rust AND safety").unwrap();
        }
        let duration = start.elapsed();
        println!("Simple query parsing: {:?}", duration / 10_000);
    }

    #[test]
    fn benchmark_complex_query_planning() {
        let dictionary = TestDictionary::new();
        let fields = TestFields::new();
        let ctx = PlanningContext::new(&dictionary, &fields);
        let mut scratch = PlannerScratch::new();
        let planner = Planner::new();

        let parsed = parse_query(
            "((rust OR cpp) AND (safety OR memory))^2.0 OR (performance AND NOT security)"
        ).unwrap();

        let start = Instant::now();
        for _ in 0..1_000 {
            let _ = planner.plan(&parsed, &ctx, &mut scratch);
            scratch.reset();
        }
        let duration = start.elapsed();
        println!("Complex query planning: {:?}", duration / 1_000);
    }
}
```

---

## 9. Verification Commands

### Build Verification

```bash
# Build crate with no_std
cargo build -p leit_query --no-default-features

# Build with alloc
cargo build -p leit_query --features alloc

# Build with std for testing
cargo build -p leit_query

# Run unit tests
cargo test -p leit_query

# Run integration tests
cargo test -p leit_query --test integration

# Run documentation tests
cargo test -p leit_query --doc
```

### Linting and Formatting

```bash
# Format code
cargo fmt -p leit_query

# Check formatting
cargo fmt -p leit_query --check

# Run clippy
cargo clippy -p leit_query -- -D warnings

# Check for no_std compatibility
cargo check -p leit_query --no-default-features --target x86_64-unknown-linux-musl
```

### Documentation

```bash
# Generate documentation
cargo doc -p leit_query --no-deps --open

# Check documentation coverage
cargo doc -p leit_query --no-deps
```

### Continuous Integration

```bash
# Run full test suite
cargo test -p leit_query --all-features

# Run with sanitizers (nightly)
cargo test -p leit_query --all-features -Zsanitizer=address

# Run benchmarks (requires nightly)
cargo bench -p leit_query

# Check for memory leaks (valgrind)
cargo test -p leit_query --no-default-features
valgrind --leak-check=full target/debug/leit_query-*
```

### Integration with Other Crates

```bash
# Verify leit_core dependency
cargo tree -p leit_query -i leit_core

# Verify leit_text dependency
cargo tree -p leit_query -i leit_text

# Check no accidental std dependencies
cargo tree -p leit_query --no-default-features | grep -E 'std|' || echo "No std deps found"
```

---

## 10. Implementation Notes

### Phase 1 Scope

The initial implementation focuses on:

1. Core parser for term, phrase, AND, OR, NOT, boost syntax
2. Basic query AST and QueryProgram representation
3. Simple planner without advanced optimizations
4. Term and field resolution
5. Error handling with position information

### Future Work (Post-Phase 1)

- Advanced query operators (wildcards, fuzzy, ranges, regex)
- Query rewrites and normalization passes
- Cost-based optimization with statistics
- Multi-term phrase queries with proximity
- Query explanation and debugging tools
- Query caching and memoization

### Architecture Alignment

This crate follows Leit's architectural principles:

- **no_std + alloc:** Kernel crate works in embedded environments
- **Arena allocation:** Compact representation without recursive allocations
- **Explicit planning:** Separate parsing from execution
- **Concrete errors:** Enum-based error handling for no_std compatibility
- **Allocation discipline:** Reusable scratch space for repeated operations

### Relationship to Other Crates

- **leit_core:** Provides core types (TermId, FieldId, EntityId)
- **leit_text:** Provides term extraction and normalization utilities
- **leit_postings:** Consumes QueryProgram for postings traversal
- **leit_score:** Consumes QueryProgram for scoring
- **leit_index:** Higher-level orchestration of query planning and execution
