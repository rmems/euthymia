use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "euthymia-readiness-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir_all(path.join("data/raw")).unwrap();
        fs::create_dir(path.join("schemas")).unwrap();
        fs::write(
            path.join("schemas/sft_record.v1.json"),
            include_bytes!("../../../schemas/sft_record.v1.json"),
        )
        .unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_euthymia-readiness"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn real_entrypoint_replays_candidates_and_provenance_without_admitting() {
    let temp = Sandbox::new();
    for batch in ["one", "two"] {
        let output = temp.run(&["--batch", batch]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let root = temp.0.join("data/raw").join(batch);
        let candidate = fs::read(root.join("candidates.jsonl")).unwrap();
        let provenance: Value =
            serde_json::from_slice(&fs::read(root.join("provenance.json")).unwrap()).unwrap();
        assert_eq!(
            provenance["candidate_file_sha256"],
            Sha256::digest(&candidate)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        assert_eq!(provenance["training_allowed"], false);
        assert_eq!(provenance["export_allowed"], false);
        assert_eq!(provenance["teacher_called"], false);
        let row: Value = serde_json::from_slice(&candidate).unwrap();
        assert!(row["verification"].is_null());
        assert!(
            fs::read(root.join("admitted/mock.jsonl"))
                .unwrap()
                .is_empty()
        );
        let report: Value =
            serde_json::from_slice(&fs::read(root.join("admission_report.json")).unwrap()).unwrap();
        assert_eq!(report["counts"]["rejected"], 1);
        assert!(
            report["rows"]["eu-fal-001"]["detail"]
                .as_str()
                .unwrap()
                .contains("unscored")
        );
    }
    for artifact in ["candidates.jsonl", "config.json", "provenance.json"] {
        assert_eq!(
            fs::read(temp.0.join("data/raw/one").join(artifact)).unwrap(),
            fs::read(temp.0.join("data/raw/two").join(artifact)).unwrap()
        );
    }
    assert!(!temp.run(&["--batch", "one"]).status.success());
}

fn remote_config() -> Value {
    json!({"provider":"deepseek", "model":"pinned-model-1017", "endpoint":"https://teacher.invalid/v1", "api_key_env":"EUTHYMIA_TEACHER_API_KEY", "seed":42, "generated_at":"2026-11-28"})
}

#[test]
fn future_config_is_syntax_only_and_live_training_export_fail_closed() {
    let temp = Sandbox::new();
    fs::write(temp.0.join("teacher.json"), remote_config().to_string()).unwrap();
    assert!(
        temp.run(&["--config", "teacher.json", "--check-config"])
            .status
            .success()
    );
    assert!(!temp.run(&["--config", "teacher.json"]).status.success());
    for purpose in ["training", "export"] {
        let output = temp.run(&["--purpose", purpose]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("training/export blocked"));
    }
    assert_eq!(fs::read_dir(temp.0.join("data/raw")).unwrap().count(), 0);
}

#[test]
fn malformed_config_and_path_escape_never_produce_candidates() {
    let temp = Sandbox::new();
    for (key, value) in [
        ("endpoint", json!("https://name:secret@teacher.invalid/v1")),
        ("endpoint", json!("https://teacher.invalid/v1?key=secret")),
        ("endpoint", json!("http://teacher.invalid/v1")),
        ("api_key_env", json!("not a variable")),
        ("model", json!("deepseek-latest")),
        ("generated_at", json!("2026-02-30")),
        ("rights_approved", json!(true)),
    ] {
        let mut config = remote_config();
        config[key] = value;
        fs::write(temp.0.join("teacher.json"), config.to_string()).unwrap();
        assert!(
            !temp
                .run(&["--config", "teacher.json", "--check-config"])
                .status
                .success(),
            "{key}"
        );
    }
    assert!(!temp.run(&["--batch", "../admitted"]).status.success());
    assert_eq!(fs::read_dir(temp.0.join("data/raw")).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn symlink_output_parent_is_refused() {
    let temp = Sandbox::new();
    fs::rename(temp.0.join("data/raw"), temp.0.join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(temp.0.join("elsewhere"), temp.0.join("data/raw")).unwrap();
    assert!(!temp.run(&[]).status.success());
    assert_eq!(fs::read_dir(temp.0.join("elsewhere")).unwrap().count(), 0);
}
