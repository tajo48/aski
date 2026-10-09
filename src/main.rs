//! aski — interactive question popups for MCP agents.
//!
//! `aski serve`  (default): MCP server over stdio exposing the `ask_user` tool.
//! `aski popup`: internal — reads a QuestionSpec JSON line from stdin, shows the
//!               question as a Catppuccin-styled window, prints the Answer JSON
//!               line to stdout and exits. The window lives only as long as the
//!               question does.

mod gui;
mod mcp;
mod spec;
mod theme;

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("popup") => gui::run_popup(),
        Some("--help") | Some("-h") | Some("help") => {
            println!(
                "aski — interactive question popups for MCP agents\n\n\
                 usage:\n  aski serve    run the MCP server on stdio (default)\n  aski popup    internal: show one question popup (JSON on stdin)"
            );
        }
        _ => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            if let Err(e) = runtime.block_on(mcp::serve()) {
                eprintln!("aski server error: {e}");
                std::process::exit(1);
            }
        }
    }
}
