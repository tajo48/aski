# aski

Interactive **human-in-the-loop questions** for MCP agents: the agent calls the
`ask_user` tool, a **Catppuccin-styled popup window appears on Wayland for exactly
as long as the question is open**, and the answer comes back as the tool result.

Inspired by Claude Code's `AskUserQuestion`, but as a standalone MCP server —
works with Zed, Claude Code, or any MCP client.

## How it works

```text
agent ── ask_user(question, options, …) ──► aski serve (MCP, stdio)
                                              │ spawns
                                              ▼
                                        aski popup (iced / winit → Wayland)
                                              │ JSON line in → popup → JSON line out
                                              ▼
agent ◄── { "status": "answered", "selections": ["chumsky"] } ──┘
```

- One popup process **per question** — the window appears on answer, cancel
  (`Esc` / close), or timeout, never before, never after.
- Crash isolation: a GUI panic can never take down the MCP server.
- Single-select: click an option (or press `1`–`9`) to answer immediately.
- Multi-select: checkboxes + `Confirm` (or `Enter`).
- Free-form "Other…" input in both modes; `Enter` submits it.
- Timeout shows a countdown and returns `status: "timeout"`.

## Tool: `ask_user`

| param          | type                 | notes                                          |
| -------------- | -------------------- | ---------------------------------------------- |
| `question`     | `string`             | required, self-contained                       |
| `header`       | `string?`            | short UI label (≤16 chars)                     |
| `options`      | `[{label, description?}]` | 2–8 entries                               |
| `multi_select` | `bool`               | default `false`                                |
| `timeout_secs` | `u64?`               | default `300`                                  |
| `flavor`       | `latte\|frappe\|macchiato\|mocha` | default `mocha`                   |
| `accent`       | `string?`            | mauve, blue, green, red, peach, … default `mauve` |

Answer JSON: `{ "status": "answered" | "cancelled" | "timeout" | "error", "selections": [...] }`

## Build & install

Cargo (Wayland session needed at runtime):

```sh
cargo build --release
```

Nix:

```sh
nix run .        # not runnable as-is; install instead:
nix build .      # result/bin/aski
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
change the defaults in `spec.rs` (`Flavor::default()` → `Mocha`/`mauve`).
