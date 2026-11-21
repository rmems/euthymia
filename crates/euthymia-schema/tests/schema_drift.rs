//! Drift test (D1): the checked-in schemas must match what the canonical
//! Rust types generate. If this fails, run
//! `cargo run -p euthymia-schema --bin schema-gen` and commit the result —
//! never hand-edit the files.

use std::fs;

use euthymia_schema::schema::{
    DPO_SCHEMA_FILE, SFT_SCHEMA_FILE, dpo_pair_schema, render_schema, schemas_dir,
    sft_record_schema,
};

#[test]
fn sft_record_schema_is_current() {
    let checked_in = fs::read_to_string(schemas_dir().join(SFT_SCHEMA_FILE))
        .expect("schemas/sft_record.v1.json is checked in");
    assert_eq!(
        checked_in,
        render_schema(&sft_record_schema()),
        "{SFT_SCHEMA_FILE} is stale — regenerate with `cargo run -p euthymia-schema --bin schema-gen`"
    );
}

#[test]
fn dpo_pair_schema_is_current() {
    let checked_in = fs::read_to_string(schemas_dir().join(DPO_SCHEMA_FILE))
        .expect("schemas/dpo_pair.v1.json is checked in");
    assert_eq!(
        checked_in,
        render_schema(&dpo_pair_schema()),
        "{DPO_SCHEMA_FILE} is stale — regenerate with `cargo run -p euthymia-schema --bin schema-gen`"
    );
}
