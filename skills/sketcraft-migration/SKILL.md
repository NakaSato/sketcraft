---
name: sketcraft-migration
description: "Use when moving SketCraft further towards a SketchUp clone on its parametric SolveCraft core: adding a SketchUp tool, key, default or look, mapping a SketchUp concept onto an existing command, hiding or removing a CAD-only feature, or renaming a SolveCraft leftover."
---

# Migrating to the SketchUp clone

SketCraft is a SketchUp-first workspace on the parametric SolveCraft core. The core is not rewritten: SketchUp behaviour is a layer in `crates/ui-egui`, and CAD-only features are hidden before they are removed. `reference.md` maps SketchUp's tools onto SketCraft's commands and lists what is open.

## The SketchUp layer
- Tab: `SKETCHUP_TAB` and its `Section`s in `crates/ui-egui/src/workspace.rs`; the same list is `SKETCHUP_LARGE_TOOLSET` in `crates/ui-egui/src/toolbar_custom.rs`. Its buttons are shortcuts: every command keeps its home tab.
- Drawing without Create Sketch: `crates/ui-egui/src/sketchup_draw.rs`. The first click picks the plane; `MODEL_TOOLS` finish the sketch first.
- Look: `crates/ui-egui/src/sketchup_screen.rs`, behind the `sketchup_screen` preference.
- Keys: `SKETCHUP_KEYS` and `use_sketchup_keys` in `crates/ui-egui/src/keymap.rs`.
- Defaults (perspective, no grid, the `HIDDEN_BY_DEFAULT` CAD tabs): `SketApp::new` and `Prefs`.
- SKP import: `crates/skp` (pure Rust; its C++ oracle is desktop-only) and `crates/io/src/skp_in.rs`.

## Adding a SketchUp tool or behaviour
1. Find the command SketCraft already has (Push/Pull is `solid.press_pull`, Follow Me is `solid.sweep`, Eraser is `selection.delete`). Only when none fits add a `CommandSpec`: `skills/sketcraft-commands/SKILL.md`.
2. Add it to the tab's `Section` and to `SKETCHUP_LARGE_TOOLSET`; give it SketchUp's key in `SKETCHUP_KEYS` when that key is free.
3. Behaviour that differs from the CAD tool goes in the front end, keyed on `app.ui.tab == SKETCHUP_TAB`, never in the command: the other tabs keep working.
4. `help.<id>` in `assets/locales/pending/` (`skills/sketcraft-ui-text/SKILL.md`); the user docs in `docs/controls.md`.
5. A `*_tests.rs` case or scenario. Tests pin the classic look (`classic_view` / `classic_look` in `crates/ui-egui/src/scenario.rs`, `classic_app` in `crates/ui-egui/src/test_support.rs`); only a test about defaults expects the SketchUp ones.

## Hiding and removing CAD features
- Hide first: a tab goes in `HIDDEN_BY_DEFAULT`, anything else behind a preference with the SketchUp default. Users show it again from the toolbar menu.
- Remove only a whole subsystem nothing references: crate, prefs page, strings, assets and `ATTRIBUTION.md` rows, docs and tests in one PR. Then `cargo xtask layers` and `cargo xtask ci`; a test left failing gets a line in `tests/KNOWN_FAILING_TESTS`.
- The kernel, sketch solver, components and tags stay: SketchUp's groups, components and tags are SketCraft's components and Tags & Layers.

## Names
- Visible text, crates, binaries and types say SketCraft (`sketcraft-*`, `SketApp`; `sketcore` for the upstream solidcore packages, whose URLs, licences, issue numbers and cache tag keep `solidcore`).
- Keep what saved data still reads: `LEGACY_STORAGE_KEY`, `LEGACY_FORMAT_PREFIX`, the `SOLVECRAFT_*` fallbacks in `crates/geom/src/env.rs`, the `.solvecraft` extension, `apps/sketcraft-cli/src/alias.rs` and `crates/io/src/legacy_dir.rs`.
- `docs/controls.md` and `docs/sidebar.md` still describe parts of the older KetchUp UI: check a line against the app before citing it.
