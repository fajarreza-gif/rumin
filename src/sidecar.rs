use crate::types::{CodeEntity, SidecarRequest, SidecarResponse};
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::info;

struct SidecarProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

pub struct SidecarClient {
    worker_path: PathBuf,
    process: Arc<Mutex<Option<SidecarProcess>>>,
    request_counter: AtomicU64,
}

#[allow(dead_code)]
impl SidecarClient {
    pub fn new(worker_path: impl Into<PathBuf>) -> Self {
        Self {
            worker_path: worker_path.into(),
            process: Arc::new(Mutex::new(None)),
            request_counter: AtomicU64::new(1),
        }
    }

    fn spawn_process(worker_path: &Path) -> Result<SidecarProcess> {
        if !worker_path.exists() {
            return Err(anyhow!("Python worker script not found at {:?}", worker_path));
        }

        info!("Starting Python Semantic Sidecar worker at {:?}", worker_path);

        let mut cmd = Command::new("python3");
        cmd.arg(worker_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);

        let mut child_proc = cmd.spawn()?;
        let stdin = child_proc.stdin.take().ok_or_else(|| anyhow!("Failed to open sidecar stdin"))?;
        let stdout = child_proc.stdout.take().ok_or_else(|| anyhow!("Failed to open sidecar stdout"))?;

        Ok(SidecarProcess {
            child: child_proc,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    /// Spawn or restart the Python worker subprocess.
    pub async fn ensure_started(&self) -> Result<()> {
        let mut proc_guard = self.process.lock().await;
        if let Some(ref mut proc) = *proc_guard {
            match proc.child.try_wait() {
                Ok(None) => return Ok(()),
                _ => {
                    *proc_guard = None;
                }
            }
        }

        let new_proc = Self::spawn_process(&self.worker_path)?;
        *proc_guard = Some(new_proc);
        Ok(())
    }

    /// Send a JSON-RPC request to the Python sidecar worker atomically under a single lock with timeout.
    async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let mut proc_guard = self.process.lock().await;

        let needs_spawn = match *proc_guard {
            Some(ref mut proc) => match proc.child.try_wait() {
                Ok(None) => false,
                _ => true,
            },
            None => true,
        };

        if needs_spawn {
            let new_proc = Self::spawn_process(&self.worker_path)?;
            *proc_guard = Some(new_proc);
        }

        let req_id = self.request_counter.fetch_add(1, Ordering::SeqCst);
        let request = SidecarRequest {
            id: req_id,
            method: method.to_string(),
            params,
        };

        let mut req_line = serde_json::to_string(&request)?;
        req_line.push('\n');

        let proc = proc_guard.as_mut().unwrap();

        // Write to sidecar stdin
        if let Err(e) = proc.stdin.write_all(req_line.as_bytes()).await {
            *proc_guard = None;
            return Err(anyhow!("Failed to write to sidecar stdin: {}", e));
        }
        if let Err(e) = proc.stdin.flush().await {
            *proc_guard = None;
            return Err(anyhow!("Failed to flush sidecar stdin: {}", e));
        }

        // Read from sidecar stdout with 10s timeout
        let mut response_line = String::new();
        match tokio::time::timeout(std::time::Duration::from_secs(10), proc.stdout.read_line(&mut response_line)).await {
            Ok(Ok(0)) => {
                *proc_guard = None;
                return Err(anyhow!("Sidecar worker closed stdout pipe (EOF)"));
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                *proc_guard = None;
                return Err(anyhow!("Error reading sidecar stdout: {}", e));
            }
            Err(_) => {
                *proc_guard = None;
                return Err(anyhow!("Sidecar request timed out after 10s"));
            }
        }

        if response_line.trim().is_empty() {
            *proc_guard = None;
            return Err(anyhow!("Empty response from sidecar"));
        }

        let resp: SidecarResponse = serde_json::from_str(&response_line)?;
        if let Some(err) = resp.error {
            return Err(anyhow!("Sidecar error: {}", err));
        }

        resp.result.ok_or_else(|| anyhow!("Sidecar returned empty result"))
    }

    /// Health-check ping to the Python sidecar.
    pub async fn ping(&self) -> Result<bool> {
        let res = self.call("ping", serde_json::json!({})).await?;
        let status = res.get("status").and_then(|s| s.as_str());
        Ok(status == Some("pong"))
    }

    /// Parse multi-dialect SQL statements through the sidecar.
    pub async fn parse_sql(&self, file_path: &str, content: &str, dialect: Option<&str>) -> Result<Vec<CodeEntity>> {
        let params = serde_json::json!({
            "file_path": file_path,
            "content": content,
            "dialect": dialect,
        });

        let res = self.call("parse_sql", params).await?;
        if let Some(raw_entities) = res.get("entities") {
            let entities: Vec<CodeEntity> = serde_json::from_value(raw_entities.clone())?;
            Ok(entities)
        } else {
            Ok(Vec::new())
        }
    }

    /// Parse Shell scripts through the sidecar.
    pub async fn parse_shell(&self, file_path: &str, content: &str) -> Result<Vec<CodeEntity>> {
        let params = serde_json::json!({
            "file_path": file_path,
            "content": content,
        });

        let res = self.call("parse_shell", params).await?;
        if let Some(raw_entities) = res.get("entities") {
            let entities: Vec<CodeEntity> = serde_json::from_value(raw_entities.clone())?;
            Ok(entities)
        } else {
            Ok(Vec::new())
        }
    }

    /// Parse XML documents through the sidecar.
    pub async fn parse_xml(&self, file_path: &str, content: &str) -> Result<Vec<CodeEntity>> {
        let params = serde_json::json!({
            "file_path": file_path,
            "content": content,
        });

        let res = self.call("parse_xml", params).await?;
        if let Some(raw_entities) = res.get("entities") {
            let entities: Vec<CodeEntity> = serde_json::from_value(raw_entities.clone())?;
            Ok(entities)
        } else {
            Ok(Vec::new())
        }
    }

    /// Parse HTML documents through the sidecar.
    pub async fn parse_html(&self, file_path: &str, content: &str) -> Result<Vec<CodeEntity>> {
        let params = serde_json::json!({
            "file_path": file_path,
            "content": content,
        });

        let res = self.call("parse_html", params).await?;
        if let Some(raw_entities) = res.get("entities") {
            let entities: Vec<CodeEntity> = serde_json::from_value(raw_entities.clone())?;
            Ok(entities)
        } else {
            Ok(Vec::new())
        }
    }

    /// Parse Apex classes and triggers through the sidecar.
    pub async fn parse_apex(&self, file_path: &str, content: &str) -> Result<Vec<CodeEntity>> {
        let params = serde_json::json!({
            "file_path": file_path,
            "content": content,
        });

        let res = self.call("parse_apex", params).await?;
        if let Some(raw_entities) = res.get("entities") {
            let entities: Vec<CodeEntity> = serde_json::from_value(raw_entities.clone())?;
            Ok(entities)
        } else {
            Ok(Vec::new())
        }
    }

    /// Parse Erlang modules and applications through the sidecar.
    pub async fn parse_erlang(&self, file_path: &str, content: &str) -> Result<Vec<CodeEntity>> {
        let params = serde_json::json!({
            "file_path": file_path,
            "content": content,
        });

        let res = self.call("parse_erlang", params).await?;
        if let Some(raw_entities) = res.get("entities") {
            let entities: Vec<CodeEntity> = serde_json::from_value(raw_entities.clone())?;
            Ok(entities)
        } else {
            Ok(Vec::new())
        }
    }

    /// Parse Apigee API proxy bundles and policies through the sidecar.
    pub async fn parse_apigee(&self, file_path: &str, content: &str) -> Result<Vec<CodeEntity>> {
        let params = serde_json::json!({
            "file_path": file_path,
            "content": content,
        });

        let res = self.call("parse_apigee", params).await?;
        if let Some(raw_entities) = res.get("entities") {
            let entities: Vec<CodeEntity> = serde_json::from_value(raw_entities.clone())?;
            Ok(entities)
        } else {
            Ok(Vec::new())
        }
    }
}
