pub use crate::types::{CodeEntity, EdgeKind, EntityType};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::HashMap;
use std::io::{Read, Write};
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
            let old_file = self.graph[existing_idx].file_path.clone();
            if old_file != file_path {
                if let Some(entities) = self.file_to_entities.get_mut(&old_file) {
                    entities.retain(|e_id| e_id != &id);
                }
            }
            self.graph[existing_idx] = entity;
            existing_idx
        } else {
            let new_idx = self.graph.add_node(entity);
            self.id_to_index.insert(id.clone(), new_idx);
            new_idx
        };

        let file_entities = self.file_to_entities.entry(file_path).or_default();
        if !file_entities.contains(&id) {
            file_entities.push(id);
        }

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

    /// Check if a directed edge exists between two entities by their IDs.
    pub fn has_edge(&self, from_id: &str, to_id: &str) -> bool {
        if let (Some(&u), Some(&v)) = (self.id_to_index.get(from_id), self.id_to_index.get(to_id)) {
            self.graph.contains_edge(u, v)
        } else {
            false
        }
    }

    /// Retrieve an entity by ID.
    pub fn get_entity(&self, id: &str) -> Option<&CodeEntity> {
        self.id_to_index.get(id).map(|&idx| &self.graph[idx])
    }

    /// Retrieve all entities currently in the graph.
    pub fn all_entities(&self) -> Vec<&CodeEntity> {
        self.graph.node_indices().map(|idx| &self.graph[idx]).collect()
    }

    /// Retrieve node index by ID.
    pub fn get_index(&self, id: &str) -> Option<NodeIndex> {
        self.id_to_index.get(id).copied()
    }

    /// Remove an individual entity by its ID and clean up references.
    pub fn remove_entity(&mut self, id: &str) -> bool {
        if let Some(idx) = self.id_to_index.remove(id) {
            let file_path = self.graph[idx].file_path.clone();
            if let Some(_removed) = self.graph.remove_node(idx) {
                if idx.index() < self.graph.node_count() {
                    let swapped_id = self.graph[idx].id.clone();
                    self.id_to_index.insert(swapped_id, idx);
                }
            }
            if let Some(entities) = self.file_to_entities.get_mut(&file_path) {
                entities.retain(|e_id| e_id != id);
            }
            true
        } else {
            false
        }
    }

    /// Remove all entities associated with a specific file (used during incremental sync).
    pub fn remove_file(&mut self, file_path: &str) {
        if file_path == "<synthetic>" {
            return;
        }
        if let Some(entity_ids) = self.file_to_entities.remove(file_path) {
            for id in entity_ids {
                // Protect canonical shared entity nodes from deletion on client file modifications
                if (id.starts_with("sobject::") || id.starts_with("erlang::") || id.starts_with("apex::") || id.starts_with("otp::") || id.starts_with("com::") || id.starts_with("dotnet::") || id.starts_with("dll::"))
                    && !file_path.ends_with(".object-meta.xml") && !file_path.ends_with(".object")
                {
                    continue;
                }
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

    pub const CHECKPOINT_MAGIC: &'static [u8; 5] = b"RUMN\x01";

    /// Save graph as high-speed compact binary compressed checkpoint atomically.
    pub fn save_binary_checkpoint(&self, path: &Path) -> anyhow::Result<()> {
        let serializable = self.to_serializable();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp_path = path.with_extension("tmp");
        {
            let file = std::fs::File::create(&tmp_path)?;
            let mut encoder = GzEncoder::new(file, Compression::fast());
            encoder.write_all(Self::CHECKPOINT_MAGIC)?;
            bincode::serialize_into(&mut encoder, &serializable)?;
            encoder.finish()?;
        }
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Load graph from high-speed binary checkpoint, with automatic fallback for legacy formats.
    pub fn load_binary_checkpoint(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path)?;
        if bytes.is_empty() {
            anyhow::bail!("Checkpoint file is empty");
        }

        // 1. Try gzip decompressing first
        if bytes.starts_with(&[0x1f, 0x8b]) {
            let mut decoder = GzDecoder::new(&bytes[..]);
            let mut decompressed = Vec::new();
            if decoder.read_to_end(&mut decompressed).is_ok() {
                if decompressed.starts_with(Self::CHECKPOINT_MAGIC) {
                    if let Ok(serializable) = bincode::deserialize::<SerializableGraph>(&decompressed[5..]) {
                        return Ok(Self::from_serializable(serializable));
                    }
                    if let Ok(serializable) = serde_json::from_slice::<SerializableGraph>(&decompressed[5..]) {
                        return Ok(Self::from_serializable(serializable));
                    }
                }
                if let Ok(serializable) = bincode::deserialize::<SerializableGraph>(&decompressed) {
                    return Ok(Self::from_serializable(serializable));
                }
                if let Ok(serializable) = serde_json::from_slice::<SerializableGraph>(&decompressed) {
                    return Ok(Self::from_serializable(serializable));
                }
            }
        }

        // 2. Uncompressed bincode with magic header
        if bytes.starts_with(Self::CHECKPOINT_MAGIC) {
            if let Ok(serializable) = bincode::deserialize::<SerializableGraph>(&bytes[5..]) {
                return Ok(Self::from_serializable(serializable));
            }
            if let Ok(serializable) = serde_json::from_slice::<SerializableGraph>(&bytes[5..]) {
                return Ok(Self::from_serializable(serializable));
            }
        }

        // 3. Legacy uncompressed JSON fallback
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
            if let Some(file_entities) = self.file_to_entities.get(sym) {
                for ent_id in file_entities {
                    if visited.insert(ent_id.clone()) {
                        queue.push_back((ent_id.clone(), 0));
                    }
                }
            } else {
                visited.insert(sym.clone());
                queue.push_back((sym.clone(), 0));
            }
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

    /// Perform comprehensive 4-quadrant architectural impact analysis:
    /// 1. Upstream Callers (UI presentation, controllers, background workers)
    /// 2. Downstream Dependencies (services called, sub-procedures)
    /// 3. Covering Unit Tests (test classes/methods traversing into this symbol)
    /// 4. Schema Footprint (database tables, columns, and custom metadata touched)
    pub fn impact_analysis(&self, target_symbol: &str, max_depth: usize) -> ImpactReport {
        use std::collections::{HashSet, VecDeque};

        let depth_limit = if max_depth == 0 { 2 } else { max_depth };

        let mut start_nodes: Vec<String> = Vec::new();
        if let Some(file_entities) = self.file_to_entities.get(target_symbol) {
            for ent_id in file_entities {
                start_nodes.push(ent_id.clone());
            }
        } else if let Some(idx) = self.find_node_by_name_or_id(target_symbol) {
            start_nodes.push(self.graph[idx].id.clone());
        } else {
            start_nodes.push(target_symbol.to_string());
        }

        let mut upstream_callers = Vec::new();
        let mut downstream_callees = Vec::new();
        let mut covering_tests = Vec::new();
        let mut schema_footprint = Vec::new();

        // 1. Upstream Traversal (Direction::Incoming)
        let mut visited_up: HashSet<String> = start_nodes.iter().cloned().collect();
        let mut queue_up: VecDeque<(String, usize)> = VecDeque::new();
        for s in &start_nodes {
            queue_up.push_back((s.clone(), 0));
        }

        while let Some((curr_id, hops)) = queue_up.pop_front() {
            if hops >= depth_limit {
                continue;
            }
            let incoming = self.neighbors(&curr_id, Direction::Incoming);
            for (neighbor, edge_kind) in incoming {
                if visited_up.insert(neighbor.id.clone()) {
                    let next_hops = hops + 1;
                    let is_test = neighbor.metadata.get("is_test").map(|v| v == "true").unwrap_or(false)
                        || neighbor.name.contains("Test")
                        || neighbor.name.starts_with("test_")
                        || neighbor.file_path.contains("/test/")
                        || neighbor.file_path.contains("/tests/")
                        || neighbor.file_path.ends_with(".test.ts")
                        || neighbor.file_path.ends_with(".test.js")
                        || neighbor.file_path.ends_with(".spec.ts");

                    let is_ui = neighbor.file_path.ends_with(".js")
                        || neighbor.file_path.ends_with(".ts")
                        || neighbor.file_path.ends_with(".cmp")
                        || neighbor.file_path.ends_with(".html")
                        || neighbor.file_path.ends_with(".oip-meta.xml")
                        || neighbor.file_path.ends_with(".os-meta.xml")
                        || neighbor.id.starts_with("ts::")
                        || neighbor.id.starts_with("omni_");

                    let is_db = neighbor.entity_type == EntityType::Table
                        || neighbor.entity_type == EntityType::Column
                        || neighbor.entity_type == EntityType::View
                        || neighbor.id.starts_with("sobject::");

                    let item = ImpactItem {
                        symbol_id: neighbor.id.clone(),
                        name: neighbor.name.clone(),
                        file_path: neighbor.file_path.clone(),
                        entity_type: neighbor.entity_type.clone(),
                        category: if is_test {
                            "unit_test".to_string()
                        } else if is_ui {
                            "ui_presentation".to_string()
                        } else if is_db {
                            "database_schema".to_string()
                        } else {
                            "service_caller".to_string()
                        },
                        hops: next_hops,
                        relation: format!("{:?}", edge_kind),
                    };

                    if is_test {
                        covering_tests.push(item);
                    } else if is_db {
                        schema_footprint.push(item);
                    } else {
                        upstream_callers.push(item);
                    }

                    queue_up.push_back((neighbor.id.clone(), next_hops));
                }
            }
        }

        // 2. Downstream Traversal (Direction::Outgoing)
        let mut visited_down: HashSet<String> = start_nodes.iter().cloned().collect();
        let mut queue_down: VecDeque<(String, usize)> = VecDeque::new();
        for s in &start_nodes {
            queue_down.push_back((s.clone(), 0));
        }

        while let Some((curr_id, hops)) = queue_down.pop_front() {
            if hops >= depth_limit {
                continue;
            }
            let outgoing = self.neighbors(&curr_id, Direction::Outgoing);
            for (neighbor, edge_kind) in outgoing {
                if visited_down.insert(neighbor.id.clone()) {
                    let next_hops = hops + 1;
                    let is_db = neighbor.entity_type == EntityType::Table
                        || neighbor.entity_type == EntityType::Column
                        || neighbor.entity_type == EntityType::View
                        || neighbor.id.starts_with("sobject::");

                    let is_rules = neighbor.id.starts_with("decision_matrix::")
                        || neighbor.id.starts_with("approval_process::")
                        || neighbor.id.starts_with("omni_")
                        || neighbor.id.starts_with("bundle::");

                    let item = ImpactItem {
                        symbol_id: neighbor.id.clone(),
                        name: neighbor.name.clone(),
                        file_path: neighbor.file_path.clone(),
                        entity_type: neighbor.entity_type.clone(),
                        category: if is_db {
                            "database_dependency".to_string()
                        } else if is_rules {
                            "business_rules".to_string()
                        } else {
                            "service_callee".to_string()
                        },
                        hops: next_hops,
                        relation: format!("{:?}", edge_kind),
                    };

                    if is_db {
                        if !schema_footprint.iter().any(|x| x.symbol_id == item.symbol_id) {
                            schema_footprint.push(item);
                        }
                    } else {
                        downstream_callees.push(item);
                    }

                    queue_down.push_back((neighbor.id.clone(), next_hops));
                }
            }
        }

        let total = upstream_callers.len() + downstream_callees.len() + covering_tests.len() + schema_footprint.len();
        ImpactReport {
            target_symbol: target_symbol.to_string(),
            total_impacted: total,
            upstream_callers,
            downstream_callees,
            covering_tests,
            schema_footprint,
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

    /// Search for a node index by qualified ID, exact name, or case-insensitive match.
    pub fn find_node_by_name_or_id(&self, query: &str) -> Option<NodeIndex> {
        if let Some(idx) = self.get_index(query) {
            return Some(idx);
        }
        for idx in self.graph.node_indices() {
            let ent = &self.graph[idx];
            if ent.name == query || ent.id == query {
                return Some(idx);
            }
        }
        let query_lower = query.to_lowercase();
        for idx in self.graph.node_indices() {
            let ent = &self.graph[idx];
            if ent.name.to_lowercase() == query_lower || ent.id.to_lowercase() == query_lower {
                return Some(idx);
            }
        }
        None
    }

    /// In-memory schema and symbol describe tool returning child fields, methods, and relationships.
    pub fn describe_entity(&self, name_or_id: &str) -> Option<EntityDescription> {
        let idx = self.find_node_by_name_or_id(name_or_id)?;
        let entity = self.graph[idx].clone();

        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let mut outgoing_relations = Vec::new();
        let mut incoming_relations = Vec::new();

        // Outgoing edges
        let mut edges_walker = self.graph.neighbors_directed(idx, Direction::Outgoing).detach();
        while let Some((edge_idx, target_idx)) = edges_walker.next(&self.graph) {
            let edge_kind = self.graph[edge_idx].clone();
            let target_ent = &self.graph[target_idx];
            if edge_kind == EdgeKind::Contains {
                if target_ent.entity_type == EntityType::Column || target_ent.entity_type == EntityType::Variable {
                    fields.push(target_ent.clone());
                } else if target_ent.entity_type == EntityType::Method || target_ent.entity_type == EntityType::Function {
                    methods.push(target_ent.clone());
                } else {
                    outgoing_relations.push((edge_kind, target_ent.id.clone(), target_ent.name.clone()));
                }
            } else {
                outgoing_relations.push((edge_kind, target_ent.id.clone(), target_ent.name.clone()));
            }
        }

        // Incoming edges
        let mut in_walker = self.graph.neighbors_directed(idx, Direction::Incoming).detach();
        while let Some((edge_idx, source_idx)) = in_walker.next(&self.graph) {
            let edge_kind = self.graph[edge_idx].clone();
            let source_ent = &self.graph[source_idx];
            incoming_relations.push((edge_kind, source_ent.id.clone(), source_ent.name.clone()));
        }

        Some(EntityDescription {
            entity,
            fields,
            methods,
            outgoing_relations,
            incoming_relations,
        })
    }

    /// Directed symbol-to-symbol pathfinding using petgraph simple paths.
    pub fn find_paths(&self, source: &str, target: &str, max_depth: usize) -> Vec<PathResult> {
        let src_idx = match self.find_node_by_name_or_id(source) {
            Some(i) => i,
            None => return Vec::new(),
        };
        let tgt_idx = match self.find_node_by_name_or_id(target) {
            Some(i) => i,
            None => return Vec::new(),
        };

        let depth_limit = max_depth.clamp(1, 10);
        let raw_paths: Vec<Vec<NodeIndex>> = petgraph::algo::all_simple_paths(
            &self.graph,
            src_idx,
            tgt_idx,
            0,
            Some(depth_limit),
        ).collect();

        let mut results = Vec::new();
        for path in raw_paths {
            let mut steps = Vec::new();
            for window in path.windows(2) {
                let from_idx = window[0];
                let to_idx = window[1];
                let from_ent = self.graph[from_idx].clone();
                let to_ent = self.graph[to_idx].clone();

                let edge_kind = self.graph.find_edge(from_idx, to_idx)
                    .map(|e| self.graph[e].clone())
                    .unwrap_or(EdgeKind::References);

                steps.push(PathStep {
                    from: from_ent,
                    edge: edge_kind,
                    to: to_ent,
                });
            }
            results.push(PathResult {
                source: self.graph[src_idx].name.clone(),
                target: self.graph[tgt_idx].name.clone(),
                hops: steps.len(),
                steps,
            });
        }

        results.sort_by_key(|p| p.hops);
        results
    }

    /// Audit Salesforce and runtime governor limits (SOQL in loops, DML in loops, unbounded queries) (Gap 8).
    pub fn audit_governor_limits(&self, repo_root: &std::path::Path) -> Vec<GovernorLimitViolation> {
        let mut violations = Vec::new();

        // 1. Database Volumetric & Unindexed Foreign-Key Scan Hazard Analysis
        for idx in self.graph.node_indices() {
            let entity = &self.graph[idx];
            if entity.entity_type != EntityType::Table {
                continue;
            }
            if entity.metadata.contains_key("is_alias_for") {
                continue;
            }
            if let Some(recorded_rows_str) = entity.metadata.get("recorded_rows") {
                if let Ok(rows) = recorded_rows_str.parse::<u64>() {
                    if rows >= 1_000_000 {
                        let indexed_cols: std::collections::HashSet<String> = entity
                            .metadata
                            .get("indexed_columns")
                            .map(|s| s.split(',').map(|c| c.trim().to_ascii_uppercase()).collect())
                            .unwrap_or_default();

                        // Check outgoing Contains edges for child columns
                        for edge in self.graph.edges_directed(idx, Direction::Outgoing) {
                            if *edge.weight() == EdgeKind::Contains {
                                let col_ent = &self.graph[edge.target()];
                                if col_ent.entity_type == EntityType::Column {
                                    let col_short_name = col_ent
                                        .metadata
                                        .get("short_name")
                                        .cloned()
                                        .unwrap_or_else(|| col_ent.name.split('.').last().unwrap_or(&col_ent.name).to_string());
                                    let col_upper = col_short_name.to_ascii_uppercase();

                                    let skip_cols = ["ROW_ID", "ID", "CONFLICT_ID", "MODIFICATION_NUM", "CREATED", "CREATED_BY", "LAST_UPD", "LAST_UPD_BY"];
                                    if skip_cols.contains(&col_upper.as_str()) {
                                        continue;
                                    }

                                    let is_fk = col_ent.metadata.contains_key("references_table")
                                        || col_upper.ends_with("_ID")
                                        || col_upper.ends_with("_OU_ID")
                                        || col_upper.ends_with("_ACCNT_ID");

                                    if is_fk && !indexed_cols.contains(&col_upper) {
                                        violations.push(GovernorLimitViolation {
                                            rule_name: "unindexed_high_cardinality_fk_scan".to_string(),
                                            severity: "critical".to_string(),
                                            file_path: entity.file_path.clone(),
                                            line_number: col_ent.line_start,
                                            symbol_id: col_ent.id.clone(),
                                            message: format!(
                                                "Full table scan hazard: Unindexed foreign key column '{}.{}' on high-volume table with {} recorded rows",
                                                entity.name, col_upper, rows
                                            ),
                                            recommendation: format!(
                                                "Create a B-tree index on ({}) to prevent sequential table scans during join/reconciliation queries",
                                                col_upper
                                            ),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let loop_start_re = regex::Regex::new(r"(?i)\b(for\s*\(|while\s*\(|do\s*\{)").unwrap();
        let soql_re = regex::Regex::new(r"(?i)\[\s*SELECT\b").unwrap();
        let dml_re = regex::Regex::new(r"(?i)\b(insert|update|delete|upsert)\s+([a-zA-Z0-9_]+)\s*;").unwrap();
        let callout_re = regex::Regex::new(r"(?i)(\.send\s*\(|Http\.send)").unwrap();
        let from_only_re = regex::Regex::new(r"(?i)\[\s*SELECT\b.*?FROM\s+([a-zA-Z0-9_]+)").unwrap();
        let sql_select_re = regex::Regex::new(r"(?i)\bSELECT\b.*?FROM\s+(?:[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?\.)?[`\x22']?([a-zA-Z0-9_#$]+)[`\x22']?").unwrap();

        for (file_path, entities) in &self.file_to_entities {
            let is_apex = file_path.ends_with(".cls") || file_path.ends_with(".trigger");
            let is_sql = file_path.ends_with(".sql");
            if !is_apex && !is_sql {
                continue;
            }

            let full_path = repo_root.join(file_path);
            let content = match std::fs::read_to_string(&full_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let primary_symbol = entities.first().cloned().unwrap_or_else(|| file_path.clone());

            if is_sql {
                for (idx, line) in content.lines().enumerate() {
                    let line_num = idx + 1;
                    let trimmed = line.trim();
                    if trimmed.starts_with("--") || trimmed.starts_with("/*") || trimmed.is_empty() {
                        continue;
                    }
                    if let Some(sel_cap) = sql_select_re.captures(trimmed) {
                        let tbl_name = sel_cap.get(2).unwrap().as_str();
                        let tbl_upper = tbl_name.to_ascii_uppercase();
                        let is_high_volume = self.find_node_by_name_or_id(&tbl_upper)
                            .or_else(|| self.find_node_by_name_or_id(tbl_name))
                            .and_then(|idx| self.graph[idx].metadata.get("recorded_rows"))
                            .and_then(|r| r.parse::<u64>().ok())
                            .map(|rows| rows >= 1_000_000)
                            .unwrap_or(false);

                        if is_high_volume {
                            let upper_line = trimmed.to_ascii_uppercase();
                            if !upper_line.contains("WHERE") && !upper_line.contains("ROWNUM") && !upper_line.contains("LIMIT") {
                                violations.push(GovernorLimitViolation {
                                    rule_name: "unbounded_high_cardinality_query".to_string(),
                                    severity: "warning".to_string(),
                                    file_path: file_path.clone(),
                                    line_number: line_num,
                                    symbol_id: primary_symbol.clone(),
                                    message: format!("Unbounded query on high-volume table '{}' without WHERE, ROWNUM, or LIMIT clause", tbl_name),
                                    recommendation: "Add selective WHERE filter or row limiting clause to prevent exhausting database buffer cache and cursor limits".to_string(),
                                });
                            }
                        }
                    }
                }
                continue;
            }

            let mut loop_depth: usize = 0;
            let mut brace_depth: usize = 0;
            let mut loop_brace_levels: Vec<usize> = Vec::new();

            for (idx, line) in content.lines().enumerate() {
                let line_num = idx + 1;
                let trimmed = line.trim();
                if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
                    continue;
                }

                // Track loops
                if loop_start_re.is_match(trimmed) {
                    loop_depth += 1;
                    loop_brace_levels.push(brace_depth + 1);
                }

                // Count braces
                for ch in trimmed.chars() {
                    if ch == '{' {
                        brace_depth += 1;
                    } else if ch == '}' {
                        if let Some(&level) = loop_brace_levels.last() {
                            if brace_depth <= level {
                                loop_brace_levels.pop();
                                loop_depth = loop_depth.saturating_sub(1);
                            }
                        }
                        brace_depth = brace_depth.saturating_sub(1);
                    }
                }

                if loop_depth > 0 {
                    // Check SOQL inside loop
                    if soql_re.is_match(trimmed) {
                        violations.push(GovernorLimitViolation {
                            rule_name: "soql_in_loop".to_string(),
                            severity: "error".to_string(),
                            file_path: file_path.clone(),
                            line_number: line_num,
                            symbol_id: primary_symbol.clone(),
                            message: format!("SOQL query inside loop on line {}: '{}'", line_num, trimmed),
                            recommendation: "Move SOQL query outside the loop into a Set/Map bulk query to avoid governor limit 101".to_string(),
                        });
                    }

                    // Check DML inside loop
                    if let Some(dml_cap) = dml_re.captures(trimmed) {
                        let op = dml_cap.get(1).unwrap().as_str();
                        let var = dml_cap.get(2).unwrap().as_str();
                        violations.push(GovernorLimitViolation {
                            rule_name: "dml_in_loop".to_string(),
                            severity: "error".to_string(),
                            file_path: file_path.clone(),
                            line_number: line_num,
                            symbol_id: primary_symbol.clone(),
                            message: format!("DML '{} {}' inside loop on line {}", op, var, line_num),
                            recommendation: "Accumulate records into a List and perform single bulk DML outside the loop".to_string(),
                        });
                    }

                    // Check Callout inside loop
                    if callout_re.is_match(trimmed) {
                        violations.push(GovernorLimitViolation {
                            rule_name: "callout_in_loop".to_string(),
                            severity: "error".to_string(),
                            file_path: file_path.clone(),
                            line_number: line_num,
                            symbol_id: primary_symbol.clone(),
                            message: format!("HTTP callout inside loop on line {}", line_num),
                            recommendation: "Queue asynchronous Queueable jobs or use bulk composite API to avoid 100-callout governor limit".to_string(),
                        });
                    }
                }

                // Check Unbounded queries on high-cardinality tables
                if let Some(soql_cap) = from_only_re.captures(trimmed) {
                    let table = soql_cap.get(1).unwrap().as_str();
                    let upper_tbl = table.to_ascii_uppercase();
                    let high_cardinality = ["ACCOUNT", "CONTACT", "ASSET", "ORDER", "ORDERITEM", "CONTRACT", "CASE", "OPPORTUNITY", "TASK", "EVENT", "LEAD"];
                    let is_high_card = high_cardinality.contains(&upper_tbl.as_str()) || upper_tbl.ends_with("__C");

                    if is_high_card {
                        let upper_trimmed = trimmed.to_ascii_uppercase();
                        if !upper_trimmed.contains("WHERE") && !upper_trimmed.contains("LIMIT") {
                            violations.push(GovernorLimitViolation {
                                rule_name: "unbounded_high_cardinality_query".to_string(),
                                severity: "warning".to_string(),
                                file_path: file_path.clone(),
                                line_number: line_num,
                                symbol_id: primary_symbol.clone(),
                                message: format!("Unbounded query on high-volume table '{}' without WHERE or LIMIT clause", table),
                                recommendation: "Add indexed selective WHERE filter or LIMIT clause to avoid 50,000 row query limit".to_string(),
                            });
                        }
                    }
                }
            }
        }

        violations
    }

    /// Federate an external repository's graph into this graph with a namespace alias (Gap 10).
    pub fn federate_graph(&mut self, other: &CodeGraph, repo_alias: &str) -> usize {
        let mut added = 0;
        let mut old_to_new_id: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        for idx in other.graph.node_indices() {
            let mut ent = other.graph[idx].clone();
            let orig_id = ent.id.clone();
            let new_id = if repo_alias.is_empty() || ent.id.starts_with(&format!("{}::", repo_alias)) {
                ent.id.clone()
            } else {
                format!("{}::{}", repo_alias, ent.id)
            };
            ent.id = new_id.clone();
            if !repo_alias.is_empty() {
                ent.metadata.insert("repo_alias".to_string(), repo_alias.to_string());
            }
            ent.metadata.insert("orig_id".to_string(), orig_id.clone());
            old_to_new_id.insert(orig_id, new_id);

            self.add_entity(ent);
            added += 1;
        }

        for edge in other.graph.edge_references() {
            let u_orig = &other.graph[edge.source()].id;
            let v_orig = &other.graph[edge.target()].id;
            let kind = edge.weight().clone();

            let u_new = old_to_new_id.get(u_orig).cloned().unwrap_or_else(|| u_orig.clone());
            let v_new = old_to_new_id.get(v_orig).cloned().unwrap_or_else(|| v_orig.clone());

            self.add_edge(&u_new, &v_new, kind);
        }

        added
    }
}

/// Governor limit violation item (Gap 8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernorLimitViolation {
    pub rule_name: String,
    pub severity: String,
    pub file_path: String,
    pub line_number: usize,
    pub symbol_id: String,
    pub message: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDescription {
    pub entity: CodeEntity,
    pub fields: Vec<CodeEntity>,
    pub methods: Vec<CodeEntity>,
    pub outgoing_relations: Vec<(EdgeKind, String, String)>,
    pub incoming_relations: Vec<(EdgeKind, String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathStep {
    pub from: CodeEntity,
    pub edge: EdgeKind,
    pub to: CodeEntity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathResult {
    pub source: String,
    pub target: String,
    pub hops: usize,
    pub steps: Vec<PathStep>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactItem {
    pub symbol_id: String,
    pub name: String,
    pub file_path: String,
    pub entity_type: EntityType,
    pub category: String,
    pub hops: usize,
    pub relation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactReport {
    pub target_symbol: String,
    pub total_impacted: usize,
    pub upstream_callers: Vec<ImpactItem>,
    pub downstream_callees: Vec<ImpactItem>,
    pub covering_tests: Vec<ImpactItem>,
    pub schema_footprint: Vec<ImpactItem>,
}
