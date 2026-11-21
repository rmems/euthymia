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

/// Render the full JSON Schema document for a record type at
/// [`CURRENT_SCHEMA_VERSION`], `$id` included. The checked-in filename must
/// carry the current version.
fn record_schema<T: schemars::JsonSchema>(file: &str) -> Value {
    assert!(
        file.contains(&format!(".v{CURRENT_SCHEMA_VERSION}.")),
        "bump the schema filename with the version"
    );
    let mut schema = serde_json::to_value(schema_for!(T)).expect("schema serializes");
    schema
        .as_object_mut()
        .expect("root schema is an object")
        .insert("$id".into(), json!(schema_id(file)));
    schema
}

/// The full JSON Schema document for [`SftRecord`].
pub fn sft_record_schema() -> Value {
    record_schema::<SftRecord>(SFT_SCHEMA_FILE)
}

/// The full JSON Schema document for [`DpoPairRecord`].
pub fn dpo_pair_schema() -> Value {
    record_schema::<DpoPairRecord>(DPO_SCHEMA_FILE)
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
/// `schema-gen`. Both documents are rendered up front and each file lands via
/// a temporary file + rename, so readers never see a partially written schema
/// and a render failure leaves both checked-in files untouched.
pub fn write_schemas(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let rendered = [
        (SFT_SCHEMA_FILE, render_schema(&sft_record_schema())),
        (DPO_SCHEMA_FILE, render_schema(&dpo_pair_schema())),
    ];
    for (file, contents) in rendered {
        let target = dir.join(file);
        let temp = dir.join(format!("{file}.tmp"));
        fs::write(&temp, contents)?;
        fs::rename(&temp, &target)?;
    }
    Ok(())
}
