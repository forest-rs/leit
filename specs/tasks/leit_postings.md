# leit_postings Crate Specification

## 1. Overview and Purpose

The `leit_postings` crate provides inverted index data structures and cursor-based traversal APIs for the Leif IR system. It serves as the core posting list storage and access layer, enabling efficient query execution through layered cursor abstractions.

### Core Responsibilities

- **Inverted Index Storage:** Store term-to-document mappings with compressed posting lists
- **Posting List Management:** Create, read, and manage posting lists for individual terms
- **Cursor-Based Traversal:** Provide layered cursor traits for efficient posting list navigation
- **Memory-Efficient Access:** Support both in-memory and file-backed posting list access patterns
- **Compression Support:** Enable delta encoding and bit-packing for space-efficient storage

### Design Philosophy

- **Layered Abstraction:** Three-tier cursor design (DocCursor → TfCursor → BlockCursor) for flexibility
- **Zero-Copy Views:** File-backed segments provide views without loading entire posting lists
- **no_std + alloc:** Kernel crate portable to embedded environments
- **Trait-Based Extension:** Pluggable storage backends through trait abstraction
- **Efficient Merging:** In-memory structures optimized for segment building and merging

### Non-Goals

- Query planning and optimization (handled by `leit_query`)
- Scoring and ranking algorithms (handled by `leit_score`)
- Index lifecycle management (handled by `leit_index`)
- Distributed index replication (handled by higher-level crates)

## 2. Dependencies

### Required Dependencies

**leit_core:**
- `TermId` — Canonicalized term identifier
- `SegmentId` — Segment identifier for multi-segment queries
- `EntityId` — Generic entity identifier trait
- `CoreError` — Core error types
- `Score` — Relevance score type

**alloc (for no_std):**
- `Vec`, `String`, `Box` for dynamic collections
- `collections::BTreeMap` for term dictionary storage

### Optional Dependencies

**std (feature flag):**
- `std::fs` — File I/O for persistent segment storage
- `std::io` — Read/write traits for file-backed posting lists
- `std::path` — Path handling for segment file management

### Dev Dependencies

- `proptest` — Property-based testing for cursor invariants
- `criterion` — Benchmarking suite for cursor performance

## 3. Target Configuration

### Primary Target: `no_std + alloc` (Kernel)

**Purpose:** Embedded and WASM environments where standard library is unavailable

**Configuration:**
```toml
[dependencies]
alloc = "1"
leit_core = { path = "../leit_core", default-features = false }

[features]
default = ["std"]
std = ["leit_core/std", "dep:std"]
```

**Constraints:**
- No use of `std::fs`, `std::io`, `std::thread`, `std::net`
- All collections use `alloc::vec`, `alloc::string`, `alloc::collections`
- Error handling uses concrete enums, not `anyhow` or `Box<dyn Error>`
- APIs work with borrowed data where practical

### Secondary Target: `std` (Storage)

**Purpose:** Full-featured environments with file system access

**Capabilities:**
- File-backed segment storage
- Memory-mapped posting list access
- Persistent segment metadata
- File I/O error handling

## 4. Public API Specification

### 4.1 Core Data Structures

#### Posting

Represents a single document occurrence in a posting list.

```rust
/// A single posting in an inverted index
/// 
/// Contains the document identifier and term frequency information.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct Posting {
    /// Document identifier
    pub doc_id: u32,
    
    /// Term frequency in this document
    pub tf: u32,
}

impl Posting {
    /// Creates a new posting
    pub const fn new(doc_id: u32, tf: u32) -> Self;
    
    /// Returns the document identifier
    pub const fn doc_id(&self) -> u32;
    
    /// Returns the term frequency
    pub const fn tf(&self) -> u32;
    
    /// Creates a minimal posting with tf=1
    pub const fn minimal(doc_id: u32) -> Self;
}

impl PartialOrd for Posting {
    /// Postings are compared by doc_id (ascending)
    fn partial_cmp(&self, other: &Self) -> Option<Ordering>;
}

impl Ord for Posting {
    /// Postings are ordered by doc_id (ascending)
    fn cmp(&self, other: &Self) -> Ordering;
}
```

#### PostingsList

Represents a complete posting list for a single term.

```rust
/// A complete posting list for a single term
/// 
/// Contains sorted postings for all documents containing the term.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PostingsList {
    /// The term this posting list belongs to
    pub term_id: TermId,
    
    /// Sorted postings for this term
    pub postings: Vec<Posting>,
    
    /// Total number of documents containing this term
    pub doc_freq: u32,
    
    /// Total term frequency across all documents
    pub total_tf: u32,
}

impl PostingsList {
    /// Creates a new empty posting list for the given term
    pub fn new(term_id: TermId) -> Self;
    
    /// Adds a posting to the list, maintaining sort order
    /// 
    /// # Panics
    /// Panics if postings are not added in doc_id order
    pub fn insert(&mut self, posting: Posting);
    
    /// Returns the number of postings in this list
    pub fn len(&self) -> usize;
    
    /// Returns true if this posting list is empty
    pub fn is_empty(&self) -> bool;
    
    /// Returns the document frequency (number of documents)
    pub fn doc_freq(&self) -> u32;
    
    /// Returns the total term frequency
    pub fn total_tf(&self) -> u32;
    
    /// Compresses this posting list using delta encoding
    pub fn compress(&mut self);
    
    /// Decompresses this posting list from delta encoding
    pub fn decompress(&mut self);
    
    /// Creates a cursor for traversing this posting list
    pub fn cursor(&self) -> InMemoryCursor;
}

impl IntoIterator for PostingsList {
    type Item = Posting;
    type IntoIter = alloc::vec::IntoIter<Posting>;
    
    fn into_iter(self) -> Self::IntoIter;
}
```

#### TermDictionary

Maps term identifiers to their posting lists.

```rust
/// Dictionary mapping term identifiers to posting lists
/// 
/// Provides efficient lookup and iteration over indexed terms.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TermDictionary {
    /// Map from term ID to posting list
    entries: BTreeMap<TermId, PostingsList>,
    
    /// Total number of unique terms
    num_terms: usize,
    
    /// Total number of postings across all terms
    num_postings: usize,
}

impl TermDictionary {
    /// Creates a new empty term dictionary
    pub fn new() -> Self;
    
    /// Returns the number of terms in the dictionary
    pub fn len(&self) -> usize;
    
    /// Returns true if the dictionary is empty
    pub fn is_empty(&self) -> bool;
    
    /// Inserts a posting list for the given term
    /// 
    /// # Returns
    /// - `Ok(())` if the insertion succeeded
    /// - `Err(CoreError)` if a posting list already exists for this term
    pub fn insert(&mut self, term_id: TermId, postings: PostingsList) -> Result<(), CoreError>;
    
    /// Gets the posting list for the given term, if it exists
    pub fn get(&self, term_id: TermId) -> Option<&PostingsList>;
    
    /// Gets a mutable reference to the posting list for the given term
    pub fn get_mut(&mut self, term_id: TermId) -> Option<&mut PostingsList>;
    
    /// Removes the posting list for the given term
    pub fn remove(&mut self, term_id: TermId) -> Option<PostingsList>;
    
    /// Returns an iterator over all term IDs in the dictionary
    pub fn iter_terms(&self) -> impl Iterator<Item = TermId> + '_;
    
    /// Returns the total number of postings across all terms
    pub fn total_postings(&self) -> usize;
    
    /// Merges another dictionary into this one
    /// 
    /// # Returns
    /// - `Ok(())` if the merge succeeded
    /// - `Err(CoreError)` if there are conflicting term IDs
    pub fn merge(&mut self, other: TermDictionary) -> Result<(), CoreError>;
}

impl Default for TermDictionary {
    fn default() -> Self;
}

impl IntoIterator for TermDictionary {
    type Item = (TermId, PostingsList);
    type IntoIter = alloc::collections::btree_map::IntoIter<TermId, PostingsList>;
    
    fn into_iter(self) -> Self::IntoIter;
}
```

### 4.2 Cursor Traits

#### DocCursor

Lowest-level cursor for document-level traversal.

```rust
/// Low-level cursor for document-level posting list traversal
/// 
/// Provides sequential access to document IDs in a posting list.
/// This is the most basic cursor layer.
pub trait DocCursor: Sized {
    /// Error type for cursor operations
    type Error: Into<CoreError>;
    
    /// Creates a new cursor for the given posting list
    fn new(postings: &PostingsList) -> Result<Self, Self::Error>
    where
        Self: Sized;
    
    /// Advances the cursor to the next document
    /// 
    /// # Returns
    /// - `Ok(Some(doc_id))` if there is a next document
    /// - `Ok(None)` if we've reached the end of the posting list
    /// - `Err(e)` if an error occurred
    fn next(&mut self) -> Result<Option<u32>, Self::Error>;
    
    /// Seeks to the first document >= target_doc_id
    /// 
    /// # Returns
    /// - `Ok(Some(doc_id))` if a document was found
    /// - `Ok(None)` if no document >= target_doc_id exists
    /// - `Err(e)` if an error occurred
    fn seek(&mut self, target_doc_id: u32) -> Result<Option<u32>, Self::Error>;
    
    /// Returns the current document ID without advancing
    /// 
    /// # Returns
    /// - `Ok(Some(doc_id))` if we're positioned at a document
    /// - `Ok(None)` if we're at the end or haven't started
    /// - `Err(e)` if an error occurred
    fn current(&self) -> Result<Option<u32>, Self::Error>;
    
    /// Resets the cursor to the beginning of the posting list
    fn reset(&mut self) -> Result<(), Self::Error>;
    
    /// Returns the total number of documents in this posting list
    fn len(&self) -> usize;
    
    /// Returns true if the cursor is at the end of the posting list
    fn is_exhausted(&self) -> bool;
}
```

#### TfCursor

Mid-level cursor that adds term frequency access.

```rust
/// Mid-level cursor that adds term frequency access
/// 
/// Extends DocCursor with access to term frequencies for each document.
/// Useful for scoring algorithms that need TF information.
pub trait TfCursor: DocCursor {
    /// Returns the term frequency for the current document
    /// 
    /// # Returns
    /// - `Ok(tf)` if we're positioned at a document
    /// - `Err(CoreError)` if we're at the end or haven't started
    fn tf(&self) -> Result<u32, Self::Error>;
    
    /// Returns both the current document ID and term frequency
    /// 
    /// # Returns
    /// - `Ok(Some((doc_id, tf)))` if we're positioned at a document
    /// - `Ok(None)` if we're at the end
    /// - `Err(e)` if an error occurred
    fn current_with_tf(&self) -> Result<Option<(u32, u32)>, Self::Error>;
    
    /// Advances to the next document and returns (doc_id, tf)
    /// 
    /// # Returns
    /// - `Ok(Some((doc_id, tf)))` if there is a next document
    /// - `Ok(None)` if we've reached the end
    /// - `Err(e)` if an error occurred
    fn next_with_tf(&mut self) -> Result<Option<(u32, u32)>, Self::Error>;
    
    /// Seeks to the first document >= target_doc_id and returns (doc_id, tf)
    /// 
    /// # Returns
    /// - `Ok(Some((doc_id, tf)))` if a document was found
    /// - `Ok(None)` if no document >= target_doc_id exists
    /// - `Err(e)` if an error occurred
    fn seek_with_tf(&mut self, target_doc_id: u32) -> Result<Option<(u32, u32)>, Self::Error>;
}
```

#### BlockCursor

High-level cursor for block-wise access.

```rust
/// High-level cursor for block-wise posting list access
/// 
/// Extends TfCursor with batch access to posting blocks.
/// Useful for optimizing sequential access patterns.
pub trait BlockCursor: TfCursor {
    /// Size of posting blocks
    const BLOCK_SIZE: usize = 128;
    
    /// Posting block type
    type Block: AsRef<[Posting]>;
    
    /// Returns the next block of postings
    /// 
    /// # Returns
    /// - `Ok(Some(block))` if there is a next block
    /// - `Ok(None)` if we've reached the end
    /// - `Err(e)` if an error occurred
    fn next_block(&mut self) -> Result<Option<Self::Block>, Self::Error>;
    
    /// Returns the current block without advancing
    /// 
    /// # Returns
    /// - `Ok(Some(block))` if we're positioned at a block
    /// - `Ok(None)` if we're at the end
    /// - `Err(e)` if an error occurred
    fn current_block(&self) -> Result<Option<Self::Block>, Self::Error>;
    
    /// Returns the number of blocks in this posting list
    fn num_blocks(&self) -> usize;
    
    /// Returns the current block index (0-based)
    fn current_block_index(&self) -> usize;
}
```

### 4.3 In-Memory Implementation

#### InMemoryPostings

In-memory storage for posting lists.

```rust
/// In-memory storage for posting lists
/// 
/// Provides a simple heap-based implementation of posting list storage.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InMemoryPostings {
    /// Term dictionary mapping term IDs to posting lists
    dictionary: TermDictionary,
    
    /// Total number of documents in this index
    num_docs: u32,
    
    /// Next available document ID
    next_doc_id: u32,
}

impl InMemoryPostings {
    /// Creates a new empty in-memory posting store
    pub fn new() -> Self;
    
    /// Returns the number of terms in the index
    pub fn num_terms(&self) -> usize;
    
    /// Returns the total number of documents
    pub fn num_docs(&self) -> u32;
    
    /// Returns the total number of postings across all terms
    pub fn total_postings(&self) -> usize;
    
    /// Inserts a posting list for the given term
    pub fn insert(&mut self, term_id: TermId, postings: PostingsList) -> Result<(), CoreError>;
    
    /// Gets the posting list for the given term
    pub fn get(&self, term_id: TermId) -> Option<&PostingsList>;
    
    /// Gets a mutable reference to the posting list for the given term
    pub fn get_mut(&mut self, term_id: TermId) -> Option<&mut PostingsList>;
    
    /// Returns the term dictionary
    pub fn dictionary(&self) -> &TermDictionary;
    
    /// Returns a mutable reference to the term dictionary
    pub fn dictionary_mut(&mut self) -> &mut TermDictionary;
    
    /// Registers a new document and returns its ID
    pub fn add_document(&mut self) -> u32;
    
    /// Clears all posting lists and resets document IDs
    pub fn clear(&mut self);
    
    /// Returns the total size in bytes (approximate)
    pub fn size_bytes(&self) -> usize;
}

impl Default for InMemoryPostings {
    fn default() -> Self;
}
```

#### InMemoryCursor

In-memory cursor implementation.

```rust
/// In-memory cursor implementation
/// 
/// Provides efficient traversal of in-memory posting lists.
#[derive(Clone, Debug)]
pub struct InMemoryCursor {
    /// Reference to the posting list being traversed
    postings: Vec<Posting>,
    
    /// Current position in the posting list
    position: usize,
    
    /// Total length of the posting list
    length: usize,
}

impl InMemoryCursor {
    /// Creates a new cursor for the given posting list
    pub fn new(postings: &PostingsList) -> Result<Self, CoreError>;
    
    /// Returns the current position (0-based index)
    pub fn position(&self) -> usize;
}

impl DocCursor for InMemoryCursor {
    type Error = CoreError;
    
    fn new(postings: &PostingsList) -> Result<Self, Self::Error>;
    
    fn next(&mut self) -> Result<Option<u32>, Self::Error>;
    
    fn seek(&mut self, target_doc_id: u32) -> Result<Option<u32>, Self::Error>;
    
    fn current(&self) -> Result<Option<u32>, Self::Error>;
    
    fn reset(&mut self) -> Result<(), Self::Error>;
    
    fn len(&self) -> usize;
    
    fn is_exhausted(&self) -> bool;
}

impl TfCursor for InMemoryCursor {
    fn tf(&self) -> Result<u32, Self::Error>;
    
    fn current_with_tf(&self) -> Result<Option<(u32, u32)>, Self::Error>;
    
    fn next_with_tf(&mut self) -> Result<Option<(u32, u32)>, Self::Error>;
    
    fn seek_with_tf(&mut self, target_doc_id: u32) -> Result<Option<(u32, u32)>, Self::Error>;
}

impl BlockCursor for InMemoryCursor {
    const BLOCK_SIZE: usize = 128;
    
    type Block = Vec<Posting>;
    
    fn next_block(&mut self) -> Result<Option<Self::Block>, Self::Error>;
    
    fn current_block(&self) -> Result<Option<Self::Block>, Self::Error>;
    
    fn num_blocks(&self) -> usize;
    
    fn current_block_index(&self) -> usize;
}
```

### 4.4 Segment Abstractions

#### SegmentMeta

Metadata about an index segment.

```rust
/// Metadata about an index segment
/// 
/// Contains information about a segment's size, document count, and posting list statistics.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SegmentMeta {
    /// Unique segment identifier
    pub segment_id: SegmentId,
    
    /// Number of documents in this segment
    pub num_docs: u32,
    
    /// Number of unique terms in this segment
    pub num_terms: usize,
    
    /// Total number of postings across all terms
    pub num_postings: usize;
    
    /// Base document ID for this segment (for multi-segment indexes)
    pub base_doc_id: u32,
    
    /// Segment creation timestamp (if available)
    pub created_at: Option<u64>,
    
    /// Segment size in bytes (if known)
    pub size_bytes: Option<usize>,
}

impl SegmentMeta {
    /// Creates a new segment metadata
    pub fn new(segment_id: SegmentId, num_docs: u32) -> Self;
    
    /// Returns the segment ID
    pub const fn segment_id(&self) -> SegmentId;
    
    /// Returns the number of documents
    pub const fn num_docs(&self) -> u32;
    
    /// Returns the number of unique terms
    pub const fn num_terms(&self) -> usize;
    
    /// Returns the total number of postings
    pub const fn num_postings(&self) -> usize;
    
    /// Returns the base document ID
    pub const fn base_doc_id(&self) -> u32;
    
    /// Returns true if this segment is empty
    pub const fn is_empty(&self) -> bool;
}
```

#### SegmentView

Trait for read-only access to segment data.

```rust
/// Read-only view into an index segment
/// 
/// Provides abstracted access to segment data, supporting both
/// in-memory and file-backed storage backends.
pub trait SegmentView: Send + Sync {
    /// Error type for operations
    type Error: Into<CoreError>;
    
    /// Returns the segment metadata
    fn meta(&self) -> &SegmentMeta;
    
    /// Returns the term dictionary for this segment
    fn dictionary(&self) -> &TermDictionary;
    
    /// Gets the posting list for the given term
    fn postings(&self, term_id: TermId) -> Result<Option<&PostingsList>, Self::Error>;
    
    /// Creates a cursor for traversing the given term's posting list
    fn cursor(&self, term_id: TermId) -> Result<Option<InMemoryCursor>, Self::Error>;
    
    /// Returns the document frequency for the given term
    fn doc_freq(&self, term_id: TermId) -> Result<Option<u32>, Self::Error>;
    
    /// Returns the total term frequency for the given term
    fn total_tf(&self, term_id: TermId) -> Result<Option<u32>, Self::Error>;
    
    /// Returns an iterator over all term IDs in this segment
    fn iter_terms(&self) -> impl Iterator<Item = TermId> + '_;
    
    /// Returns true if this segment contains the given term
    fn contains_term(&self, term_id: TermId) -> bool;
    
    /// Returns true if the given document exists in this segment
    fn contains_doc(&self, doc_id: u32) -> bool;
    
    /// Returns the approximate size in bytes
    fn size_bytes(&self) -> usize;
}

impl SegmentView for InMemoryPostings {
    type Error = CoreError;
    
    fn meta(&self) -> &SegmentMeta;
    
    fn dictionary(&self) -> &TermDictionary;
    
    fn postings(&self, term_id: TermId) -> Result<Option<&PostingsList>, Self::Error>;
    
    fn cursor(&self, term_id: TermId) -> Result<Option<InMemoryCursor>, Self::Error>;
    
    fn doc_freq(&self, term_id: TermId) -> Result<Option<u32>, Self::Error>;
    
    fn total_tf(&self, term_id: TermId) -> Result<Option<u32>, Self::Error>;
    
    fn iter_terms(&self) -> impl Iterator<Item = TermId> + '_;
    
    fn contains_term(&self, term_id: TermId) -> bool;
    
    fn contains_doc(&self, doc_id: u32) -> bool;
    
    fn size_bytes(&self) -> usize;
}
```

## 5. Layered Cursor Design

### Architecture Overview

The cursor system uses a three-layer design to provide flexibility for different access patterns:

```
┌─────────────────────────────────────────────────────────────┐
│                    BlockCursor                              │
│  • Batch access (128 postings per block)                    │
│  • Optimized for sequential reads                           │
│  • Reduces branching overhead                               │
└─────────────────────────────────────────────────────────────┘
                           ▲
                           │
┌─────────────────────────────────────────────────────────────┐
│                     TfCursor                                │
│  • Document ID + term frequency access                      │
│  • Required for scoring algorithms                          │
│  • Provides (doc_id, tf) pairs                              │
└─────────────────────────────────────────────────────────────┘
                           ▲
                           │
┌─────────────────────────────────────────────────────────────┐
│                    DocCursor                                │
│  • Basic document ID access                                 │
│  • Sequential and random seek operations                    │
│  • Minimal overhead for existence checks                    │
└─────────────────────────────────────────────────────────────┘
```

### Layer Responsibilities

**DocCursor (Layer 1):**
- Core document ID iteration
- Seek operations for skip-list optimization
- Position tracking
- End-of-list detection

**TfCursor (Layer 2):**
- Extends DocCursor with term frequency
- Scoring algorithm support (BM25, TF-IDF)
- Combined (doc_id, tf) access patterns
- Avoids redundant lookups

**BlockCursor (Layer 3):**
- Batch processing for vectorized operations
- Reduced branching for sequential access
- Cache-friendly access patterns
- SIMD-friendly data layout

### Implementation Strategy

**Blanket Implementations:**
```rust
// All BlockCursor implementations are automatically TfCursor
impl<C: BlockCursor> TfCursor for C {
    // Default implementations delegate to lower-level methods
}

// All TfCursor implementations are automatically DocCursor
impl<C: TfCursor> DocCursor for C {
    // Default implementations delegate to lower-level methods
}
```

**Cursor Usage Patterns:**

1. **Existence Checking (DocCursor only):**
   ```rust
   let mut cursor = postings.cursor()?;
   if let Some(doc_id) = cursor.seek(target_doc_id)? {
       // Document exists
   }
   ```

2. **Scoring (TfCursor):**
   ```rust
   let mut cursor = postings.cursor()?;
   while let Some((doc_id, tf)) = cursor.next_with_tf()? {
       let score = calculate_bm25(tf, cursor.len());
   }
   ```

3. **Batch Processing (BlockCursor):**
   ```rust
   let mut cursor = postings.cursor()?;
   while let Some(block) = cursor.next_block()? {
       for posting in block.as_ref() {
           // Process batch
       }
   }
   ```

## 6. Feature Flags

### std Feature

**Purpose:** Enable file-backed storage and standard library support

**Dependencies Added:**
```toml
[features]
std = ["leit_core/std"]
```

**Capabilities Enabled:**
- File I/O for persistent segment storage
- Memory-mapped posting list access
- Standard library error conversions
- Enhanced testing utilities

### Default Configuration

```toml
[features]
default = ["std"]
```

**Rationale:** Most users will want file-backed storage. Embedded users can opt-out with `default-features = false`.

## 7. Acceptance Criteria Checklist

### Core Functionality
- [ ] `Posting` type stores doc_id and tf with proper ordering
- [ ] `PostingsList` maintains sorted posting order
- [ ] `PostingsList::insert()` validates doc_id ordering
- [ ] `PostingsList` provides accurate doc_freq and total_tf statistics
- [ ] `TermDictionary` manages term-to-posting mappings correctly
- [ ] `TermDictionary::merge()` handles conflicting term IDs appropriately

### Cursor Traits
- [ ] `DocCursor` provides sequential and seek operations
- [ ] `TfCursor` extends DocCursor with term frequency access
- [ ] `BlockCursor` provides batch access with configurable block size
- [ ] All cursor traits use consistent error types
- [ ] Cursor traits are object-safe where applicable

### In-Memory Implementation
- [ ] `InMemoryPostings` stores and retrieves posting lists
- [ ] `InMemoryCursor` implements all three cursor traits
- [ ] `InMemoryCursor::seek()` uses binary search for efficiency
- [ ] `InMemoryCursor` maintains position tracking correctly

### Segment Abstractions
- [ ] `SegmentMeta` provides accurate segment statistics
- [ ] `SegmentView` trait abstracts storage backends
- [ ] `InMemoryPostings` implements `SegmentView`
- [ ] `SegmentView` operations handle missing terms gracefully

### no_std Compatibility
- [ ] Crate compiles with `--no-default-features`
- [ ] All types work without `std` feature
- [ ] Error handling uses concrete enums, not `std::error::Error`
- [ ] Collections use `alloc` crate variants

### Compression
- [ ] `PostingsList::compress()` applies delta encoding
- [ ] `PostingsList::decompress()` reverses delta encoding
- [ ] Compression reduces posting list size significantly
- [ ] Compressed posting lists can be traversed correctly

## 8. Test Plan

### 8.1 Unit Tests

#### Posting Type Tests
```rust
#[test]
fn test_posting_creation() {
    let posting = Posting::new(100, 5);
    assert_eq!(posting.doc_id(), 100);
    assert_eq!(posting.tf(), 5);
}

#[test]
fn test_posting_ordering() {
    let p1 = Posting::new(100, 5);
    let p2 = Posting::new(200, 3);
    assert!(p1 < p2);  // Ordered by doc_id
}
```

#### PostingsList Tests
```rust
#[test]
fn test_postings_list_insert_ordered() {
    let mut list = PostingsList::new(TermId::new(1));
    list.insert(Posting::new(100, 5));
    list.insert(Posting::new(200, 3));
    list.insert(Posting::new(150, 4));
    
    // Should be sorted by doc_id
    assert_eq!(list.postings[0].doc_id(), 100);
    assert_eq!(list.postings[1].doc_id(), 150);
    assert_eq!(list.postings[2].doc_id(), 200);
}

#[test]
#[should_panic]
fn test_postings_list_insert_unordered_panics() {
    let mut list = PostingsList::new(TermId::new(1));
    list.insert(Posting::new(200, 3));
    list.insert(Posting::new(100, 5));  // Should panic
}
```

#### TermDictionary Tests
```rust
#[test]
fn test_term_dictionary_insert_and_retrieve() {
    let mut dict = TermDictionary::new();
    let term_id = TermId::new(1);
    let postings = PostingsList::new(term_id);
    
    dict.insert(term_id, postings).unwrap();
    assert!(dict.get(term_id).is_some());
}

#[test]
fn test_term_dictionary_conflict_handling() {
    let mut dict = TermDictionary::new();
    let term_id = TermId::new(1);
    
    dict.insert(term_id, PostingsList::new(term_id)).unwrap();
    let result = dict.insert(term_id, PostingsList::new(term_id));
    assert!(result.is_err());
}
```

### 8.2 Cursor Traversal Tests

#### DocCursor Tests
```rust
#[test]
fn test_doc_cursor_sequential_traversal() {
    let mut list = PostingsList::new(TermId::new(1));
    list.insert(Posting::new(100, 5));
    list.insert(Posting::new(200, 3));
    list.insert(Posting::new(300, 7));
    
    let mut cursor = list.cursor();
    
    assert_eq!(cursor.next().unwrap(), Some(100));
    assert_eq!(cursor.next().unwrap(), Some(200));
    assert_eq!(cursor.next().unwrap(), Some(300));
    assert_eq!(cursor.next().unwrap(), None);
    assert!(cursor.is_exhausted());
}

#[test]
fn test_doc_cursor_seek_forward() {
    let mut list = PostingsList::new(TermId::new(1));
    list.insert(Posting::new(100, 5));
    list.insert(Posting::new(200, 3));
    list.insert(Posting::new(300, 7));
    
    let mut cursor = list.cursor();
    
    // Seek to doc_id 250
    assert_eq!(cursor.seek(250).unwrap(), Some(300));
    assert_eq!(cursor.current().unwrap(), Some(300));
}

#[test]
fn test_doc_cursor_seek_backward() {
    let mut list = PostingsList::new(TermId::new(1));
    list.insert(Posting::new(100, 5));
    list.insert(Posting::new(200, 3));
    list.insert(Posting::new(300, 7));
    
    let mut cursor = list.cursor();
    
    // Advance past first posting
    cursor.next().unwrap();
    
    // Seek backward to doc_id 150 (should find 100 from restart)
    assert_eq!(cursor.seek(150).unwrap(), Some(200));
}

#[test]
fn test_doc_cursor_reset() {
    let mut list = PostingsList::new(TermId::new(1));
    list.insert(Posting::new(100, 5));
    list.insert(Posting::new(200, 3));
    
    let mut cursor = list.cursor();
    
    cursor.next().unwrap();
    cursor.next().unwrap();
    assert!(cursor.is_exhausted());
    
    cursor.reset().unwrap();
    assert!(!cursor.is_exhausted());
    assert_eq!(cursor.current().unwrap(), Some(100));
}
```

#### TfCursor Tests
```rust
#[test]
fn test_tf_cursor_access() {
    let mut list = PostingsList::new(TermId::new(1));
    list.insert(Posting::new(100, 5));
    list.insert(Posting::new(200, 3));
    
    let mut cursor = list.cursor();
    
    assert_eq!(cursor.next_with_tf().unwrap(), Some((100, 5)));
    assert_eq!(cursor.next_with_tf().unwrap(), Some((200, 3)));
    assert_eq!(cursor.next_with_tf().unwrap(), None);
}

#[test]
fn test_tf_cursor_current_tf() {
    let mut list = PostingsList::new(TermId::new(1));
    list.insert(Posting::new(100, 5));
    
    let mut cursor = list.cursor();
    cursor.next().unwrap();
    
    assert_eq!(cursor.tf().unwrap(), 5);
}
```

#### BlockCursor Tests
```rust
#[test]
fn test_block_cursor_traversal() {
    let mut list = PostingsList::new(TermId::new(1));
    for i in 0..256 {
        list.insert(Posting::new(i * 10, 1));
    }
    
    let mut cursor = list.cursor();
    
    // First block should contain 128 postings
    let block1 = cursor.next_block().unwrap().unwrap();
    assert_eq!(block1.as_ref().len(), 128);
    assert_eq!(block1.as_ref()[0].doc_id(), 0);
    assert_eq!(block1.as_ref()[127].doc_id(), 1270);
    
    // Second block should contain remaining 128 postings
    let block2 = cursor.next_block().unwrap().unwrap();
    assert_eq!(block2.as_ref().len(), 128);
    assert_eq!(block2.as_ref()[0].doc_id(), 1280);
    assert_eq!(block2.as_ref()[127].doc_id(), 2550);
    
    // No more blocks
    assert!(cursor.next_block().unwrap().is_none());
}

#[test]
fn test_block_cursor_metadata() {
    let mut list = PostingsList::new(TermId::new(1));
    for i in 0..256 {
        list.insert(Posting::new(i * 10, 1));
    }
    
    let cursor = list.cursor();
    
    assert_eq!(cursor.num_blocks(), 2);
    assert_eq!(cursor.current_block_index(), 0);
}
```

### 8.3 Compression Tests

```rust
#[test]
fn test_postings_list_compression() {
    let mut list = PostingsList::new(TermId::new(1));
    // Create a posting list with gaps
    list.insert(Posting::new(1000, 5));
    list.insert(Posting::new(1050, 3));
    list.insert(Posting::new(1100, 7));
    
    let original_size = list.postings.len();
    
    list.compress();
    
    // After compression, doc_ids should be delta-encoded
    // The exact format depends on implementation
    
    list.decompress();
    
    // After decompression, should match original
    assert_eq!(list.postings.len(), original_size);
    assert_eq!(list.postings[0].doc_id(), 1000);
    assert_eq!(list.postings[1].doc_id(), 1050);
    assert_eq!(list.postings[2].doc_id(), 1100);
}
```

### 8.4 Integration Tests

#### Multi-Segment Tests
```rust
#[test]
fn test_segment_view_abstraction() {
    let mut postings1 = InMemoryPostings::new();
    let mut postings2 = InMemoryPostings::new();
    
    // Add different terms to each segment
    postings1.insert(TermId::new(1), PostingsList::new(TermId::new(1))).unwrap();
    postings2.insert(TermId::new(2), PostingsList::new(TermId::new(2))).unwrap();
    
    // Test SegmentView trait
    assert!(postings1.contains_term(TermId::new(1)));
    assert!(!postings1.contains_term(TermId::new(2)));
    assert!(postings2.contains_term(TermId::new(2)));
}
```

#### Dictionary Merge Tests
```rust
#[test]
fn test_dictionary_merge() {
    let mut dict1 = TermDictionary::new();
    let mut dict2 = TermDictionary::new();
    
    dict1.insert(TermId::new(1), PostingsList::new(TermId::new(1))).unwrap();
    dict2.insert(TermId::new(2), PostingsList::new(TermId::new(2))).unwrap();
    
    dict1.merge(dict2).unwrap();
    
    assert_eq!(dict1.len(), 2);
    assert!(dict1.get(TermId::new(1)).is_some());
    assert!(dict1.get(TermId::new(2)).is_some());
}

#[test]
fn test_dictionary_merge_conflict() {
    let mut dict1 = TermDictionary::new();
    let mut dict2 = TermDictionary::new();
    
    let term_id = TermId::new(1);
    dict1.insert(term_id, PostingsList::new(term_id)).unwrap();
    dict2.insert(term_id, PostingsList::new(term_id)).unwrap();
    
    let result = dict1.merge(dict2);
    assert!(result.is_err());
}
```

### 8.5 Property-Based Tests

Using `proptest`:

```rust
#[proptest]
fn test_posting_list_roundtrip(postings: Vec<Posting>) {
    // Ensure postings are sorted by doc_id
    let mut sorted = postings.clone();
    sorted.sort_by_key(|p| p.doc_id());
    
    let mut list = PostingsList::new(TermId::new(1));
    for posting in sorted {
        list.insert(posting);
    }
    
    // Traversal should produce all postings in order
    let mut cursor = list.cursor();
    for expected in &sorted {
        let (doc_id, tf) = cursor.next_with_tf().unwrap().unwrap();
        assert_eq!(doc_id, expected.doc_id());
        assert_eq!(tf, expected.tf());
    }
    
    assert!(cursor.next_with_tf().unwrap().is_none());
}

#[proptest]
fn test_seek_invariants(doc_ids: Vec<u32>, seek_targets: Vec<u32>) {
    // Test that seek maintains invariants
    // 1. seek returns a doc_id >= target
    // 2. subsequent next() returns strictly increasing doc_ids
    
    let mut sorted = doc_ids.clone();
    sorted.sort();
    sorted.dedup();
    
    let mut list = PostingsList::new(TermId::new(1));
    for doc_id in sorted {
        list.insert(Posting::new(doc_id, 1));
    }
    
    let mut cursor = list.cursor();
    
    for target in seek_targets {
        if let Some(found) = cursor.seek(target).unwrap() {
            assert!(found >= target);
        }
    }
}
```

### 8.6 Performance Benchmarks

Using `criterion`:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_sequential_traversal(c: &mut Criterion) {
    let mut list = PostingsList::new(TermId::new(1));
    for i in 0..10000 {
        list.insert(Posting::new(i, i % 10 + 1));
    }
    
    c.bench_function("sequential_traversal", |b| {
        b.iter(|| {
            let mut cursor = list.cursor();
            while let Some(_) = cursor.next().unwrap() {
                black_box(());
            }
        });
    });
}

fn bench_random_seek(c: &mut Criterion) {
    let mut list = PostingsList::new(TermId::new(1));
    for i in 0..10000 {
        list.insert(Posting::new(i, i % 10 + 1));
    }
    
    let targets: Vec<u32> = (0..1000).map(|i| i * 10).collect();
    
    c.bench_function("random_seek", |b| {
        b.iter(|| {
            let mut cursor = list.cursor();
            for target in &targets {
                black_box(cursor.seek(*target));
            }
        });
    });
}

criterion_group!(benches, bench_sequential_traversal, bench_random_seek);
criterion_main!(benches);
```

## 9. Verification Commands

```bash
# Verify crate compiles without default features (no_std)
cargo check -p leit_postings --no-default-features

# Verify crate compiles with std feature
cargo check -p leit_postings --features std

# Verify crate compiles with all features
cargo check -p leit_postings --all-features

# Run all tests
cargo test -p leit_postings

# Run tests without std (no_std mode)
cargo test -p leit_postings --no-default-features

# Run tests with std only
cargo test -p leit_postings --no-default-features --features std

# Run specific test modules
cargo test -p leit_postings --test cursor_tests
cargo test -p leit_postings --test compression_tests
cargo test -p leit_postings --test integration_tests

# Run clippy with all features
cargo clippy -p leit_postings --all-features -- -D warnings

# Run clippy in no_std mode
cargo clippy -p leit_postings --no-default-features -- -D warnings

# Check formatting
cargo fmt -p leit_postings -- --check

# Generate documentation
cargo doc -p leit_postings --no-deps --document-private-items

# Run benchmarks (requires --release)
cargo bench -p leit_postings

# Run property-based tests
cargo test -p leit_postings --features proptest

# Check for unused dependencies
cargo check -p leit_postings --all-features 2>&1 | grep "unused"

# Verify public API completeness
cargo doc -p leit_postings --no-deps --open 2>&1 | grep -i "warning"

# Test crate size in no_std mode
cargo build -p leit_postings --no-default-features --release
ls -lh target/*/release/libleit_postings.rlib

# Verify trait implementations
cargo test -p leit_postings -- --list | grep -i "cursor\|segment"

# Run documentation tests
cargo test -p leit_postings --doc

# Check for unsafe code blocks
grep -r "unsafe" crates/leit_postings/src/

# Verify no_std compatibility
cargo check -p leit_postings --no-default-features --target thumbv7em-none-eabihf

# Test cross-compilation
cargo check -p leit_postings --no-default-features --target wasm32-wasi
```

## 10. Documentation Requirements

All public items must have comprehensive rustdoc comments:

```rust
/// Brief one-line summary.
///
/// Longer description explaining the purpose, usage, and important details.
///
/// # Examples
///
/// ```
/// use leit_postings::{PostingsList, Posting, TermId};
///
/// let mut list = PostingsList::new(TermId::new(1));
/// list.insert(Posting::new(100, 5));
/// assert_eq!(list.len(), 1);
/// ```
///
/// # Errors
///
/// - Returns `CoreError::InvalidInput` if...
///
/// # Panics
///
/// - Panics if...
///
/// # Safety
///
/// - No safety concerns (or document unsafe requirements)
```

### Special Documentation Requirements

**Cursor Traits:**
- Document complexity of operations (O(1) vs O(log n))
- Explain seeking behavior and positioning invariants
- Provide examples for common usage patterns
- Document error conditions and recovery strategies

**SegmentView Trait:**
- Explain abstraction purpose and storage backend flexibility
- Document lifetime requirements
- Provide examples for both in-memory and file-backed scenarios

**Compression:**
- Document compression algorithm and format
- Explain trade-offs between compression and decompression speed
- Provide guidance on when to use compression

## 11. Release Checklist

- [ ] All acceptance criteria pass
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] Property-based tests pass (if implemented)
- [ ] Benchmarks meet performance targets
- [ ] Documentation is complete and builds without warnings
- [ ] `cargo clippy` produces no warnings
- [ ] `cargo fmt --check` passes
- [ ] Crate compiles in `no_std` mode
- [ ] Crate compiles with `std` feature
- [ ] Public API is stable and well-documented
- [ ] Examples in documentation compile and run
- [ ] Minimum supported Rust version is documented
- [ ] `CHANGELOG.md` is updated
- [ ] Crate version is bumped (if releasing)
- [ ] All dependencies are at minimum compatible versions
- [ ] No unsafe code without documented justification
- [ ] Memory usage is within acceptable limits
