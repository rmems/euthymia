# Hermes offline readiness handoff

Hermes is the command runner; DeepSeek is a future teacher. This bounded smoke
uses the existing Rust schema validator and admission pipeline from PR #32.
It creates one fixed, fictional, non-clinical everyday puzzle example. It does
not implement the quota generator (#16) or the release handoff (RM-2588 / #26).

## First run

Required cwd: the root of this `rmems/euthymia` checkout, containing `Cargo.toml`,
`schemas/`, and `data/raw/`. Use the toolchain pinned in `rust-toolchain.toml`.
If Cargo is installed but absent from PATH, add your installation's `bin` directory.
Build dependencies must already be cached for Cargo's `--offline` mode; normal
dependency setup is separate from the smoke and never requires teacher access.

Copyable command for Hermes, from the repository root:

```sh
cargo run --offline --locked -p euthymia-readiness -- --batch hermes-smoke-001
```

Inputs: built-in mock configuration (seed 42, fixture date 2026-11-28) and the
embedded fictional template. No personal input, external sources, credentials,
network requests, judge scores or user conversations are read. No Hermes setup
or authentication changes are required. The seed is recorded for replay; this
one-example smoke does not vary its fixed template by seed or meet taxonomy quotas.

Outputs are local and gitignored under `data/raw/hermes-smoke-001/`:

- `config.json`: effective configuration, containing no secret values.
- `candidates.jsonl`: one structurally valid, unreviewed mock candidate.
- `scores.jsonl`: empty, because no reviewer supplied a score.
- `admitted/mock.jsonl`: empty; `rejected/mock.jsonl`: the unscored candidate.
- `admission_report.json`: the real admission pipeline's rejection evidence.
- `provenance.json`: content/config/source/schema/contract/taxonomy/lockfile hashes,
  explicit unreviewed rights, pending human review, and training/export disabled.

Expected successful result: **1 valid candidate, 0 admitted, 1 unscored rejection**.
An existing batch directory is refused; use a new name for another run. Failed
runs may leave diagnostic files but never a successful `provenance.json`.
The candidate, config and provenance bytes reproduce across output directories.
The parent's admission diagnostics include paths and current verification dates;
those diagnostic bytes are not claimed to reproduce across paths or days.

Independently check the generated candidate with the existing CLI:

```sh
cargo run --offline --locked -p euthymia-schema --bin euthymia-validate -- sft data/raw/hermes-smoke-001/candidates.jsonl
```

## Prepare a future DeepSeek selection, without enabling generation

Create a local JSON config such as `data/raw/teacher-plan.json`. Replace the
placeholder endpoint with the intended credential-free HTTPS route and the model
with its exact reviewed variant identifier. The example model reflects the desired
variant in issue #16, not a verified provider listing or a rights approval:

```json
{
  "provider": "deepseek",
  "model": "deepseek/deepseek-v4.1-flash-1017",
  "endpoint": "https://teacher.invalid/v1",
  "api_key_env": "EUTHYMIA_TEACHER_API_KEY",
  "seed": 42,
  "generated_at": "2026-11-28"
}
```

```sh
cargo run --offline --locked -p euthymia-readiness -- --config data/raw/teacher-plan.json --check-config
```

`api_key_env` is an environment variable **name only**; do not put a key in this
file. This command never reads that variable, resolves the host or contacts a
provider. Configuration validation is structural only. Endpoint existence,
model availability, API compatibility and hosted terms are unverified. Floating
`latest` aliases, unknown fields and credential-bearing URLs are rejected.

Running a DeepSeek config without `--check-config` fails before writing output.
There is no live transport or enable-live flag in this prerequisite. Issue #16
must supply that implementation after exact route/model terms evidence is pinned
and reviewed, including serving-provider provenance. Raw outputs must still pass
the contract's privacy/safety gates and independent review. No hosted-output
rights are inferred from a weights license or from a successful mock run.

`--purpose training` and `--purpose export` always fail before output writes.
This smoke cannot approve rights or human review. It cannot produce a release,
training split, downstream export or publishable dataset. RM-2588 still owns the
immutable handoff, permission checks, split isolation and downstream compatibility;
#20 owns assembly. Existing gold human-review caveats remain in force.
