use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub server_name: String,
    pub default_repo_root: PathBuf,
    pub default_pinned_terms: Vec<String>,
    pub default_invariants: Vec<String>,
}

impl ServerConfig {
    pub fn resolved_default_repo_root(&self) -> PathBuf {
        // Updated by Grok Build 0.1 for GitHub issue #7: prefer NEURALFORGE_MEMORY_CANON_ROOT, fallback to legacy AGENTOS_CORE_CANON_ROOT for compat
        env::var("NEURALFORGE_MEMORY_CANON_ROOT")
            .or_else(|_| env::var("AGENTOS_CORE_CANON_ROOT"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| self.default_repo_root.clone())
    }

    pub fn policy_markdown(&self) -> String {
        let bullets = self
            .default_invariants
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "# Canon Policy\n\nDefault canon root: `{}`\n\n{}\n",
            self.resolved_default_repo_root().display(),
            bullets
        )
    }
}

pub fn discover_repo_home() -> Result<PathBuf> {
    // Updated by Grok Build 0.1 for GitHub issue #7: prefer NEURALFORGE_MEMORY_HOME, fallback AGENTOS_CORE_HOME (legacy)
    if let Ok(home) = env::var("NEURALFORGE_MEMORY_HOME").or_else(|_| env::var("AGENTOS_CORE_HOME"))
    {
        let path = PathBuf::from(home);
        if path.exists() {
            return Ok(path);
        }
    }

    // Walk up ancestors (starting at current dir)
    let mut current = env::current_dir().context("failed to resolve current directory")?;
    loop {
        if current.join("config/server.toml").exists() {
            return Ok(current);
        }
        if !current.pop() {
            break;
        }
    }

    bail!(
        "unable to locate repo home: expected config/server.toml in current directory or ancestors"
    )
}

pub fn load_server_config(repo_home: &Path) -> Result<ServerConfig> {
    let path = repo_home.join("config/server.toml");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}