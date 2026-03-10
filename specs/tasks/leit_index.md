# leit_index Crate Specification

**Status:** 📋 Specification  
**Phase:** 1  
**Component:** Index Integration and Execution Layer  
**Dependencies:** leit_core, leit_text, leit_query, leit_postings, leit_score, leit_collect  

---

## 1. Overview and Purpose

The `leit_index` crate provides the integration layer that orchestrates all other Leit components into a cohesive, searchable index. It bridges the gap between kernel-level components and user-facing APIs, providing a convenient interface for building, querying, and managing in-memory indices.

### Core Responsibilities

- **Index Building:** Construct inverted indices from collections of entities
- **Query Execution:** Coordinate query parsing, planning, and execution across components
- **Entity Projection:** Transform raw entity data into indexed representations
- **Workspace Management:** Manage reusable execution workspace to minimize allocations
- **Integration:** Provide a unified API that composes all kernel components

### Design Philosophy

- **std target:** Boundary layer that integrates no_std kernels with std conveniences
- **Zero-copy where possible:** Borrow data rather than clone when practical
- **Allocation discipline:** Reusable workspace for query execution
- **Type-safe projections:** Compile-time guarantees for entity indexing
- **Clear separation:** Index building vs. query execution concerns

### Non-Goals

- Persistent storage (handled by higher-level crates)
- Network/distributed indexing (out of scope)
- Complex index lifecycle management (write-once, read-many focus)
- Real-time index updates (batch-oriented building)

---

## 2. Dependencies

### Required Dependencies

**leit_core:**
- `TermId` — Canonicalized term identifier
- `FieldId` — Field identifier for multi-field indices
- `EntityId` — Generic entity identifier trait
- `DocId` — Document identifier within the index
- Error types for common vocabulary

**leit_text:**
- `Tokenizer` — Text tokenization for field extraction
- `TextNormalizer` — Text normalization pipeline
- Term extraction utilities

**leit_query:**
- `parse_query` — Query string parsing
- `QueryProgram` — Arena-based query representation
- `Planner` — Query planning and optimization
- `PlanningContext` — Context for term/field resolution

**leit_postings:**
- `PostingsList` — Inverted list data structure
- `PostingsReader` — Postings traversal interface
- `PositionIterator` — Term position iteration

**leit_score:**
- `Scorer` — Scoring trait implementation
- `BM25` — BM25 scoring algorithm
- `BM25F` — Multi-field BM25F scoring

**leit_collect:**
- `Collector` — Result collection trait
- `TopDocsCollector` — Top-K result collection
- `ScoreDoc` — Scored document result type

### Optional Dependencies

**std (required):**
- `Vec`, `HashMap`, `String` for index structures
- `collections::HashSet` for deduplication

---

## 3. Target: std (Boundary Layer)

### Platform Requirements

```toml
[dependencies]
leit_core = { path = "../leit_core" }
leit_text = { path = "../leit_text" }
leit_query = { path = "../leit_query" }
leit_postings = { path = "../leit_postings" }
leit_score = { path = "../leit_score" }
leit_collect = { path = "../leit_collect" }
```

### Crate Attributes

```rust
// Standard library target (no #![no_std])
// This crate integrates no_std kernel components with std conveniences
```

### Constraints

- May use `std::collections` for index structures
- May use `std::io` for file-based index loading (future work)
- Error handling should still prefer concrete enums over `anyhow`
- APIs should remain generic over entity types
- Thread-safety is a concern but not initial focus

### Testing Strategy

- Unit tests for individual components
- Integration tests for end-to-end workflows
- Property tests for correctness invariants
- Performance benchmarks for query execution

---

## 4. Public API Specification

### 4.1 Projection<E> Trait

The core abstraction for transforming user entities into indexed representations.

```rust
/// Projection from user entity type to indexed fields
///
/// This trait defines how to extract searchable content from an entity.
/// Implementations transform entities into sequences of (field, text) pairs
/// that will be tokenized and indexed.
///
/// # Type Parameters
/// * `E` - The entity type being indexed (e.g., `Document`, `Post`, `Product`)
///
/// # Example
/// ```rust
/// struct Document {
///     id: u64,
///     title: String,
///     body: String,
/// }
///
/// impl Projection<Document> for Document {
///     fn project<'a>(&'a self) -> Vec<(FieldId, &'a str)> {
///         vec![
///             (FieldId(0), &self.title),
///             (FieldId(1), &self.body),
///         ]
///     }
///     
///     fn id(&self) -> u64 {
///         self.id
///     }
/// }
/// ```
pub trait Projection<E>: Sized {
    /// Project entity into field-text pairs for indexing
    ///
    /// # Returns
    /// Vector of (field_id, text_content) pairs
    ///
    /// # Lifecycle
    /// The returned text slices must outlive the indexing operation.
    /// Typically this means borrowing from `&self`.
    fn project<'a>(&'a self) -> Vec<(FieldId, &'a str)>;
    
    /// Extract entity identifier
    ///
    /// # Returns
    /// Unique identifier for this entity (used as DocId)
    fn id(&self) -> u64;
    
    /// Optional: Field-level boost factors
    ///
    /// # Returns
    /// Boost factor for each field (default: 1.0)
    ///
    /// # Default
    /// Returns 1.0 for all fields
    #[inline]
    fn field_boost(&self, _field: FieldId) -> f32 {
        1.0
    }
}
```

### 4.2 InMemoryIndexBuilder

Builder for constructing in-memory indices from collections of entities.

```rust
/// Builder for in-memory search indices
///
/// # Type Parameters
/// * `E` - The entity type being indexed
///
/// # Lifecycle
/// 1. Create builder with `InMemoryIndexBuilder::new()`
/// 2. Configure fields with `register_field()`
/// 3. Add entities with `add()` or `extend()`
/// 4. Build index with `build()`
/// 5. Builder is consumed, index is ready for queries
///
/// # Example
/// ```rust
/// let mut builder = InMemoryIndexBuilder::new();
/// builder.register_field("title", 2.0);  // Title weighted 2x
/// builder.register_field("body", 1.0);
/// 
/// for doc in documents {
///     builder.add(&doc)?;
/// }
/// 
/// let index = builder.build()?;
/// ```
pub struct InMemoryIndexBuilder<E> {
    /// Field registry (name -> FieldId mapping)
    fields: HashMap<String, FieldInfo>,
    
    /// Term dictionary (term string -> TermId)
    dictionary: HashMap<String, TermId>,
    
    /// Inverted index (term -> postings list)
    postings: HashMap<TermId, PostingsList>,
    
    /// Document store (DocId -> entity reference)
    documents: Vec<EntityRef<E>>,
    
    /// Field norms (DocId -> field -> length)
    field_norms: Vec<HashMap<FieldId, u32>>,
    
    /// Document frequencies (TermId -> doc_count)
    doc_freq: HashMap<TermId, u32>,
    
    /// Collection statistics
    stats: IndexStats,
    
    /// Tokenizer for text processing
    tokenizer: Tokenizer,
    
    /// Normalizer for text normalization
    normalizer: TextNormalizer,
}

/// Field metadata
#[derive(Debug, Clone)]
struct FieldInfo {
    /// Field identifier
    id: FieldId,
    /// Field-level boost factor
    boost: f32,
    /// Number of documents with content in this field
    doc_count: u32,
}

/// Reference to entity in the index
struct EntityRef<E> {
    /// Document identifier
    doc_id: DocId,
    /// External entity ID
    entity_id: u64,
    /// Reference to entity data
    entity: E,
}

/// Index statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct IndexStats {
    /// Total number of documents
    pub doc_count: u32,
    /// Total number of unique terms
    pub term_count: u32,
    /// Total number of tokens (sum of all field lengths)
    pub total_tokens: u64,
    /// Average document length
    pub avg_doc_len: f32,
    /// Maximum document length
    pub max_doc_len: u32,
}

impl<E> InMemoryIndexBuilder<E> 
where 
    E: Clone + 'static,
{
    /// Create a new index builder
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
            dictionary: HashMap::new(),
            postings: HashMap::new(),
            documents: Vec::new(),
            field_norms: Vec::new(),
            doc_freq: HashMap::new(),
            stats: IndexStats::default(),
            tokenizer: Tokenizer::default(),
            normalizer: TextNormalizer::default(),
        }
    }
    
    /// Register a field for indexing
    ///
    /// # Arguments
    /// * `name` - Field name (e.g., "title", "body")
    /// * `boost` - Field-level boost factor (default: 1.0)
    ///
    /// # Returns
    /// FieldId for this field
    pub fn register_field(&mut self, name: &str, boost: f32) -> FieldId {
        let field_id = FieldId(self.fields.len() as u32);
        self.fields.insert(
            name.to_string(),
            FieldInfo {
                id: field_id,
                boost,
                doc_count: 0,
            },
        );
        field_id
    }
    
    /// Add a single entity to the index
    ///
    /// # Arguments
    /// * `entity` - Entity to index (must implement Projection<E>)
    /// * `projection` - Projection implementation for entity type
    ///
    /// # Returns
    /// Ok(()) on success, Err on indexing failure
    pub fn add<P>(&mut self, entity: &E) -> Result<(), IndexError>
    where
        P: Projection<E>,
    {
        // Implementation: 
        // 1. Extract entity ID and DocId
        // 2. Project entity into fields
        // 3. For each field: tokenize, normalize, add to postings
        // 4. Update statistics
        unimplemented!()
    }
    
    /// Add multiple entities to the index
    ///
    /// # Arguments
    /// * `entities` - Iterator over entities to index
    ///
    /// # Returns
    /// Ok(()) on success, Err on indexing failure
    pub fn extend<P, I>(&mut self, entities: I) -> Result<(), IndexError>
    where
        P: Projection<E>,
        I: IntoIterator<Item = E>,
    {
        for entity in entities {
            self.add::<P>(&entity)?;
        }
        Ok(())
    }
    
    /// Configure tokenizer for text processing
    pub fn with_tokenizer(mut self, tokenizer: Tokenizer) -> Self {
        self.tokenizer = tokenizer;
        self
    }
    
    /// Configure normalizer for text normalization
    pub fn with_normalizer(mut self, normalizer: TextNormalizer) -> Self {
        self.normalizer = normalizer;
        self
    }
    
    /// Build the in-memory index
    ///
    /// # Returns
    /// Built `InMemoryIndex<E>` ready for querying
    ///
    /// # Panics
    /// Panics if no entities were added
    pub fn build(self) -> Result<InMemoryIndex<E>, IndexError> {
        // Implementation:
        // 1. Validate index has documents
        // 2. Compute final statistics
        // 3. Create InMemoryIndex from builder state
        unimplemented!()
    }
}

impl<E> Default for InMemoryIndexBuilder<E>
where
    E: Clone + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}
```

### 4.3 InMemoryIndex

The main index type for querying indexed entities.

```rust
/// In-memory search index
///
/// # Type Parameters
/// * `E` - The entity type being indexed
///
/// # Usage
/// ```rust
/// let index = builder.build()?;
/// 
/// // Execute query
/// let results = index.search("rust AND safety", &mut workspace)?;
/// 
/// for result in results {
///     println!("Doc {}: {}", result.doc_id, result.score);
/// }
/// ```
pub struct InMemoryIndex<E> {
    /// Field registry
    fields: HashMap<String, FieldInfo>,
    
    /// Term dictionary
    dictionary: HashMap<String, TermId>,
    
    /// Inverted index
    postings: HashMap<TermId, PostingsList>,
    
    /// Document store
    documents: Vec<EntityRef<E>>,
    
    /// Field norms
    field_norms: Vec<HashMap<FieldId, u32>>,
    
    /// Document frequencies
    doc_freq: HashMap<TermId, u32>,
    
    /// Index statistics
    stats: IndexStats,
    
    /// Query planner
    planner: Planner,
}

impl<E> InMemoryIndex<E> 
where 
    E: Clone + 'static,
{
    /// Execute a query string against this index
    ///
    /// # Arguments
    /// * `query` - Query string (e.g., "rust AND safety")
    /// * `workspace` - Reusable execution workspace
    ///
    /// # Returns
    /// Vector of scored document results, sorted by relevance
    ///
    /// # Errors
    /// Returns error if query parsing or execution fails
    ///
    /// # Example
    /// ```rust
    /// let mut workspace = ExecutionWorkspace::new();
    /// let results = index.search("title:rust OR body:safety", &mut workspace)?;
    /// ```
    pub fn search(
        &self,
        query: &str,
        workspace: &mut ExecutionWorkspace,
    ) -> Result<Vec<ScoreDoc<E>>, IndexError> {
        // Implementation:
        // 1. Parse query string
        // 2. Plan query into QueryProgram
        // 3. Execute query using workspace
        // 4. Collect top-K results
        // 5. Return sorted results
        unimplemented!()
    }
    
    /// Execute query with custom collector
    ///
    /// # Arguments
    /// * `query` - Query string
    /// * `collector` - Custom collector for result gathering
    /// * `workspace` - Reusable execution workspace
    ///
    /// # Returns
    /// Collected results from the collector
    pub fn search_with_collector<C>(
        &self,
        query: &str,
        collector: &mut C,
        workspace: &mut ExecutionWorkspace,
    ) -> Result<C::Output, IndexError>
    where
        C: Collector<E>,
    {
        // Implementation:
        // 1. Parse and plan query
        // 2. Execute with custom collector
        // 3. Return collector's output
        unimplemented!()
    }
    
    /// Get entity by document ID
    ///
    /// # Arguments
    /// * `doc_id` - Document identifier
    ///
    /// # Returns
    /// Reference to entity if found, None otherwise
    pub fn get(&self, doc_id: DocId) -> Option<&E> {
        self.documents
            .get(doc_id.0 as usize)
            .map(|ref_| &ref_.entity)
    }
    
    /// Get index statistics
    pub fn stats(&self) -> &IndexStats {
        &self.stats
    }
    
    /// Get field information
    pub fn field_info(&self, field_id: FieldId) -> Option<&FieldInfo> {
        self.fields.values().find(|f| f.id == field_id)
    }
    
    /// Create new execution workspace for this index
    pub fn create_workspace(&self) -> ExecutionWorkspace {
        ExecutionWorkspace::new()
    }
}
```

### 4.4 execute Method

Low-level query execution interface for pre-planned queries.

```rust
impl<E> InMemoryIndex<E>
where
    E: Clone + 'static,
{
    /// Execute a pre-planned query program
    ///
    /// # Arguments
    /// * `program` - Pre-planned query program
    /// * `collector` - Result collector
    /// * `workspace` - Reusable execution workspace
    ///
    /// # Returns
    /// Collected results from the collector
    ///
    /// # Use Case
    /// Useful when the same query is executed multiple times
    /// (plan once, execute many)
    pub fn execute<C>(
        &self,
        program: &QueryProgram,
        collector: &mut C,
        workspace: &mut ExecutionWorkspace,
    ) -> Result<C::Output, IndexError>
    where
        C: Collector<E>,
    {
        // Implementation:
        // 1. Create execution engine
        // 2. Execute query program
        // 3. Return collected results
        unimplemented!()
    }
}
```

### 4.5 ExecutionWorkspace

Reusable scratch space for query execution to minimize allocations.

```rust
/// Reusable workspace for query execution
///
/// This workspace holds temporary buffers and state used during
/// query execution. Reusing workspaces across multiple queries
/// significantly reduces allocation overhead.
///
/// # Lifecycle
/// ```rust
/// let mut workspace = ExecutionWorkspace::new();
/// 
/// for query in queries {
///     let results = index.search(query, &mut workspace)?;
///     workspace.reset();  // Clear for next query
/// }
/// ```
pub struct ExecutionWorkspace {
    /// Temporary buffer for doc ID accumulation
    doc_buffer: Vec<DocId>,
    
    /// Temporary buffer for score accumulation
    score_buffer: Vec<Score>,
    
    /// Temporary buffer for term postings
    postings_buffer: Vec<PostingsReader>,
    
    /// Temporary buffer for query execution stack
    execution_stack: Vec<ExecutionContext>,
    
    /// Pre-allocated memory for intermediate results
    scratch: Vec<u8>,
    
    /// Reserved capacity
    capacity: usize,
}

/// Query execution context (pushed onto stack)
struct ExecutionContext {
    /// Node being executed
    node: QueryNodeId,
    /// Intermediate results
    results: Vec<DocId>,
}

impl ExecutionWorkspace {
    /// Create new workspace with default capacity
    pub fn new() -> Self {
        Self {
            doc_buffer: Vec::with_capacity(1024),
            score_buffer: Vec::with_capacity(1024),
            postings_buffer: Vec::with_capacity(16),
            execution_stack: Vec::with_capacity(32),
            scratch: Vec::with_capacity(4096),
            capacity: 1024,
        }
    }
    
    /// Create workspace with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            doc_buffer: Vec::with_capacity(capacity),
            score_buffer: Vec::with_capacity(capacity),
            postings_buffer: Vec::with_capacity(capacity / 64),
            execution_stack: Vec::with_capacity(32),
            scratch: Vec::with_capacity(capacity * 4),
            capacity,
        }
    }
    
    /// Clear all buffers (retains allocated capacity)
    pub fn reset(&mut self) {
        self.doc_buffer.clear();
        self.score_buffer.clear();
        self.postings_buffer.clear();
        self.execution_stack.clear();
        self.scratch.clear();
    }
    
    /// Reserve additional capacity
    pub fn reserve(&mut self, additional: usize) {
        self.doc_buffer.reserve(additional);
        self.score_buffer.reserve(additional);
        self.scratch.reserve(additional * 4);
    }
}

impl Default for ExecutionWorkspace {
    fn default() -> Self {
        Self::new()
    }
}
```

### 4.6 ExecutionEngine

Low-level query execution engine that operates on query programs.

```rust
/// Query execution engine
///
/// The execution engine traverses query programs and coordinates
/// postings traversal, scoring, and collection.
///
/// # Architecture
/// 1. Query program traversal (iterative, not recursive)
/// 2. Postings list intersection/union
/// 3. Score computation for matched documents
/// 4. Result collection via collector trait
pub struct ExecutionEngine<'index, E> 
where
    E: Clone + 'static,
{
    /// Reference to index being queried
    index: &'index InMemoryIndex<E>,
    /// Workspace for temporary state
    workspace: &'index mut ExecutionWorkspace,
    /// Scorer for ranking
    scorer: BM25,
}

impl<'index, E> ExecutionEngine<'index, E>
where
    E: Clone + 'static,
{
    /// Create new execution engine
    fn new(
        index: &'index InMemoryIndex<E>,
        workspace: &'index mut ExecutionWorkspace,
    ) -> Self {
        // Initialize BM25 scorer with index statistics
        let scorer = BM25::new()
            .with_avg_doc_len(index.stats.avg_doc_len)
            .with_doc_count(index.stats.doc_count);
        
        Self {
            index,
            workspace,
            scorer,
        }
    }
    
    /// Execute query program with collector
    fn execute<C>(
        &mut self,
        program: &QueryProgram,
        collector: &mut C,
    ) -> Result<C::Output, IndexError>
    where
        C: Collector<E>,
    {
        // Implementation:
        // 1. Traverse query program iteratively
        // 2. For each term node: fetch postings, accumulate matches
        // 3. For each AND node: intersect child results
        // 4. For each OR node: union child results
        // 5. Score matched documents
        // 6. Collect results via collector
        unimplemented!()
    }
    
    /// Evaluate single query node
    fn evaluate_node(
        &mut self,
        node: QueryNodeId,
        program: &QueryProgram,
    ) -> Result<Vec<DocId>, IndexError> {
        match &program.nodes[node.0 as usize] {
            QueryNode::Term { field, term, .. } => {
                // Fetch postings for term
                // Return matching doc IDs
                unimplemented!()
            }
            QueryNode::And { children, .. } => {
                // Intersect child results
                unimplemented!()
            }
            QueryNode::Or { children, .. } => {
                // Union child results
                unimplemented!()
            }
            QueryNode::Not { child } => {
                // Exclude child results
                unimplemented!()
            }
            QueryNode::ConstantScore { child, score } => {
                // Apply constant score wrapper
                unimplemented!()
            }
        }
    }
}
```

### 4.7 IndexError

Error type for index operations.

```rust
/// Errors that can occur during index operations
#[derive(Debug, Clone, PartialEq)]
pub enum IndexError {
    /// Query parsing failed
    QueryError {
        source: QueryError,
    },
    
    /// Field not registered
    FieldNotFound {
        field: String,
    },
    
    /// Term not in dictionary
    TermNotFound {
        term: String,
    },
    
    /// Index is empty (no documents)
    EmptyIndex,
    
    /// Document ID out of range
    InvalidDocId {
        doc_id: DocId,
    },
    
    /// Index build failed
    BuildError {
        message: String,
    },
    
    /// Execution failed
    ExecutionError {
        message: String,
    },
}

impl From<QueryError> for IndexError {
    fn from(err: QueryError) -> Self {
        IndexError::QueryError { source: err }
    }
}
```

---

## 5. Entity Projection Pattern

### Concept

The entity projection pattern decouples user entity types from index internals. Instead of requiring entities to implement specific index-related methods, we use a separate `Projection<E>` trait that knows how to extract indexable content from entities.

### Benefits

1. **Separation of concerns:** Entities don't need to know about indexing
2. **Flexibility:** Same entity can be indexed multiple ways with different projections
3. **Zero-cost:** Trait is monomorphized, no dynamic dispatch
4. **Borrowing:** Projections can borrow from entities, avoiding clones

### Example Implementations

#### Simple Document

```rust
struct Document {
    id: u64,
    title: String,
    body: String,
}

struct DocumentProjection;

impl Projection<Document> for DocumentProjection {
    fn project<'a>(&'a self, entity: &'a Document) -> Vec<(FieldId, &'a str)> {
        vec![
            (FieldId(0), &entity.title),
            (FieldId(1), &entity.body),
        ]
    }
    
    fn id(&self, entity: &Document) -> u64 {
        entity.id
    }
    
    fn field_boost(&self, field: FieldId) -> f32 {
        match field.0 {
            0 => 2.0,  // Title weighted higher
            1 => 1.0,  // Body default weight
            _ => 1.0,
        }
    }
}
```

#### Multi-field Entity

```rust
struct Product {
    sku: String,
    name: String,
    description: String,
    category: String,
    tags: Vec<String>,
}

struct ProductProjection;

impl Projection<Product> for ProductProjection {
    fn project<'a>(&'a self, entity: &'a Product) -> Vec<(FieldId, &'a str)> {
        let mut fields = vec![
            (FieldId(0), &entity.name),
            (FieldId(1), &entity.description),
            (FieldId(2), &entity.category),
        ];
        
        // Add tags as additional fields
        for (i, tag) in entity.tags.iter().enumerate() {
            fields.push((FieldId(3 + i as u32), tag.as_str()));
        }
        
        fields
    }
    
    fn id(&self, entity: &Product) -> u64 {
        // Hash SKU to u64
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        entity.sku.hash(&mut hasher);
        hasher.finish()
    }
}
```

### Usage Pattern

```rust
// Register fields
let mut builder = InMemoryIndexBuilder::new();
builder.register_field("title", 2.0);
builder.register_field("body", 1.0);

// Index documents with projection
for doc in &documents {
    builder.add::<DocumentProjection>(doc)?;
}

let index = builder.build()?;
```

---

## 6. Execution Pipeline Walkthrough

### Step-by-Step Query Execution

#### 1. Query Parsing

```
User Input: "rust AND safety"
         ↓
parse_query()
         ↓
QueryAst::And {
    children: [
        QueryAst::Term { value: "rust" },
        QueryAst::Term { value: "safety" },
    ],
}
```

#### 2. Query Planning

```
QueryAst
         ↓
Planner::plan()
         ↓
Term Resolution (via dictionary)
- "rust" → TermId(42)
- "safety" → TermId(137)
         ↓
QueryProgram {
    nodes: [
        And { children: 0..2, boost: 1.0 },
        Term { field: FieldId(0), term: TermId(42), boost: 1.0 },
        Term { field: FieldId(0), term: TermId(137), boost: 1.0 },
    ],
    children: [NodeId(1), NodeId(2)],
    root: NodeId(0),
}
```

#### 3. Query Execution

```
QueryProgram
         ↓
ExecutionEngine::execute()
         ↓
Iterative Traversal:
- Process root (And node)
  - Push to stack
  - Process first child (Term "rust")
    - Fetch postings for TermId(42)
    - Get DocIds [0, 5, 23, 47, ...]
  - Process second child (Term "safety")
    - Fetch postings for TermId(137)
    - Get DocIds [0, 7, 23, 31, 47, ...]
  - Intersect results
    - Common DocIds: [0, 23, 47, ...]
         ↓
Scoring:
- For each matched doc:
  - Compute BM25 score
  - Apply field boosts
  - Accumulate final score
         ↓
Collection:
- Collect top-K results via TopDocsCollector
- Sort by score descending
```

#### 4. Result Return

```
TopDocsCollector
         ↓
Vec<ScoreDoc> [
    ScoreDoc { doc_id: DocId(23), score: 3.42 },
    ScoreDoc { doc_id: DocId(0), score: 2.87 },
    ScoreDoc { doc_id: DocId(47), score: 1.95 },
]
         ↓
Return to user
```

### Data Flow Diagram

```
┌─────────────┐
│ Query String│
└──────┬──────┘
       │
       ▼
┌─────────────┐
│   Parser    │ (leit_query)
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ QueryAst    │
└──────┬──────┘
       │
       ▼
┌─────────────────────┐
│ Planner + Dictionary│ (leit_query + index)
└──────┬──────────────┘
       │
       ▼
┌─────────────────┐
│ QueryProgram    │
└──────┬──────────┘
       │
       ▼
┌─────────────────────────────┐
│ ExecutionEngine             │
│  ├─ Postings (leit_postings)│
│  ├─ Scorer (leit_score)     │
│  └─ Collector (leit_collect)│
└──────┬──────────────────────┘
       │
       ▼
┌─────────────────┐
│ Scored Results  │
└─────────────────┘
```

### Workspace Reuse Pattern

```rust
let mut workspace = ExecutionWorkspace::new();

for query in &queries {
    // Reuse workspace across queries
    let results = index.search(query, &mut workspace)?;
    
    // Clear buffers (retains capacity)
    workspace.reset();
}
```

---

## 7. Acceptance Criteria Checklist

### Core Functionality

- [ ] Build in-memory index from entity collection
- [ ] Register fields with boost factors
- [ ] Implement Projection trait for entity types
- [ ] Parse query strings into QueryProgram
- [ ] Execute queries against index
- [ ] Return scored results sorted by relevance
- [ ] Support AND, OR, NOT query operators
- [ ] Support field-specific queries
- [ ] Support phrase queries
- [ ] Support boost factors

### Error Handling

- [ ] Report query parsing errors clearly
- [ ] Handle unregistered field names
- [ ] Handle terms not in dictionary
- [ ] Validate index has documents before query
- [ ] Return concrete error types (not anyhow)

### Performance

- [ ] Workspace reuse reduces allocations by >80%
- [ ] Query execution time <10ms for simple queries on 10K docs
- [ ] Index building time <100ms for 10K simple documents
- [ ] Memory usage bounded and predictable
- [ ] No unnecessary clones in hot path

### Integration

- [ ] Correctly uses leit_core types (TermId, FieldId, DocId)
- [ ] Uses leit_text for tokenization and normalization
- [ ] Uses leit_query for parsing and planning
- [ ] Uses leit_postings for inverted index traversal
- [ ] Uses leit_score for BM25 scoring
- [ ] Uses leit_collect for result gathering

### Testing

- [ ] Unit tests for Projection trait implementations
- [ ] Unit tests for index building
- [ ] Unit tests for query execution
- [ ] Integration tests for end-to-end workflows
- [ ] Property tests for correctness invariants
- [ ] Performance benchmarks for query execution

### Documentation

- [ ] All public types have rustdoc comments
- [ ] All public methods have rustdoc with examples
- [ ] Module-level documentation explains architecture
- [ ] Projection pattern documented with examples
- [ ] Execution pipeline documented with diagrams

---

## 8. Test Plan

### Unit Tests

#### Projection Tests

```rust
#[cfg(test)]
mod projection_tests {
    use super::*;

    struct TestEntity {
        id: u64,
        text: String,
    }

    struct TestProjection;

    impl Projection<TestEntity> for TestProjection {
        fn project<'a>(&'a self, entity: &'a TestEntity) -> Vec<(FieldId, &'a str)> {
            vec![(FieldId(0), &entity.text)]
        }

        fn id(&self, entity: &TestEntity) -> u64 {
            entity.id
        }
    }

    #[test]
    fn test_projection_borrows() {
        let entity = TestEntity {
            id: 42,
            text: "test content".to_string(),
        };
        let projection = TestProjection;
        
        let fields = projection.project(&entity);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].1, "test content");
    }

    #[test]
    fn test_projection_id() {
        let entity = TestEntity {
            id: 123,
            text: "test".to_string(),
        };
        let projection = TestProjection;
        
        assert_eq!(projection.id(&entity), 123);
    }
}
```

#### Index Building Tests

```rust
#[cfg(test)]
mod builder_tests {
    use super::*;

    #[test]
    fn test_builder_register_field() {
        let mut builder = InMemoryIndexBuilder::<TestEntity>::new();
        let field_id = builder.register_field("text", 1.0);
        
        assert_eq!(field_id, FieldId(0));
    }

    #[test]
    fn test_builder_add_document() {
        let mut builder = InMemoryIndexBuilder::new();
        builder.register_field("text", 1.0);
        
        let entity = TestEntity {
            id: 1,
            text: "test content".to_string(),
        };
        
        builder.add::<TestProjection>(&entity).unwrap();
    }

    #[test]
    fn test_builder_stats() {
        let mut builder = InMemoryIndexBuilder::new();
        builder.register_field("text", 1.0);
        
        for i in 0..10 {
            let entity = TestEntity {
                id: i,
                text: "test content".to_string(),
            };
            builder.add::<TestProjection>(&entity).unwrap();
        }
        
        let index = builder.build().unwrap();
        let stats = index.stats();
        
        assert_eq!(stats.doc_count, 10);
    }
}
```

#### Query Execution Tests

```rust
#[cfg(test)]
mod query_tests {
    use super::*;

    fn build_test_index() -> InMemoryIndex<TestEntity> {
        let mut builder = InMemoryIndexBuilder::new();
        builder.register_field("text", 1.0);
        
        let entities = vec![
            TestEntity { id: 1, text: "rust programming".to_string() },
            TestEntity { id: 2, text: "python programming".to_string() },
            TestEntity { id: 3, text: "rust and python".to_string() },
        ];
        
        for entity in &entities {
            builder.add::<TestProjection>(entity).unwrap();
        }
        
        builder.build().unwrap()
    }

    #[test]
    fn test_simple_term_query() {
        let index = build_test_index();
        let mut workspace = ExecutionWorkspace::new();
        
        let results = index.search("rust", &mut workspace).unwrap();
        
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|r| r.doc_id.0 == 0)); // Document 1
        assert!(results.iter().any(|r| r.doc_id.0 == 2)); // Document 3
    }

    #[test]
    fn test_and_query() {
        let index = build_test_index();
        let mut workspace = ExecutionWorkspace::new();
        
        let results = index.search("rust AND programming", &mut workspace).unwrap();
        
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id.0, 0); // Document 1
    }

    #[test]
    fn test_or_query() {
        let index = build_test_index();
        let mut workspace = ExecutionWorkspace::new();
        
        let results = index.search("rust OR python", &mut workspace).unwrap();
        
        assert_eq!(results.len(), 3); // All documents match
    }
}
```

### Integration Tests

#### End-to-End Workflow

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_full_workflow() {
        // 1. Create index builder
        let mut builder = InMemoryIndexBuilder::new();
        builder.register_field("title", 2.0);
        builder.register_field("body", 1.0);
        
        // 2. Add documents
        let docs = vec![
            Document {
                id: 1,
                title: "Rust Programming".to_string(),
                body: "Learn Rust systems programming".to_string(),
            },
            Document {
                id: 2,
                title: "Python Basics".to_string(),
                body: "Introduction to Python".to_string(),
            },
        ];
        
        for doc in &docs {
            builder.add::<DocumentProjection>(doc).unwrap();
        }
        
        // 3. Build index
        let index = builder.build().unwrap();
        
        // 4. Execute query
        let mut workspace = ExecutionWorkspace::new();
        let results = index.search("title:rust OR body:python", &mut workspace).unwrap();
        
        // 5. Verify results
        assert_eq!(results.len(), 2);
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn test_workspace_reuse() {
        let index = build_test_index();
        let mut workspace = ExecutionWorkspace::new();
        
        // Execute multiple queries reusing workspace
        for query in &["rust", "python", "programming"] {
            let results = index.search(query, &mut workspace).unwrap();
            assert!(!results.is_empty());
            workspace.reset();
        }
    }
}
```

### Property Tests

```rust
#[cfg(test)]
mod property_tests {
    use super::*;

    #[test]
    fn test_result_ordering() {
        // Results should be sorted by score descending
        let index = build_test_index();
        let mut workspace = ExecutionWorkspace::new();
        
        let results = index.search("rust", &mut workspace).unwrap();
        
        for i in 1..results.len() {
            assert!(results[i-1].score >= results[i].score);
        }
    }

    #[test]
    fn test_idempotent_queries() {
        // Same query should return same results
        let index = build_test_index();
        let mut workspace = ExecutionWorkspace::new();
        
        let results1 = index.search("rust", &mut workspace).unwrap();
        workspace.reset();
        
        let results2 = index.search("rust", &mut workspace).unwrap();
        
        assert_eq!(results1.len(), results2.len());
        for (r1, r2) in results1.iter().zip(results2.iter()) {
            assert_eq!(r1.doc_id, r2.doc_id);
        }
    }
}
```

### Performance Benchmarks

```rust
#[cfg(test)]
mod benchmarks {
    use super::*;
    use std::time::Instant;

    fn build_large_index(size: usize) -> InMemoryIndex<Document> {
        let mut builder = InMemoryIndexBuilder::new();
        builder.register_field("title", 2.0);
        builder.register_field("body", 1.0);
        
        for i in 0..size {
            let doc = Document {
                id: i as u64,
                title: format!("Document {}", i),
                body: "test content with repeated words ".repeat(10),
            };
            builder.add::<DocumentProjection>(&doc).unwrap();
        }
        
        builder.build().unwrap()
    }

    #[test]
    fn benchmark_query_execution() {
        let index = build_large_index(10_000);
        let mut workspace = ExecutionWorkspace::new();
        
        let start = Instant::now();
        for _ in 0..1_000 {
            let _ = index.search("test", &mut workspace).unwrap();
            workspace.reset();
        }
        let duration = start.elapsed();
        
        println!("Average query time: {:?}", duration / 1_000);
    }

    #[test]
    fn benchmark_index_building() {
        let start = Instant::now();
        let _index = build_large_index(10_000);
        let duration = start.elapsed();
        
        println!("Index build time: {:?}", duration);
    }
}
```

---

## 9. Verification Commands

### Build Verification

```bash
# Build crate
cargo build -p leit_index

# Run unit tests
cargo test -p leit_index

# Run integration tests
cargo test -p leit_index --test integration

# Run documentation tests
cargo test -p leit_index --doc

# Build with optimizations
cargo build -p leit_index --release
```

### Linting and Formatting

```bash
# Format code
cargo fmt -p leit_index

# Check formatting
cargo fmt -p leit_index --check

# Run clippy
cargo clippy -p leit_index -- -D warnings

# Check for unused dependencies
cargo +nightly udeps -p leit_index
```

### Documentation

```bash
# Generate documentation
cargo doc -p leit_index --no-deps --open

# Check documentation coverage
cargo doc -p leit_index --no-deps
```

### Integration Testing

```bash
# Verify all kernel crate dependencies
cargo tree -p leit_index -i leit_core
cargo tree -p leit_index -i leit_text
cargo tree -p leit_index -i leit_query
cargo tree -p leit_index -i leit_postings
cargo tree -p leit_index -i leit_score
cargo tree -p leit_index -i leit_collect

# Run full test suite
cargo test -p leit_index --all-features

# Run with sanitizers (nightly)
cargo test -p leit_index --all-features -Zsanitizer=address

# Run benchmarks (requires nightly)
cargo bench -p leit_index
```

### Dependency Verification

```bash
# Check dependency graph
cargo tree -p leit_index

# Verify no circular dependencies
cargo tree -p leit_index --duplicates

# Check for crate features
cargo metadata --format-version 1 | jq '.packages[] | select(.name=="leit_index") | .features'
```

---

## 10. Vertical Slice Example

### Complete Working Example

This example demonstrates the full leit_index API in action.

```rust
use leit_core::{DocId, FieldId, Score};
use leit_index::{
    InMemoryIndexBuilder, InMemoryIndex, 
    Projection, ExecutionWorkspace, IndexError,
};

// User's entity type
struct BlogPost {
    id: u64,
    title: String,
    body: String,
    tags: Vec<String>,
}

// Projection implementation for BlogPost
struct BlogPostProjection;

impl Projection<BlogPost> for BlogPostProjection {
    fn project<'a>(&'a self, post: &'a BlogPost) -> Vec<(FieldId, &'a str)> {
        let mut fields = vec![
            (FieldId(0), &post.title),
            (FieldId(1), &post.body),
        ];
        
        // Add tags as searchable fields
        for tag in &post.tags {
            fields.push((FieldId(2), tag.as_str()));
        }
        
        fields
    }
    
    fn id(&self, post: &BlogPost) -> u64 {
        post.id
    }
    
    fn field_boost(&self, field: FieldId) -> f32 {
        match field.0 {
            0 => 2.0,  // Title weighted 2x
            1 => 1.0,  // Body default weight
            2 => 1.5,  // Tags weighted 1.5x
            _ => 1.0,
        }
    }
}

fn main() -> Result<(), IndexError> {
    // 1. Create index builder
    let mut builder = InMemoryIndexBuilder::new();
    
    // 2. Register fields
    let title_field = builder.register_field("title", 2.0);
    let body_field = builder.register_field("body", 1.0);
    let tags_field = builder.register_field("tags", 1.5);
    
    // 3. Create sample documents
    let posts = vec![
        BlogPost {
            id: 1,
            title: "Introduction to Rust".to_string(),
            body: "Rust is a systems programming language focused on safety and performance".to_string(),
            tags: vec!["rust".to_string(), "programming".to_string()],
        },
        BlogPost {
            id: 2,
            title: "Learning Python".to_string(),
            body: "Python is great for beginners and data science".to_string(),
            tags: vec!["python".to_string(), "beginner".to_string()],
        },
        BlogPost {
            id: 3,
            title: "Rust vs Python Comparison".to_string(),
            body: "Comparing Rust and Python for different use cases".to_string(),
            tags: vec!["rust".to_string(), "python".to_string(), "comparison".to_string()],
        },
    ];
    
    // 4. Index documents
    for post in &posts {
        builder.add::<BlogPostProjection>(post)?;
    }
    
    // 5. Build index
    let index = builder.build()?;
    
    // 6. Print index statistics
    let stats = index.stats();
    println!("Index Statistics:");
    println!("  Documents: {}", stats.doc_count);
    println!("  Terms: {}", stats.term_count);
    println!("  Avg doc length: {:.2}", stats.avg_doc_len);
    println!();
    
    // 7. Execute queries
    let mut workspace = ExecutionWorkspace::new();
    
    let queries = vec![
        "rust",
        "title:introduction",
        "rust AND python",
        "rust OR performance",
        "title:rust AND body:safety",
    ];
    
    for query in &queries {
        println!("Query: {}", query);
        
        match index.search(query, &mut workspace) {
            Ok(results) => {
                println!("  Found {} results:", results.len());
                for (i, result) in results.iter().take(5).enumerate() {
                    let post = index.get(result.doc_id).unwrap();
                    println!("    {}. [Score: {:.2}] {} - {}", 
                        i + 1,
                        result.score,
                        post.title,
                        post.tags.join(", ")
                    );
                }
            }
            Err(e) => {
                println!("  Error: {:?}", e);
            }
        }
        
        workspace.reset();
        println!();
    }
    
    // 8. Example: Custom collector for top 3 results
    use leit_collect::TopDocsCollector;
    
    let query = "rust OR python";
    println!("Top 3 results for '{}':", query);
    
    let mut collector = TopDocsCollector::new(3);
    let results = index.search_with_collector(query, &mut collector, &mut workspace)?;
    
    for (i, result) in results.iter().enumerate() {
        let post = index.get(result.doc_id).unwrap();
        println!("  {}. [Score: {:.2}] {}", i + 1, result.score, post.title);
    }
    
    Ok(())
}
```

### Expected Output

```
Index Statistics:
  Documents: 3
  Terms: 24
  Avg doc length: 12.67

Query: rust
  Found 2 results:
    1. [Score: 1.42] Introduction to Rust - rust, programming
    2. [Score: 0.89] Rust vs Python Comparison - rust, python, comparison

Query: title:introduction
  Found 1 results:
    1. [Score: 2.15] Introduction to Rust - rust, programming

Query: rust AND python
  Found 1 results:
    1. [Score: 1.23] Rust vs Python Comparison - rust, python, comparison

Query: rust OR performance
  Found 2 results:
    1. [Score: 1.42] Introduction to Rust - rust, programming
    2. [Score: 0.95] Rust vs Python Comparison - rust, python, comparison

Query: title:rust AND body:safety
  Found 1 results:
    1. [Score: 1.87] Introduction to Rust - rust, programming

Top 3 results for 'rust OR python':
  1. [Score: 1.42] Introduction to Rust
  2. [Score: 1.12] Rust vs Python Comparison
  3. [Score: 0.98] Learning Python
```

---

## 11. Implementation Notes

### Phase 1 Scope

The initial implementation focuses on:

1. Core index building with Projection trait
2. Basic query execution (AND, OR, NOT)
3. In-memory storage only
4. Single-threaded execution
5. BM25 scoring
6. Top-K result collection

### Future Work (Post-Phase 1)

- Persistent index storage
- Real-time index updates
- Multi-threaded query execution
- Advanced query operators (phrases, ranges, wildcards)
- Query caching and optimization
- Index statistics and analytics
- Fielded search with multiple fields
- Custom scoring models

### Architecture Alignment

This crate follows Leit's architectural principles:

- **Integration layer:** Composes all kernel components
- **Type-safe projections:** Compile-time guarantees for entity indexing
- **Allocation discipline:** Reusable workspace minimizes allocations
- **Concrete errors:** Enum-based error handling
- **Clear separation:** Building vs. querying concerns

### Relationship to Other Crates

- **leit_core:** Provides core types (TermId, FieldId, DocId)
- **leit_text:** Provides tokenization and normalization
- **leit_query:** Provides query parsing and planning
- **leit_postings:** Provides inverted index traversal
- **leit_score:** Provides scoring algorithms
- **leit_collect:** Provides result collection strategies
- **leit_index:** Orchestrates all components into cohesive API
