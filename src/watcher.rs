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

#[derive(Clone)]
pub struct FileWatcher {
    repo_root: PathBuf,
    graph: Arc<ArcSwap<CodeGraph>>,
    sidecar: Arc<SidecarClient>,
    running: Arc<AtomicBool>,
}

pub struct WatcherGuard {
    _watcher: RecommendedWatcher,
    file_watcher: FileWatcher,
}

impl Drop for WatcherGuard {
    fn drop(&mut self) {
        self.file_watcher.stop();
    }
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

    pub fn start(&self) -> anyhow::Result<WatcherGuard> {
        let (async_tx, mut async_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = async_tx.send(res);
            },
            Config::default(),
        )?;

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
                // Wait asynchronously until an event arrives - zero CPU when idle
                let first_res = match async_rx.recv().await {
                    Some(res) => res,
                    None => break,
                };

                let mut should_sync = match first_res {
                    Ok(ref event) => Self::is_relevant_event(event, &repo_root),
                    Err(ref e) => {
                        error!("Watch error: {:?}", e);
                        false
                    }
                };

                // Debounce window (150ms): drain rapid successive events
                tokio::time::sleep(Duration::from_millis(150)).await;
                while let Ok(res) = async_rx.try_recv() {
                    match res {
                        Ok(event) => {
                            if Self::is_relevant_event(&event, &repo_root) {
                                should_sync = true;
                            }
                        }
                        Err(e) => error!("Watch error: {:?}", e),
                    }
                }

                if should_sync && running.load(Ordering::SeqCst) {
                    info!("Change detected by watcher, performing lock-free incremental sync...");
                    let mut g = (*graph.load_full()).clone();
                    if let Err(e) = indexer.sync_repository(&mut g, &sidecar, false).await {
                        error!("Incremental sync failed: {:?}", e);
                    } else {
                        let synced_arc = Arc::new(g);
                        graph.rcu(|current| {
                            let mut has_new_episode = false;
                            for idx in current.graph.node_indices() {
                                let ent = &current.graph[idx];
                                if ent.id.starts_with("episode::") && synced_arc.get_entity(&ent.id).is_none() {
                                    has_new_episode = true;
                                    break;
                                }
                            }
                            if !has_new_episode {
                                synced_arc.clone()
                            } else {
                                let mut merged = (*synced_arc).clone();
                                for idx in current.graph.node_indices() {
                                    let ent = &current.graph[idx];
                                    if ent.id.starts_with("episode::") && merged.get_entity(&ent.id).is_none() {
                                        merged.add_entity(ent.clone());
                                    }
                                }
                                Arc::new(merged)
                            }
                        });
                    }
                }
            }
        });

        Ok(WatcherGuard {
            _watcher: watcher,
            file_watcher: self.clone(),
        })
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
