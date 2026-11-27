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

fn ids_in_file(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .expect("input jsonl reads")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .expect("input row parses")
                .get("scenario_id")
                .and_then(Value::as_str)
                .expect("scenario id")
                .to_string()
        })
        .collect()
}

#[test]
fn gold_calibration_admits_positives_and_rejects_all_negatives() {
    let root = workspace_root();
    let positive = root.join("data/gold/sft_gold.jsonl");
    let negative = root.join("data/gold/sft_gold_negative.jsonl");
    let temp = test_dir("gold-calibration");

    let mut score_lines = Vec::new();
    for file in [&positive, &negative] {
        for id in ids_in_file(file) {
            // Negative fixtures deliberately receive high scores and no scorer
            // flag: calibration proves the deterministic §3 screen, not the
            // score file, is doing the rejection.
            score_lines.push(score_line(&id, 5, None));
        }
    }
    let scores = temp.join("scores.jsonl");
    fs::write(&scores, score_lines.join("\n") + "\n").expect("scores write");

    let args = [
        "--scores".into(),
        scores.display().to_string(),
        "--batch".into(),
        "gold-calibration".into(),
        "--output-root".into(),
        temp.display().to_string(),
        positive.display().to_string(),
        negative.display().to_string(),
    ];
    let output = run_admit(&args);
    assert!(
        output.status.success(),
        "admission should run, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let admitted = read_jsonl(&temp.join("admitted/gold-calibration.jsonl"));
    let rejected = read_jsonl(&temp.join("rejected/gold-calibration.jsonl"));
    assert_eq!(admitted.len(), 14, "all positive gold rows admit");
    assert_eq!(rejected.len(), 4, "all negative gold fixtures reject");
    assert!(
        admitted
            .iter()
            .all(|row| row["verification"]["status"] == "admitted"
                && row["verification"]["rubric_score"] == 5
                && row["verification"]["privacy_pass"] == true
                && row["verification"]["critical_failure"].is_null()),
        "admitted rows carry re-derived verification metadata"
    );

    let expected = [
        ("eu-bnd-903", "revenge_or_harassment"),
        ("eu-brk-903", "fostering_ai_dependence"),
        ("eu-dis-903", "shame_based_discipline"),
        ("eu-rec-903", "enabling_substance_use"),
    ];
    for (id, failure) in expected {
        let row = rejected
            .iter()
            .find(|row| row["scenario_id"] == id)
            .unwrap_or_else(|| panic!("{id} in rejected output"));
        assert_eq!(row["verification"]["status"], "rejected");
        assert_eq!(row["verification"]["critical_failure"], failure);
    }

    let report_path = temp.join("admission_report.json");
    let report_text = fs::read_to_string(&report_path).expect("report reads");
    let report: Value = serde_json::from_str(&report_text).expect("report parses");
    assert_eq!(report["counts"]["input_rows"], 18);
    assert_eq!(report["counts"]["admitted"], 14);
    assert_eq!(report["counts"]["rejected"], 4);
    for (id, rule) in expected {
        assert_eq!(report["rows"][id]["verdict"], "rejected");
        assert_eq!(report["rows"][id]["rule"], rule);
    }
    for input in report["input_files"].as_array().expect("input files array") {
        assert_eq!(
            input["sha256"].as_str().expect("sha256 string").len(),
            64,
            "input hashes are sha256 hex"
        );
    }

    assert_eq!(
        report["score_file"]["sha256"]
            .as_str()
            .expect("score hash")
            .len(),
        64,
        "score-file hash is sha256 hex"
    );
    assert_eq!(
        report["rows"]["eu-brk-901"]["scorer_rationale"],
        "integration-test score"
    );

    // Reproducibility: same inputs, same scorer, same pipeline version -> the
    // same report bytes on the same run date.
    let rerun = run_admit(&args);
    assert!(rerun.status.success());
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

#[test]
fn synthetic_batch_exercises_every_non_lexical_reject_path() {
    let temp = test_dir("synthetic-rules");

    let mut schema_invalid = row_with_id("eu-brk-101");
    schema_invalid["secondary_competencies"] = json!(["behavioral_control", "behavioral_control"]);

    let mut privacy = row_with_id("eu-brk-102");
    privacy["user_message"] = json!("Email me at person@example.com before I decide what to send.");

    let unscored = row_with_id("eu-brk-103");
    let low_score = row_with_id("eu-brk-104");
    let scorer_flagged = row_with_id("eu-brk-105");
    let admitted_control = row_with_id("eu-brk-106");

    let batch = temp.join("synthetic.jsonl");
    let rows = [
        schema_invalid,
        privacy,
        unscored,
        low_score,
        scorer_flagged,
        admitted_control,
    ];
    fs::write(
        &batch,
        rows.iter()
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .expect("batch writes");

    let scores = temp.join("scores.jsonl");
    fs::write(
        &scores,
        [
            score_line("eu-brk-104", 2, None),
            score_line("eu-brk-105", 5, Some("fostering_ai_dependence")),
            score_line("eu-brk-106", 5, None),
        ]
        .join("\n")
            + "\n",
    )
    .expect("scores write");

    let output = run_admit(&[
        "--scores".into(),
        scores.display().to_string(),
        "--batch".into(),
        "synthetic".into(),
        "--output-root".into(),
        temp.display().to_string(),
        batch.display().to_string(),
    ]);
    assert!(
        output.status.success(),
        "admission should run, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_str(
        &fs::read_to_string(temp.join("admission_report.json")).expect("report reads"),
    )
    .expect("report parses");
    let rows = &report["rows"];
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

    assert_eq!(report["counts"]["per_rule"]["schema_invalid"], 1);
    assert_eq!(report["counts"]["per_rule"]["privacy_violation"], 1);
    assert_eq!(report["counts"]["per_rule"]["rubric_below_threshold"], 2);
    assert_eq!(report["counts"]["per_rule"]["fostering_ai_dependence"], 1);

    let admitted = read_jsonl(&temp.join("admitted/synthetic.jsonl"));
    assert_eq!(admitted.len(), 1);
    assert_eq!(admitted[0]["scenario_id"], "eu-brk-106");
}
