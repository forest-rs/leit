use leit_core::{EntityId, FieldId, Hit, Score};
use leit_index::{InMemoryIndexBuilder, InMemoryIndex, Projection};
use leit_query::{QueryProgram, QueryBuilder};
use leit_collect::TopKCollector;

// Define entity types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct NoteId(u64);

impl EntityId for NoteId {}

#[derive(Debug, Clone)]
pub enum Language {
    En,
    Ru,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub id: NoteId,
    pub title: String,
    pub body: String,
    pub language: Language,
}

// Define projection
pub struct NoteProjection;

impl Projection<Note> for NoteProjection {
    type Id = NoteId;
    
    fn entity_id(&self, entity: &Note) -> NoteId {
        entity.id.clone()
    }
    
    fn for_each_text_field(&self, entity: &Note, f: &mut dyn FnMut(FieldId, &str)) {
        f(FieldId::new(0), &entity.title);
        f(FieldId::new(1), &entity.body);
    }
}

#[test]
fn test_e2e_entity_search() {
    // Create test entities
    let note1 = Note {
        id: NoteId(1),
        title: "Rust Programming".to_string(),
        body: "Rust is a systems programming language focused on safety and performance.".to_string(),
        language: Language::En,
    };
    
    let note2 = Note {
        id: NoteId(2),
        title: "Information Retrieval".to_string(),
        body: "Search engines use inverted indices for fast text retrieval.".to_string(),
        language: Language::En,
    };
    
    let note3 = Note {
        id: NoteId(3),
        title: "Rust for Web".to_string(),
        body: "Building web services in Rust with good performance.".to_string(),
        language: Language::En,
    };
    
    // Build index
    let projection = NoteProjection;
    let mut builder = InMemoryIndexBuilder::new();
    builder.add_entity(&note1, &projection);
    builder.add_entity(&note2, &projection);
    builder.add_entity(&note3, &projection);
    let index = builder.finish();
    
    // Build query
    let query: QueryProgram = QueryBuilder::new()
        .term(FieldId::new(0), "rust")
        .unwrap()
        .or(QueryBuilder::new().term(FieldId::new(1), "retrieval").unwrap())
        .build();
    
    // Execute search
    let plan = index.create_execution_plan(&query);
    let mut workspace = ExecutionWorkspace::new();
    let mut collector = TopKCollector::new(10);
    
    let results: Vec<Hit<NoteId>> = index.execute(&plan, &mut workspace, &mut collector);
    
    // Verify results
    assert!(!results.is_empty(), "Should find at least one result");
    
    // Notes 1 and 2 should match (rust in title, retrieval in body)
    let ids: Vec<u64> = results.iter().map(|h| h.id.0).collect();
    assert!(ids.contains(&1), "Should find note 1 with 'rust' in title");
    assert!(ids.contains(&2), "Should find note 2 with 'retrieval' in body");
}
