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
        assert_eq!(provenance["teacher_called_by_this_command"], false);
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
    json!({"provider":"deepseek", "model":"deepseek/deepseek-v4.1-flash-1017", "endpoint":"https://openrouter.ai/api/v1/chat/completions", "api_key_env":"OPENROUTER_API_KEY", "seed":42, "generated_at":"2026-11-28"})
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

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn emitted_mock_request_replays_through_the_real_entrypoint() {
    let temp = Sandbox::new();
    assert!(
        temp.run(&["--emit-request", "--batch", "request"])
            .status
            .success()
    );
    let request_path = "data/raw/request/request.json";
    let request = fs::read(temp.0.join(request_path)).unwrap();
    assert!(!temp.0.join("data/raw/request/candidates.jsonl").exists());
    for batch in ["replay-one", "replay-two"] {
        let output = temp.run(&[
            "--request",
            request_path,
            "--mock-response",
            "--batch",
            batch,
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let root = temp.0.join("data/raw").join(batch);
        let p: Value =
            serde_json::from_slice(&fs::read(root.join("provenance.json")).unwrap()).unwrap();
        assert_eq!(p["request_sha256"], sha(&request));
        assert_eq!(
            p["response_sha256"],
            sha(&fs::read(root.join("response.json")).unwrap())
        );
        assert_eq!(p["admitted"], 0);
        assert_eq!(p["human_review"], "pending");
    }
    for name in ["candidates.jsonl", "response.json", "provenance.json"] {
        assert_eq!(
            fs::read(temp.0.join("data/raw/replay-one").join(name)).unwrap(),
            fs::read(temp.0.join("data/raw/replay-two").join(name)).unwrap()
        );
    }
}

#[test]
fn recorded_envelope_binds_identity_and_content_without_approving_it() {
    let temp = Sandbox::new();
    fs::write(temp.0.join("teacher.json"), remote_config().to_string()).unwrap();
    assert!(
        temp.run(&[
            "--config",
            "teacher.json",
            "--emit-request",
            "--batch",
            "request"
        ])
        .status
        .success()
    );
    let request_path = "data/raw/request/request.json";
    let request = fs::read(temp.0.join(request_path)).unwrap();
    // Synthetic stand-in for a future Hermes recording. No provider is contacted.
    let envelope = json!({"version":1,"request_sha256":sha(&request),"provider":"deepseek","model":"deepseek/deepseek-v4.1-flash-1017","serving_provider":"deepseek","origin":"synthetic_fixture","content":"A short pause can make room to try one puzzle piece again. The choice is yours."});
    fs::write(temp.0.join("response.json"), envelope.to_string()).unwrap();
    assert!(
        temp.run(&[
            "--request",
            request_path,
            "--response",
            "response.json",
            "--batch",
            "recording"
        ])
        .status
        .success()
    );
    let row: Value = serde_json::from_slice(
        &fs::read(temp.0.join("data/raw/recording/candidates.jsonl")).unwrap(),
    )
    .unwrap();
    assert_eq!(row["response"], envelope["content"]);
    assert!(row["verification"].is_null());
    let p: Value = serde_json::from_slice(
        &fs::read(temp.0.join("data/raw/recording/provenance.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(p["training_allowed"], false);
    assert_eq!(p["teacher_rights"], "unreviewed");
    assert_eq!(p["teacher_called_by_this_command"], false);
    // Simulate attribution from a recorded teacher without any provider call.
    let mut attributed = envelope.clone();
    attributed["origin"] = json!("recorded_teacher");
    fs::write(temp.0.join("attributed.json"), attributed.to_string()).unwrap();
    assert!(
        temp.run(&[
            "--request",
            request_path,
            "--response",
            "attributed.json",
            "--batch",
            "attributed"
        ])
        .status
        .success()
    );
    let attributed_row: Value = serde_json::from_slice(
        &fs::read(temp.0.join("data/raw/attributed/candidates.jsonl")).unwrap(),
    )
    .unwrap();
    assert_eq!(attributed_row["provenance"]["generator_kind"], "teacher");
    assert_eq!(
        attributed_row["provenance"]["generator_id"],
        "deepseek/deepseek-v4.1-flash-1017@openrouter/deepseek"
    );
    assert!(
        fs::read(temp.0.join("data/raw/attributed/admitted/replay.jsonl"))
            .unwrap()
            .is_empty()
    );
    for (key, value) in [
        ("request_sha256", json!("0".repeat(64))),
        ("model", json!("different-model")),
        ("provider", json!("mock")),
        ("serving_provider", json!("")),
        ("serving_provider", json!("another-provider")),
        ("version", json!(2)),
        ("content", json!("")),
        ("content", json!("x".repeat(4097))),
        ("content", json!("A caf\u{e9} puzzle.")),
        ("rights_approved", json!(true)),
        ("human_approved", json!(true)),
    ] {
        let mut bad = envelope.clone();
        bad[key] = value;
        fs::write(temp.0.join("bad.json"), bad.to_string()).unwrap();
        assert!(
            !temp
                .run(&[
                    "--request",
                    request_path,
                    "--response",
                    "bad.json",
                    "--batch",
                    "must-not-exist"
                ])
                .status
                .success(),
            "{key}"
        );
        assert!(!temp.0.join("data/raw/must-not-exist").exists());
    }
    let mut missing = envelope.clone();
    missing.as_object_mut().unwrap().remove("serving_provider");
    fs::write(temp.0.join("bad.json"), missing.to_string()).unwrap();
    assert!(
        !temp
            .run(&["--request", request_path, "--response", "bad.json"])
            .status
            .success()
    );
    assert!(
        !temp
            .run(&["--request", request_path, "--mock-response"])
            .status
            .success()
    );
}

#[test]
fn changed_request_and_incomplete_modes_fail_before_outputs() {
    let temp = Sandbox::new();
    assert!(
        temp.run(&["--emit-request", "--batch", "request"])
            .status
            .success()
    );
    let path = temp.0.join("data/raw/request/request.json");
    let original: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for (key, value) in [
        ("version", json!(2)),
        ("scope", json!("arbitrary-prompt")),
        ("contract_sha256", json!("changed")),
        ("request_body", json!({})),
    ] {
        let mut bad = original.clone();
        bad[key] = value;
        fs::write(temp.0.join("bad-request.json"), bad.to_string()).unwrap();
        assert!(
            !temp
                .run(&[
                    "--request",
                    "bad-request.json",
                    "--mock-response",
                    "--batch",
                    "no-output"
                ])
                .status
                .success()
        );
        assert!(!temp.0.join("data/raw/no-output").exists());
    }
    assert!(
        !temp
            .run(&["--request", "data/raw/request/request.json"])
            .status
            .success()
    );
    assert!(!temp.run(&["--response", "absent.json"]).status.success());
    assert!(
        !temp
            .run(&["--emit-request", "--purpose", "export"])
            .status
            .success()
    );
}

#[test]
fn openrouter_wire_pins_are_preserved_and_cannot_be_weakened_on_replay() {
    let temp = Sandbox::new();
    fs::write(
        temp.0.join("plan.json"),
        include_bytes!("../../../docs/openrouter-teacher-plan.json"),
    )
    .unwrap();
    assert!(
        temp.run(&[
            "--config",
            "plan.json",
            "--emit-request",
            "--batch",
            "openrouter"
        ])
        .status
        .success()
    );
    let path = temp.0.join("data/raw/openrouter/request.json");
    let bytes = fs::read(&path).unwrap();
    let request: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(request["transport"], "openrouter");
    assert_eq!(request["base_url"], "https://openrouter.ai/api/v1");
    assert_eq!(
        request["config"]["endpoint"],
        "https://openrouter.ai/api/v1/chat/completions"
    );
    assert_eq!(request["config"]["api_key_env"], "OPENROUTER_API_KEY");
    assert_eq!(
        request["request_body"]["model"],
        "deepseek/deepseek-v4.1-flash-1017"
    );
    assert_eq!(
        request["request_body"]["provider"],
        json!({"only":["deepseek"],"allow_fallbacks":false})
    );
    for pin in [
        json!({"only":["deepseek"],"allow_fallbacks":true}),
        json!({"only":["another-provider"],"allow_fallbacks":false}),
        Value::Null,
    ] {
        let mut altered = request.clone();
        altered["request_body"]["provider"] = pin;
        fs::write(temp.0.join("altered.json"), altered.to_string()).unwrap();
        let output = temp.run(&[
            "--request",
            "altered.json",
            "--response",
            "absent.json",
            "--batch",
            "no-output",
        ]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("pins mismatch"));
        assert!(!temp.0.join("data/raw/no-output").exists());
    }
    for (key, value) in [
        ("endpoint", "https://api.deepseek.com/chat/completions"),
        ("model", "deepseek/deepseek-v4.1-flash-latest"),
        ("api_key_env", "DEEPSEEK_API_KEY"),
    ] {
        let mut config = remote_config();
        config[key] = json!(value);
        fs::write(temp.0.join("wrong-plan.json"), config.to_string()).unwrap();
        assert!(
            !temp
                .run(&[
                    "--config",
                    "wrong-plan.json",
                    "--emit-request",
                    "--batch",
                    "no-output"
                ])
                .status
                .success()
        );
        assert!(!temp.0.join("data/raw/no-output").exists());
    }
}
