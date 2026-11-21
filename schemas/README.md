# Record schemas

JSON Schemas for euthymia data records, locked by the 2026-11-20 decision
record on [issue #13](https://github.com/rmems/euthymia/issues/13).

| File | Record type | Version |
| --- | --- | --- |
| [`sft_record.v1.json`](sft_record.v1.json) | `SftRecord` — one SFT training row | 1 |
| [`dpo_pair.v1.json`](dpo_pair.v1.json) | `DpoPairRecord` — one DPO preference pair | 1 |

## The types are the source of truth

These files are **generated** from the canonical Rust types in
`crates/euthymia-schema` (`SftRecord`, `DpoPairRecord`, `Provenance`,
`VerificationResult`, and the closed enum sets). **Regenerate; never
hand-edit.** A drift test (`cargo test -p euthymia-schema`) fails if a
checked-in file differs from what the types produce.

Regenerate after changing any type in `crates/euthymia-schema/src/`:

```sh
cargo run -p euthymia-schema --bin schema-gen
```

## Versioning

`schema_version` is a `u32`, currently `1` (`CURRENT_SCHEMA_VERSION`). A
record carrying any other version fails to deserialize under the current
build and fails JSON Schema validation (`"const": 1`). Schema changes bump
the version and mint new `.v<N>.json` files — admitted data never silently
changes shape. The version is carried in the filename and in `$id`.

## Validation

JSON Schema alone cannot express every record rule. The
`euthymia-validate` binary applies the schema plus the cross-field rules
(secondary competency ≠ primary, secondary uniqueness and cap, the §3
negative-fixture rules, DPO chosen ≠ rejected, ID patterns):

```sh
cargo run -p euthymia-schema --bin euthymia-validate -- sft path/to/records.jsonl
cargo run -p euthymia-schema --bin euthymia-validate -- dpo path/to/pairs.jsonl
```

It prints one `<file>:<line>: <error>` per failure and exits non-zero if any
line fails. This is the same binary CI runs, so local and CI validation
cannot disagree.
