# Wilo Agent contributor instructions

## Project identity

This repository is `rmems/NeuralForge-Memory`.

<!-- Updated by Grok Build 0.1 for GitHub issue #7: repo name change cleanup -->

NeuralForge-Memory is the personal RAG + vector DB + MCP focused on memory. It powers the Hermes (Herems) tool used as an AI Tutor / Hypeman. The actual Hermes RAG execution lives in Oricale (running HCL infrastructure in the separate `Dioscuri-Cloud` repo and using the "ori cli" — consent given). This repo is the dedicated memory/RAG/MCP component supporting that vision.

(Do not rename it to Metis. Metis is reserved for a future model architecture.)

## Architecture boundaries

Keep Wilo Agent as a small control plane.

Core layers:

- Agent control plane: planning, task state, structured logs, workflow runner
- Tool plane: permissioned local tools, MCP tools, shell/git/cargo/julia/nvidia-smi wrappers
- Context plane: RAG, vector DB, repo indexing, memory, manifests
- Model plane: Ollama first, optional model providers later
- Safety/permissions: allowlists, read-only mode, safe-build mode, no silent destructive actions

## Current engineering rules

Prefer small, focused diffs.

Do not introduce dependency soup.

Do not add model backends unless the issue explicitly asks.

Do not split the repo into workspace crates unless the issue explicitly asks.

Do not perform broad renames or cleanups without an authorizing issue (see #7 for the current NeuralForge-Memory identity alignment + legacy agentos cleanup).

Do not add destructive shell execution.

Do not silently mask routing, provider, API-key, or config errors.

## Rust workflow

Use stable Rust.

Before opening a PR, run:

```bash
cargo fmt --check
cargo test
```