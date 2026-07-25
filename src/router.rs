use std::env;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[cfg(unix)]
const SYSTEM_ROUTER_CONFIG_PATH: &str = "/etc/neuralforge-memory/configs/model_router.json";
#[cfg(unix)]
const SYSTEM_ROUTER_CONFIG_PATH_LEGACY: &str = "/etc/agentos/configs/model_router.json";

fn env_non_empty(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelProvider {
    Ollama,
    OllamaCloud,
    Openai,
    Anthropic,
    #[serde(rename = "google-ai-studio")]
    GoogleAiStudio,
    Google,
    Azure,
    Openrouter,
    #[serde(rename = "nvidia-nim")]
    NvidiaNim,
    Codex,
    #[serde(rename = "opencode")]
    OpenCode,
}

impl ModelProvider {
    pub fn valid_names() -> &'static [&'static str] {
        &[
            "ollama",
            "ollama-cloud",
            "openai",
            "anthropic",
            "google-ai-studio",
            "google",
            "azure",
            "openrouter",
            "nvidia-nim",
            "codex",
            "opencode",
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::OllamaCloud => "ollama-cloud",
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::GoogleAiStudio => "google-ai-studio",
            Self::Google => "google",
            Self::Azure => "azure",
            Self::Openrouter => "openrouter",
            Self::NvidiaNim => "nvidia-nim",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ollama" => Some(Self::Ollama),
            "ollama-cloud" | "ollamacloud" => Some(Self::OllamaCloud),
            "openai" => Some(Self::Openai),
            "anthropic" => Some(Self::Anthropic),
            "google-ai-studio" | "googleaistudio" => Some(Self::GoogleAiStudio),
            "google" => Some(Self::Google),
            "azure" => Some(Self::Azure),
            "openrouter" => Some(Self::Openrouter),
            "nvidia-nim" | "nvidianim" => Some(Self::NvidiaNim),
            "codex" => Some(Self::Codex),
            "opencode" => Some(Self::OpenCode),
            _ => None,
        }
    }

    pub fn api_key_env_var(&self) -> Option<&'static str> {
        match self {
            Self::Openai => Some("OPENAI_API_KEY"),
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            Self::GoogleAiStudio => Some("GOOGLE_AI_STUDIO_API_KEY"),
            Self::Google => Some("GEMINI_API_KEY"),
            Self::Azure => Some("AZURE_OPENAI_API_KEY"),
            Self::Openrouter => Some("OPENROUTER_API_KEY"),
            Self::NvidiaNim => Some("NVIDIA_NIM_API_KEY"),
            Self::OllamaCloud => Some("OLLAMA_API_KEY"),
            Self::Ollama | Self::Codex | Self::OpenCode => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum ProviderRole {
    DefaultCoding,
    LongContext,
    LocalPrivate,
    RagEmbedding,
    RagEmbeddingCloud,
    CudaGpu,
    Fallback,
}

impl ProviderRole {
    pub fn to_env_key(&self) -> String {
        match self {
            Self::DefaultCoding => "NEURALFORGE_ROLE_DEFAULT_CODING".to_string(),
            Self::LongContext => "NEURALFORGE_ROLE_LONG_CONTEXT".to_string(),
            Self::LocalPrivate => "NEURALFORGE_ROLE_LOCAL_PRIVATE".to_string(),
            Self::RagEmbedding => "NEURALFORGE_ROLE_RAG_EMBEDDING".to_string(),
            Self::RagEmbeddingCloud => "NEURALFORGE_ROLE_RAG_EMBEDDING_CLOUD".to_string(),
            Self::CudaGpu => "NEURALFORGE_ROLE_CUDA_GPU".to_string(),
            Self::Fallback => "NEURALFORGE_ROLE_FALLBACK".to_string(),
        }
    }

    /// Legacy AgentOS-named env key, kept as a transition fallback (#7 / PR#8).
    pub fn to_legacy_env_key(&self) -> String {
        match self {
            Self::DefaultCoding => "AGENTOS_ROLE_DEFAULT_CODING".to_string(),
            Self::LongContext => "AGENTOS_ROLE_LONG_CONTEXT".to_string(),
            Self::LocalPrivate => "AGENTOS_ROLE_LOCAL_PRIVATE".to_string(),
            Self::RagEmbedding => "AGENTOS_ROLE_RAG_EMBEDDING".to_string(),
            Self::RagEmbeddingCloud => "AGENTOS_ROLE_RAG_EMBEDDING_CLOUD".to_string(),
            Self::CudaGpu => "AGENTOS_ROLE_CUDA_GPU".to_string(),
            Self::Fallback => "AGENTOS_ROLE_FALLBACK".to_string(),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::DefaultCoding => "default_coding",
            Self::LongContext => "long_context",
            Self::LocalPrivate => "local_private",
            Self::RagEmbedding => "rag_embedding",
            Self::RagEmbeddingCloud => "rag_embedding_cloud",
            Self::CudaGpu => "cuda_gpu",
            Self::Fallback => "fallback",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::DefaultCoding,
            Self::LongContext,
            Self::LocalPrivate,
            Self::RagEmbedding,
            Self::RagEmbeddingCloud,
            Self::CudaGpu,
            Self::Fallback,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleProvider {
    pub role: ProviderRole,
    pub provider: ModelProvider,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModelRouter {
    pub roles: Vec<RoleProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouterConfig {
    pub roles: Vec<RoleProvider>,
}

impl Default for ModelRouter {
    fn default() -> Self {
        // Built-in defaults only. Config/env overlays are applied explicitly by callers.
        Self::built_in_defaults()
    }
}

impl ModelRouter {
    pub fn built_in_defaults() -> Self {
        Self {
            roles: vec![RoleProvider {
                role: ProviderRole::RagEmbedding,
                provider: ModelProvider::Ollama,
                model: Some("nomic-embed-text:latest".to_string()),
            }],
        }
    }

    pub fn load_system_config_if_present() -> Result<Option<Self>> {
        #[cfg(unix)]
        {
            let path = crate::config::resolve_system_config_path(
                SYSTEM_ROUTER_CONFIG_PATH,
                SYSTEM_ROUTER_CONFIG_PATH_LEGACY,
            )?;
            // symlink_metadata(), not exists(): exists() follows symlinks, so a dangling
            // symlink at the resolved path would report "nothing here" and silently fall
            // back to built-in routing - the exact masking resolve_system_config_path was
            // hardened to avoid. symlink_metadata() sees the link itself, so a dangling
            // symlink is treated as "something is here" and its read failure below is
            // surfaced instead of swallowed.
            match path.symlink_metadata() {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(err) => {
                    return Err(err).with_context(|| format!("failed to check {}", path.display()));
                }
                Ok(_) => {}
            }
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let config: ModelRouterConfig = serde_json::from_str(&raw)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            Ok(Some(Self {
                roles: config.roles,
            }))
        }
        #[cfg(not(unix))]
        {
            Ok(None)
        }
    }

    pub fn from_env() -> Result<Self> {
        let mut roles: Vec<RoleProvider> = Vec::new();
        let all_roles = ProviderRole::all();

        for role in all_roles {
            let key = role.to_env_key();
            let legacy_key = role.to_legacy_env_key();
            let Some(provider_str) = env_non_empty(&key).or_else(|| env_non_empty(&legacy_key))
            else {
                continue;
            };

            let Some(provider) = ModelProvider::from_str(&provider_str) else {
                bail!(
                    "unknown provider '{}' for {} (valid: {})",
                    provider_str,
                    key,
                    ModelProvider::valid_names().join(", ")
                );
            };

            if let Some(key_var) = provider.api_key_env_var()
                && env_non_empty(key_var).is_none()
            {
                bail!(
                    "{} is required for {} provider in role {} but not set",
                    key_var,
                    provider.name(),
                    role.name()
                );
            }

            let model = env_non_empty(&format!("{key}_MODEL"))
                .or_else(|| env_non_empty(&format!("{legacy_key}_MODEL")));
            roles.push(RoleProvider {
                role,
                provider,
                model,
            });
        }

        // Return only explicit overrides. Callers can layer these on top of defaults.
        Ok(Self { roles })
    }

    pub fn get_provider(&self, role: ProviderRole) -> Option<&RoleProvider> {
        self.roles.iter().find(|r| r.role == role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
        let lock = ENV_LOCK.get_or_init(|| Mutex::new(()));
        let _guard = lock.lock().expect("env lock poisoned");
        f()
    }

    fn clear_role_env() {
        for role in ProviderRole::all() {
            for key in [role.to_env_key(), role.to_legacy_env_key()] {
                unsafe {
                    std::env::remove_var(&key);
                    std::env::remove_var(format!("{key}_MODEL"));
                }
            }
        }
        for key in [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "GEMINI_API_KEY",
            "GOOGLE_AI_STUDIO_API_KEY",
            "AZURE_OPENAI_API_KEY",
            "OPENROUTER_API_KEY",
            "NVIDIA_NIM_API_KEY",
            "OLLAMA_API_KEY",
        ] {
            unsafe {
                std::env::remove_var(key);
            }
        }
    }

    #[test]
    fn model_provider_from_str_handles_case_insensitive() {
        assert_eq!(
            ModelProvider::from_str("OPENAI"),
            Some(ModelProvider::Openai)
        );
        assert_eq!(
            ModelProvider::from_str("openai"),
            Some(ModelProvider::Openai)
        );
        assert_eq!(
            ModelProvider::from_str("OpenAI"),
            Some(ModelProvider::Openai)
        );
    }

    #[test]
    fn model_provider_api_key_env_vars_are_explicit() {
        assert_eq!(ModelProvider::Ollama.api_key_env_var(), None);
        assert_eq!(ModelProvider::Codex.api_key_env_var(), None);
        assert_eq!(ModelProvider::OpenCode.api_key_env_var(), None);
        assert_eq!(
            ModelProvider::Openai.api_key_env_var(),
            Some("OPENAI_API_KEY")
        );
        assert_eq!(
            ModelProvider::GoogleAiStudio.api_key_env_var(),
            Some("GOOGLE_AI_STUDIO_API_KEY")
        );
    }

    #[test]
    fn provider_role_all_includes_expected_roles() {
        let all = ProviderRole::all();
        assert!(all.contains(&ProviderRole::DefaultCoding));
        assert!(all.contains(&ProviderRole::RagEmbedding));
        assert!(all.contains(&ProviderRole::Fallback));
    }

    #[test]
    fn provider_role_env_keys_are_correct() {
        assert_eq!(
            ProviderRole::DefaultCoding.to_env_key(),
            "NEURALFORGE_ROLE_DEFAULT_CODING"
        );
        assert_eq!(
            ProviderRole::RagEmbedding.to_env_key(),
            "NEURALFORGE_ROLE_RAG_EMBEDDING"
        );
        assert_eq!(
            ProviderRole::DefaultCoding.to_legacy_env_key(),
            "AGENTOS_ROLE_DEFAULT_CODING"
        );
        assert_eq!(
            ProviderRole::RagEmbedding.to_legacy_env_key(),
            "AGENTOS_ROLE_RAG_EMBEDDING"
        );
    }

    #[test]
    fn env_parsing_new_role_key_takes_precedence_over_legacy() {
        with_env_lock(|| {
            clear_role_env();
            unsafe {
                std::env::set_var("NEURALFORGE_ROLE_DEFAULT_CODING", "codex");
                std::env::set_var("AGENTOS_ROLE_DEFAULT_CODING", "opencode");
            }
            let router = ModelRouter::from_env().unwrap();
            let rp = router.get_provider(ProviderRole::DefaultCoding).unwrap();
            assert_eq!(rp.provider, ModelProvider::Codex);
        });
    }

    #[test]
    fn default_router_has_rag_embedding_by_default() {
        let router = ModelRouter::default();
        let rag = router.get_provider(ProviderRole::RagEmbedding);
        assert!(rag.is_some());
        assert_eq!(rag.unwrap().provider, ModelProvider::Ollama);
    }

    #[test]
    fn env_parsing_unknown_provider_errors() {
        with_env_lock(|| {
            clear_role_env();
            unsafe {
                std::env::set_var("AGENTOS_ROLE_DEFAULT_CODING", "definitely-not-a-provider");
            }
            let err = ModelRouter::from_env().unwrap_err().to_string();
            assert!(err.contains("unknown provider"), "{err}");
        });
    }

    #[test]
    fn env_parsing_missing_key_errors_for_keyed_provider() {
        with_env_lock(|| {
            clear_role_env();
            unsafe {
                std::env::set_var("AGENTOS_ROLE_DEFAULT_CODING", "openai");
            }
            let err = ModelRouter::from_env().unwrap_err().to_string();
            assert!(err.contains("OPENAI_API_KEY"), "{err}");
        });
    }

    #[test]
    fn env_parsing_missing_key_errors_for_google_ai_studio() {
        with_env_lock(|| {
            clear_role_env();
            unsafe {
                std::env::set_var("AGENTOS_ROLE_DEFAULT_CODING", "google-ai-studio");
            }
            let err = ModelRouter::from_env().unwrap_err().to_string();
            assert!(err.contains("GOOGLE_AI_STUDIO_API_KEY"), "{err}");
        });
    }

    #[test]
    fn env_parsing_codex_does_not_require_key() {
        with_env_lock(|| {
            clear_role_env();
            unsafe {
                std::env::set_var("AGENTOS_ROLE_DEFAULT_CODING", "codex");
            }
            let router = ModelRouter::from_env().unwrap();
            let rp = router.get_provider(ProviderRole::DefaultCoding).unwrap();
            assert_eq!(rp.provider, ModelProvider::Codex);
        });
    }

    #[test]
    fn env_parsing_opencode_does_not_require_key() {
        with_env_lock(|| {
            clear_role_env();
            unsafe {
                std::env::set_var("AGENTOS_ROLE_DEFAULT_CODING", "opencode");
            }
            let router = ModelRouter::from_env().unwrap();
            let rp = router.get_provider(ProviderRole::DefaultCoding).unwrap();
            assert_eq!(rp.provider, ModelProvider::OpenCode);
        });
    }

    #[test]
    fn env_parsing_empty_key_is_treated_as_missing() {
        with_env_lock(|| {
            clear_role_env();
            unsafe {
                std::env::set_var("AGENTOS_ROLE_DEFAULT_CODING", "openai");
                std::env::set_var("OPENAI_API_KEY", "   ");
            }
            let err = ModelRouter::from_env().unwrap_err().to_string();
            assert!(err.contains("OPENAI_API_KEY"), "{err}");
        });
    }

    #[test]
    fn env_parsing_valid_provider_with_key_yields_override() {
        with_env_lock(|| {
            clear_role_env();
            unsafe {
                std::env::set_var("AGENTOS_ROLE_DEFAULT_CODING", "openai");
                std::env::set_var("OPENAI_API_KEY", "test");
            }
            let router = ModelRouter::from_env().unwrap();
            let rp = router.get_provider(ProviderRole::DefaultCoding).unwrap();
            assert_eq!(rp.provider, ModelProvider::Openai);
        });
    }

    #[test]
    fn env_parsing_no_role_vars_returns_empty_overrides() {
        with_env_lock(|| {
            clear_role_env();
            let router = ModelRouter::from_env().unwrap();
            assert!(router.roles.is_empty());
        });
    }
}
