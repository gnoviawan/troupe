/**
 * Terminal Intelligence System - Rust Backend
 *
 * This module provides AI-powered analysis of terminal commands, including:
 * - Command signature extraction and semantic fingerprinting
 * - Command execution analysis (success/failure, duration, output)
 * - Pattern detection across command sequences
 * - Output parsing and summarization
 * - Performance metrics and analytics
 *
 * This is part of the Troupe Terminal Intelligence System that enables
 * developers to better understand and optimize their command workflows.
 */

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::Utc;
use log::{info, debug, warn};

/// Represents the analysis of a single command execution
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
pub struct CommandAnalysis {
    pub id: String,
    pub terminal_id: String,
    pub project_id: String,
    pub command: String,
    pub signature: CommandSignature,
    pub exit_code: Option<i32>,
    pub duration: u64,
    pub timestamp: i64,
    pub output: String,
    pub output_summary: Option<String>,
    pub succeeded: bool,
    pub tags: Vec<String>,
    pub related_commands: Vec<String>,
    pub context: CommandContext,
}

/// Semantic fingerprint of a command for matching and analysis
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
pub struct CommandSignature {
    /// Normalized command without arguments (e.g., 'git commit', 'npm run')
    pub command: String,
    /// Hash of the command intent (for deduplication)
    pub intent_hash: String,
    /// Semantic tokens for search indexing
    pub semantic_tokens: Vec<String>,
    /// Command category
    pub category: CommandCategory,
    /// Confidence score (0-1)
    pub category_confidence: f32,
}

/// Command categories for classification
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(crate = "serde", rename_all = "lowercase")]
pub enum CommandCategory {
    Git,
    Build,
    Test,
    Dev,
    Deploy,
    Other,
}

impl CommandCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Git => "git",
            Self::Build => "build",
            Self::Test => "test",
            Self::Dev => "dev",
            Self::Deploy => "deploy",
            Self::Other => "other",
        }
    }
}

impl std::fmt::Display for CommandCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Context information for command execution
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(crate = "serde")]
pub struct CommandContext {
    pub branch: Option<String>,
    pub cwd: Option<String>,
    pub git_status: Option<String>,
}

/// Pattern detected across multiple command executions
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(crate = "serde")]
pub struct CommandPattern {
    pub id: String,
    pub commands: Vec<CommandSignature>,
    pub frequency: u32,
    pub success_rate: f32,
    pub average_duration: u64,
    pub name: Option<String>,
    pub confidence: f32,
}

/// Command analyzer for semantic analysis and pattern detection
pub struct CommandAnalyzer {
    command_history: Vec<CommandAnalysis>,
    patterns: Vec<CommandPattern>,
}

impl CommandAnalyzer {
    pub fn new() -> Self {
        Self {
            command_history: Vec::new(),
            patterns: Vec::new(),
        }
    }

    /// Extract command signature from raw command string
    pub fn extract_signature(&self, command: &str) -> CommandSignature {
        let normalized = self.normalize_command(command);
        let category = self.classify_command(&normalized);
        let semantic_tokens = self.extract_semantic_tokens(&normalized);
        let intent_hash = self.compute_intent_hash(&normalized);

        CommandSignature {
            command: normalized.clone(),
            intent_hash,
            semantic_tokens,
            category: category.0,
            category_confidence: category.1,
        }
    }

    /// Analyze command execution result
    pub fn analyze_execution(
        &mut self,
        command: &str,
        exit_code: Option<i32>,
        output: &str,
        duration: u64,
        terminal_id: &str,
        project_id: &str,
        context: CommandContext,
    ) -> CommandAnalysis {
        let signature = self.extract_signature(command);
        let succeeded = exit_code.map_or(false, |code| code == 0);
        let output_summary = self.summarize_output(output, succeeded);
        let tags = self.extract_tags(&signature, output, succeeded);
        let related_commands = self.find_related_commands(&signature);

        let analysis = CommandAnalysis {
            id: uuid::Uuid::new_v4().to_string(),
            terminal_id: terminal_id.to_string(),
            project_id: project_id.to_string(),
            command: command.to_string(),
            signature,
            exit_code,
            duration,
            timestamp: Utc::now().timestamp_millis(),
            output: output.to_string(),
            output_summary,
            succeeded,
            tags,
            related_commands,
            context,
        };

        self.command_history.push(analysis.clone());
        info!("Analyzed command: {} ({}ms, {})", command, duration,
            if succeeded { "success" } else { "failed" });

        analysis
    }

    /// Detect patterns in command history
    pub fn detect_patterns(&mut self) -> Vec<CommandPattern> {
        self.patterns.clear();

        // Look for sequences of 2-5 commands that repeat
        for window_size in 2..=5 {
            let mut sequence_counts: HashMap<Vec<String>, Vec<usize>> = HashMap::new();

            for window in self.command_history.windows(window_size) {
                let sequence: Vec<String> =
                    window.iter().map(|c| c.signature.command.clone()).collect();
                sequence_counts.entry(sequence).or_insert_with(Vec::new);
                sequence_counts.get_mut(&sequence.to_vec()).unwrap().push(0);
            }

            // Convert to patterns
            for (sequence, occurrences) in sequence_counts.into_iter() {
                if occurrences.len() >= 2 {
                    if let Some(pattern) = self.create_pattern_from_sequence(&sequence) {
                        self.patterns.push(pattern);
                    }
                }
            }
        }

        // Sort by frequency
        self.patterns.sort_by(|a, b| b.frequency.cmp(&a.frequency));

        debug!("Detected {} patterns", self.patterns.len());
        self.patterns.clone()
    }

    /// Normalize command for comparison
    fn normalize_command(&self, command: &str) -> String {
        // Extract base command and common flags
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return String::new();
        }

        // For git commands, preserve main command + subcommand
        if parts[0] == "git" && parts.len() > 1 {
            return format!("{} {}", parts[0], parts[1]);
        }

        // For npm/yarn, preserve script name
        if (parts[0] == "npm" || parts[0] == "yarn") && parts.len() > 1 && parts[1] == "run" && parts.len() > 2 {
            return format!("{} {}", parts[0], parts[2]);
        }

        // Default: use first word
        parts[0].to_string()
    }

    /// Classify command into categories
    fn classify_command(&self, command: &str) -> (CommandCategory, f32) {
        let lower = command.to_lowercase();

        let (category, confidence) = match () {
            _ if lower.starts_with("git") => (CommandCategory::Git, 0.95),
            _ if lower.contains("build") || lower.contains("compile") => (CommandCategory::Build, 0.9),
            _ if lower.contains("test") || lower.contains("jest") || lower.contains("pytest") => {
                (CommandCategory::Test, 0.9)
            }
            _ if lower.contains("dev") || lower.contains("start") || lower.contains("watch") => {
                (CommandCategory::Dev, 0.85)
            }
            _ if lower.contains("deploy") || lower.contains("publish") || lower.contains("push") => {
                (CommandCategory::Deploy, 0.85)
            }
            _ => (CommandCategory::Other, 0.5),
        };

        (category, confidence)
    }

    /// Extract semantic tokens for search indexing
    fn extract_semantic_tokens(&self, command: &str) -> Vec<String> {
        let mut tokens = Vec::new();

        for part in command.split_whitespace() {
            tokens.push(part.to_lowercase());

            // Add subcommands as separate tokens
            if part.contains('-') {
                for subpart in part.split('-') {
                    if !subpart.is_empty() {
                        tokens.push(subpart.to_lowercase());
                    }
                }
            }
        }

        tokens.sort();
        tokens.dedup();
        tokens
    }

    /// Compute intent hash for deduplication
    fn compute_intent_hash(&self, command: &str) -> String {
        // Simple hash based on normalized command
        format!("{:x}", fxhash::hash64(&command))
    }

    /// Summarize output based on success/failure
    fn summarize_output(&self, output: &str, succeeded: bool) -> Option<String> {
        let lines: Vec<&str> = output.lines().collect();

        if lines.is_empty() {
            return None;
        }

        // Look for error patterns
        if !succeeded {
            for line in lines.iter().rev() {
                if line.to_lowercase().contains("error")
                    || line.to_lowercase().contains("failed")
                    || line.to_lowercase().contains("fail") {
                    return Some(line.to_string());
                }
            }
        }

        // Return last non-empty line
        for line in lines.iter().rev() {
            if !line.trim().is_empty() {
                return Some(line.to_string());
            }
        }

        None
    }

    /// Extract tags from command and output
    fn extract_tags(&self, signature: &CommandSignature, output: &str, succeeded: bool) -> Vec<String> {
        let mut tags = vec![signature.category.as_str().to_string()];

        if !succeeded {
            tags.push("failed".to_string());
        }

        // Look for performance issues
        let lower_output = output.to_lowercase();
        if lower_output.contains("warning") {
            tags.push("warning".to_string());
        }
        if lower_output.contains("deprecated") {
            tags.push("deprecated".to_string());
        }

        tags
    }

    /// Find commands related to this command
    fn find_related_commands(&self, signature: &CommandSignature) -> Vec<String> {
        self.command_history
            .iter()
            .filter(|cmd| {
                cmd.signature.category == signature.category
                    && cmd.signature.command != signature.command
            })
            .take(5)
            .map(|cmd| cmd.id.clone())
            .collect()
    }

    /// Create pattern from sequence of commands
    fn create_pattern_from_sequence(&self, sequence: &[String]) -> Option<CommandPattern> {
        // Find all occurrences of this sequence
        let mut occurrences = Vec::new();

        for window in self.command_history.windows(sequence.len()) {
            let window_commands: Vec<&str> = window.iter().map(|c| c.command.as_str()).collect();
            if window_commands == sequence {
                occurrences.push(window);
            }
        }

        if occurrences.is_empty() {
            return None;
        }

        // Calculate statistics
        let success_count = occurrences.iter().filter(|w| w.iter().all(|c| c.succeeded)).count();
        let success_rate = success_count as f32 / occurrences.len() as f32;
        let average_duration = occurrences.iter()
            .flat_map(|w| w.iter().map(|c| c.duration))
            .sum::<u64>() / occurrences.len() as u64;

        let signatures: Vec<CommandSignature> = sequence
            .iter()
            .map(|cmd| self.extract_signature(cmd))
            .collect();

        Some(CommandPattern {
            id: uuid::Uuid::new_v4().to_string(),
            commands: signatures,
            frequency: occurrences.len() as u32,
            success_rate,
            average_duration,
            name: None,
            confidence: (occurrences.len() as f32 / 10.0).min(1.0),
        })
    }

    pub fn get_history(&self) -> &[CommandAnalysis] {
        &self.command_history
    }

    pub fn get_patterns(&self) -> &[CommandPattern] {
        &self.patterns
    }
}

impl Default for CommandAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_command() {
        let analyzer = CommandAnalyzer::new();
        assert_eq!(analyzer.normalize_command("git commit -m 'test'"), "git commit");
        assert_eq!(analyzer.normalize_command("npm run build"), "npm build");
        assert_eq!(analyzer.normalize_command("ls -la"), "ls");
    }

    #[test]
    fn test_classify_command() {
        let analyzer = CommandAnalyzer::new();
        assert_eq!(analyzer.classify_command("git commit").0, CommandCategory::Git);
        assert_eq!(analyzer.classify_command("npm run build").0, CommandCategory::Build);
        assert_eq!(analyzer.classify_command("npm test").0, CommandCategory::Test);
    }

    #[test]
    fn test_extract_signature() {
        let analyzer = CommandAnalyzer::new();
        let sig = analyzer.extract_signature("git commit -m 'test'");
        assert!(sig.semantic_tokens.contains(&"git".to_string()));
        assert_eq!(sig.category, CommandCategory::Git);
    }
}
