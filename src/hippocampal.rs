use crate::graph::CodeGraph;
use crate::types::{ContextBudget, EdgeKind, QueryResult, ScoredSymbol};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use std::collections::{HashMap, HashSet};

pub struct HippocampalEngine {
    pub alpha: f64,        // Restart / teleport probability (default 0.20)
    pub max_iterations: usize,
    pub tolerance: f64,
}

impl Default for HippocampalEngine {
    fn default() -> Self {
        Self {
            alpha: 0.20,
            max_iterations: 30,
            tolerance: 1e-5,
        }
    }
}

#[allow(dead_code)]
impl HippocampalEngine {
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha,
            ..Default::default()
        }
    }

    /// Tokenize text into normalized tokens (splitting camelCase, snake_case, and non-alphanumeric).
    pub fn tokenize(text: &str) -> HashSet<String> {
        let mut tokens = HashSet::new();
        let trimmed_lower = text.trim().to_lowercase();
        if !trimmed_lower.is_empty() {
            tokens.insert(trimmed_lower.clone());
            let alpha_num: String = trimmed_lower.chars().filter(|c| c.is_alphanumeric()).collect();
            if !alpha_num.is_empty() {
                tokens.insert(alpha_num);
            }
        }
        let mut current = String::new();

        for ch in text.chars() {
            if ch.is_alphanumeric() {
                // Check for camelCase boundary
                if ch.is_uppercase() && !current.is_empty() && !current.chars().all(|c| c.is_uppercase()) {
                    if current.len() > 1 || text.len() == 1 {
                        tokens.insert(current.to_lowercase());
                    }
                    current.clear();
                }
                current.push(ch);
            } else {
                if !current.is_empty() {
                    if current.len() > 1 || text.len() == 1 {
                        tokens.insert(current.to_lowercase());
                    }
                    current.clear();
                }
            }
        }
        if !current.is_empty() {
            if current.len() > 1 || text.len() == 1 {
                tokens.insert(current.to_lowercase());
            }
        }

        tokens
    }

    /// Match tokens against node names, paths, and docstrings to discover candidate seed nodes.
    pub fn find_seed_nodes(&self, graph: &CodeGraph, query_tokens: &HashSet<String>) -> HashMap<String, f64> {
        self.find_seed_nodes_with_prefix(graph, query_tokens, None)
    }

    /// Match tokens with optional path prefix filter.
    pub fn find_seed_nodes_with_prefix(
        &self,
        graph: &CodeGraph,
        query_tokens: &HashSet<String>,
        path_prefix: Option<&str>,
    ) -> HashMap<String, f64> {
        let mut seed_scores: HashMap<String, f64> = HashMap::new();

        for idx in graph.graph.node_indices() {
            let entity = &graph.graph[idx];
            if let Some(prefix) = path_prefix {
                if !entity.file_path.starts_with(prefix) && entity.file_path != "<salesforce-apex>" && entity.file_path != "<salesforce-schema>" {
                    continue;
                }
            }
            let name_tokens = Self::tokenize(&entity.name);
            let path_tokens = Self::tokenize(&entity.file_path);
            let clean_name = entity.name.trim().to_lowercase();
            let symbol_suffix = entity.id.split("::").last().unwrap_or("").to_lowercase();
            let short_name = entity
                .metadata
                .get("short_name")
                .map(|s| s.to_lowercase())
                .unwrap_or_else(|| clean_name.split('.').last().unwrap_or("").to_string());

            let mut match_score = 0.0;

            for token in query_tokens {
                if clean_name == *token {
                    match_score += 50.0; // Significant bonus for exact entity name match
                }
                if symbol_suffix == *token {
                    match_score += 40.0; // Significant bonus for exact symbol ID suffix match
                }
                if short_name == *token {
                    match_score += 45.0; // Exact column / short name match bonus
                }
                if name_tokens.contains(token) {
                    match_score += 3.0; // Higher weight for exact name match
                }
                if path_tokens.contains(token) {
                    match_score += 1.0;
                }
                if let Some(sig) = &entity.signature {
                    if sig.to_lowercase().contains(token) {
                        match_score += 1.5;
                    }
                }
                if let Some(doc) = &entity.docstring {
                    if doc.to_lowercase().contains(token) {
                        match_score += 1.0;
                    }
                }
            }

            if match_score > 0.0 {
                seed_scores.insert(entity.id.clone(), match_score);
            }
        }

        seed_scores
    }

    /// Run Personalized PageRank (PPR) over the CodeGraph from initial seed activations.
    pub fn personalized_pagerank(
        &self,
        graph: &CodeGraph,
        seeds: &HashMap<String, f64>,
    ) -> Vec<(String, f64)> {
        self.personalized_pagerank_with_damping(graph, seeds, false)
    }

    /// Run Personalized PageRank (PPR) with optional hub-node degree dampening.
    pub fn personalized_pagerank_with_damping(
        &self,
        graph: &CodeGraph,
        seeds: &HashMap<String, f64>,
        dampen_hubs: bool,
    ) -> Vec<(String, f64)> {
        let n = graph.node_count();
        if n == 0 {
            return Vec::new();
        }

        // Build mapping from NodeIndex to contiguous matrix index 0..N-1
        let nodes: Vec<_> = graph.graph.node_indices().collect();
        let mut idx_to_pos = HashMap::with_capacity(n);
        for (pos, &idx) in nodes.iter().enumerate() {
            idx_to_pos.insert(idx, pos);
        }

        // 1. Build initial restart vector v
        let mut v = vec![0.0; n];
        let total_seed_weight: f64 = seeds.values().sum();

        if total_seed_weight > 0.0 {
            for (id, weight) in seeds {
                if let Some(idx) = graph.get_index(id) {
                    if let Some(&pos) = idx_to_pos.get(&idx) {
                        v[pos] = weight / total_seed_weight;
                    }
                }
            }
        } else {
            // Uniform restart if no seed matches
            let uniform = 1.0 / (n as f64);
            v.fill(uniform);
        }

        // 2. Precompute out-degrees and adjacency
        let mut out_degrees = vec![0usize; n];
        let mut outgoing_edges: Vec<Vec<usize>> = vec![Vec::new(); n];

        for &u in &nodes {
            let u_pos = idx_to_pos[&u];
            let mut neighbors: Vec<usize> = graph
                .graph
                .neighbors_directed(u, Direction::Outgoing)
                .map(|v| idx_to_pos[&v])
                .collect();

            // Bidirectional flow across Contains edges (Column/Field/Method -> Table/Class/Interface)
            for edge in graph.graph.edges_directed(u, Direction::Incoming) {
                if *edge.weight() == EdgeKind::Contains {
                    let parent_pos = idx_to_pos[&edge.source()];
                    if !neighbors.contains(&parent_pos) {
                        neighbors.push(parent_pos);
                    }
                }
            }

            out_degrees[u_pos] = neighbors.len();
            outgoing_edges[u_pos] = neighbors;
        }

        // 3. Power iteration: p^(k+1) = (1 - alpha) * M * p^(k) + alpha * v
        let mut p = v.clone();
        let mut p_next = vec![0.0; n];

        for _ in 0..self.max_iterations {
            p_next.fill(0.0);

            let mut dangling_weight = 0.0;
            for i in 0..n {
                if out_degrees[i] == 0 {
                    dangling_weight += p[i];
                } else {
                    let share = p[i] / (out_degrees[i] as f64);
                    for &target in &outgoing_edges[i] {
                        p_next[target] += share;
                    }
                }
            }

            // Distribute dangling weight uniformly
            let dangling_share = dangling_weight / (n as f64);

            let mut max_diff = 0.0f64;
            for i in 0..n {
                p_next[i] = (1.0 - self.alpha) * (p_next[i] + dangling_share) + self.alpha * v[i];
                let diff = (p_next[i] - p[i]).abs();
                if diff > max_diff {
                    max_diff = diff;
                }
            }

            p = p_next.clone();

            if max_diff < self.tolerance {
                break;
            }
        }

        // 4. Collect top-scoring node indices, avoiding cloning strings for all N nodes
        let in_degrees: Vec<usize> = if dampen_hubs {
            nodes.iter().map(|&u| graph.graph.neighbors_directed(u, Direction::Incoming).count()).collect()
        } else {
            Vec::new()
        };

        let mut indexed: Vec<(usize, f64)> = (0..n).map(|i| {
            let mut score = p[i];
            if dampen_hubs && in_degrees.get(i).copied().unwrap_or(0) > 3 {
                let in_deg = in_degrees[i] as f64;
                score /= 1.0 + in_deg.sqrt() * 0.5;
            }
            (i, score)
        }).collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let scored: Vec<(String, f64)> = indexed
            .into_iter()
            .take(100)
            .map(|(pos, score)| (graph.graph[nodes[pos]].id.clone(), score))
            .collect();

        scored
    }

    /// Query the associative code memory under strict context budgets.
    pub fn query(&self, graph: &CodeGraph, query_str: &str, budget: &ContextBudget) -> QueryResult {
        self.query_with_root(graph, query_str, budget, None)
    }

    /// Query the associative code memory under strict context budgets with explicit repo root.
    pub fn query_with_root(
        &self,
        graph: &CodeGraph,
        query_str: &str,
        budget: &ContextBudget,
        repo_root: Option<&std::path::Path>,
    ) -> QueryResult {
        let tokens = Self::tokenize(query_str);
        let seeds = self.find_seed_nodes_with_prefix(graph, &tokens, budget.path_prefix.as_deref());
        let scored = self.personalized_pagerank_with_damping(graph, &seeds, budget.dampen_hubs);

        let mut top_symbols = Vec::new();
        let mut formatted_lines = Vec::new();
        // formatted_lines.push(format!("# HIPPOCAMPAL ASSOCIATIVE CONTEXT (Query: \"{}\")", query_str));
        formatted_lines.push(format!("Active Knowledge Graph: {} nodes, {} edges\n", graph.node_count(), graph.edge_count()));

        let mut explanatory_paths = Vec::new();
        let max_chars = budget.total_token_limit.saturating_mul(4);
        let mut current_chars = formatted_lines.iter().map(|l| l.len() + 1).sum::<usize>();

        for (id, score) in scored.iter() {
            if top_symbols.len() >= budget.max_symbols || current_chars >= max_chars {
                break;
            }

            if let Some(entity) = graph.get_entity(id) {
                // If path_prefix is specified, enforce on extracted symbols
                if let Some(ref prefix) = budget.path_prefix {
                    if !entity.file_path.starts_with(prefix) && entity.file_path != "<salesforce-apex>" && entity.file_path != "<salesforce-schema>" {
                        continue;
                    }
                }

                // If types filter is specified, check if entity matches
                if let Some(ref target_types) = budget.types {
                    let type_str = match entity.entity_type {
                        crate::types::EntityType::Class => "class",
                        crate::types::EntityType::Module => "module",
                        crate::types::EntityType::Function => "function",
                        crate::types::EntityType::Method => "method",
                        crate::types::EntityType::Interface => "interface",
                        crate::types::EntityType::Struct => "struct",
                        crate::types::EntityType::Table => "table",
                        crate::types::EntityType::View => "view",
                        crate::types::EntityType::Column => "column",
                        crate::types::EntityType::Rule => "rule",
                        crate::types::EntityType::Target => "target",
                        crate::types::EntityType::Variable => "variable",
                        crate::types::EntityType::Tag => "tag",
                        crate::types::EntityType::Unknown => "unknown",
                    };
                    let is_cmdt = entity.id.contains("__mdt")
                        || entity.metadata.get("kind").map(|k| k == "CustomMetadata").unwrap_or(false);
                    let matches_type = target_types.iter().any(|t| {
                        let t_lower = t.to_lowercase();
                        t_lower == type_str
                            || (t_lower == "custom_metadata" && is_cmdt)
                            || (t_lower == "apex_class" && entity.id.starts_with("apex::"))
                            || (t_lower == "sobject" && entity.id.starts_with("sobject::"))
                            || (t_lower == "decision_matrix" && entity.id.starts_with("decision_matrix::"))
                            || (t_lower == "approval_process" && entity.id.starts_with("approval_process::"))
                    });
                    if !matches_type {
                        continue;
                    }
                }
                let mut symbol_lines = Vec::new();
                symbol_lines.push(format!(
                    "--- Symbol: {} (PPR Score: {:.4}) ---",
                    entity.id, score
                ));
                symbol_lines.push(format!(
                    "File: {}:{}-{}",
                    entity.file_path, entity.line_start, entity.line_end
                ));
                if let Some(sig) = &entity.signature {
                    symbol_lines.push(format!("Signature: {}", sig));
                }
                if let Some(doc) = &entity.docstring {
                    symbol_lines.push(format!("Doc: {}", doc));
                }

                if budget.expand_bodies {
                    let actual_path = if let Some(root) = repo_root {
                        root.join(&entity.file_path)
                    } else {
                        std::path::PathBuf::from(&entity.file_path)
                    };

                    if let Ok(file_content) = std::fs::read_to_string(&actual_path) {
                        let lines: Vec<&str> = file_content.lines().collect();
                        if entity.line_start > 0 && entity.line_start <= lines.len() {
                            let end_line = entity.line_end.min(lines.len()).min(entity.line_start + 25);
                            let slice = lines[entity.line_start - 1..end_line].join("\n");
                            symbol_lines.push("```".to_string());
                            symbol_lines.push(slice);
                            symbol_lines.push("```".to_string());
                        }
                    } else if entity.id.starts_with("episode::") || entity.file_path.contains("episodes/") {
                        if let Some(doc) = &entity.docstring {
                            symbol_lines.push("```markdown".to_string());
                            symbol_lines.push(doc.clone());
                            symbol_lines.push("```".to_string());
                        }
                    }
                }
                symbol_lines.push(String::new());

                let symbol_chars = symbol_lines.iter().map(|l| l.len() + 1).sum::<usize>();
                if current_chars + symbol_chars > max_chars && !top_symbols.is_empty() {
                    break;
                }

                top_symbols.push(ScoredSymbol {
                    id: entity.id.clone(),
                    name: entity.name.clone(),
                    score: *score,
                    file_path: entity.file_path.clone(),
                    entity_type: entity.entity_type.clone(),
                    signature: entity.signature.clone(),
                });

                current_chars += symbol_chars;
                formatted_lines.extend(symbol_lines);

                // Find 1-hop dependencies for explanatory causal paths
                let neighbors = graph.neighbors(id, Direction::Outgoing);
                if !neighbors.is_empty() {
                    let path: Vec<String> = std::iter::once(id.clone())
                        .chain(neighbors.iter().take(3).map(|(ent, _)| ent.id.clone()))
                        .collect();
                    explanatory_paths.push(path);
                }
            }
        }

        let mut formatted_context = formatted_lines.join("\n");
        if formatted_context.len() > max_chars && max_chars > 0 {
            formatted_context.truncate(max_chars);
        }
        let estimated_tokens = formatted_context.len() / 4;

        QueryResult {
            query: query_str.to_string(),
            top_symbols,
            explanatory_paths,
            formatted_context,
            estimated_tokens,
        }
    }
}
