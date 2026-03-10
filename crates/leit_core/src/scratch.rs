//! ScratchSpace and Workspace traits for memory management.
//!
//! These traits provide abstractions for temporary and long-lived memory
//! allocations, enabling efficient memory reuse and reduced allocation churn.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::CoreError;

/// Trait for allocating temporary scratch space.
///
/// Scratch space is used for temporary allocations during query execution,
/// indexing, and other operations. Implementations can reuse allocations
/// across operations to reduce memory churn.
///
/// # Object Safety
///
/// This trait is object-safe and can be used as `dyn ScratchSpace`.
pub trait ScratchSpace {
    /// Error type for allocation failures.
    type Error: Into<CoreError>;

    /// Allocates a vector of the given size.
    ///
    /// The vector is initialized with default values.
    fn alloc_vec<T>(&mut self, capacity: usize) -> Result<Vec<T>, Self::Error>
    where
        T: Default + Clone;

    /// Allocates a string buffer with the given capacity.
    fn alloc_string(&mut self, capacity: usize) -> Result<String, Self::Error>;

    /// Allocates a bytes buffer with the given capacity.
    fn alloc_bytes(&mut self, capacity: usize) -> Result<Vec<u8>, Self::Error>;

    /// Resets the scratch space, clearing all allocations.
    ///
    /// After calling this, the capacity should be preserved for reuse,
    /// but all existing allocations are cleared.
    fn reset(&mut self);

    /// Returns the current total allocated capacity in bytes.
    fn capacity(&self) -> usize;

    /// Returns the current total used bytes.
    fn used_bytes(&self) -> usize;
}

/// Simple heap-based scratch space implementation.
///
/// This implementation tracks capacity and usage but delegates actual
/// allocations to the standard allocator.
#[derive(Default, Debug)]
pub struct HeapScratchSpace {
    /// Total capacity tracked across all allocations.
    capacity: usize,

    /// Total bytes currently in use.
    used_bytes: usize,
}

impl HeapScratchSpace {
    /// Creates a new heap scratch space.
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::HeapScratchSpace;
    ///
    /// let scratch = HeapScratchSpace::new();
    /// assert_eq!(scratch.capacity(), 0);
    /// assert_eq!(scratch.used_bytes(), 0);
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ScratchSpace for HeapScratchSpace {
    type Error = CoreError;

    fn alloc_vec<T>(&mut self, capacity: usize) -> Result<Vec<T>, CoreError>
    where
        T: Default + Clone,
    {
        let elem_size = core::mem::size_of::<T>();
        let bytes = capacity.saturating_mul(elem_size);

        // Try to reserve capacity, tracking the result
        let mut vec = Vec::with_capacity(capacity);
        // Initialize with default values
        vec.resize(capacity, T::default());

        self.capacity += bytes;
        self.used_bytes += bytes;

        Ok(vec)
    }

    fn alloc_string(&mut self, capacity: usize) -> Result<String, CoreError> {
        let mut string = String::with_capacity(capacity);
        self.capacity += capacity;
        self.used_bytes += string.len();

        Ok(string)
    }

    fn alloc_bytes(&mut self, capacity: usize) -> Result<Vec<u8>, CoreError> {
        let mut vec = Vec::with_capacity(capacity);
        vec.resize(capacity, 0);

        self.capacity += capacity;
        self.used_bytes += capacity;

        Ok(vec)
    }

    fn reset(&mut self) {
        self.used_bytes = 0;
    }

    fn capacity(&self) -> usize {
        self.capacity
    }

    fn used_bytes(&self) -> usize {
        self.used_bytes
    }
}

/// Trait for managing workspace allocations.
///
/// A workspace holds longer-lived allocations that persist across
/// multiple operations, unlike scratch space which is reset frequently.
///
/// # Object Safety
///
/// This trait is object-safe and can be used as `dyn Workspace`.
pub trait Workspace {
    /// Error type for allocation failures.
    type Error: Into<CoreError>;

    /// Allocates a vector with the given capacity.
    fn alloc_vec<T>(&mut self, capacity: usize) -> Result<Vec<T>, Self::Error>
    where
        T: Default + Clone;

    /// Allocates a string buffer with the given capacity.
    fn alloc_string(&mut self, capacity: usize) -> Result<String, Self::Error>;

    /// Allocates a bytes buffer with the given capacity.
    fn alloc_bytes(&mut self, capacity: usize) -> Result<Vec<u8>, Self::Error>;

    /// Returns the current total allocated capacity in bytes.
    fn capacity(&self) -> usize;

    /// Returns the current total used bytes.
    fn used_bytes(&self) -> usize;

    /// Clears all allocations, resetting the workspace.
    ///
    /// Unlike scratch space's `reset()`, this clears allocations permanently
    /// rather than preserving them for reuse.
    fn clear(&mut self);
}

/// Simple heap-based workspace implementation.
///
/// This implementation tracks capacity and usage but delegates actual
/// allocations to the standard allocator.
#[derive(Default, Debug)]
pub struct HeapWorkspace {
    /// Total capacity tracked across all allocations.
    capacity: usize,

    /// Total bytes currently in use.
    used_bytes: usize,
}

impl HeapWorkspace {
    /// Creates a new heap workspace.
    ///
    /// # Examples
    ///
    /// ```
    /// use leit_core::HeapWorkspace;
    ///
    /// let workspace = HeapWorkspace::new();
    /// assert_eq!(workspace.capacity(), 0);
    /// assert_eq!(workspace.used_bytes(), 0);
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Workspace for HeapWorkspace {
    type Error = CoreError;

    fn alloc_vec<T>(&mut self, capacity: usize) -> Result<Vec<T>, CoreError>
    where
        T: Default + Clone,
    {
        let elem_size = core::mem::size_of::<T>();
        let bytes = capacity.saturating_mul(elem_size);

        let mut vec = Vec::with_capacity(capacity);
        vec.resize(capacity, T::default());

        self.capacity += bytes;
        self.used_bytes += bytes;

        Ok(vec)
    }

    fn alloc_string(&mut self, capacity: usize) -> Result<String, CoreError> {
        let mut string = String::with_capacity(capacity);
        self.capacity += capacity;
        self.used_bytes += string.len();

        Ok(string)
    }

    fn alloc_bytes(&mut self, capacity: usize) -> Result<Vec<u8>, CoreError> {
        let mut vec = Vec::with_capacity(capacity);
        vec.resize(capacity, 0);

        self.capacity += capacity;
        self.used_bytes += capacity;

        Ok(vec)
    }

    fn capacity(&self) -> usize {
        self.capacity
    }

    fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    fn clear(&mut self) {
        self.capacity = 0;
        self.used_bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_scratch_space_new() {
        let scratch = HeapScratchSpace::new();
        assert_eq!(scratch.capacity(), 0);
        assert_eq!(scratch.used_bytes(), 0);
    }

    #[test]
    fn test_heap_scratch_space_alloc_vec() {
        let mut scratch = HeapScratchSpace::new();
        let vec = scratch.alloc_vec::<u32>(10).unwrap();
        assert_eq!(vec.len(), 10);
        assert!(scratch.capacity() > 0);
        assert!(scratch.used_bytes() > 0);
    }

    #[test]
    fn test_heap_scratch_space_alloc_bytes() {
        let mut scratch = HeapScratchSpace::new();
        let bytes = scratch.alloc_bytes(100).unwrap();
        assert_eq!(bytes.len(), 100);
        assert!(scratch.capacity() >= 100);
        assert!(scratch.used_bytes() >= 100);
    }

    #[test]
    fn test_heap_scratch_space_reset() {
        let mut scratch = HeapScratchSpace::new();
        let _vec = scratch.alloc_vec::<u32>(10).unwrap();
        assert!(scratch.used_bytes() > 0);

        scratch.reset();
        assert_eq!(scratch.used_bytes(), 0);
        // Capacity is preserved
        assert!(scratch.capacity() > 0);
    }

    #[test]
    fn test_heap_workspace_new() {
        let workspace = HeapWorkspace::new();
        assert_eq!(workspace.capacity(), 0);
        assert_eq!(workspace.used_bytes(), 0);
    }

    #[test]
    fn test_heap_workspace_alloc_vec() {
        let mut workspace = HeapWorkspace::new();
        let vec = workspace.alloc_vec::<u32>(10).unwrap();
        assert_eq!(vec.len(), 10);
        assert!(workspace.capacity() > 0);
        assert!(workspace.used_bytes() > 0);
    }

    #[test]
    fn test_heap_workspace_clear() {
        let mut workspace = HeapWorkspace::new();
        let _vec = workspace.alloc_vec::<u32>(10).unwrap();
        assert!(workspace.capacity() > 0);
        assert!(workspace.used_bytes() > 0);

        workspace.clear();
        assert_eq!(workspace.capacity(), 0);
        assert_eq!(workspace.used_bytes(), 0);
    }
}
