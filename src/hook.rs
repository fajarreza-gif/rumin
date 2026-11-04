use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub struct CircuitBreaker;

#[derive(Debug, Clone, serde::Serialize)]
pub struct HookCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl CircuitBreaker {
    pub const DEFAULT_LINE_LIMIT: usize = 50;

    /// Validate if reading a file slice conforms to token circuit breaker rules
    pub fn check_slice(lines: usize, max_lines: Option<usize>) -> HookCheckResult {
        let limit = max_lines.unwrap_or(Self::DEFAULT_LINE_LIMIT);
        if lines > limit {
            HookCheckResult {
                allowed: false,
                reason: Some(format!(
                    "[Token Circuit Breaker]: Requested {} lines exceeds maximum allowed threshold of {} lines.",
                    lines, limit
                )),
            }
        } else {
            HookCheckResult {
                allowed: true,
                reason: None,
            }
        }
    }

    /// Check if a command contains hazardous operations
    pub fn check_command(command: &str) -> HookCheckResult {
        let trimmed = command.trim();
        let dangerous = [
            "rm -rf /",
            "rm -rf ~",
            ":(){ :|:& };:",
            "mkfs",
            "dd if=/dev/zero",
        ];

        for pattern in &dangerous {
            if trimmed.contains(pattern) {
                return HookCheckResult {
                    allowed: false,
                    reason: Some(format!("Forbidden hazardous command pattern detected: '{}'", pattern)),
                };
            }
        }

        HookCheckResult {
            allowed: true,
            reason: None,
        }
    }

    /// Install git pre-commit hook into repository non-destructively
    pub fn install_git_hook(repo_root: &Path) -> Result<PathBuf> {
        let hooks_dir = repo_root.join(".git/hooks");
        fs::create_dir_all(&hooks_dir)?;
        let hook_path = hooks_dir.join("pre-commit");

        let safety_snippet = r#"
# ai-rumin safety hook
if command -v ai-rumin &> /dev/null; then
    ai-rumin verify || exit 1
fi
"#;

        if hook_path.exists() {
            let existing = fs::read_to_string(&hook_path)?;
            if !existing.contains("ai-rumin verify") {
                // Non-destructive backup of existing pre-commit hook
                let backup_path = hooks_dir.join("pre-commit.rumin.bak");
                let _ = fs::copy(&hook_path, backup_path);

                use std::io::Write;
                let mut file = fs::OpenOptions::new().append(true).open(&hook_path)?;
                file.write_all(safety_snippet.as_bytes())?;
            }
        } else {
            let script = format!("#!/usr/bin/env bash\n{}", safety_snippet);
            fs::write(&hook_path, script)?;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&hook_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&hook_path, perms)?;
        }

        Ok(hook_path)
    }
}
