//! Shared wire types between the MCP server (`aski serve`) and the popup process (`aski popup`).

use serde::{Deserialize, Serialize};

use rmcp::schemars;

/// A single predefined answer option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PromptOption {
    /// Short label, shown as the button title.
    pub label: String,
    /// Longer explanation of the trade-offs of this option.
    #[serde(default)]
    pub description: Option<String>,
}

/// Catppuccin flavor for the popup theme.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
    Latte,
    Frappe,
    Macchiato,
    #[default]
    Mocha,
}

/// Full description of a question, sent over stdin to `aski popup` as one JSON line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionSpec {
    pub question: String,
    #[serde(default)]
    pub header: Option<String>,
    pub options: Vec<PromptOption>,
    #[serde(default)]
    pub multi_select: bool,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub flavor: Option<Flavor>,
    #[serde(default)]
    pub accent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerStatus {
    Answered,
    Cancelled,
    Timeout,
    Error,
}

/// What the popup writes back to stdout as one JSON line, then exits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Answer {
    pub status: AnswerStatus,
    #[serde(default)]
    pub selections: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
