use crate::graph::CodeGraph;
use crate::types::EdgeKind;
use anyhow::Result;
use petgraph::Direction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConsolidationReport {
    pub pruned_nodes: usize,
    pub shortcut_edges_created: usize,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub duration_ms: u128,
}

pub struct MemoryConsolidator;

impl MemoryConsolidator {
    pub fn consolidate(graph: &mut CodeGraph) -> Result<ConsolidationReport> {
        let start = std::time::Instant::now();
        let mut shortcut_edges_created = 0;
        let mut shortcuts_to_add: Vec<(String, String)> = Vec::new();

        // 1. Identify high-confidence 2-hop transitive paths: A -> B -> C (capped at 100 to prevent memory explosion)
        let max_shortcuts = 100;
        'outer: for idx in graph.graph.node_indices() {
            let u_id = graph.graph[idx].id.clone();
            let out_neighbors = graph.neighbors(&u_id, Direction::Outgoing);

            for (mid_entity, edge_k1) in &out_neighbors {
                if **edge_k1 == EdgeKind::Calls || **edge_k1 == EdgeKind::DependsOn {
                    let second_hop = graph.neighbors(&mid_entity.id, Direction::Outgoing);
                    for (target_entity, edge_k2) in second_hop {
                        if (*edge_k2 == EdgeKind::Calls || *edge_k2 == EdgeKind::References)
                            && target_entity.id != u_id
                        {
                            shortcuts_to_add.push((u_id.clone(), target_entity.id.clone()));
                            if shortcuts_to_add.len() >= max_shortcuts {
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }

        // Add shortcut edges
        for (u, v) in shortcuts_to_add {
            if graph.add_edge(&u, &v, EdgeKind::References) {
                shortcut_edges_created += 1;
            }
        }

        // 2. Identify and prune dead orphaned temporary entities
        let mut to_remove = Vec::new();
        for idx in graph.graph.node_indices() {
            let entity = &graph.graph[idx];
            if entity.file_path.starts_with(".tmp") || entity.id.starts_with("temp::") {
                to_remove.push(entity.id.clone());
            }
        }

        let mut pruned_nodes = 0;
        for id in to_remove {
            if graph.remove_entity(&id) {
                pruned_nodes += 1;
            }
        }

        let duration_ms = start.elapsed().as_millis();
        Ok(ConsolidationReport {
            pruned_nodes,
            shortcut_edges_created,
            total_nodes: graph.node_count(),
            total_edges: graph.edge_count(),
            duration_ms,
        })
    }
}
