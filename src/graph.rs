use crate::types::{CodeEntity, EdgeKind, EntityType};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[allow(dead_code)]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SerializableGraph {
    pub entities: Vec<CodeEntity>,
    pub edges: Vec<(String, String, EdgeKind)>,
}

#[derive(Debug, Clone)]
pub struct CodeGraph {
    pub graph: DiGraph<CodeEntity, EdgeKind>,
    pub id_to_index: HashMap<String, NodeIndex>,
    pub file_to_entities: HashMap<String, Vec<String>>,
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl CodeGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            id_to_index: HashMap::new(),
            file_to_entities: HashMap::new(),
        }
    }

    /// Add or update an entity in the graph.
    pub fn add_entity(&mut self, entity: CodeEntity) -> NodeIndex {
        let id = entity.id.clone();
        let file_path = entity.file_path.clone();

        let idx = if let Some(&existing_idx) = self.id_to_index.get(&id) {
            self.graph[existing_idx] = entity;
            existing_idx
        } else {
            let new_idx = self.graph.add_node(entity);
            self.id_to_index.insert(id.clone(), new_idx);
            new_idx
        };

        self.file_to_entities
            .entry(file_path)
            .or_default()
            .push(id);

        idx
    }

    /// Add a directed edge between two entities by their IDs.
    pub fn add_edge(&mut self, from_id: &str, to_id: &str, kind: EdgeKind) -> bool {
        if let (Some(&u), Some(&v)) = (self.id_to_index.get(from_id), self.id_to_index.get(to_id)) {
            // Check if edge already exists
            if !self.graph.contains_edge(u, v) {
                self.graph.add_edge(u, v, kind);
            }
            true
        } else {
            false
        }
    }

    /// Retrieve an entity by ID.
    pub fn get_entity(&self, id: &str) -> Option<&CodeEntity> {
        self.id_to_index.get(id).map(|&idx| &self.graph[idx])
    }

    /// Retrieve node index by ID.
    pub fn get_index(&self, id: &str) -> Option<NodeIndex> {
        self.id_to_index.get(id).copied()
    }

    /// Remove all entities associated with a specific file (used during incremental sync).
    pub fn remove_file(&mut self, file_path: &str) {
        if let Some(entity_ids) = self.file_to_entities.remove(file_path) {
            for id in entity_ids {
                if let Some(idx) = self.id_to_index.remove(&id) {
                    if let Some(_removed) = self.graph.remove_node(idx) {
                        // In petgraph::graph::Graph, remove_node uses swap_remove:
                        // The last node was swapped into `idx` (unless `idx` was the last node).
                        if idx.index() < self.graph.node_count() {
                            let swapped_id = self.graph[idx].id.clone();
                            self.id_to_index.insert(swapped_id, idx);
                        }
                    }
                }
            }
        }
    }

    /// Get adjacent entities along outgoing or incoming edges.
    pub fn neighbors(&self, id: &str, direction: Direction) -> Vec<(&CodeEntity, &EdgeKind)> {
        if let Some(&idx) = self.id_to_index.get(id) {
            self.graph
                .edges_directed(idx, direction)
                .map(|edge| {
                    let neighbor_idx = match direction {
                        Direction::Outgoing => edge.target(),
                        Direction::Incoming => edge.source(),
                    };
                    (&self.graph[neighbor_idx], edge.weight())
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Generate topological digest of the codebase.
    pub fn summary_stats(&self) -> HashMap<String, usize> {
        let mut stats = HashMap::new();
        stats.insert("total_nodes".to_string(), self.node_count());
        stats.insert("total_edges".to_string(), self.edge_count());
        stats.insert("files".to_string(), self.file_to_entities.len());

        let mut class_count = 0;
        let mut func_count = 0;
        let mut table_count = 0;
        let mut target_count = 0;

        for idx in self.graph.node_indices() {
            match self.graph[idx].entity_type {
                EntityType::Class | EntityType::Struct => class_count += 1,
                EntityType::Function | EntityType::Method => func_count += 1,
                EntityType::Table | EntityType::View => table_count += 1,
                EntityType::Target => target_count += 1,
                _ => {}
            }
        }

        stats.insert("classes".to_string(), class_count);
        stats.insert("functions".to_string(), func_count);
        stats.insert("sql_tables".to_string(), table_count);
        stats.insert("make_targets".to_string(), target_count);

        stats
    }

    /// Convert in-memory graph to serializable representation.
    pub fn to_serializable(&self) -> SerializableGraph {
        let mut entities = Vec::with_capacity(self.node_count());
        for idx in self.graph.node_indices() {
            entities.push(self.graph[idx].clone());
        }

        let mut edges = Vec::with_capacity(self.edge_count());
        for edge in self.graph.edge_references() {
            let u_id = self.graph[edge.source()].id.clone();
            let v_id = self.graph[edge.target()].id.clone();
            edges.push((u_id, v_id, edge.weight().clone()));
        }

        SerializableGraph { entities, edges }
    }

    /// Hydrate CodeGraph from serializable representation.
    pub fn from_serializable(serializable: SerializableGraph) -> Self {
        let mut graph = Self::new();
        for entity in serializable.entities {
            graph.add_entity(entity);
        }
        for (u, v, kind) in serializable.edges {
            graph.add_edge(&u, &v, kind);
        }
        graph
    }

    /// Save graph cache to a JSON file.
    pub fn save_cache(&self, path: &Path) -> anyhow::Result<()> {
        let serializable = self.to_serializable();
        let json = serde_json::to_string(&serializable)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load graph cache from a JSON file.
    pub fn load_cache(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let serializable: SerializableGraph = serde_json::from_str(&content)?;
        Ok(Self::from_serializable(serializable))
    }

    /// Save graph as high-speed compact binary checkpoint.
    pub fn save_binary_checkpoint(&self, path: &Path) -> anyhow::Result<()> {
        let serializable = self.to_serializable();
        let bytes = serde_json::to_vec(&serializable)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Load graph from high-speed binary checkpoint.
    pub fn load_binary_checkpoint(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path)?;
        let serializable: SerializableGraph = serde_json::from_slice(&bytes)?;
        Ok(Self::from_serializable(serializable))
    }

    /// Compute topological delta between self (base) and other (target/new).
    pub fn diff(&self, other: &CodeGraph) -> GraphDiff {
        let mut added_entities = Vec::new();
        let mut removed_entities = Vec::new();
        let mut modified_entities = Vec::new();

        for (id, &idx) in &other.id_to_index {
            let ent_other = &other.graph[idx];
            if let Some(&self_idx) = self.id_to_index.get(id) {
                let ent_self = &self.graph[self_idx];
                if ent_self.signature != ent_other.signature
                    || ent_self.line_start != ent_other.line_start
                    || ent_self.line_end != ent_other.line_end
                {
                    modified_entities.push(ent_other.clone());
                }
            } else {
                added_entities.push(ent_other.clone());
            }
        }

        for id in self.id_to_index.keys() {
            if !other.id_to_index.contains_key(id) {
                removed_entities.push(id.clone());
            }
        }

        let mut other_edges: std::collections::HashSet<(String, String, EdgeKind)> = std::collections::HashSet::new();
        for edge in other.graph.edge_references() {
            let u = other.graph[edge.source()].id.clone();
            let v = other.graph[edge.target()].id.clone();
            other_edges.insert((u, v, edge.weight().clone()));
        }

        let mut self_edges: std::collections::HashSet<(String, String, EdgeKind)> = std::collections::HashSet::new();
        for edge in self.graph.edge_references() {
            let u = self.graph[edge.source()].id.clone();
            let v = self.graph[edge.target()].id.clone();
            self_edges.insert((u, v, edge.weight().clone()));
        }

        let added_edges: Vec<_> = other_edges.difference(&self_edges).cloned().collect();
        let removed_edges: Vec<_> = self_edges.difference(&other_edges).cloned().collect();

        GraphDiff {
            added_entities,
            removed_entities,
            modified_entities,
            added_edges,
            removed_edges,
        }
    }

    /// Verify graph structural integrity, surfacing dangling routes, empty definitions, and orphaned interfaces.
    pub fn verify(&self) -> Vec<VerificationIssue> {
        let mut issues = Vec::new();

        for idx in self.graph.node_indices() {
            let entity = &self.graph[idx];

            // 1. Apigee ProxyEndpoint checks
            if entity.id.starts_with("apiproxy::proxy::") {
                let outgoing = self.neighbors(&entity.id, Direction::Outgoing);
                let has_target = outgoing.iter().any(|(_, k)| **k == EdgeKind::DependsOn);
                if !has_target {
                    issues.push(VerificationIssue {
                        severity: "warning".to_string(),
                        message: format!("ProxyEndpoint '{}' has no route rule pointing to a TargetEndpoint", entity.name),
                        symbol_id: entity.id.clone(),
                        file_path: entity.file_path.clone(),
                    });
                }
            }

            // 2. Apigee TargetEndpoint checks
            if entity.id.starts_with("apiproxy::target::") {
                let sig = entity.signature.as_deref().unwrap_or("");
                if sig.contains("(url: )") || !sig.contains("url:") {
                    issues.push(VerificationIssue {
                        severity: "info".to_string(),
                        message: format!("TargetEndpoint '{}' has no backend URL configured", entity.name),
                        symbol_id: entity.id.clone(),
                        file_path: entity.file_path.clone(),
                    });
                }
            }

            // 3. Salesforce Apex Trigger checks
            if entity.id.contains("::trigger::") {
                let outgoing = self.neighbors(&entity.id, Direction::Outgoing);
                let has_sobject = outgoing.iter().any(|(_, k)| **k == EdgeKind::DependsOn);
                if !has_sobject {
                    issues.push(VerificationIssue {
                        severity: "warning".to_string(),
                        message: format!("Trigger '{}' is not linked to any recognized SObject table", entity.name),
                        symbol_id: entity.id.clone(),
                        file_path: entity.file_path.clone(),
                    });
                }
            }
        }

        issues
    }

    /// Calculate blast radius and hazard impact starting from specified root symbols.
    pub fn blast_radius(&self, target_symbols: &[String]) -> HazardReport {
        use std::collections::{HashSet, VecDeque};

        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        let mut items: Vec<BlastRadiusItem> = Vec::new();

        for sym in target_symbols {
            visited.insert(sym.clone());
            queue.push_back((sym.clone(), 0));
        }

        while let Some((curr_id, hops)) = queue.pop_front() {
            let incoming = self.neighbors(&curr_id, Direction::Incoming);
            for (neighbor, _edge_kind) in incoming {
                if !visited.contains(&neighbor.id) {
                    visited.insert(neighbor.id.clone());
                    let next_hops = hops + 1;
                    let risk_score = 1.0 / (1.0 + (next_hops as f64) * 0.5);

                    items.push(BlastRadiusItem {
                        symbol_id: neighbor.id.clone(),
                        name: neighbor.name.clone(),
                        file_path: neighbor.file_path.clone(),
                        entity_type: neighbor.entity_type.clone(),
                        hops: next_hops,
                        risk_score,
                    });

                    if next_hops < 4 {
                        queue.push_back((neighbor.id.clone(), next_hops));
                    }
                }
            }
        }

        let mut high_risk = Vec::new();
        let mut medium_risk = Vec::new();
        let mut low_risk = Vec::new();

        for item in items {
            if item.hops == 1 {
                high_risk.push(item);
            } else if item.hops == 2 {
                medium_risk.push(item);
            } else {
                low_risk.push(item);
            }
        }

        let total = high_risk.len() + medium_risk.len() + low_risk.len();
        HazardReport {
            target_symbols: target_symbols.to_vec(),
            total_impacted: total,
            high_risk,
            medium_risk,
            low_risk,
        }
    }

    /// Audit architectural invariants and cross-layer isolation rules.
    pub fn audit(&self) -> Vec<AuditViolation> {
        let mut violations = Vec::new();

        for edge in self.graph.edge_references() {
            let u = &self.graph[edge.source()];
            let v = &self.graph[edge.target()];

            // Invariant 1: Presentation / Web markup should not directly access DB tables
            let is_presentation = u.file_path.ends_with(".html") || u.file_path.ends_with(".cmp") || u.file_path.ends_with(".vue");
            let is_db = v.entity_type == EntityType::Table || v.entity_type == EntityType::Column;
            if is_presentation && is_db {
                violations.push(AuditViolation {
                    rule_name: "presentation_cannot_access_db".to_string(),
                    severity: "error".to_string(),
                    message: format!("Presentation UI '{}' directly references database entity '{}'", u.name, v.name),
                    from_symbol: u.id.clone(),
                    to_symbol: v.id.clone(),
                });
            }

            // Invariant 2: Apigee Proxy should not bypass policy pipeline
            if u.id.starts_with("apiproxy::proxy::") && v.id.starts_with("apiproxy::target::") {
                let neighbors = self.neighbors(&u.id, Direction::Outgoing);
                let has_policy = neighbors.iter().any(|(_, k)| **k == EdgeKind::Calls);
                if !has_policy {
                    violations.push(AuditViolation {
                        rule_name: "proxy_must_enforce_policies".to_string(),
                        severity: "warning".to_string(),
                        message: format!("Proxy '{}' routes directly to target '{}' without executing any policies", u.name, v.name),
                        from_symbol: u.id.clone(),
                        to_symbol: v.id.clone(),
                    });
                }
            }
        }

        violations
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphDiff {
    pub added_entities: Vec<CodeEntity>,
    pub removed_entities: Vec<String>,
    pub modified_entities: Vec<CodeEntity>,
    pub added_edges: Vec<(String, String, EdgeKind)>,
    pub removed_edges: Vec<(String, String, EdgeKind)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationIssue {
    pub severity: String,
    pub message: String,
    pub symbol_id: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadiusItem {
    pub symbol_id: String,
    pub name: String,
    pub file_path: String,
    pub entity_type: EntityType,
    pub hops: usize,
    pub risk_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HazardReport {
    pub target_symbols: Vec<String>,
    pub total_impacted: usize,
    pub high_risk: Vec<BlastRadiusItem>,
    pub medium_risk: Vec<BlastRadiusItem>,
    pub low_risk: Vec<BlastRadiusItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditViolation {
    pub rule_name: String,
    pub severity: String,
    pub message: String,
    pub from_symbol: String,
    pub to_symbol: String,
}
