# Rewrite on an edge/face core: plan

[Documentation](README.md) · Related: [SketchUp migration plan](sketchup-migration-plan.md), `skills/sketcraft-migration/SKILL.md`

Status: proposed (Nov 2026). Nothing in this plan is started. No code is deleted until its phase
says so.

## Why

SketCraft is a SketchUp clone built on a parametric CAD engine: B-rep solids with a feature
timeline (`crates/kernel`, `smlib`, `sketch`, `doc`, `engine`, about 436k lines). SketchUp
models differently: loose edges and faces that glue together, groups and components, and no
history. Every SketchUp tool today is translated into CAD features (Push/Pull is an Extrude,
Rotate is a Move feature, a line is a sketch curve). That makes the CAD engine impossible to
delete, slows the app on large models and keeps SketchUp behaviours (sticky geometry, faces that
appear when edges close, soft edges) out of reach.

The goal: keep the GUI, and replace the CAD engine with an edge/face core built for SketchUp's
way of modelling. Then delete the CAD engine and the CAD-only GUI.

## What exists to build on

**The old edge/face core.** `crates/sketcraft-core` (removed in `0fc9a6c`; full tree at
`cae01a1^`), about 17k lines with 222 tests. It has:
- an indexed polygon mesh: vertices, edges and faces with holes, plus groups, components and tags;
- automatic faces when edges close a loop, with edges splitting at T-junctions;
- Push/Pull (oblique pulls, holes, caps), offset of whole faces, Move, Rotate, Scale, Flip,
  Erase, Paint (RGB only);
- Solid Tools (Outer Shell, Union, Intersect, Trim, Split) by BSP CSG;
- inference (endpoint, midpoint, on edge, on face, axes), picking, box select;
- undo by whole-model diff; linear dimensions; leader text;
- JSON files with versioning.

It lacks:
- arcs and true circles (circles are N loose edges), and soft or smooth edges;
- Follow Me, section planes, real materials, Intersect Faces;
- adjacency data: every lookup scans the whole model, and every operation and undo step copies
  the whole model.

Components are synced world-space copies, not shared definitions. The woodworking and
collaboration code (joinery, stock materials, canopy, attribution) does not belong in a SketchUp
clone.

**Kept from today's tree.** None of these depend on the CAD engine:
- `crates/geom` (vectors, planes, meshes);
- `crates/render` (display data);
- `crates/skp` (the pure-Rust `.skp` reader);
- the GUI's building blocks in `crates/ui-egui`: `ui_kit`, theme, preferences, translations,
  fonts, icons, design tabs, `navigation`, camera and view history, the ribbon;
- the SketchUp-tab tools written for this migration (`rotate_tool`, `tape_tool`,
  `press_pull_repeat`, `sketchup_draw`).

**Tied to the CAD engine today and needing to be untangled:**
- `crates/gpu` depends on `kernel` and `engine`;
- `crates/io` depends on `kernel` and `doc`;
- `crates/mcp` depends on `engine`;
- the GUI reads `session.doc` (the CAD design) about 3,900 times, and 72 files read CAD geometry
  state.

## Approach

Build the new core beside the old engine and switch the SketchUp tab over one tool at a time,
behind a preference, so the app keeps working at every step. Delete the CAD engine last, when
nothing reaches it.

The new core is not the old crate restored as is. It reuses its algorithms (face finding,
Push/Pull, inference, CSG) and tests, but fixes the data structure first.

## Phases

Each phase ends with `cargo xtask ci` green and a release that works.

### Phase 0: checkpoint
Commit and push the pending SketchUp work. Tag the last release on the CAD engine, so it can be
rebuilt to convert old designs.

### Phase 1: the core crate, `crates/mesh`
A new L1 crate, depending only on `geom`.

1. **Data.** Vertices, edges and faces with holes in arenas with stable ids. Adjacency is kept
   up to date: edge → faces, vertex → edges, face → loops. Lookups no longer scan the model.
2. **Edges.** Each edge has curve membership (an arc, circle or polygon keeps its centre,
   radius and segment count, so it can be edited and exported as a curve), and soft, smooth and
   hidden flags.
3. **Components and tags.** Groups are anonymous components. Components are shared definitions
   placed by a transform, as SketchUp stores them, not synced copies. Tags are on entities.
4. **Operations,** ported from the old core with its tests:
   - draw edges, with faces made automatically and edges split where they meet;
   - Push/Pull;
   - Offset, of faces and of edges;
   - Move, Rotate and Scale of vertices, edges and faces, faces folding as SketchUp's do;
   - Erase, Paint, Make Group/Component, Explode.
5. **New operations:**
   - arcs (2-point, 3-point, pie), circles and polygons as curves;
   - Follow Me;
   - Intersect Faces;
   - soften/smooth;
   - section planes (as model state).
6. **Undo.** Each operation records its own change, not a copy of the model.
7. **Hostile input.** Robust geometry predicates and a size cap per operation
   (`skills/sketcraft-crash-safety/SKILL.md`). It builds for wasm32.

Done when the crate passes its ported and new tests, and a 1-million-edge model loads and
Push/Pulls interactively (`skills/sketcraft-performance/SKILL.md`).

### Phase 2: the command layer, `crates/model-engine`
A `Session` over `mesh`: commands, selection, undo, units, autosave and files.

1. **Commands.** The same `CommandSpec` pattern as today's engine, so the toolbar, CLI, control
   channel and MCP keep one path. The ids stay where they make sense (`sketch.line` becomes a
   model-space line, `solid.press_pull` keeps its id), so keymaps, toolbars and preferences
   carry over. It starts with the 31 commands the SketchUp tab uses (the tab's sections in
   `crates/ui-egui/src/workspace.rs`).
2. **File format.** A new `.sketcraft` format: versioned JSON of the mesh, with a pending-format
   file (`skills/sketcraft-file-formats/SKILL.md`).
3. **Imports.** `.skp` import from `crates/skp` straight into the mesh (edges, faces, soft edges,
   groups, components, tags, materials), with no conversion to solids.

Done when the CLI can build, save, reopen and export a model with every SketchUp-tab command.

### Phase 3: the GUI on the new session
1. **Renderer.** Make `crates/gpu` draw from `render`'s display data only, without `kernel` or
   `engine`, so both sessions can feed it.
2. **Switch.** A preference (off by default) runs a design tab on the new session.
3. **Tools.** Move the SketchUp tab's tools over one at a time: picking and inference on the
   mesh, the drawing tools without sketches (`sketchup_draw` goes away), Push/Pull, Move,
   Rotate, Scale, Offset, Follow Me, Eraser, Paint, Tape, Dimension, Text, Section, Make
   Component, Tags.
4. **Panels.** Outliner and Entity Info read the mesh session. The ribbon shows only the SketchUp
   tab and View.

Done when the SketchUp tab works entirely on the new session and the old one is used by nothing
the tab reaches.

### Phase 4: switch over
1. The new session becomes the default for new and opened designs.
2. **Old designs.** `.sketcraft` and `.solvecraft` files with CAD history open through a one-way
   converter. It rebuilds the design with the CAD engine, then turns each body's faces into
   edges and faces, merging coplanar triangles. History is not kept. The converter ships for two
   releases, then becomes a CLI-only tool built from the Phase 0 tag.
3. Release notes say that CAD history is gone.

### Phase 5: delete the CAD engine
In one PR per part, each with `cargo xtask layers` and `cargo xtask ci`, measuring the wasm size
before and after:

| Delete | About | Note |
|---|---|---|
| `crates/kernel`, `crates/smlib`, the `sketcore` dependency | 95k lines + the external kernel | B-rep geometry |
| `crates/sketch` | 32k | Constraint sketches |
| `crates/doc` | 83k | The parametric design and its format (after the converter leaves) |
| `crates/engine`'s CAD commands, then `crates/engine` | 226k | Replaced by `model-engine` |
| `crates/step` and the CAD parts of `crates/io` | about 15k | STEP and CAD formats |
| CAD GUI in `crates/ui-egui` | most of its 372k | Dialogs, timeline, feature browser, CAD tabs, parameters, configurations, sheet metal, weldments, joints, motion |
| Tests | 1,088 kernel cases, oracle cases, the action matrix, CAD scenarios | They test deleted code |

Done when the workspace holds `geom`, `render`, `gpu`, `mesh`, `model-engine`, `skp`, the
mesh-format parts of `io`, `mcp`, `ui-egui` and the apps.

## Decisions for you

1. **Render workspace** (ray-traced images, 55k lines in `render` plus its GUI): keep it on the
   new core, or delete it in Phase 5.
2. **Old designs:** convert them (as in Phase 4), or drop support for them.
3. **Agents and MCP:** keep the MCP server and the agent chat on the new commands, or remove
   them.
4. **File formats** beyond `.skp`: which to keep (glTF, OBJ, STL, 3MF, DXF, Collada) and which
   are new work (Collada export is on the SketchUp migration plan).

## Risks

- **Size.** Phases 1 to 3 are weeks of work. Building beside the old engine keeps the app
  shippable, but both engines live in the tree until Phase 5.
- **Robust geometry.** Edge/face modelling with sticky geometry and automatic faces fails on
  near-coplanar and near-degenerate input unless the predicates are robust. Phase 1 needs a fuzz
  test (as `sketcraft-cli fuzz` does for the CAD engine today) before Phase 3 starts.
- **The GUI's coupling.** The amount of GUI code tied to `session.doc` means much of the GUI
  around the SketchUp tools is rewritten in Phase 3, not reused. The reusable part is the shell
  (`ui_kit`, theme, prefs, ribbon, navigation, viewport rendering), not the CAD dialogs.
- **Lost CAD history** for existing designs. The converter keeps their shape, not their
  features.
