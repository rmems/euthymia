//! JSON Schema generation (D1). The Rust types are the source of truth;
//! these functions render the checked-in `schemas/*.v<N>.json` files and are
//! shared by `schema-gen` and the drift test.

use std::fs;
use std::path::{Path, PathBuf};

use schemars::schema_for;
use serde_json::{Value, json};

use crate::records::{CURRENT_SCHEMA_VERSION, DpoPairRecord, SftRecord};

/// Checked-in filename for the SFT record schema (version in the name, D1).
pub const SFT_SCHEMA_FILE: &str = "sft_record.v1.json";
/// Checked-in filename for the DPO pair schema (version in the name, D1).
pub const DPO_SCHEMA_FILE: &str = "dpo_pair.v1.json";

fn schema_id(file: &str) -> String {
    format!("https://github.com/rmems/euthymia/blob/main/schemas/{file}")
}

fn with_id(schema: impl Into<Value>, file: &str) -> Value {
    let mut value = schema.into();
    value
        .as_object_mut()
        .expect("root schema is an object")
        .insert("$id".into(), json!(schema_id(file)));
    value
}

/// The full JSON Schema document for [`SftRecord`] at
/// [`CURRENT_SCHEMA_VERSION`], `$id` included.
pub fn sft_record_schema() -> Value {
    assert_eq!(
        CURRENT_SCHEMA_VERSION, 1,
        "bump SFT_SCHEMA_FILE with the version"
    );
    with_id(
        serde_json::to_value(schema_for!(SftRecord)).expect("schema serializes"),
        SFT_SCHEMA_FILE,
    )
}

/// The full JSON Schema document for [`DpoPairRecord`] at
/// [`CURRENT_SCHEMA_VERSION`], `$id` included.
pub fn dpo_pair_schema() -> Value {
    assert_eq!(
        CURRENT_SCHEMA_VERSION, 1,
        "bump DPO_SCHEMA_FILE with the version"
    );
    with_id(
        serde_json::to_value(schema_for!(DpoPairRecord)).expect("schema serializes"),
        DPO_SCHEMA_FILE,
    )
}

/// Canonical checked-in rendering: pretty JSON, trailing newline.
pub fn render_schema(schema: &Value) -> String {
    let mut rendered = serde_json::to_string_pretty(schema).expect("schema renders");
    rendered.push('\n');
    rendered
}

/// The checked-in `schemas/` directory at the workspace root.
pub fn schemas_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("schemas")
}

/// Write both schema files under `dir` in canonical rendering. Used by
/// `schema-gen`.
pub fn write_schemas(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(
        dir.join(SFT_SCHEMA_FILE),
        render_schema(&sft_record_schema()),
    )?;
    fs::write(dir.join(DPO_SCHEMA_FILE), render_schema(&dpo_pair_schema()))?;
    Ok(())
}
