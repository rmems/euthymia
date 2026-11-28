# Hermes offline readiness handoff

Hermes is the command runner; DeepSeek is a future teacher. This bounded smoke
uses the existing Rust schema validator and admission pipeline from PR #32.
It creates one fixed, fictional, non-clinical everyday puzzle example, and can
emit a request and replay a recorded response for that same bounded scenario. It does
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

- `request.json`: versioned request with the exact prompt, config and source pins.
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

## Emit and replay the Hermes handoff

A default run still uses the built-in mock. To exercise the complete file handoff
without a provider, run these from the repo root (choose fresh batch names):

```sh
cargo run --offline --locked -p euthymia-readiness -- --emit-request --batch hermes-request-001
cargo run --offline --locked -p euthymia-readiness -- --request data/raw/hermes-request-001/request.json --mock-response --batch hermes-replay-001
```

The first command writes only `request.json`. The second creates a local response
fixture and follows the same import, schema validation and admission path as a
recorded response. Expect one valid candidate and one `rubric_below_threshold`
rejection because no score exists. It writes `request.json`, `response.json`,
candidate/config files, `admitted/replay.jsonl` (empty), `rejected/replay.jsonl`,
admission diagnostics, and `provenance.json` under the
fresh replay directory. Request and response SHA-256 hashes bind the exact file
bytes to the candidate hash. Replaying identical inputs reproduces candidate and
provenance bytes; the earlier diagnostic path/date caveat still applies.

To prepare a future DeepSeek request using the config above:

```sh
cargo run --offline --locked -p euthymia-readiness -- --config data/raw/teacher-plan.json --emit-request --batch hermes-teacher-request-001
```

This is **preparation, not authorization to generate**. The request declares
`generation_authorization: "not_assessed"`. It contains a version-1
`euthymia.teacher_request`, the fixed non-clinical prompt in `request_body.messages`,
model and output token bound, source hashes and configuration. It contains no
credential values. The supplied endpoint must be the full intended API endpoint;
this program never appends a route, resolves it, or sends the request.

After Raul has separately reviewed the exact route/model terms and authorized
future generation, Hermes owns the provider call and credentials outside this
program. Hermes can send the included `request_body` to the selected endpoint
using its existing runner capabilities. No additional repository transport code is
needed for this bounded request/replay path. Actual provider compatibility remains
unverified. Only final response text belongs in the response file; exclude headers,
keys, account details, hidden reasoning, and unrelated provider payloads.

Hermes records this envelope (replace the illustrative values with the exact
request/config and actual serving identity; never invent missing evidence):

```json
{
  "version": 1,
  "request_sha256": "SHA256_OF_THE_EXACT_REQUEST_JSON_BYTES",
  "provider": "deepseek",
  "model": "deepseek/deepseek-v4.1-flash-1017",
  "serving_provider": "ACTUAL_SERVING_PROVIDER_ID",
  "origin": "recorded_teacher",
  "content": "THE_FINAL_RESPONSE_TEXT"
}
```

For a provider returning a chat-completion object, `content` is the final
`choices[0].message.content` string. A `synthetic_fixture` origin is available for
explicitly synthetic offline recordings; it never claims a teacher call.
Obtain the request hash locally with `sha256sum <request.json>`.
Then import the envelope from a local file:

```sh
cargo run --offline --locked -p euthymia-readiness -- --request data/raw/hermes-teacher-request-001/request.json --response data/raw/recorded-response.json --batch hermes-recorded-001
```

This command never calls a provider or reads a credential. It refuses mismatched
request hashes, version/prompt/source pins, provider/model identities, missing
serving-provider evidence, unknown approval fields, oversized files and invalid
content before creating output. File inputs are bounded to 64 KiB. Response text
must be nonempty printable ASCII (plus newlines/tabs) and at most 4096 bytes.
The ASCII restriction is deliberate until parent PR #32 resolves its UTF-8
slicing bug; arbitrary multilingual response admission is not ready.

Envelope metadata is unverified attribution, **not authenticated rights evidence**.
`teacher_called_by_this_command: false` describes this importer only; it does not
claim an external recording was generated without a teacher. All replayed rows
remain unreviewed candidates, receive no invented score and must be rejected by
the existing admission pipeline. Privacy/critical-failure rejection is also a
valid quarantine outcome. A successful replay means the validation/rejection
path ran, not that the content is behaviorally safe, approved or trainable.
No approval flag can promote these outputs. Raw response files stay local under
`data/raw/`; do not upload or publish them.

The full #16 quota generator remains separate. This handoff does not accept
arbitrary prompts or private source conversations. Exact hosted-output rights,
independent rubric scoring/human review, #20 assembly and RM-2588 release checks
remain required before any training or export. No hosted rights are inferred
from a weights license or a successful replay.

`--purpose training` and `--purpose export` always fail before output writes.
This smoke cannot approve rights or human review. It cannot produce a release,
training split, downstream export or publishable dataset. RM-2588 still owns the
immutable handoff, permission checks, split isolation and downstream compatibility;
#20 owns assembly. Existing gold human-review caveats remain in force.
