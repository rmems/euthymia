# euthymia

Synthetic data for **Emotional Resilience Engineering (ERE)** — training and
evaluation data that teaches models to respond to emotional distress with
directness, warmth, accountability-as-agency, and long-term resilience rather
than flattery, shame, or dependency.

**Privacy rule:** all scenarios in this dataset are fictionalized and
anonymized. No record may contain identifiable real conversations, names,
locations, employers, or any detail that could re-identify a real person
(see `contract/BEHAVIORAL_CONTRACT.md` §4 Privacy Rule). This rule applies to
every artifact the pipeline produces, before anything leaves this repo.

## The six competencies

Every scenario and eval case is classifiable against the six competencies
defined in the behavioral contract (§1): **emotional awareness** — recognizing,
naming, and tolerating one's emotional states without being captured by them;
**cognitive discipline** — examining the interpretations and reasoning patterns
behind a feeling before acting on it; **personal accountability** — owning one's
role and next choices as agency, never as a statement about worth;
**behavioral control** — acting in line with stated values even when emotion
pulls toward impulse; **healthy boundaries** — protecting one's limits with
others without controlling them; and **long-term resilience** — building
sustainable recovery and repair patterns over short-term relief.

## Repository layout

```
contract/    Behavioral contract — the canonical rules every artifact answers to
             (contract/BEHAVIORAL_CONTRACT.md)
taxonomy/    Scenario taxonomy and coverage matrix
data/        Dataset artifacts
data/gold/       Contract calibration exemplar rows (gold seed set; human
                 review pending): sft_gold.jsonl, sft_gold_negative.jsonl,
                 and REVIEW_NOTES.md
data/raw/        Raw teacher-model outputs — local only, gitignored
data/admitted/   Records that passed verification
data/rejected/   Records that failed verification (diagnostic audit artifacts)
schemas/     Record schemas — SFT JSONL + DPO preference pairs
eval/        Eval harness — held-out scenarios, rubric scoring, critical-failure gate
scripts/     Pipeline scripts and tooling
Cargo.toml   Rust workspace root — all pipeline code is Rust
```

`data/gold/sft_gold.jsonl` holds the accepted calibration rows,
`data/gold/sft_gold_negative.jsonl` holds machine-labeled §3 negative fixtures,
and `data/gold/REVIEW_NOTES.md` is the row-by-row contract review and index.
The gold rows are pending human review while `PENDING_HUMAN_REVIEW` remains in
their verification metadata. Validate both gold files together with:

```sh
cargo run -p euthymia-schema --bin euthymia-validate -- sft data/gold/sft_gold.jsonl data/gold/sft_gold_negative.jsonl
```

This validator command checks schema and structural consistency only; it does
not complete human review or behaviorally approve the responses.

## Verification and admission

`euthymia-admit` is the deterministic gate between raw candidates and admitted
data. It reuses the same validator implementation as `euthymia-validate`, then
applies the privacy screen, the §3 critical-failure screen, and file-based
rubric scores through the `Scorer` interface. Incoming verification metadata is
ignored and every verdict is re-derived.

```sh
cargo run -p euthymia-verify --bin euthymia-admit -- \
  --scores path/to/scores.jsonl \
  --batch batch-name \
  --output-root data \
  data/raw/candidates.jsonl
```

The score file is JSONL keyed by `scenario_id`:

```json
{"scenario_id":"eu-brk-001","content_sha256":"<sha256 of exact candidate JSONL line>","score":5,"critical_failure":null,"scorer_id":"reviewing-agent-v1","rationale":"one-line rationale"}
```

`content_sha256` is required and must hash the exact candidate JSONL line bytes
(excluding the line ending). If a candidate is edited after scoring, the hash no
longer matches and the row fails closed instead of reusing a stale review.

Rows admit only when they are schema-valid, privacy-passing, free of critical
failures, and scored at least 4. Admitted rows are written to
`data/admitted/<batch>.jsonl`; rejected rows are written to
`data/rejected/<batch>.jsonl` when they are representable as records. The
machine-readable `data/admission_report.json` records per-row verdicts, the
firing rule, contract sections evaluated, per-rule counts, and sha256 hashes of
all candidate input files and the score file. Rejected output is diagnostic
audit data, never training positive data.

The report's rejection-rule vocabulary is `schema_invalid`,
`privacy_violation`, `rubric_below_threshold`, and the four §3 critical-failure
IDs. Negative fixtures continue to use exact contract citations in
`expected_rejection`; those citations are labels for calibration, not the
admission report's internal rule IDs.

## Data flow

The pipeline is landing stage by stage. The intended flow:

```
contract → taxonomy → generation → verification → assembly → Hugging Face
```

1. **Contract** — `contract/BEHAVIORAL_CONTRACT.md` defines the six
   competencies, tone rules, critical failures (hard rejects), the privacy
   rule, positive targets, and conformance requirements. *(landed)*
2. **Taxonomy** — scenarios are enumerated per competency with coverage
   targets. *(planned)*
3. **Generation** — SFT records and DPO preference pairs are generated against
   the taxonomy (procedural + teacher generation). *(planned)*
4. **Verification** — rubric scoring, critical-failure screen (§3), and
   privacy screen (§4); records are admitted or rejected. *(landed)*
5. **Assembly** — admitted records are deduplicated, split, and packaged with
   per-competency counts and a dataset card. *(planned)*
6. **Hugging Face** — the assembled dataset is published to the Hugging Face
   Hub with integrity checks. *(planned)*

## Development

For a bounded offline first run from Hermes, see
[the readiness handoff](docs/offline-readiness.md). The mock checks candidate
structure and exercises admission rejection without approving training or export.

All pipeline code is Rust, built as a single Cargo workspace. The toolchain is
pinned in `rust-toolchain.toml`; shared dependencies are declared once in the
root `Cargo.toml` `[workspace.dependencies]` and member crates opt in as their
issues land.

```sh
cargo metadata   # verify the workspace resolves
```
