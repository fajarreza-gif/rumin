use crate::graph::CodeGraph;
use crate::types::{ContextBudget, QueryResult, ScoredSymbol};
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
        let mut current = String::new();

        for ch in text.chars() {
            if ch.is_alphanumeric() {
                // Check for camelCase boundary
                if ch.is_uppercase() && !current.is_empty() && !current.chars().all(|c| c.is_uppercase()) {
                    tokens.insert(current.to_lowercase());
                    current.clear();
                }
                current.push(ch);
            } else {
                if !current.is_empty() {
                    tokens.insert(current.to_lowercase());
                    current.clear();
                }
            }
        }
        if !current.is_empty() {
            tokens.insert(current.to_lowercase());
        }

        tokens
    }

    /// Match tokens against node names, paths, and docstrings to discover candidate seed nodes.
    pub fn find_seed_nodes(&self, graph: &CodeGraph, query_tokens: &HashSet<String>) -> HashMap<String, f64> {
        let mut seed_scores: HashMap<String, f64> = HashMap::new();

        for idx in graph.graph.node_indices() {
            let entity = &graph.graph[idx];
            let name_tokens = Self::tokenize(&entity.name);
            let path_tokens = Self::tokenize(&entity.file_path);

            let mut match_score = 0.0;

            for token in query_tokens {
                if name_tokens.contains(token) {
                    match_score += 3.0; // Higher weight for exact name match
                }
                if path_tokens.contains(token) {
                    match_score += 1.0;
                }
                if let Some(doc) = &entity.docstring {
                    if doc.to_lowercase().contains(token) {
                        match_score += 0.5;
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
            let neighbors: Vec<usize> = graph
                .graph
                .neighbors_directed(u, Direction::Outgoing)
                .map(|v| idx_to_pos[&v])
                .collect();
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

        // 4. Collect scores and sort descending
        let mut scored: Vec<(String, f64)> = nodes
            .iter()
            .enumerate()
            .map(|(pos, &node_idx)| (graph.graph[node_idx].id.clone(), p[pos]))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }

    /// Query the associative code memory under strict context budgets.
    pub fn query(&self, graph: &CodeGraph, query_str: &str, budget: &ContextBudget) -> QueryResult {
        let tokens = Self::tokenize(query_str);
        let seeds = self.find_seed_nodes(graph, &tokens);
        let scored = self.personalized_pagerank(graph, &seeds);

        let mut top_symbols = Vec::new();
        let mut formatted_lines = Vec::new();
        formatted_lines.push(format!("# HIPPOCAMPAL ASSOCIATIVE CONTEXT (Query: \"{}\")", query_str));
        formatted_lines.push(format!("Active Knowledge Graph: {} nodes, {} edges\n", graph.node_count(), graph.edge_count()));

        let mut explanatory_paths = Vec::new();

        for (id, score) in scored.iter().take(budget.max_symbols) {
            if let Some(entity) = graph.get_entity(id) {
                top_symbols.push(ScoredSymbol {
                    id: entity.id.clone(),
                    name: entity.name.clone(),
                    score: *score,
                    file_path: entity.file_path.clone(),
                    entity_type: entity.entity_type.clone(),
                    signature: entity.signature.clone(),
                });

                formatted_lines.push(format!(
                    "--- Symbol: {} (PPR Score: {:.4}) ---",
                    entity.id, score
                ));
                formatted_lines.push(format!(
                    "File: {}:{}-{}",
                    entity.file_path, entity.line_start, entity.line_end
                ));
                if let Some(sig) = &entity.signature {
                    formatted_lines.push(format!("Signature: {}", sig));
                }
                if let Some(doc) = &entity.docstring {
                    formatted_lines.push(format!("Doc: {}", doc));
                }
                formatted_lines.push(String::new());

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

        let formatted_context = formatted_lines.join("\n");
        let estimated_tokens = formatted_context.len() / 4; // Standard heuristic: 4 chars / token

        QueryResult {
            query: query_str.to_string(),
            top_symbols,
            explanatory_paths,
            formatted_context,
            estimated_tokens,
        }
    }
}
