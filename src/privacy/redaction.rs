use regex::Regex;

/// Redaction placeholder
const REDACTED: &str = "[REDACTED]";

/// Default redaction patterns
pub mod patterns {
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
}

/// Privacy redactor for sensitive data in prompts
#[derive(Clone)]
pub struct Redactor {
    patterns: Vec<Regex>,
}

impl Redactor {
    /// Create a redactor with custom patterns
    pub fn new(pattern_strings: &[&str]) -> Self {
        let patterns = pattern_strings
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        Self { patterns }
    }

    /// Create a redactor with default security patterns
    pub fn default_patterns() -> Self {
        Self::new(&[
            patterns::API_KEY,
            patterns::EMAIL,
            patterns::PASSWORD,
            patterns::AWS_KEY,
            patterns::PRIVATE_KEY,
            patterns::BEARER_TOKEN,
            patterns::GITHUB_TOKEN,
            patterns::GENERIC_SECRET,
        ])
    }

    /// Create a redactor with no patterns (no redaction)
    pub fn none() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Add a custom pattern
    pub fn add_pattern(&mut self, pattern: &str) -> Result<(), regex::Error> {
        let regex = Regex::new(pattern)?;
        self.patterns.push(regex);
        Ok(())
    }

    /// Redact sensitive data from text
    pub fn redact(&self, text: &str) -> String {
        let mut result = text.to_string();

        for pattern in &self.patterns {
            result = pattern.replace_all(&result, REDACTED).to_string();
        }

        result
    }

    /// Check if text contains sensitive data
    pub fn contains_sensitive(&self, text: &str) -> bool {
        self.patterns.iter().any(|p| p.is_match(text))
    }

    /// Get list of matches in text (for debugging/preview)
    pub fn find_sensitive(&self, text: &str) -> Vec<String> {
        self.patterns
            .iter()
            .flat_map(|p| p.find_iter(text).map(|m| m.as_str().to_string()))
            .collect()
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
}
