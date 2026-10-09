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
    /// Extra content (code sample, mockup, config diff) shown in a preview panel
    /// while this option is focused, so the user can compare variants directly.
    #[serde(default)]
    pub preview: Option<String>,
}

/// A single question inside a popup run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub question: String,
    #[serde(default)]
    pub header: Option<String>,
    pub options: Vec<PromptOption>,
    #[serde(default)]
    pub multi_select: bool,
}

/// Everything one popup process needs: 1-4 questions asked in sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopupSpec {
    pub questions: Vec<Question>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub flavor: Option<Flavor>,
    #[serde(default)]
    pub accent: Option<String>,
}

/// Catppuccin flavor for the popup theme.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
    Latte,
    #[default]
    Frappe,
    Macchiato,
    Mocha,
}

/// The user's answer to one question.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QuestionAnswer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default)]
    pub selections: Vec<String>,
}

/// What the popup writes back to stdout as one JSON line, then exits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Answer {
    pub status: AnswerStatus,
    #[serde(default)]
    pub answers: Vec<QuestionAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerStatus {
    Answered,
    Cancelled,
    Timeout,
    Error,
}
