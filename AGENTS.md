# Persistent options and settings

Every future user-facing option, setting, tool choice, and expandable section throughout the app must remember its last state across reloads and app restarts. This applies to toolbars, menus, dialogs, and panels as well as the right sidebar. Persistence is part of implementing a control, not a separate follow-up task.

- Put user preferences in the serialized `Prefs` in `crates/ui-egui/src/prefs.rs` (stored under `prefs::STORAGE_KEY`; pages live in `crates/ui-egui/src/prefs/`).
- Use the `ui_kit` section helpers in `crates/ui-egui/src/ui_kit/layout.rs` (`section`, `section_count`, `section_method`, `editor_section`, `method_section`) for expandable sections; they keep the open state under the id you pass. For panel headers use `ui_kit::panel::collapsible_header`, and draw disclosures with `ui_kit/disclosure.rs`. Do not add raw `egui::CollapsingHeader`/`CollapsingState` disclosures. Supply a stable, unique id. Ids must not depend on display titles, section order, or docked/floating layout.
- Keep preference loading compatible with older saved data using defaults for added fields. Preserve existing section order and append new sections.
- Keep model-specific state, such as tag visibility and entity assignments, in the document; do not apply those values to unrelated projects.
- Change preferences on the app's `Prefs` so the apps' preference writer picks them up (desktop: `PrefsWriter::store` in `apps/sketcraft/src/main.rs`, which writes and flushes changed prefs each frame, rate-limited; web: `apps/sketcraft-web/src/main.rs`). Preferences must survive an immediate reload without requiring a model edit or waiting for framework autosave.
- Add regression coverage for newly persisted controls, defaults, and restoration where appropriate, including changes saved without a model edit.

# Release workflow

Whenever the user asks to deploy, treat it as a request to push a new version tag and follow this release workflow. Unless the user specifies a version or bump type, increment the patch version.

Whenever the user asks to push a new version tag:

1. Prepare user-facing release notes for the new version in `assets/release-notes.json` first, using the changes since the previous release. Keep releases newest first, preserve older entries, and include the version, release date, title, and categorized changes. This file powers Help → Release notes and the GitHub Release description.
2. Update the workspace version and lockfile as needed. Check release metadata with `python3 scripts/desktop_release.py version --tag vX.Y.Z`, and preview the published text with `python3 scripts/desktop_release.py notes --tag vX.Y.Z`.
3. Do not start local builds, compilation commands, tests, packaging-helper tests, or benchmarks for a deploy request. This includes `cargo build`, `cargo run`, `cargo check`, `cargo test`, and `trunk build`. Desktop and browser builds, tests, and packaging belong in GitHub Actions, using `.github/workflows/build.yml` and `.github/workflows/pages.yml`.
4. Refresh the README screenshot by starting an existing desktop executable directly, without compiling it. Use `python3 -B scripts/capture_readme_desktop.py` (or pass `--binary /path/to/sketcraft`). Capture the example project as a 1920 × 1080 JPEG and update the README image before committing. Do not require a Web build or use a build command as a screenshot fallback. If no desktop executable or capture support is available, report the screenshot prerequisite without starting a build or tests.
5. Commit and push the updated screenshot and README, release notes, version changes, and other changes intended for that release. Deployment sends these changes to GitHub; it must not start another local build or test run.
6. After the release files and screenshot changes have been pushed successfully, create and push the matching version tag. Never push the tag before its release notes and release changes. Report the GitHub Actions status; CI performs the release builds and tests before publishing desktop packages and deploying the website.

# Compilation checks

Trunk output is saved to `/tmp/ketchup-trunk.log` (started with `trunk serve 2>&1 | tee /tmp/ketchup-trunk.log`). Read this file for the compilation check.

Only at the end of each change, immediately before reporting completion, check the currently running SketCraft process, if one exists, and its associated compiler or development-server output for new compilation errors. Perform this check once per fix, not during intermediate edits. Use the existing process and logs; do not start a new build just for this check. If the output is unavailable, report that the check could not be completed.

# Modal actions

In every modal, place the least impactful action (usually Cancel or Close) on the right. Use the shared modal pieces in `crates/ui-egui/src/ui_kit/`: `confirm::Confirm` for confirmations, `container::ModalHost` for other modals, and `actions::footer` for the button row. Do not build modals with raw `egui::Modal` or `egui::Window`. For unsaved changes, the visual order is Discard changes, Save, Cancel. Apply this convention to all future modals.

# Inspector tables

All current and future right-sidebar tables use the shared table in `crates/ui-egui/src/ui_kit/table.rs` (`table_custom` with `Column`s, then `Table::headers` and `Table::row`/`TableRow::cell`) so headers and rows share columns. Tables fill the available width, cells align to consistent column boundaries and are always vertically centered within each row, and inputs fill their cells. Use minimum column widths with the existing persisted horizontal scroll helpers for wide catalogs; do not introduce standalone `egui::Grid` layouts in the inspector.

- Always vertically center the contents of every table cell, including headers, text, values, and controls. This applies to all current and future tables throughout the app. Use the shared table layout’s default centering; never opt a table into top alignment. Wrapped or multiline content must remain centered relative to the tallest cell in its row.

# Project names

- “SketCraft” (formerly “KetchUp” / “Ketchup app”) refers to the existing Rust desktop/WebAssembly client at this repository root.
- “Ketchup Server” refers to the separate PHP repository at `/home/dcoulombe/dev/ketchup-server`. When the user names Ketchup Server, scope the work to that directory and read `/home/dcoulombe/dev/ketchup-server/AGENTS.md`.

# Application language

All user-facing application text must be in English, even when the user requests features or changes in French. This includes section titles, menus, controls, tooltips, notifications, and errors. Use “Automatic Fasteners” for the automatic fastener section. Preserve stable internal preference keys and saved-project compatibility when translating visible text.

# PDF export header

This tree has no PDF export yet (`crates/io/src/paper.rs` writes sheets as SVG; PDF is planned there). When PDF export is added, build one shared header module for every PDF export: the bundled SketCraft logo and brand, report type, project name, generation date, and separator on every page, with a body-top offset that all body content stays below. Preserve page size, orientation, readability and existing transparency. Every later PDF export must reuse it; do not introduce a separate PDF header implementation.
