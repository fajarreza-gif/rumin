use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Semantic type of a structural code entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Module,
    Class,
    Function,
    Method,
    Interface,
    Struct,
    Target,       // Makefile / Build target
    Table,        // SQL Table
    View,         // SQL View
    Column,       // SQL Column
    Variable,     // Exported shell variable / constant
    Rule,         // Static hazard or invariant rule
    Tag,          // XML / HTML element or component tag
    #[serde(other)]
    Unknown,
}

/// A deterministic structural code node in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEntity {
    pub id: String,
    pub name: String,
    pub entity_type: EntityType,
    pub file_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl CodeEntity {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        entity_type: EntityType,
        file_path: impl Into<String>,
        line_start: usize,
        line_end: usize,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            entity_type,
            file_path: file_path.into(),
            line_start,
            line_end,
            signature: None,
            docstring: None,
            metadata: HashMap::new(),
        }
    }
}

/// Relationship between two code entities.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Contains,
    Calls,
    Imports,
    DependsOn,
    References,
    Implements,
    DefinesTable,
}

/// Token budget constraints for context delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudget {
    pub total_token_limit: usize,
    pub max_symbols: usize,
    pub include_signatures: bool,
    pub expand_bodies: bool,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            total_token_limit: 1500,
            max_symbols: 15,
            include_signatures: true,
            expand_bodies: false,
        }
    }
}

/// Context result assembled via hippocampal activation spread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub query: String,
    pub top_symbols: Vec<ScoredSymbol>,
    pub explanatory_paths: Vec<Vec<String>>,
    pub formatted_context: String,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredSymbol {
    pub id: String,
    pub name: String,
    pub score: f64,
    pub file_path: String,
    pub entity_type: EntityType,
    pub signature: Option<String>,
}

/// Request sent to the Python semantic sidecar worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarRequest {
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

/// Response returned from the Python semantic sidecar worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarResponse {
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}
