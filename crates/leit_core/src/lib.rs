//! # Leit Core
//!
//! Core functionality for the Leit in-memory search engine framework.
//!
//! This crate provides foundational types, traits, and utilities for building
//! efficient search engines with no_std compatibility and optional alloc support.
//!
//! ## Architecture
//!
//! `leit_core` is the foundational crate (Layer 0) of the Leif project. It provides
//! core types, traits, and error handling used across the entire codebase. This crate
//! contains no business logic or storage implementation—it defines the fundamental
//! abstractions that higher-level crates depend on.
//!
//! ## Modules
//!
//! - **identifiers**: Core identifier types (FieldId, TermId, SegmentId, etc.)
//! - **entity**: EntityId trait for entity identification
//! - **score**: Score type for relevance scores and weights
//! - **hit**: Hit type for search results
//! - **error**: CoreError enum for unified error handling
//! - **scratch**: ScratchSpace and Workspace traits for memory management
//!
//! ## Features
//!
//! - `alloc`: Enables heap-allocated types (requires `#![no_std]` with alloc)
//! - `std`: Enables standard library integration (Display impls, Error trait, etc.)
//!
//! ## Key Types
//!
//! - **Identifier Types**: `FieldId`, `TermId`, `SegmentId`, `QueryNodeId`, `CursorSlotId`
//!   - All are `#[repr(transparent)]` over `u32` for efficiency
//!   - All support `const fn` constructors for use in const contexts
//!
//! - **EntityId Trait**: Marker trait for types that can be entity identifiers
//!   - Implemented for `u32`, `u64`, and all identifier types
//!   - Provides generic interface for entity identification
//!
//! - **Score Type**: `Score` - relevance scores with automatic clamping to [0.0, 1.0]
//!   - `Score::new()` clamps to [0.0, 1.0]
//!   - `Score::new_unchecked()` bypasses clamping for weights/boosts
//!   - Supports arithmetic operations
//!
//! - **Hit Type**: `Hit<Id>` - search results with ID and score
//!   - Ordered by score (descending)
//!   - Useful with `BinaryHeap` and sorting operations
//!
//! - **Error Type**: `CoreError` - unified error handling
//!   - No allocations (uses `&'static str`)
//!   - Compatible with `no_std`
//!
//! - **Memory Management**: `ScratchSpace`, `Workspace`, `HeapScratchSpace`, `HeapWorkspace`
//!   - Temporary vs. long-lived allocation strategies
//!   - Object-safe traits for dynamic dispatch
//!
//! ## Examples
//!
//! ```rust
//! use leit_core::{FieldId, Score, Hit};
//!
//! let field = FieldId::new(5);
//! assert_eq!(field.into_u32(), 5);
//!
//! let score = Score::new(0.85);
//! let hit = Hit::new(42u32, score);
//! assert_eq!(hit.score.into_f32(), 0.85);
//! ```
//!
//! ## no_std Support
//!
//! This crate is designed to work without the standard library. When the `std` feature
//! is disabled (default), only `core` and `alloc` are used. The `std` feature enables:
//!
//! - `Display` implementations for identifier types
//! - `Error` trait implementation for `CoreError`
//! - Conversion from `CoreError` to `std::io::Error`
//!
//! ## MSRV
//!
//! The Minimum Supported Rust Version is 1.70.0.

#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

// Public modules
mod identifiers;
mod entity;
mod score;
mod hit;
mod error;
mod scratch;

// Re-export all public items from modules

// Identifiers
pub use identifiers::{CursorSlotId, FieldId, QueryNodeId, SegmentId, TermId};

// Entity identification
pub use entity::EntityId;

// Scoring
pub use score::Score;

// Search results
pub use hit::Hit;

// Error handling
pub use error::CoreError;

// Memory management
pub use scratch::{HeapScratchSpace, HeapWorkspace, ScratchSpace, Workspace};
