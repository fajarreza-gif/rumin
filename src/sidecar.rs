use crate::types::{CodeEntity, SidecarRequest, SidecarResponse};
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::info;

pub struct SidecarClient {
    worker_path: PathBuf,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    stdout: Arc<Mutex<Option<BufReader<ChildStdout>>>>,
    child: Arc<Mutex<Option<Child>>>,
    request_counter: AtomicU64,
}

#[allow(dead_code)]
impl SidecarClient {
    pub fn new(worker_path: impl Into<PathBuf>) -> Self {
        Self {
            worker_path: worker_path.into(),
            stdin: Arc::new(Mutex::new(None)),
            stdout: Arc::new(Mutex::new(None)),
            child: Arc::new(Mutex::new(None)),
            request_counter: AtomicU64::new(1),
        }
    }

    /// Spawn or restart the Python worker subprocess.
    pub async fn ensure_started(&self) -> Result<()> {
        let mut child_guard = self.child.lock().await;
        if child_guard.is_some() {
            return Ok(());
        }

        if !self.worker_path.exists() {
            return Err(anyhow!("Python worker script not found at {:?}", self.worker_path));
        }

        info!("Starting Python Semantic Sidecar worker at {:?}", self.worker_path);

        let mut cmd = Command::new("python3");
        cmd.arg(&self.worker_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child_proc = cmd.spawn()?;
        let stdin = child_proc.stdin.take().ok_or_else(|| anyhow!("Failed to open sidecar stdin"))?;
        let stdout = child_proc.stdout.take().ok_or_else(|| anyhow!("Failed to open sidecar stdout"))?;

        *self.stdin.lock().await = Some(stdin);
        *self.stdout.lock().await = Some(BufReader::new(stdout));
        *child_guard = Some(child_proc);

        Ok(())
    }

    /// Send a JSON-RPC request to the Python sidecar worker.
    async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        self.ensure_started().await?;

        let req_id = self.request_counter.fetch_add(1, Ordering::SeqCst);
        let request = SidecarRequest {
            id: req_id,
            method: method.to_string(),
            params,
        };

        let mut req_line = serde_json::to_string(&request)?;
        req_line.push('\n');

        // Write to sidecar stdin
        {
            let mut stdin_guard = self.stdin.lock().await;
            if let Some(stdin) = stdin_guard.as_mut() {
                stdin.write_all(req_line.as_bytes()).await?;
                stdin.flush().await?;
            } else {
                return Err(anyhow!("Sidecar stdin unavailable"));
            }
        }

        // Read from sidecar stdout
        let mut response_line = String::new();
        {
            let mut stdout_guard = self.stdout.lock().await;
            if let Some(stdout) = stdout_guard.as_mut() {
                stdout.read_line(&mut response_line).await?;
            } else {
                return Err(anyhow!("Sidecar stdout unavailable"));
            }
        }

        if response_line.trim().is_empty() {
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
