use super::*;

#[test]
fn sketch_create_families_have_icons_and_open_their_variants() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    h.frames(4);
    h.step(&json!({"click":{"handle":"panel:SKETCH:CREATE"}})).unwrap();
    h.frames(4);
    let family = crate::scenario::rect_of("menu:Line").expect("Line is a shared menu row");
    assert!(family.contains(crate::scenario::handle_at("create_family:Line").unwrap()));
    h.step(&json!({"click":{"handle":"create_family:Line"}})).unwrap();
    h.frames(4);
    for id in ["sketch.line", "sketch.line.midpoint"] {
        assert!(crate::scenario::handle_at(&format!("dropdown:{id}")).is_some(), "missing variant {id}");
    }
    h.step(&json!({"click":{"handle":"dropdown:sketch.line.midpoint"}})).unwrap();
    assert_eq!(h.app.tool.as_ref().map(|tool| tool.cmd), Some("sketch.line.midpoint"));
}

/// Plain rows (Point) and family rows (Slot) span the same width, so a plain row's hover
/// background and pin reach the drop-down's right edge.
#[test]
fn sketch_create_plain_and_family_rows_share_both_edges() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    h.frames(4);
    h.step(&json!({"click":{"handle":"panel:SKETCH:CREATE"}})).unwrap();
    h.frames(4);
    let point = crate::scenario::rect_of("menu:sketch.point").expect("Point is a plain row");
    let slot = crate::scenario::rect_of("menu:Slot").expect("Slot is a family row");
    assert!((point.left() - slot.left()).abs() < 0.5, "left edges differ: Point {point:?}, Slot {slot:?}");
    assert!((point.right() - slot.right()).abs() < 0.5, "right edges differ: Point {point:?}, Slot {slot:?}");
}

#[test]
fn sketch_create_family_rows_paint_the_first_variants_icons() {
    let mut h = crate::scenario::Harness::new();
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    let cmds: Vec<_> = sketcraft_engine::command_specs().into_iter().filter(|c| c.tab == "SKETCH" && c.panel == "CREATE").collect();
    crate::icon_svg::audit::start();
    let mut output = h.ctx.run_ui(egui::RawInput::default(), |ui| {
        sketch_create_menu(&mut h.app, ui, &cmds, 200.0, &mut None, &mut None);
    });
    output.textures_delta.clear();
    let requested = crate::icon_svg::audit::take();
    crate::icon_svg::audit::stop();
    for family in crate::sketch_families::FAMILIES {
        let first = sketcraft_engine::find_command(family.commands[0]).unwrap();
        assert!(!first.icon.is_empty());
        assert!(requested.iter().any(|icon| icon.name == first.icon && !icon.placeholder), "{} must paint {}", family.label, first.icon);
    }
}

/// A drop-down taller than the window stops at its bottom and scrolls: the last row is
/// reached with the arrow keys and comes into view.
#[test]
fn a_tall_drop_down_scrolls_inside_a_short_window() {
    for mode in [crate::shell::LayoutMode::Floating, crate::shell::LayoutMode::Docked] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.set", json!({"layout":mode}));
        h.call("ui.resize", json!({"width":1200,"height":320}));
        h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
        h.frames(4);
        h.step(&json!({"click":{"handle":"panel:SKETCH:CREATE"}})).unwrap();
        h.frames(4);
        let menu = crate::scenario::rect_of("toolbar_dropdown").expect("the drop-down is open");
        assert!(menu.bottom() <= 320.0, "the drop-down runs past the window: {menu:?}");
        assert!(crate::scenario::handle_at("dropdown:sketch.text").is_some_and(|p| !menu.contains(p)), "Text starts below the fold");
        // Up from no focus goes to the last enabled row; it scrolls into view.
        h.step(&json!({"key":"ArrowUp"})).unwrap();
        h.frames(10);
        let hot = crate::scenario::handles("dropdown_hot:");
        let [(_, at)] = hot.as_slice() else { panic!("one highlighted row: {hot:?}") };
        let menu = crate::scenario::rect_of("toolbar_dropdown").unwrap();
        assert!(menu.contains(*at), "the focused last row is out of view: {at:?} in {menu:?}");
    }
}

/// Every row of a drop-down is shown when the window has room, even after a shorter list was
/// shown first: another tab's panel of the same name, or the same panel collapsed (#874).
#[test]
fn a_drop_down_shows_all_its_rows_after_a_shorter_one() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1000}));
    let open = |h: &mut crate::scenario::Harness, tab: &str, panel: &str| {
        h.app.ui.tab = tab.into();
        h.frames(4);
        h.step(&json!({"click":{"handle":format!("panel:{tab}:{panel}")}})).unwrap();
        h.frames(6);
        let menu = crate::scenario::rect_of("toolbar_dropdown").expect("the drop-down is open");
        let rows = crate::scenario::handles("dropdown:");
        h.step(&json!({"key":"Escape"})).unwrap();
        h.frames(4);
        (menu, rows)
    };
    let (short, short_rows) = open(&mut h, "MANAGE", "RULES");
    let (menu, rows) = open(&mut h, "MODEL", "MODIFY");
    assert!(rows.len() > short_rows.len(), "MODEL MODIFY lists more than MANAGE RULES: {rows:?} vs {short_rows:?}");
    assert!(menu.height() > short.height(), "the longer list is taller: {menu:?} vs {short:?}");
    for (id, at) in &rows {
        assert!(menu.contains(*at), "{id} at {at:?} is cut off by the drop-down {menu:?}");
    }
}

/// The grid's rows: a heading starts a new one, and the arrow keys move along and between
/// them over the enabled icons only, wrapping at the ends (#4739).
#[test]
fn grid_rows_break_at_headings_and_arrow_keys_skip_disabled_icons() {
    let c = |id| sketcraft_engine::find_command(id).unwrap();
    let grouped = [
        (Some("Primitives"), c("solid.box")),
        (None, c("solid.cylinder")),
        (None, c("solid.sphere")),
        (None, c("solid.torus")),
        (None, c("solid.pipe")),
        (Some("Other"), c("solid.extrude")),
        (None, c("solid.revolve")),
    ];
    // Too narrow for the groups side by side (two icons wide): rows as wide as the window, a
    // heading starting a row.
    let rows = grid_layout(&grouped, &[], 100.0);
    let at = |l: &GridLayout| l.cells.iter().map(|(_, at)| *at).collect::<Vec<_>>();
    assert_eq!(at(&rows), [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (3, 0), (3, 1)]);
    assert_eq!(rows.headings.len(), 2);
    // Room enough: each group a column of two under its heading, the groups side by side.
    let side = grid_layout(&grouped, &[], 1000.0);
    assert_eq!(at(&side), [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (0, 2), (0, 3)]);
    let headings: Vec<_> = side.headings.iter().map(|(p, _, h)| (p.x, *h)).collect();
    assert_eq!(headings, [(0.0, "Primitives"), (2.0 * GRID_CELL, "Other")]);
    assert_eq!(side.size, vec2(4.0 * GRID_CELL, GRID_HEADING + 3.0 * GRID_CELL));
    assert_eq!(side.separators, [2.0 * GRID_CELL], "a line between the two groups");
    assert!(rows.separators.is_empty(), "rows have none");
    assert!(side.cells.iter().all(|(r, _)| r.top() >= GRID_HEADING), "icons below the headings");
    // A menu without groups keeps its rows of five.
    let plain: Vec<_> = grouped.iter().map(|(_, c)| (None, *c)).collect();
    assert_eq!(at(&grid_layout(&plain, &[], 1000.0)), [(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (1, 0), (1, 1)]);
    let cells = at(&side);
    // `solid.pipe` (index 4) is disabled: `enabled` holds the others.
    let enabled = [0, 1, 2, 3, 5, 6];
    let step = |at, key| grid_step(&cells, &enabled, at, key).map(|k: usize| enabled[k]);
    assert_eq!((step(None, GridKey::Right), step(None, GridKey::Down)), (Some(0), Some(0)));
    assert_eq!((step(None, GridKey::Left), step(None, GridKey::Up)), (Some(6), Some(6)));
    assert_eq!(step(Some(2), GridKey::Right), Some(3), "Right runs on into the next row");
    assert_eq!(step(Some(0), GridKey::Left), Some(6), "and wraps");
    assert_eq!(step(Some(1), GridKey::Down), Some(3), "Down goes to the nearest enabled icon below");
    assert_eq!(step(Some(4), GridKey::Down), Some(3), "from the next group's column too");
    assert_eq!(step(Some(2), GridKey::Down), Some(0), "from the last enabled row Down wraps to the first");
    assert_eq!(step(Some(5), GridKey::Up), Some(3), "from the first row Up wraps to the last, nearest column");
    assert_eq!(step(Some(1), GridKey::Up), Some(3));
    assert_eq!(grid_step(&cells, &[], None, GridKey::Down), None);
}

/// A command in no group (Model › Modify's Delete) stands apart after the groups, without a
/// heading: leaving out a pinned group (Specialise's Convert to Sheet Metal) must not move it
/// under the group before (Bodies) (#4739).
#[test]
fn a_command_in_no_group_stays_apart_when_a_group_is_left_out() {
    let specs = sketcraft_engine::command_specs();
    let (cmds, _) = crate::workspace::layout("MODEL", "MODIFY", &specs).unwrap();
    let loose = crate::workspace::ungrouped("MODEL", "MODIFY", &cmds);
    assert_eq!(loose, ["timeline.delete"]);
    let column_of = |cmds: &[&'static sketcraft_engine::CommandSpec], id: &str| {
        let grouped = crate::workspace::grouped_commands("MODEL", "MODIFY", cmds);
        let layout = grid_layout(&grouped, &loose, 4000.0);
        let x = |id: &str| grouped.iter().position(|(_, c)| c.id == id).and_then(|i| layout.cells.get(i)).map(|(r, _)| r.left());
        let last_group = layout.separators.last().copied();
        (x(id), last_group)
    };
    let (delete, separator) = column_of(&cmds, "timeline.delete");
    assert_eq!(delete, separator, "Delete in a column of its own after Specialise");
    let skipped = skip_on_ribbon(cmds.clone(), &["sheet.convert"]);
    let (delete, separator) = column_of(&skipped, "timeline.delete");
    let (remove, _) = column_of(&skipped, "solid.remove");
    assert_eq!(delete, separator, "still apart without Specialise");
    assert!(delete > remove, "not under Bodies: {delete:?} vs {remove:?}");
}

/// Group headings sit below the drop-down's own header in the hierarchy: a smaller font than
/// the header's, and written as named rather than in capitals (list and grid alike).
#[test]
fn group_headings_are_smaller_than_the_drop_down_header() {
    let t = Tokens::get();
    assert!(group_heading_font().size < t.font_caption, "{} vs the header's {}", group_heading_font().size, t.font_caption);
    assert_eq!(group_heading_text("Edges"), "Edges", "written as named, not in capitals");
}

/// Every panel's drop-down of every tab, and Sketch › Create's families, is at most
/// GRID_ROWS high in the grid when the window is wide enough: big menus grow to the right.
#[test]
fn every_grid_menu_is_at_most_three_rows_high() {
    let rows = |l: &GridLayout| l.cells.iter().map(|(r, _)| r.top().round() as i32).collect::<std::collections::BTreeSet<_>>().len();
    let tabs: std::collections::BTreeSet<&str> = crate::workspace::SECTIONS.iter().map(|s| s.tab).collect();
    for tab in tabs {
        for (panel, cmds, _) in crate::toolbar_custom::panel_lists(tab) {
            let grouped = crate::workspace::grouped_commands(tab, panel, &cmds);
            let layout = grid_layout(&grouped, &crate::workspace::ungrouped(tab, panel, &cmds), 4000.0);
            assert!(rows(&layout) <= GRID_ROWS, "{tab} {panel}: {} rows", rows(&layout));
            if tab == "SKETCH" && panel == "CREATE" {
                let families = grid_layout(&sketch_create_grouped(&cmds), &[], 4000.0);
                assert!(rows(&families) <= GRID_ROWS, "Sketch › Create families: {} rows", rows(&families));
            }
        }
    }
    // A window too narrow for the width wraps into more rows instead of running off it.
    let c = |id| sketcraft_engine::find_command(id).unwrap();
    let many: Vec<_> = sketcraft_engine::command_specs().into_iter().take(30).map(|c| (None, c)).collect();
    let narrow = grid_layout(&many, &[], 4.0 * GRID_CELL);
    assert!(narrow.size.x <= 4.0 * GRID_CELL && rows(&narrow) > GRID_ROWS);
    let few = [(None, c("solid.box")), (None, c("solid.sphere"))];
    assert_eq!(grid_layout(&few, &[], 4000.0).size, vec2(2.0 * GRID_CELL, GRID_CELL), "a small menu is one row");
}

/// With `ribbon_icon_grid` on, a panel's drop-down shows the same commands as its list, as
/// large icons in rows of at most five; arrow keys move across and down; a click starts the
/// command, and a drag pins it, as from the list (#4739).
#[test]
fn the_icon_grid_shows_the_lists_commands_and_acts_like_it() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1000}));
    let open = |h: &mut crate::scenario::Harness| {
        h.frames(4);
        h.step(&json!({"click":{"handle":"panel:MODEL:CREATE"}})).unwrap();
        h.frames(6);
        let menu = crate::scenario::rect_of("toolbar_dropdown").expect("the drop-down is open");
        (menu, crate::scenario::handles("dropdown:"))
    };
    let (_, list) = open(&mut h);
    h.step(&json!({"key":"Escape"})).unwrap();
    assert!(!h.app.preferences.ribbon_icon_grid, "the list stays the default");
    h.call("ui.prefs", json!({"ribbon_icon_grid": true}));
    assert!(h.app.preferences.ribbon_icon_grid);
    let (menu, grid) = open(&mut h);
    let ids = |rows: &[(String, egui::Pos2)]| rows.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&grid), ids(&list), "the grid shows the list's commands in its order");
    assert!(grid.len() > GRID_COLUMNS * GRID_ROWS, "MODEL CREATE is a big menu: {grid:?}");
    for (id, at) in &grid {
        assert!(menu.contains(*at), "{id} at {at:?} is outside the drop-down {menu:?}");
    }
    let ys: std::collections::BTreeSet<i32> = grid.iter().map(|(_, at)| at.y.round() as i32).collect();
    assert_eq!(ys.len(), GRID_ROWS, "laid out in {GRID_ROWS} rows, growing to the right: {ys:?}");
    let columns = grid.len().div_ceil(GRID_ROWS);
    assert!(menu.width() <= columns as f32 * GRID_CELL + 40.0, "no wider than its columns: {menu:?}");

    let hot = |h: &mut crate::scenario::Harness, key: &str| {
        h.step(&json!({"key": key})).unwrap();
        h.frames(3);
        let hot = crate::scenario::handles("dropdown_hot:");
        let [(_, at)] = hot.as_slice() else { panic!("one highlighted icon after {key}: {hot:?}") };
        *at
    };
    let first = hot(&mut h, "ArrowRight");
    let second = hot(&mut h, "ArrowRight");
    assert!(second.x > first.x && (second.y - first.y).abs() < 1.0, "Right moves along the row: {first:?} → {second:?}");
    let below = hot(&mut h, "ArrowDown");
    assert!(below.y > second.y, "Down moves to the row below: {second:?} → {below:?}");

    h.step(&json!({"click":{"handle":"dropdown:solid.box"}})).unwrap();
    h.frames(3);
    assert!(crate::scenario::rect_of("toolbar_dropdown").is_none(), "a click closes the drop-down");
    assert!(h.app.dialog.is_some() || h.app.tool.is_some(), "and starts the command");
    h.step(&json!({"key":"Escape"})).unwrap();
    h.frames(3);

    open(&mut h);
    let label = crate::scenario::handle_at("panel:MODEL:CREATE").unwrap();
    h.step(&json!({"drag":[{"handle":"dropdown:solid.cylinder"}, [label.x, label.y]],"steps":12})).unwrap();
    h.step(&json!({"key":"Escape"})).unwrap();
    assert!(crate::toolbar_custom::pinned_on(&h.app, "MODEL", "solid.cylinder").is_some(), "dragged onto the ribbon, it is pinned");
    assert!(h.app.session.doc.features.is_empty(), "pinning does not run the command");
}

/// A right-click on a drop-down's command, in the list and in the grid, opens its menu:
/// Pin to Toolbar, Remove from Toolbar and Pin to Shortcuts act on that command.
#[test]
fn a_right_click_in_the_drop_down_pins_and_unpins() {
    for grid in [false, true] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":1600,"height":1000}));
        h.app.preferences.ribbon_icon_grid = grid;
        let pick = |h: &mut crate::scenario::Harness, item: &str| {
            h.frames(4);
            h.step(&json!({"click":{"handle":"panel:MODEL:CREATE"}})).unwrap();
            h.frames(4);
            h.step(&json!({"click":{"handle":"dropdown:solid.box"},"button":"right"})).unwrap();
            h.frames(4);
            assert!(h.app.viewport.context_menu.is_some(), "grid {grid}: the right-click opens the command's menu");
            h.step(&json!({"widget": item})).unwrap_or_else(|e| panic!("grid {grid}: {item}: {e}"));
            h.frames(4);
        };
        pick(&mut h, "Pin to Toolbar");
        assert!(crate::toolbar_custom::pinned_on(&h.app, "MODEL", "solid.box").is_some(), "grid {grid}: pinned");
        pick(&mut h, "Remove from Toolbar");
        assert!(crate::toolbar_custom::pinned_on(&h.app, "MODEL", "solid.box").is_none(), "grid {grid}: unpinned");
        pick(&mut h, "Pin to Shortcuts");
        assert!(h.app.sbox.pinned.iter().any(|p| p == "solid.box"), "grid {grid}: a shortcut");
        // The pin shown on the pointed-at row or icon pins it, and again unpins it. A grid
        // icon's pin takes clicks only after the pointer has rested on it for PIN_DELAY.
        for pinned in [true, false] {
            h.step(&json!({"click":{"handle":"panel:MODEL:CREATE"}})).unwrap();
            h.frames(4);
            h.step(&json!({"move":{"handle":"dropdown:solid.box"}})).unwrap();
            h.frames(3);
            if grid {
                assert!(crate::scenario::handle_at("dropdown_pin:solid.box").is_none(), "no pin to click straight away");
                h.frames((PIN_DELAY * 60.0) as usize + 5);
            }
            h.step(&json!({"click":{"handle":"dropdown_pin:solid.box"}})).unwrap();
            h.frames(3);
            h.step(&json!({"key":"Escape"})).unwrap();
            assert_eq!(crate::toolbar_custom::pinned_on(&h.app, "MODEL", "solid.box").is_some(), pinned, "grid {grid}: the hover pin");
        }
        assert!(h.app.session.doc.features.is_empty(), "grid {grid}: pinning does not run the command");
    }
}

/// A quick click on a grid icon's corner, where its pin appears after PIN_DELAY, starts the
/// command instead of pinning it.
#[test]
fn a_quick_click_on_the_pin_corner_starts_the_command() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1000}));
    h.app.preferences.ribbon_icon_grid = true;
    h.frames(4);
    h.step(&json!({"click":{"handle":"panel:MODEL:CREATE"}})).unwrap();
    h.frames(4);
    let at = crate::scenario::handle_at("dropdown:solid.box").unwrap() + vec2(GRID_CELL / 2.0 - 7.0, -GRID_CELL / 2.0 + 7.0);
    h.step(&json!({"click":[at.x, at.y]})).unwrap();
    h.frames(3);
    assert!(crate::toolbar_custom::pinned_on(&h.app, "MODEL", "solid.box").is_none(), "not pinned");
    assert!(h.app.dialog.is_some() || h.app.tool.is_some(), "the command starts");
}

/// A command pinned from the grid becomes a ribbon button; Remove from Toolbar in the
/// button's own right-click menu takes it off again, with both options on.
#[test]
fn a_pinned_button_is_removed_from_its_own_menu() {
    for mode in [crate::shell::LayoutMode::Floating, crate::shell::LayoutMode::Docked] {
        a_pinned_button_is_removed_in(mode);
    }
}

fn a_pinned_button_is_removed_in(mode: crate::shell::LayoutMode) {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.set", json!({"layout":mode}));
    h.call("ui.resize", json!({"width":1600,"height":1000}));
    h.app.preferences.ribbon_icon_grid = true;
    h.app.preferences.ribbon_menus_skip_pinned = true;
    h.frames(4);
    h.step(&json!({"click":{"handle":"panel:MODEL:CREATE"}})).unwrap();
    h.frames(4);
    h.step(&json!({"move":{"handle":"dropdown:solid.loft"}})).unwrap();
    h.frames((PIN_DELAY * 60.0) as usize + 5);
    h.step(&json!({"click":{"handle":"dropdown_pin:solid.loft"}})).unwrap();
    h.frames(3);
    h.step(&json!({"key":"Escape"})).unwrap();
    h.frames(4);
    assert!(crate::scenario::handle_at("button:solid.loft").is_some(), "pinned, Loft is a ribbon button");
    h.step(&json!({"click":{"handle":"button:solid.loft"},"button":"right"})).unwrap();
    h.frames(4);
    assert!(h.app.viewport.context_menu.is_some(), "the button's menu opens");
    h.step(&json!({"widget":"Remove from Toolbar"})).unwrap();
    h.frames(4);
    assert!(crate::toolbar_custom::pinned_on(&h.app, "MODEL", "solid.loft").is_none(), "removed: {:?}", h.app.toolbar_custom.panels);
    assert!(crate::scenario::handle_at("button:solid.loft").is_none(), "and gone from the ribbon");
}

/// Sketch › Create in the grid shows every variant of each family (Line, Rectangle, …) under
/// the family's heading, then the other tools, as large icons; a click starts the tool.
#[test]
fn sketch_create_in_the_grid_shows_every_variant() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1000}));
    h.app.preferences.ribbon_icon_grid = true;
    h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
    h.frames(4);
    h.step(&json!({"click":{"handle":"panel:SKETCH:CREATE"}})).unwrap();
    h.frames(4);
    let cells: Vec<String> = crate::scenario::handles("dropdown:").into_iter().map(|(id, _)| id).collect();
    for id in ["sketch.line", "sketch.rectangle.center", "sketch.circle.three_point", "sketch.arc.tangent", "sketch.polygon.edge", "sketch.point"]
    {
        assert!(cells.iter().any(|c| *c == format!("dropdown:{id}")), "{id} is an icon of the grid: {cells:?}");
    }
    let grouped = sketch_create_grouped(
        &sketcraft_engine::command_specs().into_iter().filter(|c| c.id.starts_with("sketch.rectangle")).collect::<Vec<_>>(),
    );
    assert_eq!(grouped.first().and_then(|(h, _)| *h), Some("Rectangle"));
    assert!(grouped.iter().skip(1).all(|(h, _)| h.is_none()), "one heading per family");
    h.step(&json!({"click":{"handle":"dropdown:sketch.rectangle.center"}})).unwrap();
    h.frames(3);
    assert!(crate::scenario::rect_of("toolbar_dropdown").is_none(), "a click closes the drop-down");
    assert_eq!(h.app.tool.as_ref().map(|t| t.cmd), Some("sketch.rectangle.center"), "and starts the tool");
}

/// With `ribbon_menus_skip_pinned`, a drop-down leaves out the commands the ribbon shows as
/// buttons, in the list and in the grid; a panel too narrow for its buttons lists them again.
#[test]
fn pinned_commands_can_be_left_out_of_the_drop_down() {
    for grid in [false, true] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":1600,"height":1000}));
        h.app.preferences.ribbon_icon_grid = grid;
        let rows = |h: &mut crate::scenario::Harness, panel: &str| {
            h.frames(4);
            h.step(&json!({"click":{"handle":panel}})).unwrap();
            h.frames(4);
            let rows: Vec<String> = crate::scenario::handles("dropdown:").into_iter().map(|(id, _)| id).collect();
            h.step(&json!({"key":"Escape"})).unwrap();
            rows
        };
        let all = rows(&mut h, "panel:MODEL:CREATE");
        assert!(all.iter().any(|id| id == "dropdown:solid.extrude"), "grid {grid}: listed by default: {all:?}");
        h.call("ui.prefs", json!({"ribbon_menus_skip_pinned": true}));
        let rest = rows(&mut h, "panel:MODEL:CREATE");
        for pinned in ["solid.extrude", "solid.revolve", "solid.hole"] {
            assert!(!rest.iter().any(|id| *id == format!("dropdown:{pinned}")), "grid {grid}: {pinned} is on the ribbon: {rest:?}");
        }
        assert_eq!(rest.len() + 3, all.len(), "grid {grid}: only the ribbon's buttons are left out");
        // In a window too narrow for the Extrude button, MODEL CREATE's menu lists it again.
        let narrow = (3..=16).rev().map(|w| w as f32 * 100.0).find(|w| {
            h.call("ui.resize", json!({"width": w, "height": 1000}));
            h.frames(4);
            crate::scenario::handle_at("button:solid.extrude").is_none()
        });
        assert!(narrow.is_some(), "grid {grid}: a narrow window has no room for the Extrude button");
        let panel = ["panel:MODEL:CREATE", "collapsed:MODEL:CREATE"].into_iter().find(|p| crate::scenario::handle_at(p).is_some());
        let Some(panel) = panel else { panic!("grid {grid}: MODEL CREATE is in the » menu at {narrow:?}") };
        let folded = rows(&mut h, panel);
        assert!(folded.iter().any(|id| id == "dropdown:solid.extrude"), "grid {grid}: Extrude, off the ribbon, is listed: {folded:?}");
    }
    let box_ = sketcraft_engine::find_command("solid.box").unwrap();
    assert_eq!(skip_on_ribbon(vec![box_], &["solid.box"]).len(), 1, "a drop-down is never left empty");
}
