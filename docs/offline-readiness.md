# Hermes offline readiness handoff — DeepSeek through OpenRouter

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

## Pinned OpenRouter pilot, without enabling generation

The teacher is DeepSeek, served through **OpenRouter**. This pilot accepts only:

- Base URL: `https://openrouter.ai/api/v1`
- Full endpoint: `https://openrouter.ai/api/v1/chat/completions`
- Model: `deepseek/deepseek-v4.1-flash-1017`
- Credential variable **name only**: `OPENROUTER_API_KEY`, owned by Hermes.

These are routing pins, not permission or legal clearance. See the official
[OpenRouter quickstart](https://openrouter.ai/docs/quickstart),
[model catalog entry](https://openrouter.ai/deepseek/deepseek-v4.1-flash-1017), and
[provider routing documentation](https://openrouter.ai/docs/guides/routing/provider-selection).
No other repository's rights approval is inherited.

The checked-in [openrouter-teacher-plan.json](openrouter-teacher-plan.json) contains
only configuration, no secrets. Check it offline from the repository root:

```sh
cargo run --offline --locked -p euthymia-readiness -- --config docs/openrouter-teacher-plan.json --check-config
```

```json
{
  "provider": "deepseek",
  "model": "deepseek/deepseek-v4.1-flash-1017",
  "endpoint": "https://openrouter.ai/api/v1/chat/completions",
  "api_key_env": "OPENROUTER_API_KEY",
  "seed": 42,
  "generated_at": "2026-11-28"
}
```

`provider` in configuration identifies the teacher; the emitted request separately
identifies `transport: "openrouter"`. Do not put a key in this file. This program
never reads, stores or logs the key variable. Configuration checks enforce these
exact pins; direct-provider endpoints, alternate/floating models, different key
variable names and credential-bearing URLs are refused. The date is an explicit
fixture date; a future authorized run must use its actual generation date.
Provider API compatibility and hosted terms remain unverified.

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

To prepare a future DeepSeek-through-OpenRouter request:

```sh
cargo run --offline --locked -p euthymia-readiness -- --config docs/openrouter-teacher-plan.json --emit-request --batch hermes-teacher-request-001
```

This is **preparation, not authorization to generate**. The request declares
`generation_authorization: "not_assessed"`. It contains a version-1
`euthymia.teacher_request`, the fixed non-clinical prompt in `request_body.messages`,
model and output token bound, source hashes and configuration. It contains no
credential values. Its full endpoint is pinned to OpenRouter chat completions;
this program never appends a route, resolves it, or sends the request. The emitted
`request_body` includes the exact model and this OpenRouter wire field:

```json
"provider": {"only": ["deepseek"], "allow_fallbacks": false}
```

These constraints are preserved in the saved request and enforced by the replay
validator: removing or changing them invalidates the request. **External runner
gap:** this repository has no Hermes transport implementation and cannot prove
that Hermes forwards `request_body` unchanged, that OpenRouter receives the
routing fields, or that an external call used the requested route. Hermes must
preserve the whole body, including `provider`, in any separately authorized
future call. No wire-level enforcement or provider execution has been verified.

After Raul has separately reviewed the exact route/model terms and authorized
future generation, Hermes owns the provider call and credentials outside this
program. Hermes can send the included `request_body` to the pinned OpenRouter endpoint
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
  "serving_provider": "deepseek",
  "origin": "recorded_teacher",
  "content": "THE_FINAL_RESPONSE_TEXT"
}
```

For an OpenRouter chat-completion object, `content` is the final
`choices[0].message.content` string. A `synthetic_fixture` origin is available for
explicitly synthetic offline recordings; it never claims a teacher call.
Record the actual serving identity from returned provider evidence, normalized to
the canonical `deepseek` slug; do not copy the desired identity from the request
as proof. Missing or alternate serving-provider evidence is rejected. The
importer checks the envelope assertion but cannot authenticate it.
Obtain the request hash locally with `sha256sum <request.json>`.
Then import the envelope from a local file:

```sh
cargo run --offline --locked -p euthymia-readiness -- --request data/raw/hermes-teacher-request-001/request.json --response data/raw/recorded-response.json --batch hermes-recorded-001
```

This command never calls OpenRouter or reads a credential. It refuses mismatched
request hashes, version/prompt/source pins, provider/model identities, missing
serving-provider evidence, unknown approval fields, oversized files and invalid
content before creating output. File inputs are bounded to 64 KiB. Response text
must be nonempty printable ASCII (plus newlines/tabs) and at most 4096 bytes.
The ASCII restriction is deliberate until parent PR #32 resolves its UTF-8
slicing bug; arbitrary multilingual response admission is not ready.

Envelope metadata is unverified attribution, **not authenticated rights evidence**.
`external_transport_verified: false` explicitly preserves the runner gap.
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
