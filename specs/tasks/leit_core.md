# leit_core Crate Specification

## 1. Overview and Purpose

`leit_core` is the foundational crate (Layer 0) of the Leif project. It provides core types, traits, and error handling used across the entire codebase. This crate contains no business logic or storage implementation—it defines the fundamental abstractions that higher-level crates depend on.

**Key responsibilities:**
- Define identifier types for all system entities
- Provide core traits for entity identification and workspace management
- Define common error types
- Provide primitive query result types
- Support `no_std` + `alloc` environments for embedded and WASM targets

## 2. Dependencies

**External Dependencies:** None

**Internal Dependencies:** None

This is Layer 0 - the foundation of the dependency graph. Other crates depend on this crate.

## 3. Target Environment

- **Target:** `no_std` + `alloc`
- **Rust Edition:** 2021
- **Minimum Supported Rust Version (MSRV):** 1.70.0

### no_std Configuration

```toml
[dependencies]
alloc = "1"

[features]
default = ["std"]
std = ["alloc"]
```

### Conditional Compilation

- Core types must work without `std`
- `std` feature enables additional error conversions and convenience methods
- All type definitions must be `#[cfg_attr(not(feature = "std"), no_std)]` compatible

## 4. Public API Specification

### 4.1 Identifier Types

All identifier types are newtype wrappers around `u32` for efficiency and cache-friendliness.

```rust
/// Unique identifier for a field in the schema
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct FieldId(pub u32);

impl FieldId {
    /// Creates a new FieldId
    pub const fn new(id: u32) -> Self;
    
    /// Returns the underlying value
    pub const fn into_u32(self) -> u32;
}

impl Debug for FieldId;
impl Display for FieldId;

/// Unique identifier for a term in the inverted index
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct TermId(pub u32);

impl TermId {
    /// Creates a new TermId
    pub const fn new(id: u32) -> Self;
    
    /// Returns the underlying value
    pub const fn into_u32(self) -> u32;
}

impl Debug for TermId;
impl Display for TermId;

/// Unique identifier for a segment (data shard)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct SegmentId(pub u32);

impl SegmentId {
    /// Creates a new SegmentId
    pub const fn new(id: u32) -> Self;
    
    /// Returns the underlying value
    pub const fn into_u32(self) -> u32;
    
    /// Special sentinel value indicating "no segment"
    pub const INVALID: SegmentId = SegmentId(u32::MAX);
}

impl Debug for SegmentId;
impl Display for SegmentId;

/// Unique identifier for a node in a query plan tree
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct QueryNodeId(pub u32);

impl QueryNodeId {
    /// Creates a new QueryNodeId
    pub const fn new(id: u32) -> Self;
    
    /// Returns the underlying value
    pub const fn into_u32(self) -> u32;
}

impl Debug for QueryNodeId;
impl Display for QueryNodeId;

/// Slot identifier for cursor positions during query execution
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct CursorSlotId(pub u32);

impl CursorSlotId {
    /// Creates a new CursorSlotId
    pub const fn new(id: u32) -> Self;
    
    /// Returns the underlying value
    pub const fn into_u32(self) -> u32;
    
    /// Maximum number of cursor slots supported
    pub const MAX_SLOTS: u32 = 256;
}

impl Debug for CursorSlotId;
impl Display for CursorSlotId;
```

### 4.2 EntityId Trait

Trait for types that can serve as entity identifiers across the system.

```rust
/// Trait for entity identifiers used throughout Leif
pub trait EntityId: Copy + PartialEq + Eq + Hash + Debug + Display + Send + Sync + 'static {
    /// Convert to u64 for serialization/hashing
    fn as_u64(self) -> u64;
    
    /// Convert from u64
    fn from_u64(id: u64) -> Self;
    
    /// Create a new ID with the given value
    fn new(id: u32) -> Self;
    
    /// Get the underlying u32 value
    fn into_u32(self) -> u32;
    
    /// Check if this is an invalid/sentinel ID
    fn is_invalid(self) -> bool;
}

// Blanket implementations for standard integer types
impl EntityId for u32;
impl EntityId for u64;

// Macro to generate EntityId implementations for newtype wrappers
#[macro_export]
macro_rules! impl_entity_id {
    ($ty:ident) => {
        impl EntityId for $ty {
            fn as_u64(self) -> u64 {
                self.0 as u64
            }
            
            fn from_u64(id: u64) -> Self {
                Self(id as u32)
            }
            
            fn new(id: u32) -> Self {
                Self(id)
            }
            
            fn into_u32(self) -> u32 {
                self.0
            }
            
            fn is_invalid(self) -> bool {
                self.0 == u32::MAX
            }
        }
    };
}
```

### 4.3 Score Type

Type for relevance scores and weights.

```rust
/// Relevance score or weight
/// 
/// Represents a floating-point score in the range [0.0, 1.0] for relevance,
/// or any f32 value for weights/boosts.
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
#[repr(transparent)]
pub struct Score(pub f32);

impl Score {
    /// Zero score
    pub const ZERO: Score = Score(0.0);
    
    /// Perfect score
    pub const ONE: Score = Score(1.0);
    
    /// Creates a new score, clamped to [0.0, 1.0]
    pub fn new(score: f32) -> Self;
    
    /// Creates a score without clamping
    pub fn new_unchecked(score: f32) -> Self;
    
    /// Returns the underlying f32 value
    pub const fn into_f32(self) -> f32;
    
    /// Checks if this is a zero score
    pub const fn is_zero(self) -> bool;
    
    /// Checks if this is a perfect score
    pub const fn is_one(self) -> bool;
}

impl From<f32> for Score;
impl From<Score> for f32;
impl Add for Score;
impl AddAssign for Score;
impl Sub for Score;
impl SubAssign for Score;
impl Mul<f32> for Score;
impl MulAssign<f32> for Score;

#[cfg(feature = "std")]
impl Display for Score;
```

### 4.4 Hit<Id> Type

Represents a single search result hit.

```rust
/// A single search result hit
/// 
/// Contains an entity ID and its relevance score.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hit<Id> {
    /// The entity identifier
    pub id: Id,
    
    /// The relevance score
    pub score: Score,
}

impl<Id: Copy> Hit<Id> {
    /// Creates a new hit with the given ID and score
    pub fn new(id: Id, score: Score) -> Self;
    
    /// Creates a new hit with score 1.0
    pub fn perfect(id: Id) -> Self;
    
    /// Creates a new hit with score 0.0
    pub fn zero(id: Id) -> Self;
    
    /// Returns true if the score is zero
    pub fn is_zero(&self) -> bool;
}

impl<Id: Copy> PartialOrd for Hit<Id> {
    /// Hits are compared by score (descending)
    fn partial_cmp(&self, other: &Self) -> Option<Ordering>;
}

impl<Id: Copy> Ord for Hit<Id> {
    /// Hits are compared by score (descending)
    fn cmp(&self, other: &Self) -> Ordering;
}

#[cfg(feature = "std")]
impl<Id: Display + Copy> Display for Hit<Id> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result;
}
```

### 4.5 CoreError Type

Error type for core operations.

```rust
/// Core error type for Leif
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CoreError {
    /// The requested entity was not found
    NotFound {
        /// Type of entity that was not found
        entity_type: &'static str,
        /// Identifier that was not found
        id: u64,
    },
    
    /// The requested operation is not supported
    Unsupported {
        /// Description of what is unsupported
        operation: &'static str,
    },
    
    /// Invalid input provided
    InvalidInput {
        /// Description of the validation error
        reason: &'static str,
    },
    
    /// A limit was exceeded
    LimitExceeded {
        /// The limit that was exceeded
        limit: &'static str,
        /// Current value that exceeded the limit
        value: u64,
        /// Maximum allowed value
        max: u64,
    },
    
    /// An invariant was violated
    InvariantViolated {
        /// Description of the invariant
        invariant: &'static str,
    },
    
    /// Out of memory
    OutOfMemory,
}

impl CoreError {
    /// Creates a NotFound error
    pub fn not_found(entity_type: &'static str, id: u64) -> Self;
    
    /// Creates an Unsupported error
    pub fn unsupported(operation: &'static str) -> Self;
    
    /// Creates an InvalidInput error
    pub fn invalid_input(reason: &'static str) -> Self;
    
    /// Creates a LimitExceeded error
    pub fn limit_exceeded(limit: &'static str, value: u64, max: u64) -> Self;
    
    /// Creates an InvariantViolated error
    pub fn invariant_violated(invariant: &'static str) -> Self;
}

impl Display for CoreError;
impl Error for CoreError;

#[cfg(feature = "std")]
impl From<CoreError> for std::io::Error;

#[cfg(feature = "std")]
impl From<alloc::collections::TryReserveError> for CoreError {
    fn from(_: alloc::collections::TryReserveError) -> Self {
        CoreError::OutOfMemory
    }
}
```

### 4.6 ScratchSpace Trait

Trait for temporary memory allocation during operations.

```rust
/// Trait for allocating temporary scratch space
/// 
/// Scratch space is used for temporary allocations during query execution,
/// indexing, and other operations. Implementations can reuse allocations
/// across operations to reduce memory churn.
pub trait ScratchSpace {
    /// Error type for allocation failures
    type Error: Into<CoreError>;
    
    /// Allocates a vector of the given size
    fn alloc_vec<T>(&mut self, capacity: usize) -> Result<Vec<T>, Self::Error>
    where
        T: Default;
    
    /// Allocates a string buffer with the given capacity
    fn alloc_string(&mut self, capacity: usize) -> Result<String, Self::Error>;
    
    /// Allocates a bytes buffer with the given capacity
    fn alloc_bytes(&mut self, capacity: usize) -> Result<Vec<u8>, Self::Error>;
    
    /// Resets the scratch space, clearing all allocations
    fn reset(&mut self);
    
    /// Returns the current total allocated capacity in bytes
    fn capacity(&self) -> usize;
    
    /// Returns the current total used bytes
    fn used_bytes(&self) -> usize;
}

/// Simple heap-based scratch space implementation
#[derive(Default, Debug)]
pub struct HeapScratchSpace {
    // Total capacity tracked across all allocations
    capacity: usize,
    // Total bytes currently in use
    used_bytes: usize,
}

impl HeapScratchSpace {
    /// Creates a new heap scratch space
    pub fn new() -> Self;
}

impl ScratchSpace for HeapScratchSpace {
    type Error = CoreError;
    
    fn alloc_vec<T>(&mut self, capacity: usize) -> Result<Vec<T>, Self::Error>
    where
        T: Default;
    
    fn alloc_string(&mut self, capacity: usize) -> Result<String, Self::Error>;
    
    fn alloc_bytes(&mut self, capacity: usize) -> Result<Vec<u8>, Self::Error>;
    
    fn reset(&mut self);
    
    fn capacity(&self) -> usize;
    
    fn used_bytes(&self) -> usize;
}
```

### 4.7 Workspace Trait

Trait for managing long-lived allocations and state.

```rust
/// Trait for managing workspace allocations
/// 
/// A workspace holds longer-lived allocations that persist across
/// multiple operations, unlike scratch space which is reset frequently.
pub trait Workspace {
    /// Error type for allocation failures
    type Error: Into<CoreError>;
    
    /// Allocates a vector with the given capacity
    fn alloc_vec<T>(&mut self, capacity: usize) -> Result<Vec<T>, Self::Error>
    where
        T: Default;
    
    /// Allocates a string buffer with the given capacity
    fn alloc_string(&mut self, capacity: usize) -> Result<String, Self::Error>;
    
    /// Allocates a bytes buffer with the given capacity
    fn fn alloc_bytes(&mut self, capacity: usize) -> Result<Vec<u8>, Self::Error>;
    
    /// Returns the current total allocated capacity in bytes
    fn capacity(&self) -> usize;
    
    /// Returns the current total used bytes
    fn used_bytes(&self) -> usize;
    
    /// Clears all allocations, resetting the workspace
    fn clear(&mut self);
}

/// Simple heap-based workspace implementation
#[derive(Default, Debug)]
pub struct HeapWorkspace {
    // Total capacity tracked across all allocations
    capacity: usize,
    // Total bytes currently in use
    used_bytes: usize,
}

impl HeapWorkspace {
    /// Creates a new heap workspace
    pub fn new() -> Self;
}

impl Workspace for HeapWorkspace {
    type Error = CoreError;
    
    fn alloc_vec<T>(&mut self, capacity: usize) -> Result<Vec<T>, Self::Error>
    where
        T: Default;
    
    fn alloc_string(&mut self, capacity: usize) -> Result<String, Self::Error>;
    
    fn alloc_bytes(&mut self, capacity: usize) -> Result<Vec<u8>, Self::Error>;
    
    fn capacity(&self) -> usize;
    
    fn used_bytes(&self) -> usize;
    
    fn clear(&mut self);
}
```

### 4.8 Additional Helper Types

```rust
/// Version number for schema changes
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Version(pub u32);

impl Version {
    pub const fn new(v: u32) -> Self;
    pub const fn into_u32(self) -> u32;
}

impl Display for Version;
impl Default for Version {
    fn default() -> Self {
        Version(0)
    }
}

/// Timestamp type
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Unix epoch
    pub const EPOCH: Timestamp = Timestamp(0);
    
    pub const fn new(ts: u64) -> Self;
    pub const fn into_u64(self) -> u64;
}

impl Display for Timestamp;
```

## 5. Implementation Notes and Constraints

### 5.1 Memory Layout

- All ID types must be `#[repr(transparent)]` over `u32`
- This ensures they have the same representation and can be safely transmuted if needed
- Enables efficient storage in arrays and vectors

### 5.2 Const Safety

- All constructors and accessors should be `const fn` where possible
- Enables use in const contexts and compile-time optimizations

### 5.3 no_std Constraints

- No `std` imports in the core code path
- Use `alloc` for dynamic collections
- Conditional compilation for `std` feature:
  ```rust
  #[cfg(feature = "std")]
  impl Display for Score {
      fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
          write!(f, "{}", self.0)
      }
  }
  ```

### 5.4 Error Handling

- `CoreError` must not carry `String` or `Vec` fields
- Use `&'static str` for error messages to avoid allocations
- All errors must be identifiable and actionable

### 5.5 Trait Bounds

- `EntityId` trait uses conservative bounds: `Copy + PartialEq + Eq + Hash + Debug + Display + Send + Sync + 'static`
- These bounds ensure all entity IDs can be used in concurrent contexts and hashed
- The `'static` bound enables storage in thread-local and global data structures

### 5.6 Score Type Safety

- `Score::new()` must clamp to [0.0, 1.0]
- `Score::new_unchecked()` bypasses clamping for internal use
- Arithmetic ops should maintain reasonable floating-point semantics
- NaN scores should propagate through comparisons

### 5.7 Hit Ordering

- `Hit<Id>` implements `Ord` based on score descending
- Higher scores come first (reverse ordering)
- This is useful for `BinaryHeap` and sorting operations

## 6. Acceptance Criteria Checklist

- [ ] All identifier types implement `Copy`, `Clone`, `Debug`, `Display`, and hash traits
- [ ] All identifier types are `#[repr(transparent)]` over `u32`
- [ ] All identifier type constructors are `const fn`
- [ ] `EntityId` trait is implemented for `u32`, `u64`, and all ID types
- [ ] `Score` type properly clamps values in `new()` but not `new_unchecked()`
- [ ] `Hit<Id>` implements `Ord` with descending score ordering
- [ ] `CoreError` has no `String` or `Vec` fields
- [ ] `CoreError` implements `Display` and `Error`
- [ ] `CoreError` converts to `std::io::Error` when `std` feature is enabled
- [ ] `ScratchSpace` trait is object-safe (can be used as `dyn ScratchSpace`)
- [ ] `Workspace` trait is object-safe
- [ ] `HeapScratchSpace` correctly tracks capacity and usage
- [ ] `HeapWorkspace` correctly tracks capacity and usage
- [ ] All types work without `std` feature
- [ ] Crate compiles with `default-features = false`
- [ ] All public items are documented with rustdoc
- [ ] `cargo doc --no-deps` generates documentation without warnings

## 7. Test Plan

### 7.1 Unit Tests

**Identifier Types:**
- Test creation and conversion to/from primitive types
- Test equality and ordering
- Test hash consistency
- Test sentinel values (e.g., `SegmentId::INVALID`)

**EntityId Trait:**
- Test blanket implementations for `u32` and `u64`
- Test macro-generated implementations
- Test `is_invalid()` for all ID types

**Score Type:**
- Test clamping in `new()` (values < 0.0, > 1.0, NaN)
- Test `new_unchecked()` bypasses clamping
- Test arithmetic operations
- Test comparison with NaN values
- Test `is_zero()` and `is_one()` edge cases

**Hit<Id> Type:**
- Test creation helpers (`new`, `perfect`, `zero`)
- Test `is_zero()` method
- Test ordering (higher scores come first)
- Test Display formatting when `std` feature enabled

**CoreError Type:**
- Test all error variant constructors
- Test Display formatting
- Test Error source chain (when applicable)
- Test conversion to `std::io::Error`

**ScratchSpace & HeapScratchSpace:**
- Test vector allocation with various capacities
- Test string and bytes allocation
- Test `reset()` clears allocations
- Test capacity and usage tracking
- Test error handling for allocation failures

**Workspace & HeapWorkspace:**
- Test vector, string, and bytes allocation
- Test `clear()` method
- Test capacity and usage tracking
- Test error handling

### 7.2 Integration Tests

- [ ] Test compilation with `--no-default-features`
- [ ] Test compilation with `--all-features`
- [ ] Test that all public types are re-exported from crate root
- [ ] Test that the crate can be used in a `no_std` binary

### 7.3 Property-Based Tests (Optional)

Use `proptest` for:
- Score clamping properties
- Hit ordering properties
- ID roundtrip properties

## 8. Verification Commands

```bash
# Verify crate compiles without default features (no_std)
cargo check -p leit_core --no-default-features

# Verify crate compiles with std feature
cargo check -p leit_core --features std

# Verify crate compiles with all features
cargo check -p leit_core --all-features

# Run tests
cargo test -p leit_core

# Run tests with std feature only
cargo test -p leit_core --no-default-features --features std

# Run clippy
cargo clippy -p leit_core --all-features -- -D warnings

# Check formatting
cargo fmt -p leit_core -- --check

# Generate and check documentation
cargo doc -p leit_core --no-deps --document-private-items
```

## 9. Documentation Requirements

All public items must have rustdoc comments:

```rust
/// Brief one-line summary.
///
/// Longer description if needed.
///
/// # Examples
///
/// ```
/// use leit_core::FieldId;
///
/// let id = FieldId::new(42);
/// assert_eq!(id.into_u32(), 42);
/// ```
```

Special requirements:
- All identifier types must document their domain semantics
- All traits must document required behavior and invariants
- All error variants must document when they occur and how to handle them
- Include examples for non-trivial operations

## 10. Release Checklist

- [ ] All acceptance criteria pass
- [ ] All tests pass
- [ ] Documentation is complete and builds without warnings
- [ ] `cargo clippy` produces no warnings
- [ ] `cargo fmt --check` passes
- [ ] Crate version is bumped (if updating)
- [ ] `CHANGELOG.md` is updated
- [ ] Minimum supported Rust version is documented in `README.md` or `lib.rs`
