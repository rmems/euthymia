use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::time::sleep;

use crate::rag::{VectorDbConfig, load_vector_db_config};
use crate::tools::ollama::{OllamaConfig, ollama_embed};
use crate::tools::qdrant::{QdrantVectorRecord, qdrant_delete_vectors, qdrant_upsert_vectors};

#[cfg(unix)]
const SYSTEM_MANIFEST_PATH: &str = "/etc/neuralforge-memory/configs/rag_index_manifest.json";
const SYSTEM_MANIFEST_PATH_LEGACY: &str = "/etc/agentos/configs/rag_index_manifest.json";
const DEFAULT_COLLECTION: &str = "repos";
const DEFAULT_EMBEDDING_MODEL: &str = "nomic-embed-text:latest";
const DEFAULT_QDRANT_HOST: &str = "http://127.0.0.1:6333";
const DEFAULT_OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434";
const MAX_FILE_BYTES: u64 = 2_000_000;

#[derive(Debug, Clone)]
pub struct Orchestrator {
    collection: String,
    embedding_model: String,
    batch_size: usize,
    chunk_tokens: usize,
    chunk_overlap: f32,
    manifest_path: PathBuf,
    vector_db: VectorDbConfig,
    ollama: OllamaConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationSummary {
    pub status: String,
    pub processed_chunks: usize,
    pub upserted_ids: Vec<String>,
    pub deleted_ids: Vec<String>,
    pub errors: Vec<String>,
}

impl OperationSummary {
    fn ok() -> Self {
        Self {
            status: "ok".to_string(),
            processed_chunks: 0,
            upserted_ids: Vec::new(),
            deleted_ids: Vec::new(),
            errors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIngestRequest {
    pub path: String,
    pub content: String,
    pub commit: String,
    pub source_type: String,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffIngestRequest {
    pub repo: String,
    pub commit: String,
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuildRequest {
    pub scope: String,
    pub collection: String,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupRequest {
    pub collection: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorUpsertRequest {
    pub collection: String,
    pub vectors: Vec<VectorRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorDeleteRequest {
    pub collection: String,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRecord {
    pub id: String,
    pub embedding: Vec<f32>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexedChunk {
    id: String,
    source: String,
    path: String,
    chunk_index: usize,
    checksum: String,
    embedding_model: String,
    commit: String,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IndexManifest {
    chunks: BTreeMap<String, IndexedChunk>,
}

#[derive(Debug, Clone)]
struct PreparedChunk {
    id: String,
    text: String,
    metadata: ChunkMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChunkMetadata {
    id: String,
    source: String,
    commit: String,
    lang: String,
    chunk_index: usize,
    created_at: DateTime<Utc>,
    embedding_model: String,
    checksum: String,
    tags: String,
    repo: String,
    path: String,
    text: String,
}

impl Orchestrator {
    pub fn from_env() -> Result<Self> {
        let vector_db = resolved_vector_db_config()?;
        let collection = vector_db.collection.clone();

        let embedding_model =
            env_string("EMBEDDING_MODEL").unwrap_or_else(|| DEFAULT_EMBEDDING_MODEL.to_string());
        let ollama = OllamaConfig {
            endpoint: resolve_ollama_endpoint(),
            embedding_model: embedding_model.clone(),
        };

        Ok(Self {
            collection,
            embedding_model,
            batch_size: env_usize("BATCH_SIZE", 128).max(1),
            chunk_tokens: env_usize("CHUNK_TOKENS", 800).max(1),
            chunk_overlap: env_f32("CHUNK_OVERLAP", 0.25).clamp(0.0, 0.95),
            manifest_path: rag_index_manifest_path(),
            vector_db,
            ollama,
        })
    }

    pub async fn ingest_file(&self, request: FileIngestRequest) -> Result<OperationSummary> {
        if !request.dry_run {
            self.ensure_manifest_writable()?;
        }
        let started = Instant::now();
        emit_event("ingest_started", 0, 0, 0, None, started);
        let (repo, relative_path) = split_source_path(&request.path);
        let chunks = self.prepare_chunks(&repo, &relative_path, &request.content, &request.commit);
        let summary = if chunks.is_empty() {
            self.delete_source(&format!("{repo}:{relative_path}"), request.dry_run)
                .await?
        } else {
            self.upsert_prepared(chunks, request.dry_run).await?
        };
        emit_event(
            "ingest_completed",
            summary.processed_chunks,
            summary.upserted_ids.len(),
            summary.deleted_ids.len(),
            None,
            started,
        );
        Ok(summary)
    }

    pub async fn ingest_diff(&self, request: DiffIngestRequest) -> Result<OperationSummary> {
        if !request.dry_run {
            self.ensure_manifest_writable()?;
        }
        let started = Instant::now();
        emit_event("ingest_started", 0, 0, 0, None, started);
        let repo_root = resolve_repo_root(&request.repo)?;
        let repo_name = repo_root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| request.repo.clone());
        let mut combined = OperationSummary::ok();

        let mut manifest = self.load_manifest().await?;
        let mut manifest_changed = false;

        for changed in request.changed_files {
            let relative = match safe_relative_path(&changed) {
                Ok(relative) => relative,
                Err(error) => {
                    combined.errors.push(error.to_string());
                    continue;
                }
            };
            let path = repo_root.join(&relative);
            if path.exists() {
                let canonical = path
                    .canonicalize()
                    .with_context(|| format!("failed to canonicalize {}", path.display()))?;
                if !canonical.starts_with(&repo_root) {
                    combined
                        .errors
                        .push(format!("skipped path outside repo root: {changed}"));
                    continue;
                }
                if is_binary_or_large(&canonical).await? {
                    combined
                        .errors
                        .push(format!("skipped binary or large file: {relative}"));
                    continue;
                }
                let content = tokio::fs::read_to_string(&canonical)
                    .await
                    .with_context(|| format!("failed to read {}", canonical.display()))?;
                let chunks = self.prepare_chunks(&repo_name, &relative, &content, &request.commit);
                let partial = self
                    .upsert_prepared_with_manifest(&mut manifest, chunks, request.dry_run)
                    .await?;
                manifest_changed |=
                    !partial.upserted_ids.is_empty() || !partial.deleted_ids.is_empty();
                merge_summary(&mut combined, partial);
            } else {
                let deleted = self
                    .delete_source_with_manifest(
                        &mut manifest,
                        &format!("{repo_name}:{relative}"),
                        request.dry_run,
                    )
                    .await?;
                manifest_changed |= !deleted.deleted_ids.is_empty();
                merge_summary(&mut combined, deleted);
            }
        }

        if manifest_changed && !request.dry_run {
            self.save_manifest(&manifest).await?;
        }

        combined.status = if combined.errors.is_empty() {
            "ok".to_string()
        } else {
            "partial".to_string()
        };
        emit_event(
            "ingest_completed",
            combined.processed_chunks,
            combined.upserted_ids.len(),
            combined.deleted_ids.len(),
            None,
            started,
        );
        Ok(combined)
    }

    pub async fn rebuild(&self, request: RebuildRequest) -> Result<OperationSummary> {
        if !request.dry_run {
            self.ensure_manifest_writable()?;
        }
        let started = Instant::now();
        emit_event("rebuild_started", 0, 0, 0, None, started);
        self.ensure_collection(&request.collection)?;
        if request.scope != "full" {
            bail!("only full rebuild is implemented in this pass");
        }

        let roots = repo_roots_from_env();
        let mut combined = OperationSummary::ok();
        let mut manifest = self.load_manifest().await?;
        let mut manifest_changed = false;
        for root in roots {
            if !root.is_dir() {
                combined
                    .errors
                    .push(format!("skipped missing repo root: {}", root.display()));
                continue;
            }
            let repo_name = root
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "repo".to_string());
            for file in collect_indexable_files(&root)? {
                let relative = file
                    .strip_prefix(&root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                if is_binary_or_large(&file).await? {
                    combined
                        .errors
                        .push(format!("skipped binary or large file: {relative}"));
                    continue;
                }
                let content = tokio::fs::read_to_string(&file)
                    .await
                    .with_context(|| format!("failed to read {}", file.display()))?;
                let chunks = self.prepare_chunks(&repo_name, &relative, &content, "rebuild");
                let partial = self
                    .upsert_prepared_with_manifest(&mut manifest, chunks, request.dry_run)
                    .await?;
                manifest_changed |=
                    !partial.upserted_ids.is_empty() || !partial.deleted_ids.is_empty();
                merge_summary(&mut combined, partial);
            }
        }

        if manifest_changed && !request.dry_run {
            self.save_manifest(&manifest).await?;
        }

        combined.status = if combined.errors.is_empty() {
            "ok".to_string()
        } else {
            "partial".to_string()
        };
        emit_event(
            "rebuild_completed",
            combined.processed_chunks,
            combined.upserted_ids.len(),
            combined.deleted_ids.len(),
            None,
            started,
        );
        Ok(combined)
    }

    pub async fn cleanup(&self, request: CleanupRequest) -> Result<OperationSummary> {
        self.ensure_collection(&request.collection)?;
        if !matches!(request.mode.as_str(), "stale" | "missing" | "all") {
            bail!("unsupported cleanup mode: {}", request.mode);
        }
        if matches!(request.mode.as_str(), "stale" | "missing") {
            bail!("cleanup mode '{}' is not implemented yet", request.mode);
        }
        let manifest = self.load_manifest().await?;
        let ids = manifest.chunks.keys().cloned().collect::<Vec<_>>();
        if request.mode == "all" {
            self.ensure_manifest_writable()?;
            let summary = self
                .vectors_delete(VectorDeleteRequest {
                    collection: self.collection.clone(),
                    ids,
                })
                .await?;
            self.save_manifest(&IndexManifest::default()).await?;
            return Ok(summary);
        }
        Ok(OperationSummary::ok())
    }

    pub async fn vectors_upsert(&self, request: VectorUpsertRequest) -> Result<OperationSummary> {
        self.ensure_collection(&request.collection)?;
        let started = Instant::now();
        let records = request
            .vectors
            .iter()
            .map(|vector| QdrantVectorRecord {
                id: &vector.id,
                embedding: &vector.embedding,
                metadata: &vector.metadata,
            })
            .collect::<Vec<_>>();
        retry_async(|| qdrant_upsert_vectors(&self.vector_db, &records)).await?;
        let ids = request
            .vectors
            .into_iter()
            .map(|vector| vector.id)
            .collect::<Vec<_>>();
        emit_event("upsert_success", ids.len(), ids.len(), 0, None, started);
        Ok(OperationSummary {
            status: "ok".to_string(),
            processed_chunks: ids.len(),
            upserted_ids: ids,
            deleted_ids: Vec::new(),
            errors: Vec::new(),
        })
    }

    pub async fn vectors_delete(&self, request: VectorDeleteRequest) -> Result<OperationSummary> {
        self.ensure_collection(&request.collection)?;
        let started = Instant::now();
        retry_async(|| qdrant_delete_vectors(&self.vector_db, &request.ids)).await?;
        emit_event(
            "delete_success",
            request.ids.len(),
            0,
            request.ids.len(),
            None,
            started,
        );
        Ok(OperationSummary {
            status: "ok".to_string(),
            processed_chunks: 0,
            upserted_ids: Vec::new(),
            deleted_ids: request.ids,
            errors: Vec::new(),
        })
    }

    fn ensure_collection(&self, collection: &str) -> Result<()> {
        if collection != self.collection {
            bail!(
                "collection mismatch: request={} configured={}",
                collection,
                self.collection
            );
        }
        Ok(())
    }

    fn prepare_chunks(
        &self,
        repo: &str,
        relative_path: &str,
        content: &str,
        commit: &str,
    ) -> Vec<PreparedChunk> {
        let lang = detect_lang(relative_path);
        chunk_content(
            relative_path,
            content,
            self.chunk_tokens,
            self.chunk_overlap,
        )
        .into_iter()
        .enumerate()
        .map(|(chunk_index, text)| {
            let source_path = format!("{repo}:{relative_path}");
            let id = chunk_id(&source_path, chunk_index);
            let checksum = sha256_hex(text.as_bytes());
            let metadata = ChunkMetadata {
                id: id.clone(),
                source: source_path.clone(),
                commit: commit.to_string(),
                lang: lang.to_string(),
                chunk_index,
                created_at: Utc::now(),
                embedding_model: self.embedding_model.clone(),
                checksum,
                tags: format!("repo:{repo};lang:{lang}"),
                repo: repo.to_string(),
                path: relative_path.to_string(),
                text: text.clone(),
            };
            PreparedChunk { id, text, metadata }
        })
        .collect()
    }

    async fn upsert_prepared(
        &self,
        chunks: Vec<PreparedChunk>,
        dry_run: bool,
    ) -> Result<OperationSummary> {
        let mut manifest = self.load_manifest().await?;
        let summary = self
            .upsert_prepared_with_manifest(&mut manifest, chunks, dry_run)
            .await?;
        if !dry_run && (!summary.upserted_ids.is_empty() || !summary.deleted_ids.is_empty()) {
            self.save_manifest(&manifest).await?;
        }
        Ok(summary)
    }

    async fn upsert_prepared_with_manifest(
        &self,
        manifest: &mut IndexManifest,
        chunks: Vec<PreparedChunk>,
        dry_run: bool,
    ) -> Result<OperationSummary> {
        let mut summary = OperationSummary::ok();
        summary.processed_chunks = chunks.len();
        let source = chunks
            .first()
            .map(|chunk| chunk.metadata.source.clone())
            .unwrap_or_default();
        let new_ids = chunks
            .iter()
            .map(|chunk| chunk.id.clone())
            .collect::<BTreeSet<_>>();
        let mut to_embed = Vec::new();

        for chunk in chunks {
            let changed = manifest
                .chunks
                .get(&chunk.id)
                .map(|old| {
                    old.checksum != chunk.metadata.checksum
                        || old.embedding_model != self.embedding_model
                })
                .unwrap_or(true);
            if changed {
                to_embed.push(chunk);
            }
        }

        let removed = manifest
            .chunks
            .values()
            .filter(|chunk| chunk.source == source && !new_ids.contains(&chunk.id))
            .map(|chunk| chunk.id.clone())
            .collect::<Vec<_>>();

        if dry_run {
            summary.upserted_ids = to_embed.iter().map(|chunk| chunk.id.clone()).collect();
            summary.deleted_ids = removed;
            return Ok(summary);
        }

        for batch in to_embed.chunks(self.batch_size) {
            let started = Instant::now();
            let mut records = Vec::new();
            let mut indexed = Vec::new();
            for chunk in batch {
                match retry_async(|| ollama_embed(&chunk.text, &self.ollama)).await {
                    Ok(embedding) => {
                        records.push(VectorRecord {
                            id: chunk.id.clone(),
                            embedding,
                            metadata: serde_json::to_value(&chunk.metadata)?,
                        });
                        indexed.push(IndexedChunk {
                            id: chunk.id.clone(),
                            source: chunk.metadata.source.clone(),
                            path: chunk.metadata.path.clone(),
                            chunk_index: chunk.metadata.chunk_index,
                            checksum: chunk.metadata.checksum.clone(),
                            embedding_model: chunk.metadata.embedding_model.clone(),
                            commit: chunk.metadata.commit.clone(),
                            updated_at: Utc::now(),
                        });
                    }
                    Err(error) => {
                        emit_event(
                            "embed_batch_fail",
                            batch.len(),
                            0,
                            0,
                            Some(error.to_string()),
                            started,
                        );
                        summary.errors.push(error.to_string());
                    }
                }
            }
            if !records.is_empty() {
                let qdrant_records = records
                    .iter()
                    .map(|record| QdrantVectorRecord {
                        id: &record.id,
                        embedding: &record.embedding,
                        metadata: &record.metadata,
                    })
                    .collect::<Vec<_>>();
                retry_async(|| qdrant_upsert_vectors(&self.vector_db, &qdrant_records)).await?;
                emit_event(
                    "embed_batch_success",
                    records.len(),
                    records.len(),
                    0,
                    None,
                    started,
                );
                emit_event(
                    "upsert_success",
                    records.len(),
                    records.len(),
                    0,
                    None,
                    started,
                );
                for chunk in indexed {
                    summary.upserted_ids.push(chunk.id.clone());
                    manifest.chunks.insert(chunk.id.clone(), chunk);
                }
            }
        }

        if !removed.is_empty() {
            retry_async(|| qdrant_delete_vectors(&self.vector_db, &removed)).await?;
            for id in &removed {
                manifest.chunks.remove(id);
            }
            summary.deleted_ids.extend(removed);
        }

        if !summary.errors.is_empty() {
            summary.status = "partial".to_string();
        }
        Ok(summary)
    }

    async fn delete_source(&self, source: &str, dry_run: bool) -> Result<OperationSummary> {
        let mut manifest = self.load_manifest().await?;
        let summary = self
            .delete_source_with_manifest(&mut manifest, source, dry_run)
            .await?;
        if !dry_run && !summary.deleted_ids.is_empty() {
            self.save_manifest(&manifest).await?;
        }
        Ok(summary)
    }

    async fn delete_source_with_manifest(
        &self,
        manifest: &mut IndexManifest,
        source: &str,
        dry_run: bool,
    ) -> Result<OperationSummary> {
        let ids = manifest
            .chunks
            .values()
            .filter(|chunk| chunk.source == source)
            .map(|chunk| chunk.id.clone())
            .collect::<Vec<_>>();
        if !dry_run {
            retry_async(|| qdrant_delete_vectors(&self.vector_db, &ids)).await?;
            for id in &ids {
                manifest.chunks.remove(id);
            }
        }
        Ok(OperationSummary {
            status: "ok".to_string(),
            processed_chunks: 0,
            upserted_ids: Vec::new(),
            deleted_ids: ids,
            errors: Vec::new(),
        })
    }

    fn effective_manifest_path(&self) -> PathBuf {
        effective_manifest_path_for(&self.manifest_path)
    }

    /// Resolve a writable manifest path, migrating a read-only one if needed, and
    /// return it. Callers that mutate Qdrant should call this before doing so:
    /// without it, a read-only-but-parseable manifest is only discovered at
    /// save_manifest() time, after vectors have already been mutated, leaving the
    /// manifest permanently unable to track state that Qdrant now reflects.
    fn ensure_manifest_writable(&self) -> Result<PathBuf> {
        let effective = self.effective_manifest_path();
        if effective.exists() {
            return self.ensure_existing_manifest_writable(&effective);
        }
        ensure_manifest_parent_creatable(&effective)?;
        Ok(effective)
    }

    /// `effective` is a manifest path that already exists: return it if writable,
    /// otherwise migrate it to a writable destination (recording where, so future
    /// calls reuse it), or fail loudly if no writable destination is available.
    fn ensure_existing_manifest_writable(&self, effective: &Path) -> Result<PathBuf> {
        if is_writable_file(effective) {
            return Ok(effective.to_path_buf());
        }
        if let Some(destination) = migrate_manifest(effective) {
            record_migration(&self.manifest_path, &destination)?;
            return Ok(destination);
        }
        bail!(
            "manifest at {} is not writable and no writable destination is available; move it to a writable NeuralForge-Memory config location before indexing",
            effective.display()
        );
    }

    async fn load_manifest(&self) -> Result<IndexManifest> {
        let path = self.effective_manifest_path();
        if tokio::fs::metadata(&path).await.is_err() {
            return Ok(IndexManifest::default());
        }
        let raw = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
    }

    async fn save_manifest(&self, manifest: &IndexManifest) -> Result<()> {
        let path = self.ensure_manifest_writable()?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let raw = serde_json::to_string_pretty(manifest)?;
        tokio::fs::write(&path, raw)
            .await
            .with_context(|| format!("failed to write {}", path.display()))
    }
}

fn ensure_manifest_parent_creatable(effective: &Path) -> Result<()> {
    if let Some(parent) = effective.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create manifest directory {}", parent.display()))?;
        if !is_writable_dir(parent) {
            bail!(
                "manifest directory {} is not writable; move it to a writable NeuralForge-Memory config location before indexing",
                parent.display()
            );
        }
    }
    Ok(())
}

pub(crate) fn resolved_vector_db_config() -> Result<VectorDbConfig> {
    // Only fall back to built-in defaults when no vector_db config exists at all.
    // If one exists but fails to load (unreadable, malformed, a dangling symlink),
    // that must be surfaced rather than silently swallowed into localhost defaults -
    // otherwise a migrated deployment could write to the wrong Qdrant instance.
    let mut vector_db = if crate::rag::vector_db_config_present()? {
        load_vector_db_config()?
    } else {
        VectorDbConfig {
            provider: "qdrant".to_string(),
            host: DEFAULT_QDRANT_HOST.to_string(),
            collection: DEFAULT_COLLECTION.to_string(),
            embedding_dim: 768,
            distance: "Cosine".to_string(),
        }
    };

    vector_db.collection = env_string("RAG_COLLECTION")
        .or_else(|| env_string("COLLECTION_NAME"))
        .unwrap_or_else(|| vector_db.collection.clone());

    Ok(vector_db)
}

pub(crate) fn resolve_ollama_endpoint() -> String {
    let raw = env_string("OLLAMA_ENDPOINT")
        .or_else(|| env_string("OLLAMA_HOST"))
        .unwrap_or_else(|| DEFAULT_OLLAMA_ENDPOINT.to_string());
    normalize_ollama_endpoint(&raw)
}

fn normalize_ollama_endpoint(ollama_host_or_url: &str) -> String {
    let trimmed = ollama_host_or_url.trim();
    if trimmed.is_empty() {
        return DEFAULT_OLLAMA_ENDPOINT.to_string();
    }

    // Ensure scheme
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };

    // Drop trailing slash for easier port detection
    let without_trailing = with_scheme.trim_end_matches('/');

    // Insert default port 11434 when none is specified
    if let Some((scheme, rest)) = without_trailing.split_once("://") {
        let mut parts = rest.splitn(2, '/');
        let host_port = parts.next().unwrap_or_default();
        let path = parts.next().map(|p| format!("/{p}")).unwrap_or_default();

        let has_port = if let Some(stripped) = host_port.strip_prefix('[') {
            stripped.contains(":]")
        } else {
            host_port
                .rsplit_once(':')
                .map(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
                .unwrap_or(false)
        };

        let host_port = if has_port {
            host_port.to_string()
        } else {
            format!("{host_port}:11434")
        };

        format!("{scheme}://{host_port}{path}")
    } else {
        without_trailing.to_string()
    }
}

pub(crate) fn manifest_chunk_count(raw: &str) -> Result<usize> {
    let manifest: IndexManifest =
        serde_json::from_str(raw).with_context(|| "failed to parse index manifest JSON")?;
    Ok(manifest.chunks.len())
}

fn merge_summary(target: &mut OperationSummary, other: OperationSummary) {
    target.processed_chunks += other.processed_chunks;
    target.upserted_ids.extend(other.upserted_ids);
    target.deleted_ids.extend(other.deleted_ids);
    target.errors.extend(other.errors);
}

async fn retry_async<F, Fut, T>(mut op: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut delay = Duration::from_millis(200);
    let mut last_error = None;
    for _ in 0..3 {
        match op().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                last_error = Some(error);
                sleep(delay).await;
                delay *= 2;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("operation failed without error")))
}

fn emit_event(
    event: &'static str,
    processed_chunks: usize,
    upserted: usize,
    deleted: usize,
    error: Option<String>,
    started: Instant,
) {
    tracing::info!(
        event,
        processed_chunks,
        upserted,
        deleted,
        latency_ms = started.elapsed().as_millis() as u64,
        error = error.unwrap_or_default(),
        "rag_orchestrator_event"
    );
}

fn chunk_content(path: &str, content: &str, chunk_tokens: usize, overlap: f32) -> Vec<String> {
    if path.ends_with(".md") || path.ends_with(".markdown") {
        return chunk_markdown(content, chunk_tokens, overlap);
    }
    if is_code_path(path) {
        return chunk_code(content, chunk_tokens);
    }
    chunk_tokens_window(content, chunk_tokens, overlap)
}

fn chunk_markdown(content: &str, chunk_tokens: usize, overlap: f32) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current = String::new();
    for line in content.lines() {
        if line.starts_with('#') && !current.trim().is_empty() {
            sections.extend(chunk_tokens_window(&current, chunk_tokens, overlap));
            current.clear();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        sections.extend(chunk_tokens_window(&current, chunk_tokens, overlap));
    }
    sections
}

fn chunk_code(content: &str, chunk_tokens: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in content.lines() {
        let starts_item = is_code_boundary(line);
        if starts_item && !current.trim().is_empty() && approx_tokens(&current) >= chunk_tokens / 2
        {
            chunks.extend(chunk_tokens_window(&current, chunk_tokens, 0.0));
            current.clear();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        chunks.extend(chunk_tokens_window(&current, chunk_tokens, 0.0));
    }
    chunks
}

fn chunk_tokens_window(content: &str, chunk_tokens: usize, overlap: f32) -> Vec<String> {
    let chunk_tokens = effective_chunk_tokens(chunk_tokens);
    let words = content.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() {
        return Vec::new();
    }
    let overlap_words = ((chunk_tokens as f32) * overlap).round() as usize;
    let step = chunk_tokens.saturating_sub(overlap_words).max(1);
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < words.len() {
        let end = (start + chunk_tokens).min(words.len());
        chunks.push(words[start..end].join(" "));
        if end == words.len() {
            break;
        }
        start += step;
    }
    chunks
}

fn is_code_boundary(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("fn ")
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("async fn ")
        || trimmed.starts_with("pub async fn ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("def ")
        || trimmed.starts_with("function ")
        || trimmed.starts_with("export function ")
}

fn is_code_path(path: &str) -> bool {
    matches!(
        Path::new(path).extension().and_then(|ext| ext.to_str()),
        Some("rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "go" | "java" | "c" | "cc" | "cpp" | "h")
    )
}

fn approx_tokens(content: &str) -> usize {
    let words = content.split_whitespace().count();
    let punctuation = content
        .bytes()
        .filter(|byte| {
            matches!(
                byte,
                b'{' | b'}'
                    | b'('
                    | b')'
                    | b'['
                    | b']'
                    | b'.'
                    | b':'
                    | b';'
                    | b','
                    | b'_'
                    | b'-'
                    | b'/'
                    | b'\\'
            )
        })
        .count();
    words + (punctuation / 4)
}

fn effective_chunk_tokens(chunk_tokens: usize) -> usize {
    ((chunk_tokens as f32) * 0.75).floor().max(1.0) as usize
}

fn detect_lang(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("rs") => "rust",
        Some("py") => "python",
        Some("ts" | "tsx") => "typescript",
        Some("js" | "jsx") => "javascript",
        Some("md" | "markdown") => "markdown",
        Some("toml") => "toml",
        Some("json" | "jsonc") => "json",
        Some("yaml" | "yml") => "yaml",
        _ => "text",
    }
}

fn split_source_path(path: &str) -> (String, String) {
    if let Some((repo, relative)) = path.split_once(':') {
        return (repo.to_string(), normalize_path_string(relative));
    }

    let path = PathBuf::from(path);
    if path.is_absolute() {
        if let Some((repo, relative)) = split_absolute_source_path(&path) {
            return (repo, relative);
        }
    }

    let normalized = normalize_path_string(&path.to_string_lossy());
    let mut parts = normalized.split('/').collect::<Vec<_>>();
    if parts.len() > 1 {
        let repo = parts.remove(0).to_string();
        return (repo, parts.join("/"));
    }
    ("repo".to_string(), normalized)
}

fn split_absolute_source_path(path: &Path) -> Option<(String, String)> {
    for root in repo_roots_from_env() {
        let Ok(root) = root.canonicalize() else {
            continue;
        };
        if path.starts_with(&root) {
            let repo = root.file_name()?.to_string_lossy().to_string();
            let relative = path.strip_prefix(&root).ok()?.to_string_lossy();
            return Some((repo, normalize_path_string(&relative)));
        }
    }
    None
}

fn normalize_path_string(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn safe_relative_path(path: &str) -> Result<String> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        bail!("skipped absolute path outside repo root: {path}");
    }
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::Prefix(_) | Component::RootDir
        )
    }) {
        bail!("skipped unsafe relative path: {path}");
    }
    Ok(normalize_path_string(path))
}

fn resolve_repo_root(repo: &str) -> Result<PathBuf> {
    let direct = PathBuf::from(repo);
    if direct.is_dir() {
        return direct
            .canonicalize()
            .with_context(|| format!("failed to canonicalize repo root: {repo}"));
    }
    for root in repo_roots_from_env() {
        if root
            .file_name()
            .map(|name| name.to_string_lossy() == repo)
            .unwrap_or(false)
        {
            return root
                .canonicalize()
                .with_context(|| format!("failed to canonicalize repo root: {}", root.display()));
        }
    }
    bail!("unable to resolve repo root: {repo}")
}

fn repo_roots_from_env() -> Vec<PathBuf> {
    std::env::var_os("RAG_REPO_ROOTS")
        .map(|value| crate::config::parse_rag_repo_roots(&value))
        .filter(|roots| !roots.is_empty())
        .unwrap_or_else(|| vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))])
}

fn collect_indexable_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.with_context(|| format!("failed to walk {}", root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let relative = path.strip_prefix(root)?;
        let rel = relative.to_string_lossy().replace('\\', "/");
        if rel.starts_with(".git/")
            || rel.starts_with("target/")
            || rel.starts_with("node_modules/")
            || rel.starts_with(".idea/")
            || rel.starts_with(".codex/")
        {
            continue;
        }
        if is_indexable_path(path) {
            files.push(path.to_path_buf());
        }
    }
    Ok(files)
}

fn is_indexable_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some(
            "rs" | "py"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "go"
                | "java"
                | "c"
                | "cc"
                | "cpp"
                | "h"
                | "md"
                | "markdown"
                | "toml"
                | "json"
                | "jsonc"
                | "yaml"
                | "yml"
                | "txt"
        )
    )
}

async fn is_binary_or_large(path: &Path) -> Result<bool> {
    let metadata = tokio::fs::metadata(path)
        .await
        .with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Ok(true);
    }
    Ok(!is_indexable_path(path))
}

fn chunk_id(source_path: &str, chunk_index: usize) -> String {
    sha256_hex(format!("{source_path}:{chunk_index}").as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn rag_index_manifest_path() -> PathBuf {
    // Prefer NeuralForge env names; AGENTOS_*/RAG_* kept as transition fallbacks (#7 / PR#8).
    env_string("NEURALFORGE_RAG_INDEX_MANIFEST")
        .or_else(|| env_string("NEURALFORGE_MEMORY_RAG_INDEX_MANIFEST"))
        .or_else(|| env_string("AGENTOS_RAG_INDEX_MANIFEST"))
        .or_else(|| env_string("RAG_INDEX_MANIFEST"))
        .map(PathBuf::from)
        .unwrap_or_else(default_manifest_path)
}

/// Resolve where the manifest logically lives, without touching the filesystem
/// beyond metadata/existence checks - no reads, writes, or migration happen here.
/// This is called from read-only contexts (doctor checks, `Orchestrator::from_env`
/// before any request arrives), so it must stay side-effect free; the actual
/// migration of a read-only manifest to a writable location happens lazily in
/// `Orchestrator::ensure_manifest_writable()`, right before a mutation begins.
fn default_manifest_path() -> PathBuf {
    #[cfg(unix)]
    {
        // Prefer wherever a manifest already exists so a partial migration (new config
        // dir created, but the manifest hasn't moved yet) doesn't start indexing from an
        // empty map and orphan vectors tracked only in the legacy manifest.
        for candidate in [SYSTEM_MANIFEST_PATH, SYSTEM_MANIFEST_PATH_LEGACY] {
            let system_path = PathBuf::from(candidate);
            // symlink_metadata(), not exists(): exists() follows symlinks, so a
            // dangling symlink at this location would report "nothing here" and fall
            // through to the writable-directory loop below, which would then treat
            // the same broken symlink path as a fresh location to create.
            // symlink_metadata() sees the link itself, so a dangling symlink is still
            // treated as "something is here" rather than silently skipped past.
            match system_path.symlink_metadata() {
                Ok(_) => return system_path,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return system_path,
            }
        }
        for candidate in [SYSTEM_MANIFEST_PATH, SYSTEM_MANIFEST_PATH_LEGACY] {
            let system_path = PathBuf::from(candidate);
            if system_path
                .parent()
                .map(|parent| parent.is_dir() && is_writable_dir(parent))
                .unwrap_or(false)
            {
                return system_path;
            }
        }
    }
    default_local_manifest_path()
}

fn default_local_manifest_path() -> PathBuf {
    let base = dirs::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let current = base
        .join("neuralforge-memory")
        .join("rag_index_manifest.json");
    if current.exists() {
        return current;
    }
    // An install predating the neuralforge-memory rename tracked chunks under the
    // legacy `agentos` user-local dir. Keep using that file in place rather than
    // starting fresh at the renamed path: this dir is always writable (unlike the
    // /etc system locations), so there's no need to migrate it, and starting fresh
    // would orphan vectors this manifest is the only record of.
    let legacy = base.join("agentos").join("rag_index_manifest.json");
    if legacy.exists() {
        return legacy;
    }
    current
}

/// The manifest path actually in effect for `logical_path` right now: the recorded
/// migration destination if one was already established, otherwise `logical_path`
/// itself. Pure read - performs no migration, so it's safe to call from read-only
/// contexts (health checks, doctor) without side effects.
pub(crate) fn effective_manifest_path_for(logical_path: &Path) -> PathBuf {
    let marker = migration_marker_path(logical_path);
    if let Ok(recorded) = std::fs::read_to_string(&marker) {
        let recorded = PathBuf::from(recorded.trim());
        if !recorded.as_os_str().is_empty() && recorded.exists() {
            return recorded;
        }
    }
    logical_path.to_path_buf()
}

/// Path of the marker file recording where `logical_path` was migrated to, so the
/// migration destination survives even after its content diverges from the
/// (frozen, read-only) source through normal saves. Lives under the same writable
/// user-local data dir used as the last-resort manifest destination - never beside
/// `logical_path` itself, since that location is exactly what's unwritable when a
/// migration is needed in the first place.
fn migration_marker_path(logical_path: &Path) -> PathBuf {
    // Resolve against the current directory before hashing: two deployments with
    // different working directories that both set RAG_INDEX_MANIFEST to the same
    // relative path would otherwise hash to the identical marker in the shared
    // user-local data dir and end up following each other's migration destination.
    let absolute = if logical_path.is_absolute() {
        logical_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(logical_path))
            .unwrap_or_else(|_| logical_path.to_path_buf())
    };
    let key = sha256_hex(absolute.as_os_str().as_encoded_bytes());
    let mut marker_dir = default_local_manifest_path();
    marker_dir.pop();
    marker_dir.join(format!("rag_index_manifest-{key}.migrated-to"))
}

fn record_migration(logical_path: &Path, destination: &Path) -> Result<()> {
    let marker = migration_marker_path(logical_path);
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create manifest migration marker directory {}",
                parent.display()
            )
        })?;
    }
    std::fs::write(&marker, destination.display().to_string()).with_context(|| {
        format!(
            "failed to record manifest migration marker at {}",
            marker.display()
        )
    })
}

/// Copy an existing-but-not-writable manifest's contents to the first writable
/// destination (preferring the NeuralForge-Memory system location, then the legacy
/// one, then the user-local data dir), so persistence works from the very first save.
/// Returns `None` if no destination accepted the write.
fn migrate_manifest(source: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(source).ok()?;
    // Validate before touching any destination: a malformed source must never be
    // allowed to clobber a destination that may already hold a good manifest.
    serde_json::from_str::<IndexManifest>(&raw).ok()?;

    let mut candidates: Vec<PathBuf> = Vec::new();
    #[cfg(unix)]
    for candidate in [SYSTEM_MANIFEST_PATH, SYSTEM_MANIFEST_PATH_LEGACY] {
        let path = PathBuf::from(candidate);
        if path != source {
            candidates.push(path);
        }
    }
    candidates.push(default_local_manifest_path());

    candidates
        .into_iter()
        .find(|destination| migrate_to_candidate(destination, &raw))
}

/// Write `raw` to `destination` if that's safe. Returns whether `destination` is now
/// (or already was) a usable manifest location.
fn migrate_to_candidate(destination: &Path, raw: &str) -> bool {
    if destination.exists() {
        // A prior run may have already migrated `source`'s exact content into this
        // destination - reuse it rather than re-running the migration every startup.
        // Require a byte-exact match against the current source content (migration
        // writes `raw` verbatim, so a genuine prior migration of this same source
        // reads back identically): a destination that merely happens to hold some
        // other valid-but-unrelated manifest must not be adopted, since that would
        // silently discard source's own tracking data instead of migrating it. Also
        // require it to still be writable - a byte-identical but now-read-only
        // candidate must not be reported usable, or the preflight would pass while
        // the eventual save still fails after Qdrant has already been mutated.
        return is_writable_file(destination) && is_manifest_copy_of(destination, raw);
    }
    if let Some(parent) = destination.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return false;
    }
    std::fs::write(destination, raw).is_ok()
}

fn is_manifest_copy_of(path: &Path, raw: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|existing| existing == raw)
}

fn is_writable_file(path: &Path) -> bool {
    std::fs::OpenOptions::new().write(true).open(path).is_ok()
}

fn is_writable_dir(path: &Path) -> bool {
    let test_path = path.join(format!(".neuralforge-write-test-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&test_path)
    {
        Ok(_) => std::fs::remove_file(test_path).is_ok(),
        Err(_) => false,
    }
}

pub(crate) fn env_string(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_id_is_stable_sha256_hex() {
        let id = chunk_id("neuralforge-memory:src/main.rs", 3);
        assert_eq!(id.len(), 64);
        assert_eq!(id, chunk_id("neuralforge-memory:src/main.rs", 3));
        assert_ne!(id, chunk_id("neuralforge-memory:src/main.rs", 4));
    }

    #[test]
    fn markdown_chunks_by_heading() {
        let chunks = chunk_content("README.md", "# A\none two\n# B\nthree four", 10, 0.25);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].contains("one"));
        assert!(chunks[1].contains("three"));
    }

    #[test]
    fn code_chunks_on_function_boundaries() {
        let content = "fn a() {}\n\nfn b() {}\n";
        let chunks = chunk_content("src/lib.rs", content, 2, 0.0);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn unsafe_diff_paths_are_rejected() {
        assert!(safe_relative_path("../secret.txt").is_err());
        assert!(safe_relative_path("/etc/passwd").is_err());
        assert_eq!(safe_relative_path("src/main.rs").unwrap(), "src/main.rs");
    }

    #[test]
    fn split_source_path_preserves_deep_relative_paths() {
        let left = split_source_path("repo/src/main.rs");
        let right = split_source_path("other/src/main.rs");

        assert_eq!(left, ("repo".to_string(), "src/main.rs".to_string()));
        assert_eq!(right, ("other".to_string(), "src/main.rs".to_string()));
        assert_ne!(
            format!("{}:{}", left.0, left.1),
            format!("{}:{}", right.0, right.1)
        );
    }

    #[test]
    fn approx_tokens_counts_code_density() {
        assert!(approx_tokens("foo.bar_baz(qux);") > 1);
        assert_eq!(effective_chunk_tokens(800), 600);
    }
}
