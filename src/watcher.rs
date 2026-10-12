use crate::graph::CodeGraph;
use crate::indexer::Indexer;
use crate::sidecar::SidecarClient;
use arc_swap::ArcSwap;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

pub struct FileWatcher {
    repo_root: PathBuf,
    graph: Arc<ArcSwap<CodeGraph>>,
    sidecar: Arc<SidecarClient>,
    running: Arc<AtomicBool>,
}

impl FileWatcher {
    pub fn new(
        repo_root: impl Into<PathBuf>,
        graph: Arc<ArcSwap<CodeGraph>>,
        sidecar: Arc<SidecarClient>,
    ) -> Self {
        Self {
            repo_root: repo_root.into(),
            graph,
            sidecar,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&self) -> anyhow::Result<RecommendedWatcher> {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

        watcher.watch(&self.repo_root, RecursiveMode::Recursive)?;
        info!("Started background file watcher on {:?}", self.repo_root);

        let repo_root = self.repo_root.clone();
        let graph = self.graph.clone();
        let sidecar = self.sidecar.clone();
        let running = self.running.clone();
        running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            let indexer = Indexer::new(&repo_root);
            while running.load(Ordering::SeqCst) {
                // Collect events with debounce
                let mut should_sync = false;
                while let Ok(res) = rx.try_recv() {
                    match res {
                        Ok(event) => {
                            if Self::is_relevant_event(&event, &repo_root) {
                                should_sync = true;
                            }
                        }
                        Err(e) => error!("Watch error: {:?}", e),
                    }
                }

                if should_sync {
                    info!("Change detected by watcher, performing lock-free incremental sync...");
                    let mut g = (*graph.load_full()).clone();
                    if let Err(e) = indexer.sync_repository(&mut g, &sidecar, false).await {
                        error!("Incremental sync failed: {:?}", e);
                    } else {
                        graph.store(Arc::new(g));
                    }
                }

                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        });

        Ok(watcher)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn is_relevant_event(event: &Event, _repo_root: &Path) -> bool {
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                event.paths.iter().any(|p| {
                    !p.components().any(|c| {
                        let s = c.as_os_str().to_string_lossy();
                        matches!(
                            s.as_ref(),
                            ".git" | "target" | "node_modules" | ".rumin" | ".aimem" | "build" | "dist"
                        )
                    })
                })
            }
            _ => false,
        }
    }
}
