# Leit Workspace Setup Specification

## 1. Overview

This specification defines the complete initialization of the Leit workspace as a Cargo workspace with 8 crates. The workspace is designed to support `no_std` environments for embedded and WASM targets while providing full `std` support when available.

### Workspace Structure

The workspace follows a layered architecture where crates depend only on lower layers:

```
Layer 0 (Foundation):    leit_core
Layer 1 (Primitives):    leit_score, leit_query
Layer 2 (Components):    leit_text, leit_postings, leit_fusion, leit_collect
Layer 3 (Integration):   leit_index
```

### Key Design Principles

1. **Layered Dependencies**: Higher-layer crates may depend on lower-layer crates, but never vice versa
2. **no_std Compatibility**: All crates must support `no_std + alloc` environments
3. **Feature Gates**: Optional functionality through Cargo features (e.g., `std`, `serde`)
4. **Workspace Consistency**: Shared lint configuration, dependency versions, and metadata

## 2. Workspace Cargo.toml

Create the root `Cargo.toml` with workspace-level configuration:

```toml
[workspace]
members = [
    "leit-core",
    "leit-score",
    "leit-query",
    "leit-text",
    "leit-postings",
    "leit-fusion",
    "leit-collect",
    "leit-index",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.70.0"
authors = ["Leif Contributors"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/yourusername/leif"
homepage = "https://github.com/yourusername/leif"
readme = "README.md"
keywords = ["search", "information-retrieval", "lucene"]
categories = ["database-implementations", "data-structures", "algorithms"]

[workspace.dependencies]
# External dependencies - shared versions across workspace
alloc = "1.0"

# Serialization (optional)
serde = { version = "1.0", default-features = false, optional = true }
serde_derive = { version = "1.0", optional = true }

# Testing
proptest = { version = "1.0", optional = true }
criterion = { version = "0.5", optional = true }

[workspace.lints.rust]
# Safety lints - deny unsafe patterns
unsafe_op_in_unsafe_fn = "deny"
unused_extern_crates = "warn"
unused_import_braces = "warn"
unused_qualifications = "warn"
variant_size_differences = "warn"

# Future-proofing
missing_debug_implementations = "warn"
missing_docs = "warn"

[workspace.lints.clippy]
# Pedantic lints for code quality
pedantic = "warn"

# Selective pedantic overrides
missing_errors_doc = "allow"
missing_panics_doc = "allow"
module_name_repetitions = "allow"
must_use_candidate = "allow"

# Performance lints
inline_always = "warn"
iter_without_into_iter = "warn"
missing_const_for_fn = "warn"
redundant_clone = "warn"
string_add = "warn"
string_add_assign = "warn"
unnecessary_to_owned = "warn"

# Correctness lints
arithmetic_side_effects = "warn"
cast_lossless = "warn"
cast_possible_truncation = "warn"
cast_possible_wrap = "warn"
cast_precision_loss = "warn"
cast_sign_loss = "warn"
checked_conversions = "warn"
cloned_instead_of_copied = "warn"
enum_glob_use = "warn"
explicit_into_iter_loop = "warn"
filter_map_next = "warn"
flat_map_option = "warn"
fn_params_excessive_bools = "warn"
from_iter_instead_of_collect = "warn"
implicit_clone = "warn"
inefficient_to_string = "warn"
invalid_upcast_comparisons = "warn"
iter_on_empty_collections = "warn"
iter_on_single_items = "warn"
iter_over_hash_set = "warn"
large_stack_arrays = "warn"
manual_assert = "warn"
manual_is_variant_and = "warn"
manual_ok_or = "warn"
manual_string_new = "warn"
map_unwrap_or = "warn"
match_bool = "warn"
mut_mut = "warn"
needless_bitwise_bool = "warn"
needless_continue = "warn"
needless_for_each = "warn"
no_effect_underscore_binding = "warn"
option_if_let_else = "warn"
range_minus_one = "warn"
range_plus_one = "warn"
redundant_else = "warn"
ref_binding_to_reference = "warn"
ref_option_ref = "warn"
same_name_method = "warn"
single_char_pattern = "warn"
uninlined_format_args = "warn"
unnecessary_join = "warn"
unnested_or_patterns = "warn"
unreadable_literal = "warn"
verbose_bit_mask = "warn"
zero_sized_map_values = "warn"

# Allow some reasonable patterns
bool_assert_comparison = "allow"
struct_excessive_bools = "allow"
```

## 3. Crate-Level Cargo.toml Skeletons

### 3.1 leit-core (Layer 0)

```toml
[package]
name = "leit-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
readme.workspace = true
keywords.workspace = true
categories.workspace = true
description = "Core types and traits for the Leit search library"

[features]
default = ["std"]
std = ["alloc"]
alloc = []

[dependencies]
# External (optional)
serde = { workspace = true, optional = true }

[dev-dependencies]
# Testing
proptest = { workspace = true, optional = true }

[lints]
workspace = true
```

### 3.2 leit-score (Layer 1)

```toml
[package]
name = "leit-score"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
readme.workspace = true
keywords.workspace = true
categories.workspace = true
description = "Scoring algorithms for search results (BM25, etc.)"

[features]
default = ["std"]
std = ["alloc", "leit-core/std"]
alloc = ["leit-core/alloc"]
serde = ["dep:serde", "leit-core/serde"]

[dependencies]
leit-core = { path = "../leit-core", default-features = false }

# External (optional)
serde = { workspace = true, optional = true }

[dev-dependencies]
proptest = { workspace = true, optional = true }

[lints]
workspace = true
```

### 3.3 leit-query (Layer 1)

```toml
[package]
name = "leit-query"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
readme.workspace = true
keywords.workspace = true
categories.workspace = true
description = "Query types and parsers for the Leit search library"

[features]
default = ["std"]
std = ["alloc", "leit-core/std"]
alloc = ["leit-core/alloc"]
serde = ["dep:serde", "leit-core/serde"]

[dependencies]
leit-core = { path = "../leit-core", default-features = false }

# External (optional)
serde = { workspace = true, optional = true }

[dev-dependencies]
proptest = { workspace = true, optional = true }

[lints]
workspace = true
```

### 3.4 leit-text (Layer 2)

```toml
[package]
name = "leit-text"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
readme.workspace = true
keywords.workspace = true
categories.workspace = true
description = "Text analysis and tokenization for the Leit search library"

[features]
default = ["std"]
std = ["alloc", "leit-core/std"]
alloc = ["leit-core/alloc"]
serde = ["dep:serde", "leit-core/serde"]

[dependencies]
leit-core = { path = "../leit-core", default-features = false }

# External (optional)
serde = { workspace = true, optional = true }

[dev-dependencies]
proptest = { workspace = true, optional = true }

[lints]
workspace = true
```

### 3.5 leit-postings (Layer 2)

```toml
[package]
name = "leit-postings"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
readme.workspace = true
keywords.workspace = true
categories.workspace = true
description = "Posting list data structures and compression"

[features]
default = ["std"]
std = ["alloc", "leit-core/std", "leit-score/std"]
alloc = ["leit-core/alloc", "leit-score/alloc"]
serde = ["dep:serde", "leit-core/serde", "leit-score/serde"]

[dependencies]
leit-core = { path = "../leit-core", default-features = false }
leit-score = { path = "../leit-score", default-features = false }

# External (optional)
serde = { workspace = true, optional = true }

[dev-dependencies]
proptest = { workspace = true, optional = true }

[lints]
workspace = true
```

### 3.6 leit-fusion (Layer 2)

```toml
[package]
name = "leit-fusion"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
readme.workspace = true
keywords.workspace = true
categories.workspace = true
description = "Query result fusion and combination algorithms"

[features]
default = ["std"]
std = ["alloc", "leit-core/std", "leit-score/std", "leit-query/std"]
alloc = ["leit-core/alloc", "leit-score/alloc", "leit-query/alloc"]
serde = ["dep:serde", "leit-core/serde", "leit-score/serde", "leit-query/serde"]

[dependencies]
leit-core = { path = "../leit-core", default-features = false }
leit-score = { path = "../leit-score", default-features = false }
leit-query = { path = "../leit-query", default-features = false }

# External (optional)
serde = { workspace = true, optional = true }

[dev-dependencies]
proptest = { workspace = true, optional = true }

[lints]
workspace = true
```

### 3.7 leit-collect (Layer 2)

```toml
[package]
name = "leit-collect"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
readme.workspace = true
keywords.workspace = true
categories.workspace = true
description = "Result collection and top-K selection algorithms"

[features]
default = ["std"]
std = ["alloc", "leit-core/std", "leit-score/std"]
alloc = ["leit-core/alloc", "leit-score/alloc"]
serde = ["dep:serde", "leit-core/serde", "leit-score/serde"]

[dependencies]
leit-core = { path = "../leit-core", default-features = false }
leit-score = { path = "../leit-score", default-features = false }

# External (optional)
serde = { workspace = true, optional = true }

[dev-dependencies]
proptest = { workspace = true, optional = true }

[lints]
workspace = true
```

### 3.8 leit-index (Layer 3)

```toml
[package]
name = "leit-index"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
readme.workspace = true
keywords.workspace = true
categories.workspace = true
description = "Main indexing and search interface"

[features]
default = ["std"]
std = [
    "alloc",
    "leit-core/std",
    "leit-score/std",
    "leit-query/std",
    "leit-text/std",
    "leit-postings/std",
    "leit-fusion/std",
    "leit-collect/std",
]
alloc = [
    "leit-core/alloc",
    "leit-score/alloc",
    "leit-query/alloc",
    "leit-text/alloc",
    "leit-postings/alloc",
    "leit-fusion/alloc",
    "leit-collect/alloc",
]
serde = [
    "dep:serde",
    "leit-core/serde",
    "leit-score/serde",
    "leit-query/serde",
    "leit-text/serde",
    "leit-postings/serde",
    "leit-fusion/serde",
    "leit-collect/serde",
]

[dependencies]
leit-core = { path = "../leit-core", default-features = false }
leit-score = { path = "../leit-score", default-features = false }
leit-query = { path = "../leit-query", default-features = false }
leit-text = { path = "../leit-text", default-features = false }
leit-postings = { path = "../leit-postings", default-features = false }
leit-fusion = { path = "../leit-fusion", default-features = false }
leit-collect = { path = "../leit-collect", default-features = false }

# External (optional)
serde = { workspace = true, optional = true }

[dev-dependencies]
proptest = { workspace = true, optional = true }

[lints]
workspace = true
```

## 4. Directory Structure

Create the following directory structure:

```
leif/
├── Cargo.toml                    # Workspace manifest
├── Cargo.lock                    # Generated by Cargo
├── README.md                     # Workspace overview
├── LICENSE-MIT                   # MIT license
├── LICENSE-APACHE                # Apache 2.0 license
├── .github/
│   └── workflows/
│       ├── ci.yml                # Main CI pipeline
│       ├── no_std.yml            # no_std verification
│       └── security.yml          # Security audit
├── leit-core/
│   ├── Cargo.toml                # Core crate manifest
│   ├── src/
│   │   ├── lib.rs                # Library root
│   │   └── ...
│   └── README.md                 # Crate documentation
├── leit-score/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   └── ...
│   └── README.md
├── leit-query/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   └── ...
│   └── README.md
├── leit-text/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   └── ...
│   └── README.md
├── leit-postings/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   └── ...
│   └── README.md
├── leit-fusion/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   └── ...
│   └── README.md
├── leit-collect/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   └── ...
│   └── README.md
├── leit-index/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   └── ...
│   └── README.md
└── specs/                        # Existing specifications directory
    ├── tasks/
    └── decisions/
```

## 5. no_std Verification Setup

### 5.1 Crate Initialization Template

Each crate's `lib.rs` must start with the following conditional compilation:

```rust
#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

mod prelude {
    // Prelude module with common imports
}

// Core crate code goes here
```

### 5.2 Build Targets for Verification

Create target-specific configurations for testing no_std compatibility:

**`.cargo/config.toml`** (for no_std testing):

```toml
[build]
target = "x86_64-unknown-linux-gnu"

[term]
verbose = true
color = "auto"

[net]
git-fetch-with-cli = true
```

### 5.3 no_std Verification Commands

Verify each crate can build without std:

```bash
# Test all crates with alloc only (no std)
cargo build --workspace --no-default-features --features alloc

# Test individual crates
cargo build -p leit-core --no-default-features --features alloc
cargo build -p leit-score --no-default-features --features alloc
cargo build -p leit-query --no-default-features --features alloc
cargo build -p leit-text --no-default-features --features alloc
cargo build -p leit-postings --no-default-features --features alloc
cargo build -p leit-fusion --no-default-features --features alloc
cargo build -p leit-collect --no-default-features --features alloc
cargo build -p leit-index --no-default-features --features alloc

# Test with default features (std enabled)
cargo build --workspace

# Test with serde features
cargo build --workspace --no-default-features --features "serde,alloc"
```

## 6. CI Configuration

### 6.1 Main CI Pipeline (`.github/workflows/ci.yml`)

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test:
    name: Test
    runs-on: ubuntu-latest
    strategy:
      matrix:
        rust:
          - stable
          - nightly
        features:
          - default
          - alloc
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust }}
      - name: Build
        run: cargo build --workspace --features ${{ matrix.features }}
      - name: Test
        run: cargo test --workspace --features ${{ matrix.features }}
      - name: Run linter
        run: cargo clippy --workspace --features ${{ matrix.features }} -- -D warnings

  no_std:
    name: no_std Verification
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Build without std
        run: cargo build --workspace --no-default-features --features alloc
      - name: Test without std
        run: cargo test --workspace --no-default-features --features alloc

  formatting:
    name: Formatting Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Check formatting
        run: cargo fmt --all -- --check
```

### 6.2 no_std Verification Pipeline (`.github/workflows/no_std.yml`)

```yaml
name: no_std Targets

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ubuntu-latest
    strategy:
      matrix:
        target:
          - x86_64-unknown-linux-gnu
          - aarch64-unknown-linux-gnu
          - thumbv7em-none-eabihf
          - wasm32-wasi
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - name: Build for target
        run: cargo build --workspace --no-default-features --features alloc --target ${{ matrix.target }}
```

## 7. Initial Source Files

### 7.1 Placeholder lib.rs for Each Crate

**leit-core/src/lib.rs:**

```rust
#![no_std]

//! Core types and traits for the Leit search library.
//!
//! This crate provides foundational types, traits, and error handling used
//! across the entire Leit codebase.

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

// Re-exports
pub mod error;
pub mod types;

// TODO: Implement core types
```

**All other crates** (leit-score, leit-query, etc.):

```rust
#![no_std]

//! TODO: Crate description

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

// TODO: Implement crate functionality
```

## 8. Verification Commands

Run these commands to verify the workspace is properly initialized:

```bash
# 1. Verify workspace structure
cargo tree --workspace

# 2. Verify all crates compile (std enabled)
cargo build --workspace

# 3. Verify all crates compile (no_std + alloc)
cargo build --workspace --no-default-features --features alloc

# 4. Run tests
cargo test --workspace

# 5. Run tests without std
cargo test --workspace --no-default-features --features alloc

# 6. Check formatting
cargo fmt --all -- --check

# 7. Run linter
cargo clippy --workspace --all-targets --all-features

# 8. Verify dependency graph
cargo tree --workspace --duplicates

# 9. Check for unused dependencies
cargo machete  # if cargo-machete is installed

# 10. Verify MSRV (minimum supported Rust version)
cargo +1.70.0 build --workspace
```

## 9. Acceptance Criteria

The workspace setup is complete when:

1. ✅ All 8 crates are defined in the workspace `Cargo.toml`
2. ✅ Each crate has a valid `Cargo.toml` with workspace-level dependencies
3. ✅ All crates compile successfully with default features
4. ✅ All crates compile successfully with `alloc` feature only (no std)
5. ✅ Each crate has a placeholder `lib.rs` with proper `no_std` attributes
6. ✅ The workspace passes `cargo clippy` with workspace lints
7. ✅ The workspace has CI configuration for std and no_std testing
8. ✅ All directory structure is created
9. ✅ README files exist for the workspace and each crate
10. ✅ `cargo tree --workspace` shows correct dependency structure

## 10. Next Steps

After workspace initialization:

1. Implement `leit-core` types and traits (see `leit_core.md` spec)
2. Implement scoring algorithms in `leit-score`
3. Implement query types in `leit-query`
4. Continue with remaining crates following the layer ordering
5. Add integration tests and benchmarks
6. Expand CI to include coverage reporting
7. Add documentation examples to each crate
