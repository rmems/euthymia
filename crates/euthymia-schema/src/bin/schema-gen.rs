//! Regenerate the checked-in JSON Schemas from the canonical Rust types.
//!
//!   cargo run -p euthymia-schema --bin schema-gen
//!
//! Writes schemas/sft_record.v1.json and schemas/dpo_pair.v1.json at the
//! workspace root. The drift test (`cargo test -p euthymia-schema`) fails if
//! the checked-in files differ from what this writes — regenerate, never
//! hand-edit (D1).

use euthymia_schema::schema::{DPO_SCHEMA_FILE, SFT_SCHEMA_FILE, schemas_dir, write_schemas};

fn main() -> std::io::Result<()> {
    let dir = schemas_dir();
    write_schemas(&dir)?;
    eprintln!(
        "wrote {} and {}",
        dir.join(SFT_SCHEMA_FILE).display(),
        dir.join(DPO_SCHEMA_FILE).display()
    );
    Ok(())
}
