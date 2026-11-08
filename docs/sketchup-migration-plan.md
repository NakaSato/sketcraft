# SketchUp alternative: migration plan

[Documentation](README.md) · [Developer README](../README.md) · Rules: `skills/sketcraft-migration/SKILL.md`, parity map: `skills/sketcraft-migration/reference.md`

SketCraft becomes a SketchUp alternative by layering SketchUp's tools, defaults and look over the
parametric SolveCraft core it inherited. The core (B-rep kernel, sketch solver, command registry,
components, undo, autosave) is kept. CAD-only surface is hidden first and removed only as whole
subsystems. This page is the work list; each item follows the migration skill's steps (find the
existing command, add it to the SketchUp tab and `SKETCHUP_LARGE_TOOLSET`, give it SketchUp's key,
keep SketchUp-only behaviour in the front end keyed on the tab, add `help.<id>`, docs and a test).

The longer-term replacement of the CAD engine by an edge/face core is planned in
[edge-face-rewrite-plan.md](edge-face-rewrite-plan.md).

Status words: **done** (in the tree), **partial** (the command exists, the SketchUp feel does not),
**missing** (nothing to map onto yet), **decide** (needs a maintainer decision first).

## Where the tree is today

- done: SketchUp ribbon tab with the Large Tool Set (`crates/ui-egui/src/workspace.rs`), drawing
  without Create Sketch (`sketchup_draw.rs`), the sky-and-ground model screen (`sketchup_screen.rs`),
  Use SketchUp Keys (`keymap.rs`), SketchUp defaults with the CAD tabs hidden, Line inference
  (`crates/ui-egui/src/inference.rs`), pure-Rust `.skp` import on every target (`crates/skp`).
- done: the CAD-only tools SketchUp has no match for (`CAD_ONLY` in `workspace.rs`: Fillet, Shell,
  Hole, Loft, the primitives, patterns and more) are left out of the ribbon, the command search,
  menus and shortcuts unless Preferences › Layout › Show CAD tools is on. They still run for agents,
  MCP and scripts, and features they made stay editable.
- done (Nov 2026): the Surface, Sheet Metal and Weldments tabs are removed with their commands,
  dialogs, strings, tests and kernel cases, and so are joints, motion studies and contact sets
  (the Assemble tab keeps components, positions and exploded views). Their `FeatureKind`s and the
  joint solver stay: a design that holds them opens and rebuilds, but can't edit them.
- done: names. Crates, binaries and types say SketCraft; legacy readers keep `solvecraft.prefs`,
  `solvecraft/N` tags, `SOLVECRAFT_*` env vars, the `.solvecraft` extension and the CLI alias.
- The SketchUp tab uses 31 commands of the registry. The rest is the CAD surface, hidden by tab
  and, for the CAD-only `solid.*` tools, by Show CAD tools.

## Gap analysis

| SketchUp | Status | SketCraft today | Work |
|---|---|---|---|
| Select, Eraser, Paint Bucket | done | `select.*`, `selection.delete`, `sketch.erase` (click or drag across sketch curves), `appearance.paint` | Erasing model edges; Paint Bucket with a Materials tray |
| Line, Arc, Pie, Rectangle, Circle, Polygon | done | `sketch.line`, `sketch.arc.*` (Pie: `sketch.arc.pie`), `sketch.rectangle.two_point` / `three_point`, `sketch.circle.center`, `sketch.polygon.inscribed` | Typed lengths in SketchUp's units syntax |
| Freehand | done | `sketch.freehand`: press and drag, thinned to one fit point spline | Typed stroke length |
| Push/Pull, Offset, Move, Scale, Follow Me | done | `solid.press_pull`, `solid.offset_face`, `solid.move`, `solid.scale`, `solid.sweep` | Move of edges and faces, not only bodies (double-click repeat of the last Push/Pull is done) |
| Rotate (Q) | done | `solid.rotate` (Q): the protractor on the tab (`rotate_tool.rs`), Move/Copy's Rotate type elsewhere | A live preview of the turned bodies while the angle is chosen |
| Tape Measure, Protractor | partial | `inspect.measure` with guide lines and points on the tab (`tape_tool.rs` over `construct.axis.two_points` / `construct.point.vertex`), `view.angle_readout` | The Protractor tool's angled guide lines; guides drawn dashed and infinite; Resize model by typed length |
| Dimensions, Text, 3D Text | partial | `sketch.dimension`, `sketch.text` | Model-space dimensions outside a sketch; 3D Text is missing |
| Section Plane | done | `inspect.section` | Section cuts as scene state |
| Axes tool | missing | none | Move and rotate the drawing axes; store in the design |
| Orbit, Pan, Zoom, Zoom Extents, Zoom Window, Previous/Next | done | `view.tool.*`, `view.fit`, `view.zoom_window`, `view.previous`, `view.next` | Look Around, Walk, Position Camera |
| Inference (endpoint, midpoint, on face, from point, axis lock) | partial | `crates/engine/src/snap.rs`, `inference.rs` (Line only), arrow-key axis locks | One inference engine for every tool on the tab, with SketchUp's colours and labels |
| Groups and Components | done | `component.create` (G), `component.edit_in_place`, `component.activate`, explode via `component.extract` | Group as a lightweight component; Make Unique; component browser |
| Outliner, Entity Info | partial | `browser.*` and the inspector | Present the Browser as Outliner on the SketchUp tab; an Entity Info section |
| Hide, Unhide, Soften/Smooth edges | partial | `browser.visibility` hides bodies | Hide edges and faces; soften/smooth as a render attribute |
| Intersect Faces, Solid Tools | partial | `solid.combine` (join, cut, intersect), `solid.split_body`, `sketch.intersect` | Solid Tools flyout: Outer Shell, Union, Subtract, Trim, Intersect, Split |
| Tags | partial | `tags.manage` opens `tags_panel.rs`; data in document metadata under `sketcraft.tags` | Move tags into the engine: `tags.create`, `tags.assign`, `tags.visibility`, `tags.delete` with a format pending file, undoable; keep the panel as the view |
| Scenes | partial | `view.save`, `view.list`, `view.apply` | Scene tabs under the view with camera, style, tags, section state |
| Styles | partial | `view.visual_style`, `sketchup_screen` | Style presets (edge profiles, back-edge, sketchy edges, background) saved in the design |
| Shadows | done | `view.ground_shadow`, `solar_shadows`, `solar_latitude_deg` | Shadows dialog on the tab: date, time, light and dark sliders, on faces and on ground |
| Materials | done | `appearance.*`, `material.*` | A Materials tray grouped as SketchUp's collections |
| Import `.skp` | done | `file.insert_mesh` → `crates/io/src/skp_in.rs` | Textures, materials, scenes and component definitions from the file |
| Export `.skp` | missing | none; the vendored OpenSKP reader has no writer | decide: write `.skp`, or offer Collada `.dae` as the interchange SketchUp reads |
| Collada `.dae`, `.dwg`/`.dxf` import, images | missing | DXF export of drawing views only; glTF, 3MF, STL, OBJ, PLY import exist | `.dae` import and export first; image import as a textured face |
| Extensions (Ruby API) | decide | the command registry, control channel and MCP server | Expose the registry as the scripting surface; no Ruby |
| 3D Warehouse | removed | none | Model Library folder import stays the answer |

## Phases

### Phase 1: finish the Large Tool Set
Done: Freehand drag, Eraser drag, Pie arc, Zoom Window, Previous and Next view, double-click repeat
on Push/Pull, Rotate (Q) with its protractor, Tape Measure guide lines and points, and SketchUp's default keys: every tool SketchUp gives
a key has it, except Select (Space; no Select command), Scale (S stays the S box), Zoom Window
(Ctrl+Shift+W closes a browser window) and Eraser inside a sketch (E stays on Delete); the parity
map gives the reasons. Left: the Protractor tool (angled guide lines); a live preview while
Rotate's angle is chosen; guides drawn as SketchUp's dashed infinite lines. Every tool follows the five steps in
`skills/sketcraft-migration/SKILL.md`. Done when every entry of SketchUp's Large Tool Set has a
button on the tab, SketchUp's default key where it has one and SketCraft can take it, and
`docs/controls.md` lists it.

### Phase 2: the modelling feel
One inference engine for all tools on the tab, built on `crates/engine/src/snap.rs` and the Line
inference; SketchUp's inference colours and labels; axis locks with Shift and the arrow keys;
Move, Rotate and Scale of edges and faces; hidden edges and faces; soften/smooth; the Solid Tools
flyout over `solid.combine`. Done when the sample model in SketchUp's quick start can be built on
the tab without opening another tab or a sketch dialog.

### Phase 3: organisation
Tags in the engine with a format pending file (`skills/sketcraft-file-formats/SKILL.md`), the panel
reading them; Scenes over `view.save`; Styles over `view.visual_style` and the model screen;
Shadows dialog; Outliner and Entity Info on the tab; Materials tray. Done when a design saved with
tags, scenes and a style opens with them in the last release that read the previous format.

### Phase 4: files
`.skp` import of textures, materials and scenes; Collada `.dae` in and out; image import; a
decision on `.skp` export. Readers follow `skills/sketcraft-crash-safety/SKILL.md` and run in the
browser (no C++). Done when a model round-trips SketCraft → SketchUp → SketCraft through the
chosen interchange with geometry, groups, tags and materials intact.

### Phase 5: retire the CAD surface
Surface, Sheet Metal and Weldments are removed, and Assemble keeps only components, positions
and exploded views (Nov 2026). Per hidden CAD-only tool (`CAD_ONLY`)
and per hidden workspace (Render, the timeline, parameters, drawings): keep hidden, or remove as a whole subsystem (crate, prefs page,
strings, assets, `ATTRIBUTION.md` rows, docs, tests in one PR; then `cargo xtask layers` and
`cargo xtask ci`). Measure the wasm size before and after (`skills/sketcraft-performance/SKILL.md`).
Done when the default download carries only what the SketchUp tab reaches, plus what a user can
show again from the toolbar menu.

#### Removing the CAD-only `solid.*` tools

Group 1 below is removed (Nov 2026); the 17 tools left in `CAD_ONLY` are hidden. They cannot all go at once: Push/Pull runs
the Extrude and Fillet code (`press_pull` in `crates/engine/src/cmd/features.rs`), features
already in a design reopen through their command when edited, and the tests build their models
with them. Counts are lines that name the command (Nov 2026): tests, engine and UI code outside
tests, each including the command's own definition.

| Group | Commands (test lines) | Why this group |
|---|---|---|
| Keep | `extrude` (1286), `fillet` (627), `chamfer` (168), `box` (3456), `cylinder` (775) | Push/Pull extrudes faces and edits fillet and chamfer faces; `box` and `cylinder` build most test models (255 and 55 engine lines). Hidden for good |
| 1: removed | `web`, `replace_face`, `boundary_fill`, `bounding_solid`, `draft`, `pattern.path`, `rib`, `pipe` | Commands, schemas, dialogs, help strings and their tests, oracle cases and kernel cases are gone; their `FeatureKind`s stay, so designs that hold them open and rebuild, but can't be edited. The studio scenes in `examples/studio/coffee-*.json` still call `solid.pipe` and need it rewritten as Follow Me (`solid.sweep`) |
| 2: some coupling | `loft` (140; 65 kernel cases, 31 of them also test fillets, chamfers or extrudes), `torus` (35), `coil` (44), `emboss` (44), `cone` (45), `pattern.circular` (57), `pattern.rectangular` (97), `sphere` (103), `shell` (142), `revolve` (154), `thread` (168), `hole` (338) | Shared dialog code (primitives, patterns, hole presets), the sample design and fuzzing name them |

`solid.remove` is not in the list: it is what Delete runs on a body (`crates/engine/src/cmd/delete.rs`).

Per command, one PR: the `CommandSpec` and its schema contract, its dialog `Kind` and form, its
icon, `cmd.<id>` / `help.<id>` strings and `DOCUMENTED` entry, docs, and its tests (a test that
only used it to build a model moves to `solid.box` or a sketch and Push/Pull). The `FeatureKind`
and the kernel operation stay until the format decision below, so designs that hold the feature
still open and rebuild. Measure the wasm size before and after each group.

Removing a `FeatureKind` itself needs a decision first: a design saved with that feature must
still open. Either it opens with the feature baked into a plain solid (a format pending file,
`skills/sketcraft-file-formats/SKILL.md`), or the feature keeps evaluating but can no longer be
edited. Until then, group 1 and 2 removals take away only the way to make new ones.

### Phase 6: names, docs and checks
Rewrite `docs/controls.md` and `docs/sidebar.md` from the app, not the KetchUp-era text; regenerate
`docs/features.md` with `cargo xtask features`; make `cargo xtask skills` green (a router table for
`AGENTS.md` or a check that reads `skills/README.md`, and the stale doc paths in older skills);
drop a legacy name only when the release that wrote it is two releases back.

## Rules that apply to every item
- The command comes first, in `crates/engine/src/cmd/`; the UI acts through `app.run` / `app.start`.
- SketchUp behaviour is keyed on `app.ui.tab == SKETCHUP_TAB`; the other tabs keep working.
- Every option persists in `Prefs`; model state (tags, scenes, styles, axes) goes in the design
  through a format pending file, never in preferences.
- Tests pin the classic look (`classic_view`, `classic_look`, `classic_app`); only tests about
  defaults expect the SketchUp ones.
- English UI text through the catalogs; `help.<id>` for every tab button.
- `cargo xtask ci` before a PR; a test left failing gets a line in `tests/KNOWN_FAILING_TESTS`.

## Decisions for the maintainer
1. `.skp` export: write the format, or ship Collada as the interchange.
2. Which hidden CAD subsystems are removed in Phase 5, and in what order; for the `solid.*`
   tools, whether a removed feature in an old design is baked into a plain solid or stays
   uneditable.
3. Whether the parametric timeline stays as a hidden power feature or SketchUp-tab edits collapse
   history into direct modelling.
4. Whether groups become a distinct lightweight object or stay components with a flag.
