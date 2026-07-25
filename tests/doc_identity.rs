/// Integration tests validating the documentation content changes introduced in PR #7.
///
/// These tests assert that AGENTS.md and README.md reflect the project identity
/// update from "Wilo Agent / rmems/wilo-agent" to "NeuralForge-Memory / rmems/NeuralForge-Memory",
/// and that the new Hermes context, architecture boundary rules, and CLI
/// examples are present (or absent) as expected by the diff.
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_file(name: &str) -> String {
    std::fs::read_to_string(repo_root().join(name))
        .unwrap_or_else(|e| panic!("failed to read {name}: {e}"))
}

// ─── AGENTS.md tests ──────────────────────────────────────────────────────────

#[test]
fn agents_repo_identity_is_neuralforge_memory() {
    let content = read_file("AGENTS.md");
    assert!(
        content.contains("rmems/NeuralForge-Memory"),
        "AGENTS.md must declare the repository as rmems/NeuralForge-Memory"
    );
}

#[test]
fn agents_old_repo_name_not_present() {
    let content = read_file("AGENTS.md");
    assert!(
        !content.contains("rmems/wilo-agent"),
        "AGENTS.md must not reference the old repository name rmems/wilo-agent"
    );
}

#[test]
fn agents_project_description_mentions_neuralforge_memory() {
    let content = read_file("AGENTS.md");
    assert!(
        content.contains("NeuralForge-Memory"),
        "AGENTS.md must mention NeuralForge-Memory in the project description"
    );
}

#[test]
fn agents_project_description_mentions_hermes() {
    let content = read_file("AGENTS.md");
    assert!(
        content.contains("Hermes"),
        "AGENTS.md must describe the Hermes tool that this project powers"
    );
}

#[test]
fn agents_metis_prohibition_preserved() {
    let content = read_file("AGENTS.md");
    assert!(
        content.contains("Metis"),
        "AGENTS.md must still forbid renaming the project to Metis"
    );
    assert!(
        content.to_lowercase().contains("do not rename it to metis")
            || content.contains("Do not rename it to Metis"),
        "AGENTS.md must retain the explicit 'Do not rename it to Metis' directive"
    );
}

#[test]
fn agents_rename_rule_requires_authorizing_issue() {
    let content = read_file("AGENTS.md");
    // The old rule was "Do not perform broad AgentOS-to-Wilo renames ...".
    // The new rule requires an authorizing issue for any broad rename/cleanup.
    assert!(
        content.contains("Do not perform broad renames or cleanups without an authorizing issue"),
        "AGENTS.md rename rule must require an authorizing issue for broad renames/cleanups"
    );
}

#[test]
fn agents_rename_rule_references_issue_7() {
    let content = read_file("AGENTS.md");
    assert!(
        content.contains("#7"),
        "AGENTS.md rename rule must reference issue #7 for the NeuralForge-Memory identity alignment"
    );
}

#[test]
fn agents_rename_rule_no_longer_says_agentos_to_wilo() {
    let content = read_file("AGENTS.md");
    assert!(
        !content.contains("AgentOS-to-Wilo"),
        "AGENTS.md must not contain the old 'AgentOS-to-Wilo' rename rule wording"
    );
}

#[test]
fn agents_cargo_test_code_block_is_closed() {
    let content = read_file("AGENTS.md");
    // The PR added a closing ``` that was previously missing.
    // Count occurrences: opening ``` bash and closing ```.
    let fence_count = content.matches("```").count();
    assert!(
        fence_count % 2 == 0,
        "AGENTS.md must have an even number of code-fence markers (each block opened must be closed); found {fence_count}"
    );
}

#[test]
fn agents_workflow_contains_cargo_fmt_check() {
    let content = read_file("AGENTS.md");
    assert!(
        content.contains("cargo fmt --check"),
        "AGENTS.md Rust workflow section must include cargo fmt --check"
    );
}

#[test]
fn agents_workflow_contains_cargo_test() {
    let content = read_file("AGENTS.md");
    assert!(
        content.contains("cargo test"),
        "AGENTS.md Rust workflow section must include cargo test"
    );
}

// ─── README.md tests ──────────────────────────────────────────────────────────

#[test]
fn readme_html_title_is_neuralforge_memory() {
    let content = read_file("README.md");
    // The <h1> tag should contain the new project name.
    assert!(
        content.contains("NeuralForge-Memory"),
        "README.md HTML title must be NeuralForge-Memory"
    );
    // Verify it appears inside the <h1> block that opens the file.
    let h1_region: String = content.lines().take(6).collect::<Vec<_>>().join("\n");
    assert!(
        h1_region.contains("NeuralForge-Memory"),
        "NeuralForge-Memory must appear in the opening <h1> section of README.md"
    );
}

#[test]
fn readme_subtitle_mentions_rag_and_memory() {
    let content = read_file("README.md");
    // New subtitle: "Personal RAG + vector DB + MCP for memory — powering the Hermes ..."
    assert!(
        content.contains("Personal RAG + vector DB + MCP for memory"),
        "README.md subtitle must describe the personal RAG / memory purpose"
    );
}

#[test]
fn readme_subtitle_mentions_hermes() {
    let content = read_file("README.md");
    assert!(
        content.contains("Hermes"),
        "README.md subtitle must mention Hermes as the powered tool"
    );
}

#[test]
fn readme_warning_uses_new_project_name() {
    let content = read_file("README.md");
    // The experimental warning blockquote must now say NeuralForge-Memory.
    assert!(
        content.contains("> **⚠️ Experimental — personal project.** NeuralForge-Memory"),
        "README.md experimental warning must identify the project as NeuralForge-Memory"
    );
}

#[test]
fn readme_project_description_block_mentions_hermes() {
    let content = read_file("README.md");
    assert!(
        content.contains("**Hermes"),
        "README.md project description must bold-reference Hermes"
    );
}

#[test]
fn readme_route_env_alias_examples_removed() {
    let content = read_file("README.md");
    // PR removed the --from-env and --env alias examples from the CLI section.
    assert!(
        !content.contains("--from-env"),
        "README.md must not include the removed --from-env route example"
    );
    assert!(
        !content.contains("route --env"),
        "README.md must not include the removed route --env alias example"
    );
}

#[test]
fn readme_acknowledges_previous_name_in_description() {
    let content = read_file("README.md");
    // The description should note the legacy name for traceability.
    assert!(
        content.contains("agentos-core"),
        "README.md should acknowledge the previous agentos-core name for traceability"
    );
}

#[test]
fn readme_issue_7_referenced_for_name_change() {
    let content = read_file("README.md");
    assert!(
        content.contains("#7"),
        "README.md must reference issue #7 for the name-change cleanup"
    );
}

// ─── Cross-document consistency tests ─────────────────────────────────────────

#[test]
fn both_docs_agree_on_project_name() {
    let agents = read_file("AGENTS.md");
    let readme = read_file("README.md");
    assert!(
        agents.contains("NeuralForge-Memory"),
        "AGENTS.md must contain NeuralForge-Memory"
    );
    assert!(
        readme.contains("NeuralForge-Memory"),
        "README.md must contain NeuralForge-Memory"
    );
}

#[test]
fn both_docs_mention_hermes() {
    let agents = read_file("AGENTS.md");
    let readme = read_file("README.md");
    assert!(agents.contains("Hermes"), "AGENTS.md must mention Hermes");
    assert!(readme.contains("Hermes"), "README.md must mention Hermes");
}

/// Regression: ensure neither document reverts to the old repository URL.
#[test]
fn neither_doc_contains_old_repo_url() {
    for file in &["AGENTS.md", "README.md"] {
        let content = read_file(file);
        assert!(
            !content.contains("rmems/wilo-agent"),
            "{file} must not contain the old repository URL rmems/wilo-agent"
        );
    }
}
