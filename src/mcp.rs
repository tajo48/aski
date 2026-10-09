//! MCP server (`aski serve`): exposes the `ask_user` tool over stdio.
//!
//! Each tool call spawns a short-lived `aski popup` subprocess that renders the
//! question as a Catppuccin-styled Wayland window and prints the answer as one
//! JSON line on stdout.

use crate::spec::{Answer, AnswerStatus, Flavor, PromptOption, QuestionSpec};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::Parameters;
use rmcp::handler::server::wrapper::Json;
use rmcp::model::{
    ErrorData as McpError, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::time::Duration;

use rmcp::schemars;

pub struct AskServer {
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AskUserParams {
    /// The full question shown to the user. Make it self-contained; put trade-off
    /// explanations into option descriptions rather than the question text.
    pub question: String,
    /// Short UI label for the popup (recommended max 12 characters), e.g. "Database".
    #[serde(default)]
    pub header: Option<String>,
    /// 2-8 predefined answer options, each with a short `label` and an optional
    /// `description` of its trade-offs.
    pub options: Vec<PromptOption>,
    /// Allow the user to tick multiple options before confirming.
    #[serde(default)]
    pub multi_select: bool,
    /// Seconds to wait before the popup auto-cancels. Defaults to 300.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Catppuccin flavor: latte, frappe, macchiato or mocha. Defaults to mocha.
    #[serde(default)]
    pub flavor: Option<Flavor>,
    /// Catppuccin accent color name: rosewater, flamingo, pink, mauve, red, maroon,
    /// peach, yellow, green, teal, sky, sapphire, blue or lavender. Defaults to mauve.
    #[serde(default)]
    pub accent: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct AskUserOutput {
    /// "answered", "cancelled", "timeout" or "error"
    pub status: String,
    /// Selected option labels (or the free-form answer) when status is "answered".
    pub selections: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[tool_router]
impl AskServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Block and ask the user a question through a GUI popup, then continue.
    #[tool(
        description = "Ask the user a question through an interactive GUI popup (Catppuccin-styled Wayland window) and wait for their answer. \
Use it whenever a decision has multiple valid options, when proceeding would be destructive or hard to reverse, \
or when the right choice depends on the user's preferences that are not recoverable from the repository. \
Do NOT use it for trivial choices the user has already answered. \
The popup shows the question, 2-8 predefined options (single-select answers immediately; multi_select shows checkboxes plus a Confirm button) \
and a free-form 'Other' input. The user can also close the window (status \"cancelled\") or let the timeout elapse (status \"timeout\"). \
If the user does not answer, pick the safest option yourself and state that assumption clearly."
    )]
    pub async fn ask_user(
        &self,
        Parameters(params): Parameters<AskUserParams>,
    ) -> Result<Json<AskUserOutput>, McpError> {
        if params.question.trim().is_empty() {
            return Err(McpError::invalid_params("question must not be empty", None));
        }
        if params.options.len() < 2 || params.options.len() > 8 {
            return Err(McpError::invalid_params(
                "options must contain between 2 and 8 entries",
                None,
            ));
        }
        for opt in &params.options {
            if opt.label.trim().is_empty() {
                return Err(McpError::invalid_params(
                    "every option needs a non-empty label",
                    None,
                ));
            }
        }
        let timeout_secs = params.timeout_secs.unwrap_or(300).clamp(1, 86_400);
        let header = params
            .header
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .map(|h| h.chars().take(16).collect::<String>());

        let spec = QuestionSpec {
            question: params.question,
            header,
            options: params.options,
            multi_select: params.multi_select,
            timeout_secs: Some(timeout_secs),
            flavor: params.flavor,
            accent: params.accent,
        };

        run_popup_process(spec, timeout_secs).await.map(Json)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AskServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Interactive human-in-the-loop questions as GUI popups. Call `ask_user` when a \
                 decision belongs to the user; the popup lives only for the duration of the question."
                    .into(),
            ),
        }
    }
}

async fn run_popup_process(
    spec: QuestionSpec,
    timeout_secs: u64,
) -> Result<AskUserOutput, McpError> {
    let exe = std::env::current_exe()
        .map_err(|e| McpError::internal_error(format!("cannot locate aski binary: {e}"), None))?;
    let mut child = tokio::process::Command::new(exe)
        .arg("popup")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // If we bail out (hard timeout), take the popup down with us.
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| McpError::internal_error(format!("failed to launch popup: {e}"), None))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let line = serde_json::to_string(&spec)
            .map_err(|e| McpError::internal_error(format!("cannot serialize question: {e}"), None))?;
        let _ = stdin.write_all(line.as_bytes()).await;
        let _ = stdin.write_all(b"\n").await;
        let _ = stdin.flush().await;
        // Dropping stdin signals EOF so the popup proceeds even if parsing lagged.
    }

    // The popup enforces the question timeout itself; this is only a safety net
    // in case the GUI hangs.
    let hard_deadline = Duration::from_secs(timeout_secs.saturating_add(30));
    let output = match tokio::time::timeout(hard_deadline, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Err(McpError::internal_error(
                format!("popup process failed: {e}"),
                None,
            ));
        }
        Err(_) => {
            return Ok(AskUserOutput {
                status: "timeout".into(),
                selections: vec![],
                detail: Some("popup was force-killed after the hard deadline".into()),
            });
        }
    };

    let stderr = String::from_utf8_lossy(&output.stderr);
    let answer: Answer = serde_json::from_slice(&output.stdout).map_err(|e| {
        McpError::internal_error(
            format!(
                "popup produced no valid answer ({e}); stderr: {}",
                stderr.lines().last().unwrap_or("")
            ),
            None,
        )
    })?;

    let status = match answer.status {
        AnswerStatus::Answered => "answered",
        AnswerStatus::Cancelled => "cancelled",
        AnswerStatus::Timeout => "timeout",
        AnswerStatus::Error => "error",
    };

    Ok(AskUserOutput {
        status: status.into(),
        selections: answer.selections,
        detail: answer.error,
    })
}

pub async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let server = AskServer::new();
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}
