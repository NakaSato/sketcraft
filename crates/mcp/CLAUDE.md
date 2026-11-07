# CLAUDE.md — sketcraft-mcp

The MCP server: JSON-RPC 2.0 over stdio (one JSON message per line). Tools map onto control-channel methods (`engine.execute`, `document.inspect`, `ui.render`…). They are served by an in-process headless session (`Headless`) or by a running app through its loopback control port (`Remote`). It is layer L5. Its only normal dependencies are `sketcraft-engine` and `serde_json`; tests also use `sketcraft-gpu` (allowed by `INTRA_LAYER_ORDER` in `xtask/src/layers.rs`). The binary that runs it is `sketcraft-cli mcp` (`apps/sketcraft-cli`). See the root CLAUDE.md.

## Commands

```sh
cargo test -p sketcraft-mcp                                          # all tests (src/tests.rs)
cargo test -p sketcraft-mcp skills_name_only_real_tools_and_commands # bundled skills cite real tools
cargo run -p sketcraft-cli -- mcp --headless                         # run the server on stdio
```

- `SKETCRAFT_MCP_COMPACT=1` (or `--compact`) turns on compact replies. See `src/options.rs`.
- CLI-level MCP tests live in `apps/sketcraft-cli/tests/mcp_*.rs`.

## Layout

- `src/lib.rs`: public API, `agent_scope` (the folders an agent may touch), `scratch_dir` (a private per-process folder for screenshots), `cleanup_scratch_dir`.
- `src/server.rs`: JSON-RPC framing, the MCP lifecycle, tools and resources methods, `PROTOCOL_VERSION`, `SUPPORTED_VERSIONS`.
- `src/tools.rs`: tool definitions (`tool_definitions`) and dispatch (`call_tool`). Each tool is a thin wrapper over engine commands or inspect and render methods.
- `src/backend.rs`: `Backend`, `Headless`, `Remote`, `Auto`: where a tool call ends up.
- `src/options.rs`: output options from the command line, the environment, or the app's preferences.
- `src/skill_reference.rs`: the modelling skills embedded with `include_str!` for the `skill_reference` tool.
- `src/tests.rs`: an agent builds and checks parts through MCP only.

## Rules

- Read `skills/sketcraft-agent-tools/SKILL.md` before changing a tool.
- Anything the user can do is an engine command. Add a command, not a new tool, unless the tool is about the session (open, save, screenshot, batch).
- Tools never open dialogs.
- Keep tool annotations (read-only, destructive) honest.
- File access stays inside the path scope: recovery folder, scratch folder, `--allow-path` folders, and the open design's folder. Do not widen it casually. Paths are resolved before comparing, so `..` and links that leave the scope are refused.
- Keep replies small. Do not return the unfiltered command catalogue or full topology dumps by default.
- Request lines are capped by `sketcraft_engine::MAX_LINE`, shared with the control server.
- The embedded skills are `skills/sketcraft-{checking,editing,mechanisms,modeling}`. When a command or parameter they describe changes, update them in the same change. A new bundled file goes into `src/skill_reference.rs` and into the test above.
- Crash-safety rules apply (the crate root denies `unwrap` and friends): `skills/sketcraft-crash-safety/SKILL.md`.

## Gotchas

- Doc comments cite `docs/mcp.md` and `docs/control-protocol.md`. Neither is in this tree.
