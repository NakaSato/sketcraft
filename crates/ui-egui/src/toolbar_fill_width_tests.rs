use super::*;

/// #2108: at every common window width, in the Solid and the Sketch tab, the toolbar uses the
/// width it has. No blank run wider than one button while something is hidden, every panel
/// on the toolbar shows its icons, the » menu only when even one button a panel won't fit,
/// and neither the tab row nor the title bar's own controls run into each other.
#[test]
fn toolbar_fills_its_width_from_wide_to_narrow_windows() {
    // Either header layout: the ribbon fits the same way with search in the bar.
    for (sketch, layout) in [false, true].into_iter().flat_map(|s| [(s, HeaderLayout::DESKTOP), (s, HeaderLayout::COMPACT)]) {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.custom_titlebar = true;
        h.app.header = layout;
        // The classic tab row these widths were measured for: every CAD tab shown, without the
        // SketchUp tab (with all ten tabs, 700 px scrolls the row and a tab is cut at search).
        crate::workspace::set_tab_shown(&mut h.app, crate::workspace::SKETCHUP_TAB, false);
        h.app.run("solid.box", json!({"length":10,"width":10,"height":10})).unwrap();
        if sketch {
            h.app.run("sketch.create", json!({"plane":"XY"})).unwrap();
        }
        let tab = if sketch { "SKETCH" } else { "MODEL" };
        h.app.ui.tab = tab.into();
        for w in [1920.0, 1600.0, 1440.0, 1366.0, 1280.0, 1100.0, 900.0, 700.0] {
            h.call("ui.resize", json!({"width":w,"height":900}));
            h.frames(3);
            let at = format!("{tab} at {w}, {layout:?}");
            let bar = crate::scenario::rect_of("toolbar").unwrap();
            let finish = crate::scenario::rect_of("panel:SKETCH:FINISH SKETCH");
            // The room for the other panels: the bar less the Finish group kept for it.
            let right = bar.right() - 4.0 - finish.map_or(0.0, |f| f.width() + 4.0);
            let count = |name: &str| crate::scenario::count_of(name).unwrap_or(-1.0);
            let (collapsed, trimmed, more) = (count("toolbar_collapsed"), count("toolbar_trimmed"), count("toolbar_more"));
            // FINISH EDITING is shown only while a linked design is edited in place.
            let lists: Vec<_> = crate::toolbar_custom::panel_lists(tab).into_iter().filter(|(panel, _, _)| *panel != "FINISH EDITING").collect();
            let mut end = bar.left();
            let mut on_bar = 0;
            for (panel, cmds, promote) in &lists {
                let Some(rect) = crate::scenario::rect_of(&format!("panel:{tab}:{panel}")) else { continue };
                on_bar += 1;
                if *panel != "FINISH SKETCH" {
                    end = end.max(rect.right() + PANEL_GAP);
                }
                if crate::toolbar_custom::buttons(&h.app, tab, panel, cmds, *promote).is_empty() {
                    continue;
                }
                let icons = crate::scenario::handles("button:").iter().filter(|(_, p)| rect.contains(*p)).count()
                    + usize::from(crate::scenario::handle_at(&format!("collapsed:{tab}:{panel}")).is_some());
                assert!(icons >= 1, "{at}: {panel} shows no icons");
            }
            assert_eq!(on_bar + more as usize, lists.len(), "{at}: every panel is on the toolbar or in the » menu");
            let overflow = crate::scenario::rect_of("toolbar:overflow");
            assert_eq!(overflow.is_some(), more > 0.0, "{at}: the » menu shows exactly when a panel is in it");
            let gap = right - end - if more > 0.0 { MORE_ROOM } else { 0.0 };
            assert!(gap >= -0.5, "{at}: the panels run {} px past the room", -gap);
            let hidden = collapsed + trimmed + more > 0.0;
            let button = if more > 0.0 {
                COLLAPSED_WIDTH + PANEL_GAP
            } else if sketch {
                CONSTRAINT_GRID_WIDTH + BUTTON_GAP
            } else {
                BUTTON_MIN_WIDTH + BUTTON_GAP
            };
            if hidden {
                assert!(gap < button, "{at}: {gap} px left blank while buttons are hidden");
            }
            // The » menu only when one button a panel can't fit.
            if w >= 900.0 {
                assert_eq!(more, 0.0, "{at}: the » menu while there is room");
            }
            if w >= 1600.0 {
                assert!(!hidden, "{at}: buttons hidden on a wide window");
            }
            if let Some(o) = overflow {
                assert!(o.right() <= right + 4.0, "{at}: the » button overlaps Finish or the right edge");
            }
            if sketch {
                let finish = finish.expect("Finish group stays visible");
                if more > 0.0 {
                    assert_eq!(finish.right(), bar.right(), "{at}: Finish moves right of the » menu");
                } else {
                    assert!((finish.left() - end).abs() < 0.5, "{at}: Finish follows the last panel ({} vs {end})", finish.left());
                }
                assert!(crate::scenario::handle_at("button:sketch.finish").is_some(), "{at}: Finish stays reachable");
            }
            // The assistant lives in the title bar; visible ribbon tabs fit before search (or,
            // with search in the title bar, within the ribbon card).
            let search = crate::scenario::rect_of("ribbon:search").unwrap();
            let card = crate::scenario::rect_of("ribbon:card").unwrap_or(bar);
            for t in TABS.iter().chain(["SKETCH"].iter()) {
                if let Some(r) = crate::scenario::rect_of(&format!("toolbar_tab:{t}")) {
                    assert!(r.right() < search.left() || search.bottom() <= APP_BAR_HEIGHT, "{at}: the {t} tab runs into search");
                    assert!(r.right() <= card.right(), "{at}: the {t} tab runs past the ribbon");
                }
            }
            // The title bar keeps document tabs before Assistant, Preferences, Help and Report.
            let report = crate::scenario::rect_of("titlebar:report").unwrap();
            let tabs_end = crate::scenario::handles("tab:design:").iter().map(|(_, p)| p.x).fold(0.0, f32::max);
            assert!(tabs_end < report.left(), "{at}: the document tabs run into Report a bug");
            let assistant = crate::scenario::rect_of("titlebar:assistant").unwrap();
            let help = crate::scenario::rect_of("appbar:help").unwrap();
            let prefs = crate::scenario::rect_of("titlebar:preferences").unwrap();
            assert!(tabs_end < assistant.left(), "{at}: the document tabs run into Assistant");
            assert!(
                assistant.right() <= prefs.left() && prefs.right() <= help.left() && help.right() <= report.left(),
                "{at}: the title bar's right-hand controls overlap"
            );
            if layout.search == SearchPlace::Header {
                assert!(assistant.right() <= search.left() && search.right() <= prefs.left(), "{at}: search sits between Assistant and Settings");
                assert!(tabs_end < search.left(), "{at}: the document tabs run into search");
            }
        }
    }
}

/// #7166: a panel whose buttons were all removed keeps one button, its first command's icon,
/// which opens the panel's drop-down: New Sketch stays reachable from the Model tab instead of
/// leaving an empty Sketch panel. It takes the room an empty panel took before.
#[test]
fn a_panel_without_buttons_keeps_one_that_opens_its_menu() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":900}));
    crate::toolbar_custom::remove(&mut h.app, "MODEL", "sketch.create");
    assert!(crate::toolbar_custom::pinned_on(&h.app, "MODEL", "sketch.create").is_none());
    h.frames(3);
    assert!(crate::scenario::handle_at("button:sketch.create").is_none(), "New Sketch is no longer a full button");
    let panel = crate::scenario::rect_of("panel:MODEL:SKETCH").unwrap();
    assert_eq!(panel.width(), COLLAPSED_WIDTH, "the panel takes a collapsed panel's room");
    let button = crate::scenario::handle_at("collapsed:MODEL:SKETCH").expect("the Sketch panel keeps a button");
    assert!(panel.contains(button));
    h.step(&json!({"click":[button.x, button.y]})).unwrap();
    h.frames(2);
    assert!(crate::scenario::rect_of("toolbar_dropdown").is_some(), "the button opens the panel's drop-down");
    h.step(&json!({"click":{"handle":"dropdown:sketch.create"}})).unwrap();
    assert!(matches!(h.app.dialog.as_ref().map(|d| &d.kind), Some(crate::dialogs::Kind::Sketch)), "New Sketch starts from it");
}
