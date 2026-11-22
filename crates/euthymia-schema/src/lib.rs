//! euthymia-schema — canonical record types, generated JSON Schemas, and the
//! validation rules for euthymia data records.
//!
//! The Rust types here are the source of truth (D1): `schemas/*.v1.json` are
//! generated from them via `cargo run -p euthymia-schema --bin schema-gen` and
//! checked in. Regenerate; never hand-edit. Record validation is
//! `euthymia-validate` (JSON Schema + the cross-field rules in [`validate`]),
//! the same binary CI runs.

pub mod enums;
pub mod records;
pub mod schema;
pub mod validate;

pub use enums::*;
pub use records::*;
