use std::env;
use std::ffi::OsStr;
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
    pub fn resolved_default_repo_root(&self) -> Result<PathBuf> {
        // Updated by Grok Build 0.1 for GitHub issue #7: prefer NEURALFORGE_MEMORY_CANON_ROOT, fallback to legacy AGENTOS_CORE_CANON_ROOT for compat
        // An explicitly-set-but-empty override is treated as unset, not as the current directory,
        // and a non-Unicode override is rejected rather than silently ignored.
        for var in ["NEURALFORGE_MEMORY_CANON_ROOT", "AGENTOS_CORE_CANON_ROOT"] {
            match env::var(var) {
                Ok(value) if value.is_empty() => continue,
                Ok(value) => return Ok(PathBuf::from(value)),
                Err(env::VarError::NotPresent) => continue,
                Err(env::VarError::NotUnicode(_)) => {
                    bail!("{var} is set but is not valid unicode");
                }
            }
        }
        Ok(self.default_repo_root.clone())
    }

    pub fn policy_markdown(&self) -> Result<String> {
        let bullets = self
            .default_invariants
            .iter()
            .map(|line| format!("- {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(format!(
            "# Canon Policy\n\nDefault canon root: `{}`\n\n{}\n",
            self.resolved_default_repo_root()?.display(),
            bullets
        ))
    }
}

pub fn discover_repo_home() -> Result<PathBuf> {
    // Updated by Grok Build 0.1 for GitHub issue #7: prefer NEURALFORGE_MEMORY_HOME, fallback AGENTOS_CORE_HOME (legacy)
    for var in ["NEURALFORGE_MEMORY_HOME", "AGENTOS_CORE_HOME"] {
        match env::var(var) {
            Ok(home) => {
                let path = PathBuf::from(home);
                if !path.exists() {
                    bail!("{var} is set but does not exist: {}", path.display());
                }
                if !path.is_dir() {
                    bail!("{var} is set but is not a directory: {}", path.display());
                }
                return Ok(path);
            }
            Err(env::VarError::NotPresent) => continue,
            Err(env::VarError::NotUnicode(_)) => {
                bail!("{var} is set but is not valid unicode");
            }
        }
    }

    // Walk up ancestors (starting at current dir)
    let mut current = env::current_dir().context("failed to resolve current directory")?;
    loop {
        let marker = current.join("config/server.toml");
        // symlink_metadata(), not exists(): exists() follows symlinks, so a dangling
        // symlink at the nearest checkout's marker would report "nothing here" and
        // keep walking up to an outer ancestor, silently running against the wrong
        // repository instead of surfacing the broken marker in the intended checkout.
        match marker.symlink_metadata() {
            Ok(_) => return Ok(current),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err).with_context(|| format!("failed to check {}", marker.display()));
            }
        }
        if !current.pop() {
            break;
        }
    }

    bail!(
        "unable to locate repo home: expected config/server.toml in current directory or ancestors"
    )
}

/// Resolve a system config file path: prefer the NeuralForge-Memory location if it
/// exists, falling back to the legacy AgentOS location for the #7 transition.
///
/// Uses `symlink_metadata()` rather than `exists()`/`try_exists()`: both of those
/// follow symlinks, so a dangling symlink at the preferred location resolves to
/// "doesn't exist" and would silently fall back to the legacy path, masking a
/// broken deployment. `symlink_metadata()` reports on the symlink itself, so a
/// dangling link still counts as "something is here" and surfaces later as a read
/// error instead of silently loading stale legacy config. Any other metadata error
/// (e.g. permission denied on a parent directory) is propagated rather than
/// swallowed.
pub fn resolve_system_config_path(neuralforge_path: &str, legacy_path: &str) -> Result<PathBuf> {
    let neuralforge = Path::new(neuralforge_path);
    match neuralforge.symlink_metadata() {
        Ok(_) => Ok(neuralforge.to_path_buf()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(PathBuf::from(legacy_path)),
        Err(err) => Err(err).with_context(|| format!("failed to check {}", neuralforge.display())),
    }
}

/// Split `RAG_REPO_ROOTS` using the platform path separator rules (`:` vs `;`).
pub fn parse_rag_repo_roots(value: &OsStr) -> Vec<PathBuf> {
    env::split_paths(value)
        .filter(|p| !p.as_os_str().is_empty())
        .collect()
}

pub fn load_server_config(repo_home: &Path) -> Result<ServerConfig> {
    let path = repo_home.join("config/server.toml");
    if !path.exists() {
        bail!(
            "missing {}: run from the repository root, or set NEURALFORGE_MEMORY_HOME to a checkout that contains config/server.toml (current repo_home: {})",
            path.display(),
            repo_home.display()
        );
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rag_repo_roots_uses_split_paths() {
        let dir1 = tempfile::tempdir().expect("tempdir 1");
        let dir2 = tempfile::tempdir().expect("tempdir 2");
        let joined = std::env::join_paths([dir1.path(), dir2.path()]).expect("join_paths");
        let parsed = parse_rag_repo_roots(&joined);
        assert_eq!(
            parsed,
            vec![dir1.path().to_path_buf(), dir2.path().to_path_buf()]
        );
    }
}
