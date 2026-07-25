#!/usr/bin/env bash
set -euo pipefail

# Updated by Grok Build 0.1 for GitHub issue #7
repo="neuralforge-memory"
org="${SENTRY_ORG:-}"
project="${SENTRY_PROJECT_NEURALFORGE_MEMORY:-${SENTRY_PROJECT_AGENTOS_CORE:-}}"
environment="${SENTRY_ENVIRONMENT:-local}"
git_sha="${AGENTOS_GIT_SHA:-$(git rev-parse --short HEAD 2>/dev/null || printf 'unknown')}"
release="${repo}@${git_sha}"

if [[ -z "${org}" ]]; then
  printf 'SENTRY_ORG is required
' >&2
  exit 2
fi

if [[ -z "${project}" ]]; then
  printf 'SENTRY_PROJECT_NEURALFORGE_MEMORY (or legacy SENTRY_PROJECT_AGENTOS_CORE) is required
' >&2
  exit 2
fi

if ! sentry-cli releases --org "${org}" --project "${project}" info "${release}" >/dev/null 2>&1; then
  sentry-cli releases --org "${org}" --project "${project}" new "${release}"
fi

sentry-cli releases --org "${org}" --project "${project}" set-commits "${release}" --auto --ignore-missing
sentry-cli releases --org "${org}" --project "${project}" finalize "${release}"
sentry-cli deploys --org "${org}" --project "${project}" new --release "${release}" -e "${environment}"

printf '%s
' "${release}"