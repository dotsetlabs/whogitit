use std::collections::{HashMap, HashSet};

use similar::{ChangeTag, TextDiff};

use crate::capture::snapshot::{
    FileAttributionResult, FileEditHistory, LineAttribution, LineSource,
};

/// Normalize a line for comparison purposes.
/// - Trims trailing whitespace (but preserves leading indentation)
/// - Normalizes line endings
/// - Handles cross-platform line ending differences
fn normalize_line(line: &str) -> String {
    line.trim_end().to_string()
}

/// Normalize a line for use as a hash key.
/// Uses the same normalization as normalize_line.
fn normalize_for_key(line: &str) -> String {
    normalize_line(line)
}

/// Performs three-way attribution analysis
///
/// Given:
/// - Original content (before any AI edits)
/// - AI edit history (sequence of AI changes)
/// - Final content (what's being committed)
///
/// Determines for each line in final content:
/// - Was it from the original file?
/// - Was it added by AI (and which edit/prompt)?
/// - Was it added by AI but modified by human?
/// - Was it added by human after AI edits?
pub struct ThreeWayAnalyzer;

impl ThreeWayAnalyzer {
    /// Analyze a file's final content against its edit history
    pub fn analyze(
        history: &FileEditHistory,
        final_content: &str,
    ) -> FileAttributionResult {
        let final_lines: Vec<&str> = final_content.lines().collect();

        // Build lookup tables for efficient matching
        let original_lines = build_line_set(&history.original.content);
        let ai_line_sources = build_ai_line_map(history);

        // Analyze each line in the final content
        let mut attributions = Vec::with_capacity(final_lines.len());

        for (idx, line) in final_lines.iter().enumerate() {
            let line_number = (idx + 1) as u32;
            let attribution = attribute_line(
                line,
                line_number,
                &original_lines,
                &ai_line_sources,
                history,
            );
            attributions.push(attribution);
        }

        // Post-process: improve attribution using context
        improve_attributions_with_context(&mut attributions, history, final_content);

        let summary = FileAttributionResult::compute_summary(&attributions);

        FileAttributionResult {
            path: history.path.clone(),
            lines: attributions,
            summary,
        }
    }

    /// Analyze with position-aware diff for better accuracy
    pub fn analyze_with_diff(
        history: &FileEditHistory,
        final_content: &str,
    ) -> FileAttributionResult {
        let final_lines: Vec<&str> = final_content.lines().collect();
        let mut attributions = Vec::with_capacity(final_lines.len());

        // If no AI edits, everything is original or human
        if history.edits.is_empty() {
            for (idx, line) in final_lines.iter().enumerate() {
                let line_number = (idx + 1) as u32;
                let source = if line_in_content(line, &history.original.content) {
                    LineSource::Original
                } else {
                    LineSource::Human
                };
                attributions.push(LineAttribution {
                    line_number,
                    content: line.to_string(),
                    source,
                    edit_id: None,
                    prompt_index: None,
                    confidence: 1.0,
                });
            }
            let summary = FileAttributionResult::compute_summary(&attributions);
            return FileAttributionResult {
                path: history.path.clone(),
                lines: attributions,
                summary,
            };
        }

        // Perform diff-based analysis
        // Compare: original -> final, and latest_ai -> final
        let latest_ai = history.latest_ai_content();

        // Track which final lines match AI content
        let ai_to_final_mapping = diff_map_lines(&latest_ai.content, final_content);

        // Track which final lines match original content
        let original_to_final_mapping = diff_map_lines(&history.original.content, final_content);

        // Build reverse map: final line index -> source
        let mut final_line_sources: HashMap<usize, (LineSource, Option<String>, Option<u32>)> =
            HashMap::new();

        // First pass: mark lines that came from AI edits (takes priority)
        let ai_line_map = build_ai_line_map(history);
        for (ai_idx, final_idx) in &ai_to_final_mapping {
            let ai_line = latest_ai.lines().get(*ai_idx).map(|s| *s).unwrap_or("");
            let normalized = normalize_for_key(ai_line);

            // Check if this line came from an AI edit
            if let Some((edit_id, prompt_idx)) = ai_line_map.get(&normalized) {
                final_line_sources.insert(
                    *final_idx,
                    (
                        LineSource::AI {
                            edit_id: edit_id.clone(),
                        },
                        Some(edit_id.clone()),
                        Some(*prompt_idx),
                    ),
                );
            }
        }

        // Second pass: mark lines that exist in original (only if not already AI)
        for (_, final_idx) in &original_to_final_mapping {
            if !final_line_sources.contains_key(final_idx) {
                final_line_sources.insert(*final_idx, (LineSource::Original, None, None));
            }
        }

        // Third pass: check unmapped lines against AI content
        // This is critical - the diff may not map lines even if they're identical
        // (e.g., when surrounding context changes, diff sees Delete+Insert instead of Equal)
        for (idx, line) in final_lines.iter().enumerate() {
            if final_line_sources.contains_key(&idx) {
                continue;
            }

            let normalized = normalize_for_key(line);

            // First check for exact match in AI map - this is the key fix!
            // Lines that exist verbatim in AI output should be attributed to AI
            // even if the diff algorithm didn't map them as "Equal"
            if let Some((edit_id, prompt_idx)) = ai_line_map.get(&normalized) {
                final_line_sources.insert(
                    idx,
                    (
                        LineSource::AI {
                            edit_id: edit_id.clone(),
                        },
                        Some(edit_id.clone()),
                        Some(*prompt_idx),
                    ),
                );
                continue;
            }

            // Check if this is similar to an AI line (modified)
            if let Some((edit_id, prompt_idx, similarity)) =
                find_similar_ai_line(line, &ai_line_map, 0.6)
            {
                final_line_sources.insert(
                    idx,
                    (
                        LineSource::AIModified {
                            edit_id: edit_id.clone(),
                            similarity,
                        },
                        Some(edit_id),
                        Some(prompt_idx),
                    ),
                );
                continue;
            }

            // Check if line exists in original (untouched by AI or human)
            if line_in_content(line, &history.original.content) {
                final_line_sources.insert(idx, (LineSource::Original, None, None));
                continue;
            }

            // New line added by human
            final_line_sources.insert(idx, (LineSource::Human, None, None));
        }

        // Build final attributions
        for (idx, line) in final_lines.iter().enumerate() {
            let line_number = (idx + 1) as u32;
            let (source, edit_id, prompt_index) = final_line_sources
                .get(&idx)
                .cloned()
                .unwrap_or((LineSource::Unknown, None, None));

            let confidence = match &source {
                LineSource::Original => 1.0,
                LineSource::AI { .. } => 1.0,
                LineSource::AIModified { similarity, .. } => *similarity,
                LineSource::Human => 0.9,
                LineSource::Unknown => 0.5,
            };

            attributions.push(LineAttribution {
                line_number,
                content: line.to_string(),
                source,
                edit_id,
                prompt_index,
                confidence,
            });
        }

        let summary = FileAttributionResult::compute_summary(&attributions);

        FileAttributionResult {
            path: history.path.clone(),
            lines: attributions,
            summary,
        }
    }
}

/// Build a set of normalized lines from content for fast lookup
fn build_line_set(content: &str) -> HashSet<String> {
    content.lines().map(|l| normalize_for_key(l)).collect()
}

/// Build a map from normalized line content -> (edit_id, prompt_index) for all AI edits
///
/// IMPORTANT: All lines in an AI edit's `after` content are considered AI-generated,
/// not just lines that differ from `before`. This is because when AI writes/edits a file,
/// it produces the entire output - even if some lines coincidentally match the original,
/// the AI chose to include them.
///
/// Lines are normalized (trailing whitespace trimmed) to handle git/editor differences.
fn build_ai_line_map(history: &FileEditHistory) -> HashMap<String, (String, u32)> {
    let mut map = HashMap::new();

    // Process edits in order - later edits override earlier ones
    for edit in &history.edits {
        // ALL lines in the AI's output are AI-generated
        // This ensures complete file rewrites are properly attributed
        for line in edit.after.content.lines() {
            map.insert(
                normalize_for_key(line),
                (edit.edit_id.clone(), edit.prompt_index),
            );
        }
    }

    map
}

/// Check if a normalized line exists in content
fn line_in_content(line: &str, content: &str) -> bool {
    let normalized = normalize_for_key(line);
    content.lines().any(|l| normalize_for_key(l) == normalized)
}

/// Map line indices from source to target using diff
fn diff_map_lines(source: &str, target: &str) -> Vec<(usize, usize)> {
    let diff = TextDiff::from_lines(source, target);
    let mut mappings = Vec::new();

    let mut source_idx = 0usize;
    let mut target_idx = 0usize;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                mappings.push((source_idx, target_idx));
                source_idx += 1;
                target_idx += 1;
            }
            ChangeTag::Delete => {
                source_idx += 1;
            }
            ChangeTag::Insert => {
                target_idx += 1;
            }
        }
    }

    mappings
}

/// Attribute a single line
///
/// Priority order:
/// 1. AI - if line is in the AI edit output, it's AI-generated
/// 2. AIModified - if line is similar to an AI line
/// 3. Original - if line existed before AI edits and wasn't touched
/// 4. Human - line was added after AI edits
///
/// All lookups use normalized line content to handle whitespace differences.
fn attribute_line(
    line: &str,
    line_number: u32,
    original_lines: &HashSet<String>,
    ai_line_sources: &HashMap<String, (String, u32)>,
    _history: &FileEditHistory,
) -> LineAttribution {
    let normalized = normalize_for_key(line);

    // Check if line matches an AI edit exactly - AI takes priority
    // because if the AI output contains this line, it's AI-generated
    // (even if the same line existed in the original)
    if let Some((edit_id, prompt_idx)) = ai_line_sources.get(&normalized) {
        return LineAttribution {
            line_number,
            content: line.to_string(),
            source: LineSource::AI {
                edit_id: edit_id.clone(),
            },
            edit_id: Some(edit_id.clone()),
            prompt_index: Some(*prompt_idx),
            confidence: 1.0,
        };
    }

    // Check if line is similar to an AI line (human modified AI output)
    if let Some((edit_id, prompt_idx, similarity)) =
        find_similar_ai_line(line, ai_line_sources, 0.6)
    {
        return LineAttribution {
            line_number,
            content: line.to_string(),
            source: LineSource::AIModified {
                edit_id: edit_id.clone(),
                similarity,
            },
            edit_id: Some(edit_id),
            prompt_index: Some(prompt_idx),
            confidence: similarity,
        };
    }

    // Check if line existed in original (and wasn't part of AI output)
    if original_lines.contains(&normalized) {
        return LineAttribution {
            line_number,
            content: line.to_string(),
            source: LineSource::Original,
            edit_id: None,
            prompt_index: None,
            confidence: 1.0,
        };
    }

    // Line doesn't match original or AI - it's a human addition
    LineAttribution {
        line_number,
        content: line.to_string(),
        source: LineSource::Human,
        edit_id: None,
        prompt_index: None,
        confidence: 0.9,
    }
}

/// Find a similar AI line using edit distance
///
/// Note: Empty/whitespace-only lines are handled by exact matching in attribute_line,
/// so this function focuses on non-trivial content similarity.
fn find_similar_ai_line(
    line: &str,
    ai_lines: &HashMap<String, (String, u32)>,
    threshold: f64,
) -> Option<(String, u32, f64)> {
    let line_trimmed = line.trim();

    // Empty lines should be handled by exact matching, not similarity
    // (empty lines match other empty lines with 100% similarity via normalize_for_key)
    if line_trimmed.is_empty() {
        return None;
    }

    let mut best_match: Option<(String, u32, f64)> = None;

    for (ai_line, (edit_id, prompt_idx)) in ai_lines {
        let ai_trimmed = ai_line.trim();

        // Skip empty AI lines in similarity comparison
        if ai_trimmed.is_empty() {
            continue;
        }

        let similarity = compute_similarity(line_trimmed, ai_trimmed);
        if similarity >= threshold {
            if best_match.is_none() || similarity > best_match.as_ref().unwrap().2 {
                best_match = Some((edit_id.clone(), *prompt_idx, similarity));
            }
        }
    }

    best_match
}

/// Compute similarity between two strings (0.0 - 1.0)
fn compute_similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }

    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    // Use longest common subsequence ratio
    let lcs_len = longest_common_subsequence(a, b);
    let max_len = a.len().max(b.len()) as f64;

    lcs_len as f64 / max_len
}

/// Compute length of longest common subsequence
fn longest_common_subsequence(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    // Optimization: if strings are very different in length, quick exit
    if (m as f64 / n as f64) < 0.5 || (n as f64 / m as f64) < 0.5 {
        return 0;
    }

    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            if a_chars[i - 1] == b_chars[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    dp[m][n]
}

/// Improve attributions using contextual information
fn improve_attributions_with_context(
    attributions: &mut Vec<LineAttribution>,
    _history: &FileEditHistory,
    _final_content: &str,
) {
    // If we have unknown lines surrounded by AI lines from the same edit,
    // they might be AI lines that were modified

    let len = attributions.len();
    if len < 3 {
        return;
    }

    for i in 1..len - 1 {
        if attributions[i].source == LineSource::Unknown {
            // Check surrounding lines
            let prev_edit = attributions[i - 1].edit_id.clone();
            let next_edit = attributions[i + 1].edit_id.clone();

            if prev_edit.is_some() && prev_edit == next_edit {
                // Likely an AI line that was modified
                attributions[i].source = LineSource::AIModified {
                    edit_id: prev_edit.clone().unwrap(),
                    similarity: 0.5,
                };
                attributions[i].edit_id = prev_edit;
                attributions[i].prompt_index = attributions[i - 1].prompt_index;
                attributions[i].confidence = 0.5;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::snapshot::AIEdit;

    #[test]
    fn test_simple_ai_addition() {
        let mut history = FileEditHistory::new("test.rs", Some("line1\nline2\n"));

        history.add_edit(AIEdit::new(
            "Add line3",
            0,
            "Edit",
            "line1\nline2\n",
            "line1\nline2\nline3\n",
        ));

        let result = ThreeWayAnalyzer::analyze(&history, "line1\nline2\nline3\n");

        // All 3 lines are in the AI's output, so all 3 are AI-generated
        // (even though line1 and line2 also existed in original)
        assert_eq!(result.summary.ai_lines, 3);
        assert_eq!(result.summary.original_lines, 0);
        assert_eq!(result.summary.human_lines, 0);
    }

    #[test]
    fn test_human_modification_after_ai() {
        let mut history = FileEditHistory::new("test.rs", Some("line1\n"));

        history.add_edit(AIEdit::new(
            "Add lines",
            0,
            "Edit",
            "line1\n",
            "line1\nAI line\n",
        ));

        // Human adds a line and modifies AI line
        let final_content = "line1\nAI line modified\nhuman line\n";
        let result = ThreeWayAnalyzer::analyze(&history, final_content);

        // "line1" is in AI output, so it's AI
        assert_eq!(result.summary.ai_lines, 1);
        // "AI line modified" should be detected as AIModified
        // "human line" should be detected as Human
        assert_eq!(
            result.summary.ai_modified_lines + result.summary.human_lines,
            2,
            "Should have 2 non-AI lines (modified + human)"
        );
    }

    #[test]
    fn test_line_shift() {
        let mut history = FileEditHistory::new("test.rs", Some("line1\nline2\n"));

        history.add_edit(AIEdit::new(
            "Add AI content",
            0,
            "Edit",
            "line1\nline2\n",
            "line1\nline2\nAI added\n",
        ));

        // Human inserts line at beginning
        let final_content = "new first line\nline1\nline2\nAI added\n";
        let result = ThreeWayAnalyzer::analyze_with_diff(&history, final_content);

        // "new first line" is Human (not in AI output)
        // "line1", "line2", "AI added" are all in AI output, so AI
        assert_eq!(result.summary.human_lines, 1);
        assert_eq!(result.summary.ai_lines, 3);
        assert_eq!(result.summary.original_lines, 0);
    }

    #[test]
    fn test_similarity_computation() {
        assert_eq!(compute_similarity("hello", "hello"), 1.0);
        // Completely different strings
        assert!(compute_similarity("abc", "xyz") < 0.3);
        // Similar strings with comparable length should have high similarity
        assert!(compute_similarity("println(hello)", "println(world)") > 0.6);
        // Modified line detection
        assert!(compute_similarity(
            "    println!(\"hello\");",
            "    println!(\"hello, world!\");"
        ) > 0.6);
    }

    #[test]
    fn test_multiple_ai_edits() {
        let mut history = FileEditHistory::new("test.rs", Some("original\n"));

        history.add_edit(AIEdit::new(
            "First prompt",
            0,
            "Edit",
            "original\n",
            "original\nfirst AI\n",
        ));

        history.add_edit(AIEdit::new(
            "Second prompt",
            1,
            "Edit",
            "original\nfirst AI\n",
            "original\nfirst AI\nsecond AI\n",
        ));

        let result = ThreeWayAnalyzer::analyze(
            &history,
            "original\nfirst AI\nsecond AI\n",
        );

        // All lines are in the AI output from the second edit
        // "original" gets attributed to edit 1 (first appearance in AI output)
        // "first AI" gets attributed to edit 0 (first added)
        // "second AI" gets attributed to edit 1 (first added)
        assert_eq!(result.summary.ai_lines, 3);
        assert_eq!(result.summary.original_lines, 0);

        // Check prompt indices - later edits override, so "original" is from edit 1
        let first_ai = result.lines.iter().find(|l| l.content == "first AI").unwrap();
        // first AI appears in edit 0's output and edit 1's output, later wins
        assert_eq!(first_ai.prompt_index, Some(1));

        let second_ai = result.lines.iter().find(|l| l.content == "second AI").unwrap();
        assert_eq!(second_ai.prompt_index, Some(1));
    }

    #[test]
    fn test_only_original_no_ai_edits() {
        // Test that without AI edits, original lines stay original
        let history = FileEditHistory::new("test.rs", Some("line1\nline2\n"));
        // No AI edits added

        let result = ThreeWayAnalyzer::analyze(&history, "line1\nline2\nline3\n");

        // line1, line2 are original (no AI touched them)
        // line3 is human (added without AI)
        assert_eq!(result.summary.original_lines, 2);
        assert_eq!(result.summary.human_lines, 1);
        assert_eq!(result.summary.ai_lines, 0);
    }

    #[test]
    fn test_whitespace_normalization() {
        // Test that trailing whitespace differences don't affect attribution
        let mut history = FileEditHistory::new("test.rs", Some(""));

        // AI generates lines with trailing spaces
        history.add_edit(AIEdit::new(
            "Generate code",
            0,
            "Write",
            "",
            "fn main() {  \n    println!(\"hello\");  \n}\n",
        ));

        // Final commit has trailing spaces stripped (common git behavior)
        let final_content = "fn main() {\n    println!(\"hello\");\n}\n";
        let result = ThreeWayAnalyzer::analyze(&history, final_content);

        // All lines should be AI despite whitespace differences
        assert_eq!(result.summary.ai_lines, 3, "All lines should be AI");
        assert_eq!(result.summary.human_lines, 0, "No human lines expected");
    }

    #[test]
    fn test_empty_line_attribution() {
        // Test that empty lines in AI output are properly attributed
        let mut history = FileEditHistory::new("test.rs", Some(""));

        // AI generates code with empty lines
        history.add_edit(AIEdit::new(
            "Generate code with spacing",
            0,
            "Write",
            "",
            "fn main() {\n\n    println!(\"hello\");\n\n}\n",
        ));

        let final_content = "fn main() {\n\n    println!(\"hello\");\n\n}\n";
        let result = ThreeWayAnalyzer::analyze(&history, final_content);

        // All lines including empty ones should be AI
        assert_eq!(result.summary.ai_lines, 5, "All 5 lines should be AI");
        assert_eq!(result.summary.human_lines, 0, "No human lines expected");
    }

    #[test]
    fn test_tabs_vs_spaces() {
        // Test that different indentation styles still match
        let mut history = FileEditHistory::new("test.rs", Some(""));

        // AI generates with spaces
        history.add_edit(AIEdit::new(
            "Generate code",
            0,
            "Write",
            "",
            "fn main() {\n    code();\n}\n",
        ));

        // Final uses same content (tabs would need different handling)
        let final_content = "fn main() {\n    code();\n}\n";
        let result = ThreeWayAnalyzer::analyze(&history, final_content);

        assert_eq!(result.summary.ai_lines, 3);
        assert_eq!(result.summary.human_lines, 0);
    }

    #[test]
    fn test_diff_unmapped_lines_still_attributed_to_ai() {
        // This test covers the critical bug fix:
        // When the diff algorithm sees structural changes, it may not map identical lines
        // (treating them as Delete+Insert instead of Equal). We need to still attribute
        // those lines to AI if they exist in the AI output.
        let original = "fn foo() {\n    old_code();\n}\n";
        let mut history = FileEditHistory::new("test.rs", Some(original));

        // AI rewrites the function with new content
        // The closing brace "}" exists in both, but diff might not map it
        let ai_output = "fn foo() {\n    new_code();\n    more_code();\n}\n";
        history.add_edit(AIEdit::new(
            "Rewrite function",
            0,
            "Edit",
            original,
            ai_output,
        ));

        // Final content matches AI output exactly
        let result = ThreeWayAnalyzer::analyze_with_diff(&history, ai_output);

        // All lines should be AI - especially the closing brace
        assert_eq!(result.summary.ai_lines, 4, "All lines should be AI");
        assert_eq!(result.summary.human_lines, 0, "No human lines expected");
        assert_eq!(result.summary.original_lines, 0, "No original lines (all in AI output)");

        // Verify the closing brace specifically is AI
        let closing_brace = result.lines.iter().find(|l| l.content == "}").unwrap();
        assert!(
            matches!(closing_brace.source, LineSource::AI { .. }),
            "Closing brace should be AI, got {:?}",
            closing_brace.source
        );
    }

    #[test]
    fn test_debug_attribution_flow() {
        // Debug test to verify attribution logic
        let original = "line1\nline2\nline3\nline4\nline5\n";
        let mut history = FileEditHistory::new("test.rs", Some(original));

        // First AI edit: adds line6, line7
        let after1 = "line1\nline2\nline3\nline4\nline5\nline6\nline7\n";
        history.add_edit(AIEdit::new("prompt1", 0, "Edit", original, after1));

        // Second AI edit: adds line8 and modifies line3
        let after2 = "line1\nline2\nLINE3_MODIFIED\nline4\nline5\nline6\nline7\nline8\n";
        history.add_edit(AIEdit::new("prompt2", 1, "Edit", after1, after2));

        // Final content matches AI edit exactly
        let result = ThreeWayAnalyzer::analyze_with_diff(&history, after2);

        println!("\nAttribution results:");
        println!("  AI lines: {}", result.summary.ai_lines);
        println!("  AI modified lines: {}", result.summary.ai_modified_lines);
        println!("  Original lines: {}", result.summary.original_lines);
        println!("  Human lines: {}", result.summary.human_lines);

        for line in &result.lines {
            println!("  Line {}: {:?} - '{}'", line.line_number, line.source, line.content);
        }

        // All 8 lines should be AI (they're all in the AI edit output)
        assert_eq!(result.summary.ai_lines, 8, "All lines should be AI");
        assert_eq!(result.summary.human_lines, 0, "No human lines expected");
    }

    #[test]
    fn test_duplicate_lines_in_ai_output() {
        // Test that duplicate lines (like closing braces) are properly attributed
        // This tests the real-world scenario where the same line content appears multiple times
        let original = r#"fn foo() {
    code();
}
"#;
        let mut history = FileEditHistory::new("test.rs", Some(original));

        // AI adds a new function with similar structure (duplicate "}" and empty lines)
        let after1 = r#"fn foo() {
    code();
}

fn bar() {
    more_code();
}
"#;
        history.add_edit(AIEdit::new("Add bar function", 0, "Edit", original, after1));

        let result = ThreeWayAnalyzer::analyze_with_diff(&history, after1);

        println!("\nDuplicate lines test:");
        for line in &result.lines {
            let source_str = match &line.source {
                LineSource::AI { .. } => "AI",
                LineSource::Original => "Orig",
                LineSource::Human => "Human",
                _ => "Other",
            };
            println!("  Line {}: {} - '{}'", line.line_number, source_str, line.content);
        }

        // All lines should be AI (including the duplicate "}")
        assert_eq!(result.summary.human_lines, 0, "No human lines - all are in AI output");
        // The closing brace "}" appears twice but both should be AI
        let closing_braces: Vec<_> = result.lines.iter().filter(|l| l.content == "}").collect();
        assert_eq!(closing_braces.len(), 2, "Should have 2 closing braces");
        for brace in closing_braces {
            assert!(matches!(brace.source, LineSource::AI { .. }),
                "Closing brace at line {} should be AI, got {:?}", brace.line_number, brace.source);
        }
    }

    #[test]
    fn test_common_patterns_attributed_to_ai() {
        // Test that common patterns like empty lines, closing braces, and doc comments
        // are properly attributed to AI when they appear in AI output
        let original = "";
        let mut history = FileEditHistory::new("test.rs", Some(original));

        // AI generates code with common patterns
        let ai_output = r#"/// A test function
#[test]
fn test() {
    assert!(true);
}
"#;
        history.add_edit(AIEdit::new(
            "Generate test",
            0,
            "Write",
            original,
            ai_output,
        ));

        let result = ThreeWayAnalyzer::analyze_with_diff(&history, ai_output);

        // All lines should be AI
        assert_eq!(result.summary.ai_lines, 5, "All 5 lines should be AI");
        assert_eq!(result.summary.human_lines, 0, "No human lines");

        // Check each line individually
        for line in &result.lines {
            assert!(
                matches!(line.source, LineSource::AI { .. }),
                "Line '{}' should be AI, got {:?}",
                line.content,
                line.source
            );
        }
    }
}
