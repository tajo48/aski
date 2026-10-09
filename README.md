# aski

Interactive **human-in-the-loop questions** for MCP agents: the agent calls the
`ask_user` tool, a **Catppuccin-styled popup window appears on Wayland for exactly
as long as the question is open**, and the answer comes back as the tool result.

Inspired by Claude Code's `AskUserQuestion`, but as a standalone MCP server —
works with Zed, Claude Code, or any MCP client.

## How it works

```text
agent ── ask_user(questions[1-4], …) ──► aski serve (MCP, stdio)
                                             │ spawns
                                             ▼
                                       aski popup (iced / winit → Wayland)
                                             │ JSON line in → q1 → q2 → … → submit
                                             ▼
agent ◄── { "status": "answered", "answers": [{"header": "DB", "selections": ["PostgreSQL"]}, …] } ──┘
```

- One popup process **per tool call** — up to 4 questions asked in sequence, one
  on screen at a time; the window appears on submit/cancel/timeout, never before,
  never after.
- Crash isolation: a GUI panic can never take down the MCP server.
- Single-select: click an option (or move the keyboard cursor with `↑`/`↓`) to
  highlight it, then `Next`/`Enter` commits.
- Multi-select: checkboxes + `Next` (or `Enter`).
- `↑`/`↓` move the option cursor, `Enter` commits and advances, `Esc` cancels.
- Free-form "Other…" input on every question; `Enter` submits it.
- Optional `preview` per option: a panel shows the focused option's code sample,
  mockup or diff so variants can be compared directly.
- Timeout shows a countdown and returns `status: "timeout"`.

## Tool: `ask_user`

Modeled after Claude Code's `AskUserQuestion`: 1–4 questions per call, each with
2–4 options, a free-form "Other" escape hatch, and per-question answers.

| param          | type                 | notes                                          |
| -------------- | -------------------- | ---------------------------------------------- |
| `questions`    | `[{question, header?, options, multi_select?}]` | **1–4** entries     |
| `timeout_secs` | `u64?`               | default `300`                                  |
| `flavor`       | `latte\|frappe\|macchiato\|mocha` | default `frappe`                  |
| `accent`       | `string?`            | mauve, blue, green, red, peach, … default `blue` |

Per question:

| field          | notes                                                |
| -------------- | ---------------------------------------------------- |
| `question`     | required, self-contained                             |
| `header`       | short tab label, **max 12 chars**                    |
| `options`      | **2–4** entries                                      |
| `multi_select` | default `false`                                      |

Per option: `label` (required), `description?` (trade-offs), `preview?`
(code sample / mockup shown while the option is focused).

Answer JSON: `{ "status": "answered" | "cancelled" | "timeout" | "error", "answers": [{"header": "Database", "selections": ["PostgreSQL"]}, …] }` —
plus a convenience `selections` copy when exactly one question was asked.

## Build & install

Cargo (Wayland session needed at runtime):

```sh
cargo build --release
```

Nix:

```sh
nix run github:tajo48/aski   # one-off
nix profile install github:tajo48/aski
```

The flake wraps the binary with `wayland`, `libxkbcommon`, `libGL` and
`vulkan-loader` (what winit/wgpu dlopen at startup), so it works on NixOS out
of the box.

## Hook up to Zed (`settings.json`)

```json
{
  "context_servers": {
    "aski": {
      "command": "/home/amnesia/Rust/packages/aski/target/release/aski",
      "args": ["serve"]
    }
  }
}
```

Or after `nix build`:

```json
{
  "context_servers": {
    "aski": {
      "command": "~/Rust/packages/aski/result/bin/aski",
      "args": ["serve"]
    }
  }
}
```

## Theme

All four Catppuccin flavors with all 14 accents are built in
(`src/theme.rs`, official hex values). Set per call via `flavor`/`accent`, or
change the defaults in `spec.rs`/`theme.rs` (`Flavor::default()` → `Frappe`,
accent fallback → `blue`).
