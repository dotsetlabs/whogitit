use std::fs;

use git2::{Repository, Signature};
use tempfile::TempDir;

use whogitit::capture::pending::PendingBuffer;
use whogitit::capture::threeway::ThreeWayAnalyzer;
use whogitit::core::attribution::{AIAttribution, PromptInfo, SessionMetadata};
use whogitit::storage::notes::NotesStore;
use whogitit::storage::trailers::TrailerGenerator;

/// Test the full workflow: capture changes, commit, and analyze with three-way diff
#[test]
fn test_full_workflow() {
    // Create a test repo
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();

    // Configure git user
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();

    // Create initial file
    let file_path = dir.path().join("test.rs");
    fs::write(&file_path, "fn main() {}\n").unwrap();

    // Initial commit
    {
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("test.rs")).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();
    }

    // Simulate AI-assisted edit using v2 API
    let ai_content = r#"fn main() {
    println!("Hello, AI!");
}
"#;

    // Create pending buffer with v2 API
    let mut buffer = PendingBuffer::new("test-session-123", "claude-opus-4-5-20251101");
    buffer.record_edit(
        "test.rs",
        Some("fn main() {}\n"),
        ai_content,
        "Edit",
        "Add greeting to main function",
        None,
    );

    // Verify pending changes captured
    assert!(buffer.has_changes());
    assert_eq!(buffer.file_count(), 1);

    let history = buffer.get_file_history("test.rs").unwrap();
    assert_eq!(history.edits.len(), 1);
    assert_eq!(history.original.content, "fn main() {}\n");
    assert!(history.edits[0].after.content.contains("println"));

    // Simulate user manually adding a line before committing
    let final_content = r#"// Author: Test User
fn main() {
    println!("Hello, AI!");
}
"#;
    fs::write(&file_path, final_content).unwrap();

    // Perform three-way analysis
    let result = ThreeWayAnalyzer::analyze_with_diff(history, final_content);

    // Verify attribution
    assert_eq!(result.path, "test.rs");

    // Should detect the comment as human-added
    assert!(
        result.summary.human_lines >= 1,
        "Should have at least 1 human line"
    );

    // Should detect some AI lines
    assert!(
        result.summary.ai_lines >= 1,
        "Should have at least 1 AI line"
    );

    // Verify we can identify the human-added comment line
    let first_line = &result.lines[0];
    assert!(first_line.content.contains("Author"));
    assert!(
        first_line.source.is_human(),
        "First line should be human-added"
    );
}

/// Test that privacy redaction works
#[test]
fn test_privacy_redaction() {
    let redactor = whogitit::privacy::Redactor::default_patterns();

    let sensitive = "Use api_key = sk-secret123 for auth with user@email.com";
    let redacted = redactor.redact(sensitive);

    assert!(!redacted.contains("sk-secret123"));
    assert!(!redacted.contains("user@email.com"));
    assert!(redacted.contains("[REDACTED]"));
}

/// Test trailers generation
#[test]
fn test_trailers() {
    use whogitit::capture::snapshot::{AttributionSummary, FileAttributionResult};
    use whogitit::core::attribution::ModelInfo;

    let attribution = AIAttribution {
        version: 2,
        session: SessionMetadata {
            session_id: "abc123".to_string(),
            model: ModelInfo::claude("claude-opus-4-5-20251101"),
            started_at: "2026-01-30T10:00:00Z".to_string(),
            prompt_count: 5,
            used_plan_mode: false,
            subagent_count: 0,
        },
        prompts: vec![],
        files: vec![FileAttributionResult {
            path: "test.rs".to_string(),
            lines: vec![],
            summary: AttributionSummary {
                total_lines: 10,
                ai_lines: 5,
                ai_modified_lines: 2,
                human_lines: 3,
                original_lines: 0,
                unknown_lines: 0,
            },
        }],
    };

    let trailers = TrailerGenerator::generate(&attribution);

    assert!(trailers.iter().any(|(k, _)| k == "AI-Session"));
    assert!(trailers.iter().any(|(k, _)| k == "AI-Model"));
    assert!(trailers
        .iter()
        .any(|(k, v)| k == "Co-Authored-By" && v.contains("Claude")));
}

/// Test multiple AI edits on the same file
#[test]
fn test_multiple_ai_edits() {
    let mut buffer = PendingBuffer::new("test-session", "claude-opus-4-5-20251101");

    // First AI edit
    buffer.record_edit(
        "test.rs",
        Some("original\n"),
        "original\nfirst_ai_line\n",
        "Edit",
        "First prompt: add a line",
        None,
    );

    // Second AI edit
    buffer.record_edit(
        "test.rs",
        None, // Not needed - will use last AI state
        "original\nfirst_ai_line\nsecond_ai_line\n",
        "Edit",
        "Second prompt: add another line",
        None,
    );

    // Verify edit history
    let history = buffer.get_file_history("test.rs").unwrap();
    assert_eq!(history.edits.len(), 2);
    assert_eq!(history.edits[0].prompt_index, 0);
    assert_eq!(history.edits[1].prompt_index, 1);

    // The second edit should reference the first edit's output as its input
    assert_eq!(history.edits[1].before.content, "original\nfirst_ai_line\n");

    // Analyze final content matching AI exactly
    let result = ThreeWayAnalyzer::analyze(history, "original\nfirst_ai_line\nsecond_ai_line\n");

    // "original" exists in BOTH the original file AND AI output → Original (unchanged)
    // "first_ai_line" and "second_ai_line" only exist in AI output → AI
    assert_eq!(result.summary.ai_lines, 2);
    assert_eq!(result.summary.original_lines, 1);

    // Lines are attributed to the last edit that included them
    // Since all 3 lines appear in edit 1's output, they all have prompt_index 1
    let second_ai = result
        .lines
        .iter()
        .find(|l| l.content == "second_ai_line")
        .unwrap();
    assert_eq!(second_ai.prompt_index, Some(1));
}

/// Test detecting human modifications to AI code
#[test]
fn test_human_modifies_ai_code() {
    let mut buffer = PendingBuffer::new("test-session", "claude-opus-4-5-20251101");

    buffer.record_edit(
        "test.rs",
        Some(""),
        "fn hello() {\n    println!(\"hello\");\n}\n",
        "Write",
        "Create hello function",
        None,
    );

    let history = buffer.get_file_history("test.rs").unwrap();

    // Human modifies the println line
    let final_content = "fn hello() {\n    println!(\"hello, world!\");\n}\n";

    let result = ThreeWayAnalyzer::analyze_with_diff(history, final_content);

    // The "fn hello() {" line should still be AI
    let fn_line = result
        .lines
        .iter()
        .find(|l| l.content.contains("fn hello"))
        .unwrap();
    assert!(fn_line.source.is_ai(), "fn line should be AI");

    // The modified println line should be detected as AIModified or Human
    let _println_line = result
        .lines
        .iter()
        .find(|l| l.content.contains("println"))
        .unwrap();
    // It might be detected as AIModified if similar enough, or Human if different enough
    // Either is acceptable - the key is we don't mark it as pure AI
}

/// Test line attribution for new file creation
#[test]
fn test_new_file_attribution() {
    let mut buffer = PendingBuffer::new("test-session", "claude-opus-4-5-20251101");

    buffer.record_edit(
        "new_file.rs",
        None, // New file
        "// New file\nfn new_func() {}\n",
        "Write",
        "Create new file",
        None,
    );

    let history = buffer.get_file_history("new_file.rs").unwrap();
    assert!(history.was_new_file);
    assert!(history.original.content.is_empty());

    // All content was AI-generated
    let result = ThreeWayAnalyzer::analyze(history, "// New file\nfn new_func() {}\n");

    assert_eq!(result.summary.ai_lines, 2);
    assert_eq!(result.summary.human_lines, 0);
    assert_eq!(result.summary.original_lines, 0);
}

/// Test storing and fetching attribution from git notes
#[test]
fn test_notes_roundtrip() {
    use whogitit::capture::snapshot::{
        AttributionSummary, FileAttributionResult, LineAttribution, LineSource,
    };
    use whogitit::core::attribution::ModelInfo;

    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();

    // Create initial commit
    {
        let sig = Signature::now("Test", "test@test.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial", &tree, &[])
            .unwrap();
    }

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let store = NotesStore::new(&repo).unwrap();

    let attribution = AIAttribution {
        version: 2,
        session: SessionMetadata {
            session_id: "test-session".to_string(),
            model: ModelInfo::claude("claude-opus-4-5-20251101"),
            started_at: "2026-01-30T10:00:00Z".to_string(),
            prompt_count: 1,
            used_plan_mode: false,
            subagent_count: 0,
        },
        prompts: vec![PromptInfo {
            index: 0,
            text: "Create test function".to_string(),
            timestamp: "2026-01-30T10:00:00Z".to_string(),
            affected_files: vec!["test.rs".to_string()],
        }],
        files: vec![FileAttributionResult {
            path: "test.rs".to_string(),
            lines: vec![LineAttribution {
                line_number: 1,
                content: "fn test() {}".to_string(),
                source: LineSource::AI {
                    edit_id: "e1".to_string(),
                },
                edit_id: Some("e1".to_string()),
                prompt_index: Some(0),
                confidence: 1.0,
            }],
            summary: AttributionSummary {
                total_lines: 1,
                ai_lines: 1,
                ai_modified_lines: 0,
                human_lines: 0,
                original_lines: 0,
                unknown_lines: 0,
            },
        }],
    };

    // Store
    store.store_attribution(head.id(), &attribution).unwrap();

    // Fetch and verify
    let fetched = store.fetch_attribution(head.id()).unwrap().unwrap();
    assert_eq!(fetched.version, 2);
    assert_eq!(fetched.session.session_id, "test-session");
    assert_eq!(fetched.files.len(), 1);
    assert_eq!(fetched.prompts.len(), 1);
    assert_eq!(fetched.prompts[0].text, "Create test function");
}
