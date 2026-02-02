use std::collections::{HashMap, HashSet};

use similar::{ChangeTag, TextDiff};

use crate::capture::snapshot::{
    FileAttributionResult, FileEditHistory, LineAttribution, LineSource,
};

/// Default similarity threshold for AIModified detection
/// This can be overridden via config (analysis.similarity_threshold)
pub const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.6;

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
    pub fn analyze(history: &FileEditHistory, final_content: &str) -> FileAttributionResult {
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
                DEFAULT_SIMILARITY_THRESHOLD,
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
        Self::analyze_with_diff_with_threshold(history, final_content, DEFAULT_SIMILARITY_THRESHOLD)
    }

    /// Analyze with position-aware diff for better accuracy, using a custom similarity threshold
    pub fn analyze_with_diff_with_threshold(
        history: &FileEditHistory,
        final_content: &str,
        similarity_threshold: f64,
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

        // Build lookup sets
        let original_lines = build_line_set(&history.original.content);
        let ai_line_map = build_ai_line_map(history);

        // Track which final lines match AI content
        let ai_to_final_mapping = diff_map_lines(&latest_ai.content, final_content);

        // Track which final lines match original content
        let original_to_final_mapping = diff_map_lines(&history.original.content, final_content);

        // Build reverse map: final line index -> source
        let mut final_line_sources: HashMap<usize, (LineSource, Option<String>, Option<u32>)> =
            HashMap::new();

        // First pass: mark lines that exist in original as Original
        // (Lines in both original and AI should be Original - they weren't changed)
        for (_, final_idx) in &original_to_final_mapping {
            final_line_sources.insert(*final_idx, (LineSource::Original, None, None));
        }

        // Second pass: mark lines from AI edits that weren't mapped from original
        // Key insight: if a final line has position mapping from AI but NOT from original,
        // it's AI-generated - even if the content happens to match something in original
        // (e.g., a `}` added by AI shouldn't be marked as Original just because
        // the original file also had a `}` at a different position)
        for (ai_idx, final_idx) in &ai_to_final_mapping {
            // Skip if already marked (came from original position mapping)
            if final_line_sources.contains_key(final_idx) {
                continue;
            }

            let ai_line = latest_ai.lines().get(*ai_idx).copied().unwrap_or("");
            let normalized = normalize_for_key(ai_line);

            // This line was mapped from AI output and NOT from original position
            // So it's AI-generated (regardless of whether similar content exists in original)
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

        // Third pass: check unmapped lines
        for (idx, line) in final_lines.iter().enumerate() {
            if final_line_sources.contains_key(&idx) {
                continue;
            }

            let normalized = normalize_for_key(line);

            // Check if line exists in original first
            if original_lines.contains(&normalized) {
                final_line_sources.insert(idx, (LineSource::Original, None, None));
                continue;
            }

            // Check if line is in AI output (but not original)
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
                find_similar_ai_line(line, &ai_line_map, similarity_threshold)
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

        // Post-process: improve attribution using context and block matching
        improve_attributions_with_context(&mut attributions, history, final_content);

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
    content.lines().map(normalize_for_key).collect()
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
/// 1. Original - if line existed before AI edits and is unchanged
/// 2. AI - if line is in the AI edit output but NOT in original (actually changed)
/// 3. AIModified - if line is similar to an AI line
/// 4. Human - line was added after AI edits
///
/// All lookups use normalized line content to handle whitespace differences.
fn attribute_line(
    line: &str,
    line_number: u32,
    original_lines: &HashSet<String>,
    ai_line_sources: &HashMap<String, (String, u32)>,
    _history: &FileEditHistory,
    similarity_threshold: f64,
) -> LineAttribution {
    let normalized = normalize_for_key(line);
    let in_original = original_lines.contains(&normalized);
    let in_ai = ai_line_sources.get(&normalized);

    // If line exists in original AND in AI output, it's unchanged - mark as Original
    // This prevents counting context lines that AI included but didn't change
    if in_original && in_ai.is_some() {
        return LineAttribution {
            line_number,
            content: line.to_string(),
            source: LineSource::Original,
            edit_id: None,
            prompt_index: None,
            confidence: 1.0,
        };
    }

    // If line is in original but NOT in AI output, it's still Original
    // (The AI didn't touch this line at all)
    if in_original {
        return LineAttribution {
            line_number,
            content: line.to_string(),
            source: LineSource::Original,
            edit_id: None,
            prompt_index: None,
            confidence: 1.0,
        };
    }

    // If line is in AI output but NOT in original, it's AI-generated
    if let Some((edit_id, prompt_idx)) = in_ai {
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
        find_similar_ai_line(line, ai_line_sources, similarity_threshold)
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

    // Line doesn't exist in original or AI output - must be human-added
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
        if similarity >= threshold
            && (best_match.is_none() || similarity > best_match.as_ref().unwrap().2)
        {
            best_match = Some((edit_id.clone(), *prompt_idx, similarity));
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
    attributions: &mut [LineAttribution],
    history: &FileEditHistory,
    _final_content: &str,
) {
    let len = attributions.len();
    if len < 2 {
        return;
    }

    // First pass: handle unknown lines surrounded by AI lines
    for i in 1..len - 1 {
        if attributions[i].source == LineSource::Unknown {
            let prev_edit = attributions[i - 1].edit_id.clone();
            let next_edit = attributions[i + 1].edit_id.clone();

            if prev_edit.is_some() && prev_edit == next_edit {
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

    // Second pass: block-level matching for reformatted code
    // This handles cases where formatters (rustfmt, prettier, etc.) split
    // a single AI-generated line into multiple lines
    improve_attributions_with_block_matching(attributions, history);

    // Third pass: context-based attribution for remaining unmatched lines
    // If a Human/AIModified line is surrounded by AI lines from the same edit,
    // and it looks like a fragment (continuation of a split statement), attribute it to AI
    improve_attributions_with_surrounding_context(attributions);
}

/// Check if a line looks like a fragment of a split statement
fn looks_like_fragment(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    // Starts with continuation characters (method chains, operators, closing parens)
    let starts_continuation = trimmed.starts_with('.')
        || trimmed.starts_with(',')
        || trimmed.starts_with(')')
        || trimmed.starts_with(']')
        || trimmed.starts_with('}')
        || trimmed.starts_with("&&")
        || trimmed.starts_with("||");

    // Ends with opening/continuation characters
    let ends_continuation = trimmed.ends_with('(')
        || trimmed.ends_with('[')
        || trimmed.ends_with('{')
        || trimmed.ends_with(',')
        || trimmed.ends_with('=')
        || trimmed.ends_with("&&")
        || trimmed.ends_with("||");

    // Common fragment patterns
    let is_common_fragment = trimmed == ");"
        || trimmed == ")"
        || trimmed == "};"
        || trimmed == "}"
        || trimmed == "];"
        || trimmed == "]"
        || trimmed.starts_with(".unwrap(")
        || trimmed.starts_with(".context(")
        || trimmed.starts_with(".ok_or")
        || trimmed.starts_with(".expect(")
        || trimmed.starts_with(".map(")
        || trimmed.starts_with(".and_then(")
        || trimmed.starts_with(".or_else(");

    starts_continuation || ends_continuation || is_common_fragment
}

/// Improve attributions using surrounding context
/// If Human/AIModified lines are surrounded by AI lines from the same edit,
/// and the lines look like fragments, attribute them to AI
fn improve_attributions_with_surrounding_context(attributions: &mut [LineAttribution]) {
    let len = attributions.len();
    if len < 3 {
        return;
    }

    // Make multiple passes since fixing one line might enable fixing adjacent lines
    let mut changed = true;
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 5;

    while changed && iterations < MAX_ITERATIONS {
        changed = false;
        iterations += 1;

        for i in 1..len - 1 {
            let is_unattributed = matches!(
                &attributions[i].source,
                LineSource::Human | LineSource::AIModified { .. }
            );

            if !is_unattributed {
                continue;
            }

            // Check if surrounded by AI lines from the same edit
            let prev_edit = &attributions[i - 1].edit_id;
            let next_edit = &attributions[i + 1].edit_id;
            let prev_is_ai = matches!(
                &attributions[i - 1].source,
                LineSource::AI { .. } | LineSource::AIModified { .. }
            );
            let next_is_ai = matches!(
                &attributions[i + 1].source,
                LineSource::AI { .. } | LineSource::AIModified { .. }
            );

            if prev_is_ai
                && next_is_ai
                && prev_edit.is_some()
                && prev_edit == next_edit
                && looks_like_fragment(&attributions[i].content)
            {
                let edit_id = prev_edit.clone().unwrap();
                let prompt_index = attributions[i - 1].prompt_index;

                attributions[i].source = LineSource::AI {
                    edit_id: edit_id.clone(),
                };
                attributions[i].edit_id = Some(edit_id);
                attributions[i].prompt_index = prompt_index;
                attributions[i].confidence = 0.85; // High confidence from context
                changed = true;
            }
        }
    }
}

/// Normalize a string for block comparison by collapsing whitespace
/// and removing spaces that formatters add when splitting lines
fn normalize_for_block_comparison(s: &str) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    // Remove spaces before common continuation characters that rustfmt adds
    // when splitting lines (e.g., "foo .bar()" -> "foo.bar()")
    collapsed
        .replace(" .", ".")
        .replace(" ,", ",")
        .replace(" ;", ";")
        .replace(" )", ")")
        .replace("( ", "(")
}

/// Improve attributions by matching blocks of consecutive Human lines
/// against AI-generated content.
///
/// This handles the case where code formatters split a single line into multiple lines.
/// For example, rustfmt might split:
///   `let x = foo.bar().baz().qux();`
/// into:
///   `let x = foo`
///   `    .bar()`
///   `    .baz()`
///   `    .qux();`
///
/// Each individual line doesn't match the AI output, but when joined they do.
fn improve_attributions_with_block_matching(
    attributions: &mut [LineAttribution],
    history: &FileEditHistory,
) {
    if attributions.is_empty() || history.edits.is_empty() {
        return;
    }

    // Build normalized AI lines for comparison
    // We normalize each AI line and also create joined versions of consecutive AI lines
    let mut ai_normalized_lines: Vec<(String, String, u32)> = Vec::new(); // (normalized, edit_id, prompt_idx)

    for edit in &history.edits {
        for line in edit.after.content.lines() {
            let normalized = normalize_for_block_comparison(line);
            if !normalized.is_empty() {
                ai_normalized_lines.push((normalized, edit.edit_id.clone(), edit.prompt_index));
            }
        }
    }

    // Also create joined versions of consecutive AI lines (2-8 lines joined)
    for edit in &history.edits {
        let lines: Vec<&str> = edit.after.content.lines().collect();
        for window_size in 2..=8.min(lines.len()) {
            for start in 0..=lines.len().saturating_sub(window_size) {
                let joined: String = lines[start..start + window_size]
                    .iter()
                    .map(|l| normalize_for_block_comparison(l))
                    .collect::<Vec<_>>()
                    .join(" ");
                if !joined.is_empty() {
                    ai_normalized_lines.push((joined, edit.edit_id.clone(), edit.prompt_index));
                }
            }
        }
    }

    // Find blocks of consecutive unmatched lines (Human or low-confidence AIModified)
    // Low-confidence AIModified lines are likely false positives from partial similarity matching
    let is_unmatched = |attr: &LineAttribution| -> bool {
        match &attr.source {
            LineSource::Human => true,
            LineSource::Unknown => true,
            // Include AIModified with similarity < 0.85 as potentially misattributed
            LineSource::AIModified { similarity, .. } => *similarity < 0.85,
            _ => false,
        }
    };

    let mut i = 0;
    while i < attributions.len() {
        // Find start of an unmatched block
        if !is_unmatched(&attributions[i]) {
            i += 1;
            continue;
        }

        // Find the extent of the unmatched block
        let block_start = i;
        let mut block_end = i;
        while block_end < attributions.len() && is_unmatched(&attributions[block_end]) {
            block_end += 1;
        }

        let block_len = block_end - block_start;

        // Only process blocks of 1-8 lines
        if (1..=8).contains(&block_len) {
            // Join the block content
            let block_content: String = attributions[block_start..block_end]
                .iter()
                .map(|a| normalize_for_block_comparison(&a.content))
                .collect::<Vec<_>>()
                .join(" ");

            // Find best matching AI line/block
            let mut best_match: Option<(f64, String, u32)> = None;

            for (ai_normalized, edit_id, prompt_idx) in &ai_normalized_lines {
                let similarity = compute_similarity(&block_content, ai_normalized);

                // Require similarity threshold based on block size
                // Lower thresholds because formatters can introduce small differences
                // (e.g., extra spaces, line breaks in different positions)
                let threshold = match block_len {
                    1 => 0.75, // Single lines: might be partial match of split line
                    2 => 0.70, // Common case: one line split into two
                    3..=4 => 0.65,
                    _ => 0.60,
                };

                if similarity >= threshold
                    && (best_match.is_none() || similarity > best_match.as_ref().unwrap().0)
                {
                    best_match = Some((similarity, edit_id.clone(), *prompt_idx));
                }
            }

            // If we found a match, re-attribute all lines in the block
            if let Some((similarity, edit_id, prompt_idx)) = best_match {
                for attr in attributions.iter_mut().take(block_end).skip(block_start) {
                    attr.source = LineSource::AI {
                        edit_id: edit_id.clone(),
                    };
                    attr.edit_id = Some(edit_id.clone());
                    attr.prompt_index = Some(prompt_idx);
                    attr.confidence = similarity;
                }
            }
        }

        i = block_end;
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

        // line1 and line2 exist in both original and AI output -> Original (unchanged)
        // line3 is only in AI output -> AI (actually added by AI)
        assert_eq!(result.summary.ai_lines, 1);
        assert_eq!(result.summary.original_lines, 2);
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

        // "line1" is in both original and AI output -> Original (unchanged)
        assert_eq!(result.summary.original_lines, 1);
        // "AI line modified" should be detected as AIModified (similar to "AI line")
        // "human line" should be detected as Human (not in original or AI)
        assert_eq!(
            result.summary.ai_modified_lines + result.summary.human_lines,
            2,
            "Should have 2 changed lines (modified + human)"
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

        // "new first line" is Human (not in original or AI output)
        // "line1", "line2" are in both original and AI output -> Original (unchanged)
        // "AI added" is only in AI output -> AI
        assert_eq!(result.summary.human_lines, 1);
        assert_eq!(result.summary.ai_lines, 1);
        assert_eq!(result.summary.original_lines, 2);
    }

    #[test]
    fn test_similarity_computation() {
        assert_eq!(compute_similarity("hello", "hello"), 1.0);
        // Completely different strings
        assert!(compute_similarity("abc", "xyz") < 0.3);
        // Similar strings with comparable length should have high similarity
        assert!(compute_similarity("println(hello)", "println(world)") > 0.6);
        // Modified line detection
        assert!(
            compute_similarity(
                "    println!(\"hello\");",
                "    println!(\"hello, world!\");"
            ) > 0.6
        );
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

        let result = ThreeWayAnalyzer::analyze(&history, "original\nfirst AI\nsecond AI\n");

        // "original" is in the original file AND in AI outputs -> Original (unchanged)
        // "first AI" is NOT in original, added by AI -> AI
        // "second AI" is NOT in original, added by AI -> AI
        assert_eq!(result.summary.original_lines, 1);
        assert_eq!(result.summary.ai_lines, 2);

        // Check that AI lines have correct prompt indices
        let first_ai = result
            .lines
            .iter()
            .find(|l| l.content == "first AI")
            .unwrap();
        // first AI appears in edit 0's output and edit 1's output, later wins
        assert_eq!(first_ai.prompt_index, Some(1));

        let second_ai = result
            .lines
            .iter()
            .find(|l| l.content == "second AI")
            .unwrap();
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
    fn test_diff_unmapped_lines_still_attributed_correctly() {
        // This test verifies that lines existing in BOTH original and AI output are Original,
        // while lines only in AI output are AI.
        let original = "fn foo() {\n    old_code();\n}\n";
        let mut history = FileEditHistory::new("test.rs", Some(original));

        // AI rewrites the function with new content
        // "fn foo() {" and "}" exist in both original and AI output
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

        // Lines in both original AND AI output → Original (unchanged)
        // "fn foo() {" and "}" are in both → 2 Original
        // "new_code();" and "more_code();" are only in AI → 2 AI
        assert_eq!(result.summary.ai_lines, 2, "2 lines only in AI output");
        assert_eq!(result.summary.human_lines, 0, "No human lines expected");
        assert_eq!(
            result.summary.original_lines, 2,
            "2 lines unchanged from original (fn foo and closing brace)"
        );

        // Verify the closing brace is Original (exists in both original and AI)
        let closing_brace = result.lines.iter().find(|l| l.content == "}").unwrap();
        assert!(
            matches!(closing_brace.source, LineSource::Original),
            "Closing brace should be Original (exists in both), got {:?}",
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
            println!(
                "  Line {}: {:?} - '{}'",
                line.line_number, line.source, line.content
            );
        }

        // line1, line2, line4, line5: Original (unchanged from original)
        // LINE3_MODIFIED: AI (this line was changed by AI)
        // line6, line7, line8: AI (new lines added by AI)
        assert_eq!(result.summary.ai_lines, 4, "4 lines actually changed by AI");
        assert_eq!(
            result.summary.original_lines, 4,
            "4 lines unchanged from original"
        );
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
            println!(
                "  Line {}: {} - '{}'",
                line.line_number, source_str, line.content
            );
        }

        // Lines 1-3 (fn foo, code(), }) exist in BOTH original and AI → Original
        // Lines 4-7 (empty, fn bar, more_code(), }) are only in AI → AI
        assert_eq!(
            result.summary.human_lines, 0,
            "No human lines - all are either original or AI"
        );
        assert_eq!(
            result.summary.original_lines, 3,
            "3 lines unchanged from original (fn foo, code, first closing brace)"
        );
        assert_eq!(
            result.summary.ai_lines, 4,
            "4 lines added by AI (empty, fn bar, more_code, second closing brace)"
        );

        // The closing brace "}" appears twice:
        // - Line 3: from original (exists in both) → Original
        // - Line 7: added by AI (only in AI output) → AI
        let closing_braces: Vec<_> = result.lines.iter().filter(|l| l.content == "}").collect();
        assert_eq!(closing_braces.len(), 2, "Should have 2 closing braces");

        // First closing brace (line 3) should be Original
        assert!(
            matches!(closing_braces[0].source, LineSource::Original),
            "First closing brace (line {}) should be Original, got {:?}",
            closing_braces[0].line_number,
            closing_braces[0].source
        );

        // Second closing brace (line 7) should be AI
        assert!(
            matches!(closing_braces[1].source, LineSource::AI { .. }),
            "Second closing brace (line {}) should be AI, got {:?}",
            closing_braces[1].line_number,
            closing_braces[1].source
        );
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

    #[test]
    fn test_block_matching_reformatted_method_chain() {
        // Test that a method chain split by rustfmt is still attributed to AI
        let original = "";
        let mut history = FileEditHistory::new("test.rs", Some(original));

        // AI generates a single-line method chain
        let ai_output = "let result = foo.bar().baz().qux().unwrap();\n";
        history.add_edit(AIEdit::new(
            "Generate code",
            0,
            "Write",
            original,
            ai_output,
        ));

        // But the committed code has it split by rustfmt
        let final_content = r#"let result = foo
    .bar()
    .baz()
    .qux()
    .unwrap();
"#;

        let result = ThreeWayAnalyzer::analyze_with_diff(&history, final_content);

        println!("\nBlock matching test:");
        for line in &result.lines {
            let source_str = match &line.source {
                LineSource::AI { .. } => "AI",
                LineSource::Human => "Human",
                _ => "Other",
            };
            println!(
                "  Line {}: {} - '{}'",
                line.line_number, source_str, line.content
            );
        }

        // All lines should be attributed to AI via block matching
        assert_eq!(
            result.summary.human_lines, 0,
            "No human lines - block matching should attribute all to AI"
        );
        assert!(
            result.summary.ai_lines >= 4,
            "Most lines should be AI (got {})",
            result.summary.ai_lines
        );
    }

    #[test]
    fn test_block_matching_split_assignment() {
        // Test an assignment split across two lines
        let original = "";
        let mut history = FileEditHistory::new("test.rs", Some(original));

        // AI generates a single-line assignment
        let ai_output =
            "let commit_time = DateTime::from_timestamp(commit.time().seconds(), 0).unwrap();\n";
        history.add_edit(AIEdit::new(
            "Generate code",
            0,
            "Write",
            original,
            ai_output,
        ));

        // But rustfmt splits it
        let final_content = r#"let commit_time =
    DateTime::from_timestamp(commit.time().seconds(), 0).unwrap();
"#;

        let result = ThreeWayAnalyzer::analyze_with_diff(&history, final_content);

        println!("\nSplit assignment test:");
        for line in &result.lines {
            let source_str = match &line.source {
                LineSource::AI { .. } => "AI",
                LineSource::Human => "Human",
                LineSource::Original => "Original",
                LineSource::AIModified { .. } => "AIModified",
                LineSource::Unknown => "Unknown",
            };
            println!(
                "  Line {}: {} - '{}'",
                line.line_number, source_str, line.content
            );
        }

        // Both lines should be AI
        assert_eq!(
            result.summary.human_lines, 0,
            "Both lines should be AI via block matching"
        );
        assert_eq!(result.summary.ai_lines, 2, "Both lines should be AI");
    }

    #[test]
    fn test_block_matching_closure_formatting() {
        // Test a closure that gets reformatted across multiple lines
        let original = "";
        let mut history = FileEditHistory::new("test.rs", Some(original));

        // AI generates compact closure
        let ai_output = ".map(|t| { t.with_timezone(&Utc).format(\"%Y-%m-%d\").to_string() })\n";
        history.add_edit(AIEdit::new(
            "Generate code",
            0,
            "Write",
            original,
            ai_output,
        ));

        // Rustfmt expands it
        let final_content = r#".map(|t| {
    t.with_timezone(&Utc)
        .format("%Y-%m-%d")
        .to_string()
})
"#;

        let result = ThreeWayAnalyzer::analyze_with_diff(&history, final_content);

        println!("\nClosure formatting test:");
        for line in &result.lines {
            let source_str = match &line.source {
                LineSource::AI { .. } => "AI",
                LineSource::Human => "Human",
                _ => "Other",
            };
            println!(
                "  Line {}: {} - '{}'",
                line.line_number, source_str, line.content
            );
        }

        // All lines should be AI
        assert_eq!(
            result.summary.human_lines, 0,
            "All lines should be AI via block matching"
        );
    }

    #[test]
    fn test_block_matching_ok_or_else_chain() {
        // Test the pattern seen in setup.rs where rustfmt splits ok_or_else chains
        let original = "";
        let mut history = FileEditHistory::new("test.rs", Some(original));

        // AI generates a single-line ok_or_else call
        let ai_output = r#"let hooks_dir = claude_hooks_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
"#;
        history.add_edit(AIEdit::new(
            "Generate code",
            0,
            "Write",
            original,
            ai_output,
        ));

        // rustfmt splits it into two lines
        let final_content = r#"let hooks_dir =
    claude_hooks_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
"#;

        let result = ThreeWayAnalyzer::analyze_with_diff(&history, final_content);

        println!("\nok_or_else chain test:");
        for line in &result.lines {
            let source_str = match &line.source {
                LineSource::AI { .. } => "AI",
                LineSource::Human => "Human",
                LineSource::AIModified { similarity, .. } => {
                    &format!("AIModified({:.2})", similarity)
                }
                _ => "Other",
            };
            println!(
                "  Line {}: {} - '{}'",
                line.line_number, source_str, line.content
            );
        }

        // Both lines should be AI via block matching
        assert_eq!(
            result.summary.human_lines, 0,
            "Both lines should be AI via block matching"
        );
        assert_eq!(result.summary.ai_lines, 2, "Both lines should be AI");
    }

    #[test]
    fn test_block_matching_sync_all_context() {
        // Test pattern from audit.rs: file.sync_all().context(...)?
        let original = "";
        let mut history = FileEditHistory::new("test.rs", Some(original));

        // AI generates single line
        let ai_output = r#"file.sync_all().context("Failed to sync audit log to disk")?;
"#;
        history.add_edit(AIEdit::new(
            "Generate code",
            0,
            "Write",
            original,
            ai_output,
        ));

        // rustfmt splits it
        let final_content = r#"file.sync_all()
    .context("Failed to sync audit log to disk")?;
"#;

        let result = ThreeWayAnalyzer::analyze_with_diff(&history, final_content);

        println!("\nsync_all().context() test:");
        for line in &result.lines {
            let source_str = match &line.source {
                LineSource::AI { .. } => "AI",
                LineSource::Human => "Human",
                LineSource::AIModified { similarity, .. } => {
                    &format!("AIModified({:.2})", similarity)
                }
                _ => "Other",
            };
            println!(
                "  Line {}: {} - '{}'",
                line.line_number, source_str, line.content
            );
        }

        // Both lines should be AI
        assert_eq!(
            result.summary.human_lines, 0,
            "Both lines should be AI via block matching"
        );
        assert_eq!(result.summary.ai_lines, 2, "Both lines should be AI");
    }

    #[test]
    fn test_block_matching_assert_multiline() {
        // Test pattern from redaction.rs: assert!() split across multiple lines
        let original = "";
        let mut history = FileEditHistory::new("test.rs", Some(original));

        // AI generates single-line assert
        let ai_output = r#"assert!(names.len() >= 22, "Expected at least 22 builtin patterns, got {}", names.len());
"#;
        history.add_edit(AIEdit::new(
            "Generate code",
            0,
            "Write",
            original,
            ai_output,
        ));

        // rustfmt splits it across multiple lines
        let final_content = r#"assert!(
    names.len() >= 22,
    "Expected at least 22 builtin patterns, got {}",
    names.len()
);
"#;

        let result = ThreeWayAnalyzer::analyze_with_diff(&history, final_content);

        println!("\nmultiline assert test:");
        for line in &result.lines {
            let source_str = match &line.source {
                LineSource::AI { .. } => "AI",
                LineSource::Human => "Human",
                LineSource::AIModified { similarity, .. } => {
                    &format!("AIModified({:.2})", similarity)
                }
                _ => "Other",
            };
            println!(
                "  Line {}: {} - '{}'",
                line.line_number, source_str, line.content
            );
        }

        // All 5 lines should be AI
        assert_eq!(
            result.summary.human_lines, 0,
            "All lines should be AI via block matching"
        );
        assert_eq!(result.summary.ai_lines, 5, "All 5 lines should be AI");
    }
}
