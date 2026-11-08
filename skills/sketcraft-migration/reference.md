# SketchUp parity: where each SketchUp idea lives in SketCraft

Companion to `SKILL.md`. Facts here describe the tree today; update them in the PR that changes them.

## How the tree got here

1. KetchUp: a SketchUp-style UI on `sketcraft-core`, two crates, no parametric history.
2. The upstream SolveCraft fork was brought back as the app: `apps/sketcraft`, `crates/ui-egui` and the engine crates replaced `crates/sketcraft-ui`. `sketcraft-core` stayed only for SKP import, then became `crates/skp`.
3. SketchUp-first workspace on that core: the SketchUp ribbon tab with the Large Tool Set, `sketchup_draw`, `sketchup_screen`, the SketchUp defaults, Use SketchUp Keys.
4. Renames: the fork's name → SketCraft everywhere visible; `solidcore` → `sketcore` as Cargo `package =` aliases; `SolveApp` → `SketApp`. The fork's own file tags and the 3D mouse crates were removed. The browser build moved to Trunk.
5. The SKP reader became pure Rust on every target; the vendored OpenSKP C++ reader stays on desktop as the oracle for `crates/skp/src/parity.rs`.

## Tool map

| SketchUp | SketCraft command | Key | Notes |
|---|---|---|---|
| Select | `select.clear` (clear) | Space | Selection itself is the viewport's |
| Eraser | `sketch.erase` (click or drag across sketch curves); `selection.delete` | E deletes the selection | Erases sketch curves, not model edges |
| Paint Bucket | `appearance.paint` | B | |
| Line | `sketch.line` | L | First click picks the plane on the SketchUp tab |
| Arc (3 point) | `sketch.arc.three_point` | A | |
| Rectangle | `sketch.rectangle.two_point`, `sketch.rectangle.three_point` | R | |
| Circle | `sketch.circle.center` | C | |
| Polygon | `sketch.polygon.inscribed` | | |
| Pie | `sketch.arc.pie` | | The arc and both radius lines on shared points |
| Freehand | `sketch.freehand` | | Press and drag; the samples are thinned and fitted with one spline |
| Push/Pull | `solid.press_pull` | P | Finishes a SketchUp-tab sketch first (`MODEL_TOOLS`); double-click a flat face repeats the last distance (`Prefs::press_pull_distance`, `ui-egui/src/press_pull_repeat.rs`) |
| Offset | `solid.offset_face` (tab), `sketch.offset` (key) | F | |
| Move | `solid.move` | M | |
| Scale | `solid.scale` | | S stays the S box |
| Rotate | `solid.rotate` (opens Move/Copy on its Rotate type) | Q | A Move feature turned about an axis through a point |
| Follow Me | `solid.sweep` | | |
| Tape Measure | `inspect.measure` | T | |
| Dimension, Text | `sketch.dimension`, `sketch.text` | | |
| Section Plane | `inspect.section` | | |
| Make Component | `component.create` | G | Groups and components are both SketCraft components |
| Tags (Layers) | `tags.manage` | | The Tags & Layers panel |
| Orbit, Pan, Zoom, Zoom Extents | `view.tool.orbit`, `view.tool.pan`, `view.tool.zoom`, `view.fit` | O, H, Z, Shift+Z | |
| Zoom Window, Previous, Next | `view.zoom_window`, `view.previous`, `view.next` | | Camera history lives in `ui-egui/src/view_history.rs`, per design tab |
| Import `.skp` | `file.insert_mesh` through `crates/io/src/skp_in.rs` | | Mesh bodies in a component named after the file |

The tab's `Section`s in `crates/ui-egui/src/workspace.rs` and `SKETCHUP_LARGE_TOOLSET` in `crates/ui-egui/src/toolbar_custom.rs` are the source of truth for the first two columns; `SKETCHUP_KEYS` in `crates/ui-egui/src/keymap.rs` for the third.

Inference while drawing (parallel, perpendicular, extension, tangent, centre) is `crates/ui-egui/src/inference.rs`; placing the point adds the matching constraint.

## Defaults the SketchUp workspace sets

Perspective; no grid; Browser and timeline hidden; ground shadow off; tool icon at the cursor on; the sky-and-ground model screen (`sketchup_screen`, Preferences › Layout); Surface, Assemble, Sheet Metal and Weldments tabs hidden (`HIDDEN_BY_DEFAULT`) until shown from the toolbar menu or Preferences › Layout › Toolbar tabs. Every one of them is a preference, so a user can go back to the classic CAD workspace.

## What stays from the CAD core, and why

- The B-rep kernel (`crates/kernel` over `sketcore`), the sketch solver and constraints: they are what Push/Pull, Offset and inference run on.
- Components, tags, materials, undo, autosave and recovery: SketchUp has the same ideas under other names.
- The command registry and schemas: every SketchUp tool is a command, so the CLI, control channel and MCP drive it too.
- The CAD-only tabs and their commands (joints, motion, sheet metal, weldments, parameters, drawings): hidden, not removed. Removing one is a whole-subsystem change (see `SKILL.md`).

## Open items

The phased work list is `docs/sketchup-migration-plan.md`.

- Rotate uses the Move/Copy dialog, not SketchUp's protractor cursor.
- Styles beyond the default (edge profiles, sketchy edges) and SketchUp's shadow settings are partial; shadows have date, time and latitude.
- `docs/controls.md` and `docs/sidebar.md` mix KetchUp-era descriptions (left toolbar, tool flyouts) with the current ribbon; the SketchUp-tab paragraph in `docs/controls.md` is current.
- `cargo xtask skills` expects a router table in `AGENTS.md` and reports stale doc paths in older skills; the routing table lives in `skills/README.md` in this tree.
