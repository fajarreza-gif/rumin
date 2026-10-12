use crate::graph::CodeGraph;
use crate::hippocampal::HippocampalEngine;
use crate::indexer::Indexer;
use crate::sidecar::SidecarClient;
use crate::types::ContextBudget;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, serde::Serialize)]
pub struct BenchmarkReport {
    pub total_files_indexed: usize,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub indexing_time_ms: u128,
    pub indexing_throughput_files_per_sec: f64,
    pub ppr_queries_executed: usize,
    pub ppr_avg_latency_ms: f64,
    pub ppr_throughput_qps: f64,
    pub estimated_memory_kb: usize,
}

pub struct BenchRunner;

impl BenchRunner {
    pub async fn run_benchmark(repo_root: &Path, sidecar: &SidecarClient) -> anyhow::Result<BenchmarkReport> {
        let mut graph = CodeGraph::new();
        let indexer = Indexer::new(repo_root);

        // 1. Indexing Benchmark
        let index_start = Instant::now();
        let total_files = indexer.index_repository(&mut graph, sidecar).await?;
        let indexing_time_ms = index_start.elapsed().as_millis();
        let indexing_time_secs = (indexing_time_ms as f64) / 1000.0;
        let indexing_throughput = if indexing_time_secs > 0.0 {
            (total_files as f64) / indexing_time_secs
        } else {
            0.0
        };

        // 2. PPR Latency Benchmark
        let engine = HippocampalEngine::default();
        let budget = ContextBudget::default();
        let sample_queries = [
            "account trigger rating active",
            "orders api apiproxy verifyapikey",
            "cache supervisor gen_server worker",
            "auth token session security",
            "database query transaction commit",
        ];

        let ppr_start = Instant::now();
        let num_iterations = 20;
        let total_queries = sample_queries.len() * num_iterations;

        for _ in 0..num_iterations {
            for q in &sample_queries {
                let _ = engine.query(&graph, q, &budget);
            }
        }

        let ppr_total_ms = ppr_start.elapsed().as_millis() as f64;
        let ppr_avg_latency_ms = if total_queries > 0 {
            ppr_total_ms / (total_queries as f64)
        } else {
            0.0
        };
        let ppr_throughput_qps = if ppr_total_ms > 0.0 {
            (total_queries as f64) / (ppr_total_ms / 1000.0)
        } else {
            0.0
        };

        // 3. Memory Estimate
        let estimated_memory_kb = (graph.node_count() * 256 + graph.edge_count() * 64) / 1024;

        Ok(BenchmarkReport {
            total_files_indexed: total_files,
            total_nodes: graph.node_count(),
            total_edges: graph.edge_count(),
            indexing_time_ms,
            indexing_throughput_files_per_sec: indexing_throughput,
            ppr_queries_executed: total_queries,
            ppr_avg_latency_ms,
            ppr_throughput_qps,
            estimated_memory_kb,
        })
    }
}
