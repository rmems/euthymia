//! End-to-end admission tests. These verify pipeline mechanics and calibration
//! metadata; the file-based scores stand in for a reviewing agent and are not
//! themselves behavioral approval.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "euthymia-admit-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("old test temp dir removes");
    }
    fs::create_dir_all(&dir).expect("test temp dir creates");
    dir
}

fn run_admit(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_euthymia-admit"))
        .args(args)
        .output()
        .expect("euthymia-admit runs")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "admission should run, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("output jsonl reads")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("output line parses"))
        .collect()
}

fn score_line(id: &str, score: u8, critical_failure: Option<&str>) -> String {
    json!({
        "scenario_id": id,
        "score": score,
        "critical_failure": critical_failure,
        "scorer_id": "integration-review",
        "rationale": "integration-test score"
    })
    .to_string()
}

fn write_lines(path: &Path, lines: &[String]) {
    fs::write(path, lines.join("\n") + "\n").expect("jsonl writes");
}

fn ids_in_file(path: &Path) -> Vec<String> {
    read_jsonl(path)
        .iter()
        .map(|row| {
            row["scenario_id"]
                .as_str()
                .expect("scenario id")
                .to_string()
        })
        .collect()
}

fn write_gold_scores(temp: &Path, positive: &Path, negative: &Path) -> PathBuf {
    let mut lines = Vec::new();
    for file in [positive, negative] {
        lines.extend(ids_in_file(file).iter().map(|id| score_line(id, 5, None)));
    }
    let scores = temp.join("scores.jsonl");
    write_lines(&scores, &lines);
    scores
}

fn admit_args(scores: &Path, batch: &str, output_root: &Path, inputs: &[&Path]) -> Vec<String> {
    let mut args = vec![
        "--scores".into(),
        scores.display().to_string(),
        "--batch".into(),
        batch.into(),
        "--output-root".into(),
        output_root.display().to_string(),
    ];
    args.extend(inputs.iter().map(|path| path.display().to_string()));
    args
}

fn assert_admitted_metadata(admitted: &[Value]) {
    assert!(
        admitted
            .iter()
            .all(|row| row["verification"]["status"] == "admitted"
                && row["verification"]["rubric_score"] == 5
                && row["verification"]["privacy_pass"] == true
                && row["verification"]["critical_failure"].is_null()),
        "admitted rows carry re-derived verification metadata"
    );
}

fn expected_gold_rejections() -> [(&'static str, &'static str); 4] {
    [
        ("eu-bnd-903", "revenge_or_harassment"),
        ("eu-brk-903", "fostering_ai_dependence"),
        ("eu-dis-903", "shame_based_discipline"),
        ("eu-rec-903", "enabling_substance_use"),
    ]
}

fn assert_negative_outputs(rejected: &[Value]) {
    for (id, failure) in expected_gold_rejections() {
        let row = rejected
            .iter()
            .find(|row| row["scenario_id"] == id)
            .unwrap_or_else(|| panic!("{id} in rejected output"));
        assert_eq!(row["verification"]["status"], "rejected");
        assert_eq!(row["verification"]["critical_failure"], failure);
    }
}

fn assert_hash(value: &Value, label: &str) {
    assert_eq!(
        value["sha256"].as_str().expect("sha256 string").len(),
        64,
        "{label} is sha256 hex"
    );
}

fn assert_gold_report(report: &Value) {
    assert_eq!(report["counts"]["input_rows"], 18);
    assert_eq!(report["counts"]["admitted"], 14);
    assert_eq!(report["counts"]["rejected"], 4);
    for (id, rule) in expected_gold_rejections() {
        assert_eq!(report["rows"][id]["verdict"], "rejected");
        assert_eq!(report["rows"][id]["rule"], rule);
        assert!(
            report["rows"][id]["detail"]
                .as_str()
                .expect("detail string")
                .contains("lexical §3"),
            "{id} must be rejected by the lexical screen, not the fixture fallback"
        );
    }
    for input in report["input_files"].as_array().expect("input files array") {
        assert_hash(input, "input hash");
    }
    assert_hash(&report["score_file"], "score-file hash");
    assert_eq!(
        report["rows"]["eu-brk-901"]["scorer_rationale"],
        "integration-test score"
    );
}

#[test]
fn gold_calibration_admits_positives_and_rejects_all_negatives() {
    let root = workspace_root();
    let positive = root.join("data/gold/sft_gold.jsonl");
    let negative = root.join("data/gold/sft_gold_negative.jsonl");
    let temp = test_dir("gold-calibration");
    let scores = write_gold_scores(&temp, &positive, &negative);
    let args = admit_args(&scores, "gold-calibration", &temp, &[&positive, &negative]);

    // Negative fixtures deliberately receive high scores and no scorer flags:
    // calibration proves the deterministic §3 screen, not the score file.
    assert_success(&run_admit(&args));
    let admitted = read_jsonl(&temp.join("admitted/gold-calibration.jsonl"));
    let rejected = read_jsonl(&temp.join("rejected/gold-calibration.jsonl"));
    assert_eq!(admitted.len(), 14, "all positive gold rows admit");
    assert_eq!(rejected.len(), 4, "all negative gold fixtures reject");
    assert_admitted_metadata(&admitted);
    assert_negative_outputs(&rejected);

    let report_path = temp.join("admission_report.json");
    let report_text = fs::read_to_string(&report_path).expect("report reads");
    let report: Value = serde_json::from_str(&report_text).expect("report parses");
    assert_gold_report(&report);

    // Same inputs + scorer + pipeline version reproduce the same report bytes.
    assert_success(&run_admit(&args));
    let rerun_text = fs::read_to_string(&report_path).expect("rerun report reads");
    assert_eq!(report_text, rerun_text);
}

fn base_row() -> Value {
    let path = workspace_root().join("crates/euthymia-schema/tests/fixtures/valid_sft.jsonl");
    let line = fs::read_to_string(path)
        .expect("fixture reads")
        .lines()
        .next()
        .expect("first fixture row")
        .to_string();
    let mut row: Value = serde_json::from_str(&line).expect("fixture parses");
    row["verification"] = Value::Null;
    row
}

fn row_with_id(id: &str) -> Value {
    let mut row = base_row();
    row["scenario_id"] = json!(id);
    row
}

fn synthetic_rows() -> Vec<Value> {
    let mut schema_invalid = row_with_id("eu-brk-101");
    schema_invalid["secondary_competencies"] = json!(["behavioral_control", "behavioral_control"]);
    let mut privacy = row_with_id("eu-brk-102");
    privacy["user_message"] = json!("Email me at person@example.com before I decide what to send.");
    let mut stale_verification = row_with_id("eu-brk-107");
    stale_verification["verification"] = json!({
        "status": "rejected",
        "critical_failure": "fostering_ai_dependence",
        "privacy_pass": true,
        "verified_by": "previous-run",
        "verified_at": "2026-11-20"
    });
    vec![
        schema_invalid,
        privacy,
        row_with_id("eu-brk-103"),
        row_with_id("eu-brk-104"),
        row_with_id("eu-brk-105"),
        row_with_id("eu-brk-106"),
        stale_verification,
    ]
}

fn write_synthetic_batch(temp: &Path) -> PathBuf {
    let batch = temp.join("synthetic.jsonl");
    let lines: Vec<String> = synthetic_rows().iter().map(ToString::to_string).collect();
    write_lines(&batch, &lines);
    batch
}

fn write_synthetic_scores(temp: &Path) -> PathBuf {
    let scores = temp.join("scores.jsonl");
    write_lines(
        &scores,
        &[
            score_line("eu-brk-104", 2, None),
            score_line("eu-brk-105", 5, Some("fostering_ai_dependence")),
            score_line("eu-brk-106", 5, None),
            score_line("eu-brk-107", 5, None),
        ],
    );
    scores
}

fn assert_synthetic_rows(rows: &Value) {
    assert_eq!(rows["eu-brk-101"]["rule"], "schema_invalid");
    assert_eq!(rows["eu-brk-102"]["rule"], "privacy_violation");
    assert_eq!(rows["eu-brk-103"]["rule"], "rubric_below_threshold");
    assert_eq!(rows["eu-brk-103"]["detail"], "unscored");
    assert_eq!(rows["eu-brk-104"]["rule"], "rubric_below_threshold");
    assert_eq!(rows["eu-brk-105"]["rule"], "fostering_ai_dependence");
    assert_eq!(
        rows["eu-brk-105"]["critical_failure"],
        "fostering_ai_dependence"
    );
    assert_eq!(rows["eu-brk-106"]["verdict"], "admitted");
    assert_eq!(rows["eu-brk-107"]["verdict"], "admitted");
}

fn assert_synthetic_counts(report: &Value) {
    assert_eq!(report["counts"]["per_rule"]["schema_invalid"], 1);
    assert_eq!(report["counts"]["per_rule"]["privacy_violation"], 1);
    assert_eq!(report["counts"]["per_rule"]["rubric_below_threshold"], 2);
    assert_eq!(report["counts"]["per_rule"]["fostering_ai_dependence"], 1);
}

#[test]
fn synthetic_batch_exercises_every_non_lexical_reject_path() {
    let temp = test_dir("synthetic-rules");
    let batch = write_synthetic_batch(&temp);
    let scores = write_synthetic_scores(&temp);
    let args = admit_args(&scores, "synthetic", &temp, &[&batch]);
    assert_success(&run_admit(&args));

    let report: Value = serde_json::from_str(
        &fs::read_to_string(temp.join("admission_report.json")).expect("report reads"),
    )
    .expect("report parses");
    assert_synthetic_rows(&report["rows"]);
    assert_synthetic_counts(&report);

    let admitted = read_jsonl(&temp.join("admitted/synthetic.jsonl"));
    assert_eq!(admitted.len(), 2);
    assert_eq!(admitted[0]["scenario_id"], "eu-brk-106");
    assert_eq!(admitted[1]["scenario_id"], "eu-brk-107");
    assert_eq!(admitted[1]["verification"]["status"], "admitted");
    assert!(admitted[1]["verification"]["critical_failure"].is_null());
}
