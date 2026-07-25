#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Updated by Grok Build 0.1 for GitHub issue #7
# Preferred: NEURALFORGE_MEMORY_HOME; AGENTOS_CORE_HOME kept as fallback for compat
export NEURALFORGE_MEMORY_HOME="$repo_root"
export AGENTOS_CORE_HOME="$repo_root"

bin="$repo_root/target/release/neuralforge-memory"
if [[ ! -x "$bin" ]]; then
  cargo build --manifest-path "$repo_root/Cargo.toml" --release
fi

exec "$bin" "$@"
