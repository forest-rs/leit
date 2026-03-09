// Leit Notes Example
//
// This example demonstrates how to use Leit to build a simple note search engine.
// It shows the Note/Projection pattern for indexing custom entities.

use leit_core::{EntityId, FieldId, Projection};
use leit_index::{InMemoryIndexBuilder, ExecutionWorkspace};
use leit_query::term;

// ============================================================================
// Note Entity
// ============================================================================

/// A note with an ID, title, body, and language.
#[derive(Debug, Clone)]
struct Note {
    id: NoteId,
    title: String,
    body: String,
    language: String,
}

/// Newtype wrapper for note IDs implementing EntityId.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoteId(u32);

impl NoteId {
    /// Create a new note ID.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw u32 value.
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

// SAFETY: NoteId is a newtype over u32, which is Copy + Eq + Hash + Debug + Ord
impl EntityId for NoteId {}

// ============================================================================
// Note Projection
// ============================================================================

/// Projection that extracts fields from a Note for indexing.
///
/// This defines how notes are mapped to searchable fields and how
/// note IDs are extracted.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoteProjection;

impl NoteProjection {
    /// Create a new note projection.
    pub const fn new() -> Self {
        Self
    }

    /// Field ID for the title field.
    pub const TITLE_FIELD: FieldId = FieldId::new(0);

    /// Field ID for the body field.
    pub const BODY_FIELD: FieldId = FieldId::new(1);

    /// Field ID for the language field.
    pub const LANGUAGE_FIELD: FieldId = FieldId::new(2);
}

impl Projection<Note> for NoteProjection {
    type Id = NoteId;

    fn entity_id(&self, entity: &Note) -> Self::Id {
        entity.id
    }

    fn for_each_text_field(&self, entity: &Note, f: &mut dyn FnMut(FieldId, &str)) {
        f(Self::TITLE_FIELD, &entity.title);
        f(Self::BODY_FIELD, &entity.body);
        f(Self::LANGUAGE_FIELD, &entity.language);
    }
}

// ============================================================================
// Main Example
// ============================================================================

fn main() {
    println!("=== Leit Notes Example ===\n");

    // Step 1: Create sample notes
    let notes = create_sample_notes();
    println!("Created {} sample notes:\n", notes.len());
    for note in &notes {
        println!("  - {}: {}", note.id.as_u32(), note.title);
    }
    println!();

    // Step 2: Build an in-memory index
    println!("Building index...");
    let projection = NoteProjection::new();
    let mut builder = InMemoryIndexBuilder::new();

    for note in &notes {
        builder.add_entity(note, &projection);
    }

    let index = builder.finish();
    println!("Index built successfully!\n");

    // Step 3: Execute a query
    let search_term = "rust";
    println!("Searching for: '{}'\n", search_term);

    let query = term(search_term);
    let plan = index.create_execution_plan(&query);
    let mut workspace = ExecutionWorkspace::new();

    // Use TopKCollector to get top results
    let mut collector = leit_collect::TopKCollector::<NoteId>::new(10);
    index.execute(&plan, &mut workspace, &mut collector);
    let results = collector.into_sorted_vec();

    // Step 4: Print results
    println!("Found {} results:\n", results.len());

    if results.is_empty() {
        println!("  No matching notes found.");
        println!("  (Note: This is expected as the full query execution is still being implemented)");
    } else {
        for hit in &results {
            // Find the note with this ID
            if let Some(note) = notes.iter().find(|n| n.id == hit.id) {
                println!("  Score {:.4}: {} ({})", 
                    hit.score.as_f32(), 
                    note.title, 
                    note.language
                );
                println!("    {}", note.body.lines().next().unwrap_or(""));
                println!();
            }
        }
    }

    println!("=== Example Complete ===");
}

// ============================================================================
// Sample Data
// ============================================================================

fn create_sample_notes() -> Vec<Note> {
    vec![
        Note {
            id: NoteId::new(1),
            title: "Introduction to Rust".to_string(),
            body: "Rust is a systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety.".to_string(),
            language: "en".to_string(),
        },
        Note {
            id: NoteId::new(2),
            title: "Rust Ownership System".to_string(),
            body: "The ownership system is Rust's most unique feature, enabling memory safety without a garbage collector.".to_string(),
            language: "en".to_string(),
        },
        Note {
            id: NoteId::new(3),
            title: "Pattern Matching in Rust".to_string(),
            body: "Pattern matching is a powerful feature in Rust that allows you to destructure data and control flow based on patterns.".to_string(),
            language: "en".to_string(),
        },
        Note {
            id: NoteId::new(4),
            title: "Introduction to Go".to_string(),
            body: "Go is a statically typed, compiled programming language designed at Google for building simple, reliable, and efficient software.".to_string(),
            language: "en".to_string(),
        },
        Note {
            id: NoteId::new(5),
            title: "Go Concurrency Patterns".to_string(),
            body: "Go's concurrency model makes it easy to write programs that get the most out of multi-core and networked machines.".to_string(),
            language: "en".to_string(),
        },
        Note {
            id: NoteId::new(6),
            title: "Introduction to Python".to_string(),
            body: "Python is a high-level, interpreted programming language known for its clear syntax and readability.".to_string(),
            language: "en".to_string(),
        },
        Note {
            id: NoteId::new(7),
            title: "Python Type Hints".to_string(),
            body: "Type hints in Python allow you to specify the expected types of function arguments and return values.".to_string(),
            language: "en".to_string(),
        },
        Note {
            id: NoteId::new(8),
            title: "Rust Async Programming".to_string(),
            body: "Async/await in Rust allows you to write asynchronous code that looks like synchronous code.".to_string(),
            language: "en".to_string(),
        },
        Note {
            id: NoteId::new(9),
            title: "Go Modules".to_string(),
            body: "Go modules are the official dependency management system for Go, introduced in Go 1.11.".to_string(),
            language: "en".to_string(),
        },
        Note {
            id: NoteId::new(10),
            title: "Python Decorators".to_string(),
            body: "Decorators are a powerful feature in Python that allows you to modify the behavior of functions or classes.".to_string(),
            language: "en".to_string(),
        },
    ]
}
