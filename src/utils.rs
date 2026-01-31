//! Shared utility functions and constants

/// Length of prompt preview in summaries
pub const PROMPT_PREVIEW_LEN: usize = 60;

/// Number of bytes to use from SHA256 hash for content hashing
pub const CONTENT_HASH_BYTES: usize = 16;

/// Number of bytes to use from SHA256 hash for diff hashing
pub const DIFF_HASH_BYTES: usize = 8;

/// Short commit ID length
pub const SHORT_COMMIT_LEN: usize = 7;

/// Truncate a string to max length, adding "..." if truncated
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

/// Truncate with trimming (for prompts)
pub fn truncate_prompt(text: &str, max_len: usize) -> String {
    let trimmed = text.trim();
    truncate(trimmed, max_len)
}

/// Pad a string to exactly the given length (truncate or add spaces)
pub fn pad_right(s: &str, len: usize) -> String {
    if s.len() >= len {
        s[..len].to_string()
    } else {
        format!("{}{}", s, " ".repeat(len - s.len()))
    }
}

/// Truncate or pad to exact length, using ellipsis for truncation
pub fn truncate_or_pad(s: &str, len: usize) -> String {
    if s.len() > len {
        format!("{}…", &s[..len - 1])
    } else {
        format!("{:<width$}", s, width = len)
    }
}

/// Word wrap text to a maximum width
pub fn word_wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() <= width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Hex encoding utilities
pub mod hex {
    /// Encode bytes as hex string
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hi", 2), "hi");
        assert_eq!(truncate("abc", 3), "abc");
    }

    #[test]
    fn test_truncate_prompt() {
        assert_eq!(truncate_prompt("  hello  ", 10), "hello");
        assert_eq!(truncate_prompt("  long text here  ", 8), "long ...");
    }

    #[test]
    fn test_pad_right() {
        assert_eq!(pad_right("hi", 5), "hi   ");
        assert_eq!(pad_right("hello", 3), "hel");
        assert_eq!(pad_right("abc", 3), "abc");
    }

    #[test]
    fn test_truncate_or_pad() {
        assert_eq!(truncate_or_pad("hi", 5), "hi   ");
        assert_eq!(truncate_or_pad("hello world", 5), "hell…");
    }

    #[test]
    fn test_word_wrap() {
        let lines = word_wrap("hello world foo bar", 10);
        assert_eq!(lines, vec!["hello", "world foo", "bar"]);

        let empty = word_wrap("", 10);
        assert_eq!(empty, vec![""]);
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex::encode(&[0x00, 0xff, 0x10]), "00ff10");
        assert_eq!(hex::encode(&[]), "");
    }
}
