use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuminConfig {
    pub version: String,
    pub token_budget: usize,
    pub sidecar_path: Option<String>,
    pub checkpoint_dir: String,
    pub ignored_patterns: Vec<String>,
}

impl Default for RuminConfig {
    fn default() -> Self {
        Self {
            version: "0.1.0".to_string(),
            token_budget: 1500,
            sidecar_path: Some("sidecar/worker.py".to_string()),
            checkpoint_dir: ".rumin/checkpoints".to_string(),
            ignored_patterns: vec![
                "target".to_string(),
                "node_modules".to_string(),
                ".git".to_string(),
                ".rumin".to_string(),
                "dist".to_string(),
                "build".to_string(),
                "__pycache__".to_string(),
            ],
        }
    }
}

impl RuminConfig {
    pub const DIR_NAME: &'static str = ".rumin";
    pub const CONFIG_FILE: &'static str = ".rumin/config.json";
    pub const IGNORE_FILE: &'static str = ".ruminignore";

    pub fn init_project(repo_path: &Path) -> anyhow::Result<PathBuf> {
        let rumin_dir = repo_path.join(Self::DIR_NAME);
        let checkpoint_dir = repo_path.join(".rumin/checkpoints");
        fs::create_dir_all(&checkpoint_dir)?;

        let config_path = repo_path.join(Self::CONFIG_FILE);
        if !config_path.exists() {
            let default_config = Self::default();
            let json = serde_json::to_string_pretty(&default_config)?;
            fs::write(&config_path, json)?;
        }

        let ignore_path = repo_path.join(Self::IGNORE_FILE);
        if !ignore_path.exists() {
            let default_ignore = r#"# ai-rumin ignore rules
target/
node_modules/
.git/
.rumin/
dist/
build/
__pycache__/
*.pyc
*.lock
"#;
            fs::write(&ignore_path, default_ignore)?;
        }

        Ok(rumin_dir)
    }

    pub fn load(repo_path: &Path) -> Self {
        let config_path = repo_path.join(Self::CONFIG_FILE);
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(config_path) {
                if let Ok(config) = serde_json::from_str(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }
}
