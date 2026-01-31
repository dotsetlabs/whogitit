use regex::Regex;
use serde::{Deserialize, Serialize};

/// Redaction placeholder
const REDACTED: &str = "[REDACTED]";

/// Named pattern definition
#[derive(Debug, Clone)]
pub struct NamedPattern {
    pub name: &'static str,
    pub pattern: &'static str,
    pub description: &'static str,
}

/// Default redaction patterns with names for audit trails
pub mod patterns {
    use super::NamedPattern;

    /// API key pattern: matches api_key, api-key, apikey, secret, token followed by = or :
    pub const API_KEY: &str = r"(?i)(api[_-]?key|secret|token)\s*[:=]\s*\S+";

    /// Email pattern
    pub const EMAIL: &str = r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}";

    /// Password pattern: matches password, passwd, pwd followed by = or :
    pub const PASSWORD: &str = r"(?i)(password|passwd|pwd)\s*[:=]\s*\S+";

    /// AWS key pattern
    pub const AWS_KEY: &str =
        r"(?i)(aws[_-]?)?(access[_-]?key[_-]?id|secret[_-]?access[_-]?key)\s*[:=]\s*\S+";

    /// Private key header
    pub const PRIVATE_KEY: &str = r"-----BEGIN\s+(?:RSA\s+)?PRIVATE\s+KEY-----";

    /// Bearer token
    pub const BEARER_TOKEN: &str = r"(?i)bearer\s+[a-zA-Z0-9_\-\.]+";

    /// GitHub token pattern
    pub const GITHUB_TOKEN: &str = r"gh[pousr]_[A-Za-z0-9_]{36,}";

    /// Generic secret assignment
    pub const GENERIC_SECRET: &str =
        r#"(?i)["']?(?:secret|private|credential)[_-]?(?:key)?["']?\s*[:=]\s*["']?[^"'\s]+"#;

    // === NEW PII PATTERNS ===

    /// Social Security Number pattern (US format)
    pub const SSN: &str = r"\b\d{3}-\d{2}-\d{4}\b";

    /// Credit card number pattern (major card types)
    pub const CREDIT_CARD: &str = r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12})\b";

    /// Phone number pattern (US format with optional +1)
    pub const PHONE: &str = r"\b(?:\+1[-.\s]?)?\(?[0-9]{3}\)?[-.\s]?[0-9]{3}[-.\s]?[0-9]{4}\b";

    /// Database connection string pattern
    pub const DB_CONNECTION: &str =
        r"(?i)(?:mysql|postgres|postgresql|mongodb|redis|mssql|mariadb)://[^\s]+";

    /// Slack token pattern
    pub const SLACK_TOKEN: &str = r"xox[baprs]-[0-9A-Za-z\-]+";

    /// Stripe API key pattern
    pub const STRIPE_KEY: &str = r"(?:sk|pk|rk)_(?:live|test)_[0-9a-zA-Z]{24,}";

    /// All builtin patterns with names
    pub const ALL_NAMED: &[NamedPattern] = &[
        NamedPattern {
            name: "API_KEY",
            pattern: API_KEY,
            description: "API keys and tokens (api_key=, secret=, token=)",
        },
        NamedPattern {
            name: "EMAIL",
            pattern: EMAIL,
            description: "Email addresses",
        },
        NamedPattern {
            name: "PASSWORD",
            pattern: PASSWORD,
            description: "Password assignments (password=, pwd=)",
        },
        NamedPattern {
            name: "AWS_KEY",
            pattern: AWS_KEY,
            description: "AWS access keys and secret keys",
        },
        NamedPattern {
            name: "PRIVATE_KEY",
            pattern: PRIVATE_KEY,
            description: "PEM private key headers",
        },
        NamedPattern {
            name: "BEARER_TOKEN",
            pattern: BEARER_TOKEN,
            description: "Bearer authentication tokens",
        },
        NamedPattern {
            name: "GITHUB_TOKEN",
            pattern: GITHUB_TOKEN,
            description: "GitHub personal access tokens",
        },
        NamedPattern {
            name: "GENERIC_SECRET",
            pattern: GENERIC_SECRET,
            description: "Generic secret/credential assignments",
        },
        NamedPattern {
            name: "SSN",
            pattern: SSN,
            description: "US Social Security Numbers",
        },
        NamedPattern {
            name: "CREDIT_CARD",
            pattern: CREDIT_CARD,
            description: "Credit card numbers (Visa, MC, Amex, Discover)",
        },
        NamedPattern {
            name: "PHONE",
            pattern: PHONE,
            description: "US phone numbers",
        },
        NamedPattern {
            name: "DB_CONNECTION",
            pattern: DB_CONNECTION,
            description: "Database connection strings",
        },
        NamedPattern {
            name: "SLACK_TOKEN",
            pattern: SLACK_TOKEN,
            description: "Slack API tokens",
        },
        NamedPattern {
            name: "STRIPE_KEY",
            pattern: STRIPE_KEY,
            description: "Stripe API keys",
        },
    ];
}

/// A redaction event for audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionEvent {
    /// Name of the pattern that matched
    pub pattern_name: String,
    /// Character range in original text (start, end)
    pub char_range: (usize, usize),
    /// ISO8601 timestamp
    pub timestamp: String,
    /// Preview of what was matched (first 10 chars, then ...)
    pub preview: String,
}

/// Result of redaction with audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionResult {
    /// Redacted text
    pub text: String,
    /// Audit events for each redaction
    pub events: Vec<RedactionEvent>,
    /// Total number of redactions
    pub redaction_count: usize,
}

/// Compiled pattern with name for tracking
#[derive(Clone)]
struct CompiledPattern {
    name: String,
    regex: Regex,
}

/// Privacy redactor for sensitive data in prompts
#[derive(Clone)]
pub struct Redactor {
    patterns: Vec<CompiledPattern>,
}

impl Redactor {
    /// Create a redactor with custom patterns (unnamed)
    pub fn new(pattern_strings: &[&str]) -> Self {
        let patterns = pattern_strings
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                Regex::new(p).ok().map(|regex| CompiledPattern {
                    name: format!("CUSTOM_{}", i),
                    regex,
                })
            })
            .collect();

        Self { patterns }
    }

    /// Create a redactor with named patterns
    pub fn with_named_patterns(named_patterns: &[(String, String)]) -> Self {
        let patterns = named_patterns
            .iter()
            .filter_map(|(name, pattern)| {
                Regex::new(pattern).ok().map(|regex| CompiledPattern {
                    name: name.clone(),
                    regex,
                })
            })
            .collect();

        Self { patterns }
    }

    /// Create a redactor with all default security patterns
    pub fn default_patterns() -> Self {
        let named: Vec<(String, String)> = patterns::ALL_NAMED
            .iter()
            .map(|np| (np.name.to_string(), np.pattern.to_string()))
            .collect();

        Self::with_named_patterns(&named)
    }

    /// Create a redactor with specific builtin patterns by name
    pub fn with_builtins(names: &[&str]) -> Self {
        let named: Vec<(String, String)> = patterns::ALL_NAMED
            .iter()
            .filter(|np| names.contains(&np.name))
            .map(|np| (np.name.to_string(), np.pattern.to_string()))
            .collect();

        Self::with_named_patterns(&named)
    }

    /// Create a redactor with all builtins except specified names
    pub fn without_builtins(excluded_names: &[&str]) -> Self {
        let named: Vec<(String, String)> = patterns::ALL_NAMED
            .iter()
            .filter(|np| !excluded_names.contains(&np.name))
            .map(|np| (np.name.to_string(), np.pattern.to_string()))
            .collect();

        Self::with_named_patterns(&named)
    }

    /// Create a redactor with no patterns (no redaction)
    pub fn none() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Add a custom pattern with a name
    pub fn add_named_pattern(&mut self, name: &str, pattern: &str) -> Result<(), regex::Error> {
        let regex = Regex::new(pattern)?;
        self.patterns.push(CompiledPattern {
            name: name.to_string(),
            regex,
        });
        Ok(())
    }

    /// Add a custom pattern (backward compatible)
    pub fn add_pattern(&mut self, pattern: &str) -> Result<(), regex::Error> {
        let name = format!("CUSTOM_{}", self.patterns.len());
        self.add_named_pattern(&name, pattern)
    }

    /// Redact sensitive data from text
    pub fn redact(&self, text: &str) -> String {
        let mut result = text.to_string();

        for cp in &self.patterns {
            result = cp.regex.replace_all(&result, REDACTED).to_string();
        }

        result
    }

    /// Redact sensitive data with full audit trail
    pub fn redact_with_audit(&self, text: &str) -> RedactionResult {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let mut events = Vec::new();

        // Collect all matches first
        for cp in &self.patterns {
            for m in cp.regex.find_iter(text) {
                let matched = m.as_str();
                let preview = if matched.len() > 10 {
                    format!("{}...", &matched[..10])
                } else {
                    matched.to_string()
                };

                events.push(RedactionEvent {
                    pattern_name: cp.name.clone(),
                    char_range: (m.start(), m.end()),
                    timestamp: timestamp.clone(),
                    preview,
                });
            }
        }

        // Sort by position for deterministic output
        events.sort_by_key(|e| e.char_range.0);

        let redaction_count = events.len();
        let text = self.redact(text);

        RedactionResult {
            text,
            events,
            redaction_count,
        }
    }

    /// Check if text contains sensitive data
    pub fn contains_sensitive(&self, text: &str) -> bool {
        self.patterns.iter().any(|cp| cp.regex.is_match(text))
    }

    /// Get list of matches in text (for debugging/preview)
    pub fn find_sensitive(&self, text: &str) -> Vec<String> {
        self.patterns
            .iter()
            .flat_map(|cp| cp.regex.find_iter(text).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// Get list of matches with pattern names
    pub fn find_sensitive_named(&self, text: &str) -> Vec<(String, String)> {
        self.patterns
            .iter()
            .flat_map(|cp| {
                cp.regex
                    .find_iter(text)
                    .map(|m| (cp.name.clone(), m.as_str().to_string()))
            })
            .collect()
    }

    /// Get names of all loaded patterns
    pub fn pattern_names(&self) -> Vec<&str> {
        self.patterns.iter().map(|cp| cp.name.as_str()).collect()
    }

    /// Get count of loaded patterns
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Self::default_patterns()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_redaction() {
        let redactor = Redactor::default_patterns();

        let input = "Use api_key = sk-1234567890abcdef for auth";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("sk-1234567890abcdef"));

        let input2 = "Set SECRET: my_secret_value";
        let output2 = redactor.redact(input2);
        assert!(output2.contains(REDACTED));
    }

    #[test]
    fn test_email_redaction() {
        let redactor = Redactor::default_patterns();

        let input = "Send to user@example.com for review";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("user@example.com"));
    }

    #[test]
    fn test_password_redaction() {
        let redactor = Redactor::default_patterns();

        let input = "password = super_secret_123";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("super_secret_123"));

        let input2 = "Use PWD: mypassword";
        let output2 = redactor.redact(input2);
        assert!(output2.contains(REDACTED));
    }

    #[test]
    fn test_aws_key_redaction() {
        let redactor = Redactor::default_patterns();

        let input = "AWS_ACCESS_KEY_ID = AKIAIOSFODNN7EXAMPLE";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_bearer_token_redaction() {
        let redactor = Redactor::default_patterns();

        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    }

    #[test]
    fn test_github_token_redaction() {
        let redactor = Redactor::default_patterns();

        let input = "GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
    }

    #[test]
    fn test_private_key_redaction() {
        let redactor = Redactor::default_patterns();

        let input = "Use this key:\n-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBg...";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
    }

    #[test]
    fn test_no_false_positives() {
        let redactor = Redactor::default_patterns();

        // Normal text should not be redacted
        let input = "Add error handling to the API endpoint";
        let output = redactor.redact(input);
        assert_eq!(input, output);

        let input2 = "The token count is 42";
        let output2 = redactor.redact(input2);
        assert_eq!(input2, output2);
    }

    #[test]
    fn test_multiple_redactions() {
        let redactor = Redactor::default_patterns();

        let input = "api_key = abc123 and email user@test.com with password = secret";
        let output = redactor.redact(input);

        // Should redact all three
        assert!(!output.contains("abc123"));
        assert!(!output.contains("user@test.com"));
        assert!(!output.contains("secret"));
        assert_eq!(output.matches(REDACTED).count(), 3);
    }

    #[test]
    fn test_contains_sensitive() {
        let redactor = Redactor::default_patterns();

        assert!(redactor.contains_sensitive("api_key = secret"));
        assert!(redactor.contains_sensitive("email: test@example.com"));
        assert!(!redactor.contains_sensitive("normal text here"));
    }

    #[test]
    fn test_find_sensitive() {
        let redactor = Redactor::default_patterns();

        let matches = redactor.find_sensitive("api_key = secret123 and user@test.com");
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_custom_pattern() {
        let mut redactor = Redactor::none();
        redactor.add_pattern(r"SSN:\s*\d{3}-\d{2}-\d{4}").unwrap();

        let input = "Customer SSN: 123-45-6789";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("123-45-6789"));
    }

    #[test]
    fn test_no_redaction() {
        let redactor = Redactor::none();

        let input = "api_key = secret123";
        let output = redactor.redact(input);
        assert_eq!(input, output);
    }

    // === NEW PATTERN TESTS ===

    #[test]
    fn test_ssn_redaction() {
        let redactor = Redactor::default_patterns();

        let input = "Customer SSN is 123-45-6789";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("123-45-6789"));
    }

    #[test]
    fn test_credit_card_redaction() {
        let redactor = Redactor::default_patterns();

        // Visa
        let input = "Card: 4111111111111111";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("4111111111111111"));

        // Mastercard
        let input2 = "Pay with 5500000000000004";
        let output2 = redactor.redact(input2);
        assert!(output2.contains(REDACTED));

        // Amex
        let input3 = "Amex: 378282246310005";
        let output3 = redactor.redact(input3);
        assert!(output3.contains(REDACTED));
    }

    #[test]
    fn test_phone_redaction() {
        let redactor = Redactor::default_patterns();

        let input = "Call me at 555-123-4567";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("555-123-4567"));

        let input2 = "Phone: +1 (555) 123-4567";
        let output2 = redactor.redact(input2);
        assert!(output2.contains(REDACTED));

        let input3 = "Contact: 555.123.4567";
        let output3 = redactor.redact(input3);
        assert!(output3.contains(REDACTED));
    }

    #[test]
    fn test_db_connection_redaction() {
        let redactor = Redactor::default_patterns();

        let input = "DATABASE_URL=postgres://user:pass@localhost:5432/db";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("postgres://"));

        let input2 = "Use mysql://root:secret@db.example.com/mydb";
        let output2 = redactor.redact(input2);
        assert!(output2.contains(REDACTED));

        let input3 = "mongodb://admin:password@cluster.mongodb.net/app";
        let output3 = redactor.redact(input3);
        assert!(output3.contains(REDACTED));
    }

    #[test]
    fn test_slack_token_redaction() {
        let redactor = Redactor::default_patterns();

        // Use obviously fake token format that matches pattern but won't trigger secret scanning
        let input = "SLACK_TOKEN=xoxb-FAKE-TOKEN-TEST";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("xoxb-"));

        let input2 = "Use xoxp-FAKE-TEST for user";
        let output2 = redactor.redact(input2);
        assert!(output2.contains(REDACTED));
    }

    #[test]
    fn test_stripe_key_redaction() {
        let redactor = Redactor::default_patterns();

        // Build test strings at runtime to avoid GitHub secret scanning false positives
        let prefix = "sk_live_";
        let suffix = "0".repeat(24); // 24 zeros - matches pattern but clearly not a real key
        let input = format!("STRIPE_KEY={}{}", prefix, suffix);
        let output = redactor.redact(&input);
        assert!(output.contains(REDACTED));
        assert!(!output.contains("sk_live_"));

        let prefix2 = "pk_test_";
        let input2 = format!("Test with {}{}", prefix2, suffix);
        let output2 = redactor.redact(&input2);
        assert!(output2.contains(REDACTED));
    }

    // === AUDIT TRAIL TESTS ===

    #[test]
    fn test_redact_with_audit() {
        let redactor = Redactor::default_patterns();

        let input = "api_key = secret123 and email user@test.com";
        let result = redactor.redact_with_audit(input);

        assert!(result.text.contains(REDACTED));
        assert_eq!(result.redaction_count, 2);
        assert_eq!(result.events.len(), 2);

        // Check that events have pattern names
        let pattern_names: Vec<_> = result
            .events
            .iter()
            .map(|e| e.pattern_name.as_str())
            .collect();
        assert!(pattern_names.contains(&"API_KEY"));
        assert!(pattern_names.contains(&"EMAIL"));
    }

    #[test]
    fn test_redact_with_audit_no_matches() {
        let redactor = Redactor::default_patterns();

        let input = "Normal text without sensitive data";
        let result = redactor.redact_with_audit(input);

        assert_eq!(result.text, input);
        assert_eq!(result.redaction_count, 0);
        assert!(result.events.is_empty());
    }

    #[test]
    fn test_find_sensitive_named() {
        let redactor = Redactor::default_patterns();

        let matches = redactor.find_sensitive_named("api_key = secret123 and 123-45-6789");
        assert!(!matches.is_empty());

        let names: Vec<_> = matches.iter().map(|(name, _)| name.as_str()).collect();
        assert!(names.contains(&"API_KEY"));
        assert!(names.contains(&"SSN"));
    }

    #[test]
    fn test_with_builtins() {
        let redactor = Redactor::with_builtins(&["EMAIL", "SSN"]);

        // Should redact email
        let input1 = "Email: test@example.com";
        let output1 = redactor.redact(input1);
        assert!(output1.contains(REDACTED));

        // Should redact SSN
        let input2 = "SSN: 123-45-6789";
        let output2 = redactor.redact(input2);
        assert!(output2.contains(REDACTED));

        // Should NOT redact API keys (not included)
        let input3 = "api_key = secret123";
        let output3 = redactor.redact(input3);
        assert_eq!(input3, output3);
    }

    #[test]
    fn test_without_builtins() {
        let redactor = Redactor::without_builtins(&["EMAIL"]);

        // Should NOT redact email (excluded)
        let input1 = "Email: test@example.com";
        let output1 = redactor.redact(input1);
        assert_eq!(input1, output1);

        // Should still redact API keys
        let input2 = "api_key = secret123";
        let output2 = redactor.redact(input2);
        assert!(output2.contains(REDACTED));
    }

    #[test]
    fn test_pattern_names() {
        let redactor = Redactor::default_patterns();
        let names = redactor.pattern_names();

        assert!(names.contains(&"API_KEY"));
        assert!(names.contains(&"EMAIL"));
        assert!(names.contains(&"SSN"));
        assert!(names.contains(&"CREDIT_CARD"));
        assert_eq!(names.len(), 14); // All 14 builtin patterns
    }

    #[test]
    fn test_add_named_pattern() {
        let mut redactor = Redactor::none();
        redactor
            .add_named_pattern("CUSTOM_ID", r"ID-\d{6}")
            .unwrap();

        let input = "Reference ID-123456";
        let output = redactor.redact(input);
        assert!(output.contains(REDACTED));

        let names = redactor.pattern_names();
        assert!(names.contains(&"CUSTOM_ID"));
    }
}
