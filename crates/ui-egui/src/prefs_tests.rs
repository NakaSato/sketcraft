#[test]
fn theme_traits_use_resolved_background_and_maximal_contrast() {
    use theme_picker::{Classification, theme_traits};
    let light = Tokens::light();
    let dark = Tokens::dark();
    assert_eq!(theme_traits(&light), Classification { dark: false, high: false });
    assert_eq!(theme_traits(&dark), Classification { dark: true, high: false });
    for (ground, ink, night) in [(egui::Color32::WHITE, egui::Color32::BLACK, false), (egui::Color32::BLACK, egui::Color32::WHITE, true)] {
        let mut palette = light;
        palette.dialog_footer = ground;
        palette.form_ink = ink;
        assert_eq!(theme_traits(&palette), Classification { dark: night, high: true });
    }
    let mut palette = light;
    for (gray, night) in [(187, true), (188, false)] {
        palette.dialog_footer = egui::Color32::from_gray(gray);
        assert_eq!(theme_traits(&palette).dark, night, "linear luminance straddles 0.5");
    }
    let maximal: Vec<_> =
        crate::theme_files::builtin().iter().filter(|f| theme_traits(&resolved_theme(&Prefs::default(), f)).high).map(|f| f.id.as_str()).collect();
    assert_eq!(maximal.len(), 4, "ordinary palettes must stay Standard: {maximal:?}");
    assert!(maximal.contains(&"1-bit") && maximal.contains(&"1-bit-dark"));
}

#[test]
fn theme_slots_default_persist_allow_command_access_and_restore_with_appearance() {
    let mut h = crate::scenario::Harness::new();
    assert_eq!((&h.app.preferences.theme_day[..], &h.app.preferences.theme_night[..]), ("light", "dark"));
    let out = super::set(&mut h.app, &json!({"theme_day":"akari", "theme_night":"onyx"})).unwrap();
    assert_eq!((out["theme_day"].as_str(), out["theme_night"].as_str()), (Some("akari"), Some("onyx")));
    let out = h.call("ui.prefs", json!({}));
    assert_eq!(out["result"]["theme_day"], "akari");
    assert_eq!(out["result"]["theme_night"], "onyx");
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert_eq!((restored.preferences.theme_day.as_str(), restored.preferences.theme_night.as_str()), ("akari", "onyx"));
    h.call("ui.prefs", json!({"theme_day":"clay", "theme_night":"clay-dark", "open":"Appearance"}));
    assert_eq!(h.app.preferences.theme_day, "clay");
    defaults(&mut h.app, PAGE_GENERAL);
    assert_eq!(h.app.preferences.theme_day, "clay", "another page's reset does not clear slots");
    defaults(&mut h.app, PAGE_APPEARANCE);
    assert_eq!((h.app.preferences.theme_day.as_str(), h.app.preferences.theme_night.as_str()), ("light", "dark"));
}

#[test]
fn theme_slots_resolve_unknown_and_recursive_ids_without_changing_saved_intent() {
    let mut prefs = Prefs { theme: "system".into(), theme_day: "missing".into(), theme_night: "system".into(), ..Default::default() };
    assert_eq!(theme_picker::active_id(&prefs, false), "light");
    assert_eq!(theme_picker::active_id(&prefs, true), "dark");
    assert_eq!((&prefs.theme_day[..], &prefs.theme_night[..]), ("missing", "system"));
    prefs.theme = "onyx".into();
    assert_eq!(theme_picker::active_id(&prefs, false), "onyx");
    assert_eq!(theme_picker::active_id(&prefs, true), "onyx");
    let mut h = crate::scenario::Harness::new();
    h.app.services.desktop_dark = Some(Box::new(|| Some(false)));
    let out = h.call("ui.prefs", json!({"theme":"system", "theme_day":"missing", "theme_night":"system"}));
    assert!(out["error"].is_null(), "{out}");
    h.frames(2);
    assert!(!h.app.ui.dark);
    assert_eq!(h.app.preferences.theme_day, "missing");
    h.app.services.desktop_dark = Some(Box::new(|| Some(true)));
    h.frames(2);
    assert!(h.app.ui.dark);
    assert_eq!(h.app.preferences.theme_night, "system");
}

#[test]
fn theme_onyx_is_reachable_selectable_and_shows_its_options_in_both_views() {
    for grid in [false, true] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":1467,"height":1146}));
        h.call("ui.prefs", json!({"language":"en","theme":"light","open":"Appearance"}));
        h.frames(5);
        if grid {
            h.step(&json!({"click":{"handle":"prefs:theme_view:1"}})).unwrap();
            h.frames(3);
            super::reveal_preference(&mut h, "prefs:theme_tile:onyx");
        } else {
            for _ in 0..theme_choices(&h.app).len() {
                if crate::scenario::rect_of("prefs:theme_tile:onyx").is_some() {
                    break;
                }
                h.step(&json!({"click":{"handle":"prefs:theme_next"}})).unwrap();
                h.frames(3);
            }
        }
        h.step(&json!({"click":{"handle":"prefs:theme_tile:onyx"}})).unwrap();
        h.frames(4);
        assert_eq!(h.app.preferences.theme, "onyx");
        assert!(crate::scenario::rect_of("prefs:theme_options_badge:onyx").is_some());
        assert!(crate::scenario::rect_of("prefs:theme_options_header").is_some());
        let out = h.call("ui.prefs", json!({}));
        assert_eq!(out["result"]["theme_option_values"]["opacity"], 80.0);
    }
}

#[test]
fn theme_system_resolves_custom_day_and_night_by_os_including_cross_tone_slots() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let desktop = Arc::new(AtomicBool::new(false));
    let reading = desktop.clone();
    let mut h = crate::scenario::Harness::new();
    h.app.services.desktop_dark = Some(Box::new(move || Some(reading.load(Ordering::Relaxed))));
    h.call("ui.prefs", json!({"theme":"system", "theme_day":"onyx", "theme_night":"akari"}));
    h.frames(3);
    assert!(h.app.ui.dark, "the OS is light, but the Day slot may hold a dark theme");
    assert_eq!(h.app.prefs_window.window_theme, Some(egui::SystemTheme::SystemDefault));
    assert_eq!(active_theme_file(&h.app).unwrap().id, "onyx");
    assert_eq!(current_theme_file(&h.app)["name"], "Onyx");
    desktop.store(true, Ordering::Relaxed);
    h.frames(3);
    assert!(!h.app.ui.dark);
    assert_eq!(active_theme_file(&h.app).unwrap().id, "akari");
    h.call("ui.prefs", json!({"theme_day":"dark", "theme_night":"light"}));
    h.frames(2);
    assert_eq!(current_theme_file(&h.app)["base"], "light");
    desktop.store(false, Ordering::Relaxed);
    h.frames(2);
    assert_eq!(current_theme_file(&h.app)["base"], "dark");
    // If OS observation temporarily disappears, the mode button must use the same
    // remembered OS tone as rendering, rather than the cross-tone slot's palette.
    h.app.services.desktop_dark = Some(Box::new(|| None));
    h.app.home.open = false;
    h.call("ui.prefs", json!({"open":"Appearance"}));
    h.frames(3);
    h.step(&json!({"click":{"handle":"prefs:theme_mode:0"}})).unwrap();
    assert_eq!(h.app.preferences.theme, "dark", "leaving System preserves the theme actually in effect");
}

#[test]
fn theme_mode_switches_preserve_the_effective_theme_and_fill_only_the_aimed_slot() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    select_theme(&mut app, "onyx");
    theme_picker::switch_mode(&mut app, true, true);
    assert_eq!((&app.preferences.theme[..], &app.preferences.theme_day[..], &app.preferences.theme_night[..]), ("system", "light", "onyx"));
    theme_picker::choose(&mut app, false, "akari");
    assert_eq!((&app.preferences.theme[..], &app.preferences.theme_day[..], &app.preferences.theme_night[..]), ("system", "akari", "onyx"));
    theme_picker::switch_mode(&mut app, false, false);
    assert_eq!(app.preferences.theme, "akari", "leaving system picks the effective Day theme, not its tone's default");
    theme_picker::switch_mode(&mut app, true, false);
    theme_picker::choose(&mut app, true, "clay-dark");
    theme_picker::switch_mode(&mut app, false, true);
    assert_eq!(app.preferences.theme, "clay-dark");
    let mut state = theme_picker::State::default();
    assert!(!state.night && !state.grid && state.tone.is_none());
    state.aim(true);
    assert!(state.night && state.tone == Some(true));
    state.aim(false);
    assert!(!state.night && state.tone == Some(false));
}

#[test]
fn theme_filter_counts_ignore_only_their_own_group() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    let file = crate::theme_files::from_value(
        &json!({"format":"sketcraft-theme/1", "name":"Imported maximal", "base":"dark", "colors":{"dialog_footer":"#000000", "form_ink":"#ffffff"}}),
    )
    .unwrap();
    crate::theme_files::add(&mut app.preferences.custom_themes, file).unwrap();
    let entries = theme_picker::entries(&app);
    let state = theme_picker::State { tone: Some(true), high: Some(false), imported: false, ..Default::default() };
    assert_eq!((state.count(&entries, "tone", false), state.count(&entries, "tone", true)), (8, 15));
    assert_eq!((state.count(&entries, "contrast", false), state.count(&entries, "contrast", true)), (15, 4));
    assert_eq!(state.count(&entries, "imported", true), 0);
    let state = theme_picker::State { high: Some(true), imported: true, ..state };
    assert_eq!(state.count(&entries, "tone", true), 1);
    assert_eq!(state.count(&entries, "contrast", true), 1);
    assert_eq!(state.count(&entries, "contrast", false), 0);
    assert_eq!(state.count(&entries, "imported", true), 1);
}

#[test]
fn theme_carousel_wheel_passes_vertical_to_page_and_handles_horizontal_and_shift() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1467,"height":1146}));
    h.call("ui.prefs", json!({"language":"en","theme":"light","open":"Appearance"}));
    h.frames(6);
    for (delta, modifiers, horizontal) in [
        (egui::vec2(1.0, -96.0), egui::Modifiers::NONE, false),
        (egui::vec2(-180.0, 0.0), egui::Modifiers::NONE, true),
        (egui::vec2(0.0, -180.0), egui::Modifiers::SHIFT, true),
    ] {
        let before = crate::scenario::rect_of("prefs:card:Light").unwrap();
        let at = crate::scenario::rect_of("prefs:theme_viewport").unwrap().center();
        h.frame_with(vec![
            egui::Event::PointerMoved(at),
            egui::Event::MouseWheel {
                source: egui::MouseWheelSource::Unknown,
                unit: egui::MouseWheelUnit::Point,
                delta,
                modifiers,
                phase: egui::TouchPhase::Move,
            },
        ]);
        h.frames(60);
        let after = crate::scenario::rect_of("prefs:card:Light").unwrap();
        if horizontal {
            assert!(after.left() < before.left() - 100.0, "horizontal wheel must move a card: {before:?} -> {after:?}");
            assert!((after.top() - before.top()).abs() < 1.0, "horizontal wheel must leave the page in place");
        } else {
            assert!((after.left() - before.left()).abs() < 1.0, "vertical wheel must not move carousel");
            assert!(after.top() < before.top() - 50.0, "vertical wheel must scroll the page");
        }
    }
}

/// Profile operations switches the profiler when it changes, and only then: an agent's
/// diagnostics.profile in between is not undone next frame.
#[test]
fn the_profile_preference_switches_the_profiler_when_it_changes() {
    use serde_json::json;
    let mut h = crate::scenario::Harness::new();
    h.frames(1);
    h.call("ui.prefs", json!({"profile_operations": true}));
    h.frames(2);
    assert!(sketcraft_engine::profile::enabled());
    h.app.session.execute("diagnostics.profile", &json!({"enabled": false})).unwrap();
    h.frames(2);
    assert!(!sketcraft_engine::profile::enabled(), "the command's choice stands");
    h.call("ui.prefs", json!({"profile_operations": false}));
    h.frames(2);
    assert!(!sketcraft_engine::profile::enabled());
}

/// Preferences saved with a setting this version no longer has still load, every other
/// setting kept: the removed key is ignored like any unknown one.
#[test]
fn saved_preferences_with_a_removed_setting_still_load() {
    use serde_json::json;
    for (removed, value) in [
        ("geometry_kernel", json!("smlib")),
        ("geometry_kernel", json!("sketcore")),
        ("metric_tap_drill_major_less_pitch", json!(true)),
        ("surface_extrude_symmetric_per_side", json!(true)),
    ] {
        let saved = json!({"length_decimals": 4, removed: value});
        let p = serde_json::from_value::<super::Prefs>(saved.clone()).unwrap();
        assert_eq!(p.length_decimals, 4, "{removed}");
        assert_eq!(super::lenient(&saved).length_decimals, 4, "{removed}");
        assert!(serde_json::to_value(&p).unwrap().get(removed).is_none(), "{removed} is not written back");
    }
}

/// #10254: the invert toggle is off by default, persists, and the Navigation page's
/// Defaults resets it.
#[test]
fn invert_navigator_arrows_defaults_off_persists_and_resets() {
    assert!(!Prefs::default().invert_navigator_arrows);
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    assert!(!app.preferences.invert_navigator_arrows);
    app.preferences.invert_navigator_arrows = true;
    assert_eq!(serde_json::to_value(&app.preferences).unwrap()["invert_navigator_arrows"], true);
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&app.prefs());
    assert!(restored.preferences.invert_navigator_arrows);
    super::reset_preferences(&mut app.preferences, super::PAGE_NAVIGATION);
    assert!(!app.preferences.invert_navigator_arrows);
}

/// #10483: the curved-roll invert toggle is off by default, persists, and the Navigation
/// page's Defaults resets it.
#[test]
fn invert_curved_roll_arrows_defaults_off_persists_and_resets() {
    assert!(!Prefs::default().invert_curved_roll_arrows);
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    assert!(!app.preferences.invert_curved_roll_arrows);
    app.preferences.invert_curved_roll_arrows = true;
    assert_eq!(serde_json::to_value(&app.preferences).unwrap()["invert_curved_roll_arrows"], true);
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&app.prefs());
    assert!(restored.preferences.invert_curved_roll_arrows);
    super::reset_preferences(&mut app.preferences, super::PAGE_NAVIGATION);
    assert!(!app.preferences.invert_curved_roll_arrows);
}

/// #10220: off by default, offered on Preferences › General only where designs are kept in
/// a browser (the web app), set through `ui.prefs` and saved with the settings.
#[test]
fn keep_unsaved_in_browser_is_off_and_only_the_web_app_offers_it() {
    use serde_json::json;
    assert!(!crate::prefs::Prefs::default().keep_unsaved_in_browser);
    let label = crate::i18n::tr("prefs.keep_unsaved_in_browser").to_string();
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"open":"General"}));
    h.frames(5);
    assert!(h.widget(&label).is_none(), "the desktop app keeps no designs in a browser");
    h.call("ui.prefs", json!({"button":"cancel"}));
    h.app.services.browser_files = Some(Box::new(|| {}));
    h.call("ui.prefs", json!({"open":"General"}));
    h.frames(5);
    assert!(h.widget(&label).is_some(), "the web app offers it beside the other saving options");
    let ask = crate::i18n::tr("prefs.keep_unsaved_also_asks").to_string();
    assert!(h.widget(&ask).is_none(), "still asking only means something once work is kept");
    let reply = h.call("ui.prefs", json!({"keep_unsaved_in_browser": true}));
    assert_eq!(reply["ok"], true, "{reply}");
    h.frames(5);
    assert!(h.widget(&ask).is_some(), "keeping work offers whether to still ask");
    h.call("ui.prefs", json!({"button":"ok"}));
    assert!(h.app.preferences.keep_unsaved_in_browser);
    assert!(h.app.preferences.keep_unsaved_also_asks, "keeping work still asks unless told not to");
    let mut restored = crate::SketApp::new(Default::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(restored.preferences.keep_unsaved_in_browser, "it is saved with the settings");
}

#[test]
fn trackpad_zoom_modifier_roundtrips_and_validates_control_values() {
    assert_eq!(Prefs::default().trackpad_zoom_modifier, TrackpadZoomModifier::Off);
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    for (id, value) in [
        ("cmd", TrackpadZoomModifier::Cmd),
        ("alt", TrackpadZoomModifier::Alt),
        ("ctrl", TrackpadZoomModifier::Ctrl),
        ("off", TrackpadZoomModifier::Off),
    ] {
        set(&mut app, &json!({"trackpad_zoom_modifier": id})).unwrap();
        assert_eq!(app.preferences.trackpad_zoom_modifier, value);
        let saved = serde_json::to_value(&app.preferences).unwrap();
        assert_eq!(saved["trackpad_zoom_modifier"], id);
        assert_eq!(lenient(&saved).trackpad_zoom_modifier, value);
        let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
        restored.load_prefs(&app.prefs());
        assert_eq!(restored.preferences.trackpad_zoom_modifier, value);
    }

    for invalid in [json!("super"), json!(""), json!(42), json!(null)] {
        assert!(set(&mut app, &json!({"trackpad_zoom_modifier": invalid})).is_err());
        assert_eq!(app.preferences.trackpad_zoom_modifier, TrackpadZoomModifier::Off);
        assert_eq!(lenient(&json!({"trackpad_zoom_modifier": invalid})).trackpad_zoom_modifier, TrackpadZoomModifier::Off);
    }
    app.load_prefs(&json!({"preferences": {"trackpad_zoom_modifier": "future"}}).to_string());
    assert_eq!(app.preferences.trackpad_zoom_modifier, TrackpadZoomModifier::Off);
    app.preferences.trackpad_zoom_modifier = TrackpadZoomModifier::Cmd;
    reset_preferences(&mut app.preferences, PAGE_NAVIGATION);
    assert_eq!(app.preferences.trackpad_zoom_modifier, TrackpadZoomModifier::Off);
}

#[test]
fn command_dialog_value_memory_defaults_off_persists_and_resets() {
    assert!(!Prefs::default().remember_dialog_values);
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    assert!(!app.preferences.remember_dialog_values);
    app.preferences.remember_dialog_values = true;
    assert_eq!(serde_json::to_value(&app.preferences).unwrap()["remember_dialog_values"], true);

    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&app.prefs());
    assert!(restored.preferences.remember_dialog_values);
    reset_preferences(&mut app.preferences, PAGE_TOOLS);
    assert!(!app.preferences.remember_dialog_values);
}

#[test]
fn navigation_bar_preferences_round_trip_and_recover_hostile_fields() {
    use crate::navigation_bar::{Edge, Idle, Labels, NavigationBar, Position, Size};
    use serde_json::json;
    assert_eq!(super::lenient(&json!({})).navigation_bar, NavigationBar::default());
    let p = super::lenient(&json!({"navigation_bar": {
        "slots":["pan", "future_slot", "pan", "separator", "separator", "separator", "separator", "separator", 7, "fit"],
        "position":"bottom_left", "edge":"top", "size":"regular", "labels":"labels", "idle":"hide"
    }}));
    assert_eq!(p.navigation_bar.slots, ["pan", "future_slot", "separator", "separator", "separator", "separator", "fit"]);
    assert_eq!(
        (p.navigation_bar.position, p.navigation_bar.edge, p.navigation_bar.size, p.navigation_bar.labels, p.navigation_bar.idle),
        (Position::Left, Edge::Top, Size::Regular, Labels::Labels, Idle::Hide)
    );
    let saved = serde_json::to_value(&p).unwrap();
    assert_eq!(serde_json::from_value::<super::Prefs>(saved).unwrap(), p);
    for field in ["position", "edge", "size", "labels", "idle"] {
        let mut value = serde_json::to_value(&p).unwrap();
        value["navigation_bar"][field] = json!({"wrong":"type"});
        let mut expected = p.clone();
        match field {
            "position" => expected.navigation_bar.position = Position::default(),
            "edge" => expected.navigation_bar.edge = Edge::default(),
            "size" => expected.navigation_bar.size = Size::default(),
            "labels" => expected.navigation_bar.labels = Labels::default(),
            _ => expected.navigation_bar.idle = Idle::default(),
        }
        assert_eq!(super::lenient(&value), expected, "{field}");
        value["navigation_bar"][field] = json!("unknown");
        assert_eq!(super::lenient(&value), expected, "{field}");
    }
    for value in [json!(null), json!(7), json!([]), json!({"slots":false})] {
        assert_eq!(super::lenient(&json!({"navigation_bar":value})).navigation_bar, NavigationBar::default());
    }
    let ids: Vec<_> = (0..100).map(|i| format!("future_{i}")).collect();
    let limited = super::lenient(&json!({"navigation_bar":{"slots":ids}}));
    assert_eq!(limited.navigation_bar.slots.len(), crate::navigation_bar::MAX_SLOTS);
    assert_eq!(limited.navigation_bar.slots.last().map(String::as_str), Some("future_15"));
    let ids: Vec<_> = ["pan".to_owned(), "pan".to_owned()].into_iter().chain((0..20).map(|i| format!("future_{i}"))).collect();
    let limited = super::lenient(&json!({"navigation_bar":{"slots":ids}}));
    assert_eq!(limited.navigation_bar.slots.len(), crate::navigation_bar::MAX_SLOTS);
    assert_eq!(limited.navigation_bar.slots.last().map(String::as_str), Some("future_14"));
}

#[test]
fn navigation_bar_control_reads_writes_validates_and_restores() {
    use crate::navigation_bar::NavigationBar;
    use serde_json::json;
    let mut h = crate::scenario::Harness::new();
    let initial = h.call("ui.prefs", json!({}));
    assert_eq!(initial["result"]["navigation_bar"], serde_json::to_value(NavigationBar::default()).unwrap());
    let edit = json!({"navigation_bar":{"slots":["zoom", "future", "zoom", "display"], "position":"bottom_right", "size":"regular", "labels":"labels", "idle":"always"}});
    h.call("ui.prefs", json!({"open":"Layout"}));
    let reply = h.call("ui.prefs", edit);
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["result"]["navigation_bar"]["slots"], json!(["zoom", "future", "display"]));
    let before = h.app.preferences.clone();
    for bad in [
        json!(null),
        json!({"slots":"fit"}),
        json!({"slots":[7]}),
        json!({"position":"top"}),
        json!({"size":true}),
        json!({"labels":"text"}),
        json!({"idle":10}),
        json!({"unknown":true}),
    ] {
        assert!(super::control(&mut h.app, &json!({"navigation_bar":bad,"zoom_reverse":true})).is_err());
        assert_eq!(h.app.preferences, before, "failed updates are atomic");
    }
    let mut restored = crate::SketApp::new(Default::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert_eq!(restored.preferences.navigation_bar, NavigationBar::default(), "a change that was not applied is not saved (#9730)");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert_eq!(h.app.preferences.navigation_bar, NavigationBar::default());
    h.call("ui.prefs", json!({"open":"Layout","navigation_bar":{"slots":[]}}));
    assert!(h.app.preferences.navigation_bar.slots.is_empty());
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert_eq!(h.app.preferences.navigation_bar, NavigationBar::default());
}

fn navbar_click(h: &mut crate::scenario::Harness, handle: &str) {
    h.call("ui.prefs", json!({"open":"Interface/Navigation bar"}));
    h.frames(20);
    scroll_to_and_click(h, handle);
}

#[test]
fn navigation_bar_editor_buttons_and_appearance_update_preferences() {
    use crate::navigation_bar::{Idle, Labels, MenuDisplay, Position, Size};
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1100}));
    h.call("ui.prefs", json!({"open":"Interface/Navigation bar", "navigation_bar":{"slots":["pan","fit","visual_style","display"]}}));
    navbar_click(&mut h, "prefs:navbar:add:spin");
    assert_eq!(h.app.preferences.navigation_bar.slots, ["pan", "fit", "visual_style", "display", "spin"]);
    navbar_click(&mut h, "prefs:navbar:slot:pan");
    navbar_click(&mut h, "prefs:navbar:right");
    assert_eq!(h.app.preferences.navigation_bar.slots, ["fit", "pan", "visual_style", "display", "spin"]);
    navbar_click(&mut h, "prefs:navbar:remove");
    assert_eq!(h.app.preferences.navigation_bar.slots, ["fit", "visual_style", "display", "spin"]);
    navbar_click(&mut h, "prefs:navbar:slot:visual_style");
    navbar_click(&mut h, "prefs:navbar:shows:0");
    assert_eq!(h.app.preferences.navigation_bar.menu_display("visual_style"), MenuDisplay::IconAndValue);
    navbar_click(&mut h, "prefs:navbar:anchor:0");
    assert_eq!(h.app.preferences.navigation_bar.position, Position::Left);
    assert_eq!(h.app.preferences.navigation_bar.edge, crate::navigation_bar::Edge::Top);
    navbar_click(&mut h, "prefs:navbar:anchor:5");
    assert_eq!(h.app.preferences.navigation_bar.position, Position::Right);
    assert_eq!(h.app.preferences.navigation_bar.edge, crate::navigation_bar::Edge::Bottom);
    navbar_click(&mut h, "prefs:navbar:size:0");
    assert_eq!(h.app.preferences.navigation_bar.size, Size::Compact);
    navbar_click(&mut h, "prefs:navbar:size:1");
    assert_eq!(h.app.preferences.navigation_bar.size, Size::Regular);
    navbar_click(&mut h, "prefs:navbar:buttons:1");
    assert_eq!(h.app.preferences.navigation_bar.labels, Labels::Labels);
    navbar_click(&mut h, "prefs:navbar:idle:0");
    assert_eq!(h.app.preferences.navigation_bar.idle, Idle::Always);
    navbar_click(&mut h, "prefs:navbar:idle:2");
    assert_eq!(h.app.preferences.navigation_bar.idle, Idle::Hide);
    let before = h.app.preferences.navigation_bar.clone();
    navbar_click(&mut h, "prefs:navbar:preview_as:1");
    assert_eq!(h.app.preferences.navigation_bar, before, "canvas preview is transient");
    navbar_click(&mut h, "prefs:navbar:add:zoom");
    assert_eq!(h.app.preferences.navigation_bar.slots.last().map(String::as_str), Some("zoom"));
    let shared = h.app.preferences.navigation_bar.clone();
    navbar_click(&mut h, "prefs:navbar:preview_as:0");
    assert_eq!(h.app.preferences.navigation_bar, shared, "Render edits persist in the shared Design preview");
    navbar_click(&mut h, "prefs:navbar:show");
    assert!(!h.app.ui.show_navigation_bar);
    navbar_click(&mut h, "prefs:navbar:show");
    assert!(h.app.ui.show_navigation_bar);
}

#[test]
fn navigation_bar_appearance_segments_stay_inside_the_page_at_narrow_widths() {
    for width in [1600, 880, 640, 600, 480] {
        let mut h = crate::scenario::Harness::new();
        h.app.preferences.language = "en".into();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":width,"height":1100}));
        h.call("ui.prefs", json!({"open":"Interface/Navigation bar", "navigation_bar":{"slots":["fit","visual_style","display","spin"]}}));
        h.frames(20);
        let header = crate::scenario::rect_of("dialog:header").unwrap();
        let search = crate::scenario::rect_of("prefs:search:frame").unwrap();
        assert!(header.contains_rect(search), "titlebar search stays inside the window at {width}");
        assert!(search.width() <= 230.0 && search.width() >= 100.0, "usable bounded search at {width}");
        let body = crate::scenario::rect_of("prefs:page_body").unwrap();
        for index in 0..6 {
            let cell = crate::scenario::rect_of(&format!("prefs:navbar:anchor:{index}")).unwrap();
            assert!(cell.left() >= body.left() && cell.right() <= body.right() + 0.5, "anchor {index} at {width}: {cell:?} outside {body:?}");
            assert!(cell.width() > 20.0 && cell.height() > 20.0, "anchor {index} stays a usable target at {width}");
        }
        for (key, label, labels) in [
            ("size", "Size", &["Compact", "Regular"][..]),
            ("buttons", "Buttons", &["Icons", "Icons + labels"][..]),
            ("idle", "When idle", &["Always", "Fade", "Hover"][..]),
        ] {
            let body = crate::scenario::rect_of("prefs:page_body").unwrap();
            let row = crate::scenario::rect_of(&format!("prefs:navbar:{key}")).unwrap();
            assert!(row.left() >= body.left() && row.right() <= body.right() + 0.5, "{key} at {width}: {row:?} outside {body:?}");
            assert_eq!(row.height(), crate::ui_kit::row_height(), "standard control height");
            let dropdown = crate::scenario::rect_of(&format!("dialog:segments:{label}")).is_none();
            if !dropdown {
                let control = crate::scenario::rect_of(&format!("dialog:control:{label}")).unwrap();
                assert!((row.width() - control.width()).abs() < 0.5, "{key} fills its property column at {width}");
            }
            if dropdown {
                // Standard Segments fall back to the kit dropdown when their labels cannot fit.
                navbar_click(&mut h, &format!("prefs:navbar:{key}"));
                h.frames(2);
            }
            for (index, option) in labels.iter().enumerate() {
                let cell = crate::scenario::rect_of(&format!("prefs:navbar:{key}:{index}")).unwrap();
                let at = crate::scenario::handle_at(&format!("prefs:navbar:{key}:{index}")).unwrap();
                assert!(cell.contains(at), "the scenario target is the actual choice");
                let mut text_found = false;
                for shape in &h.shapes {
                    crate::test_support::each_shape(&shape.shape, &mut |shape| {
                        if let egui::Shape::Text(text) = shape
                            && text.galley.job.text == *option
                        {
                            let bounds = text.galley.rect.translate(text.pos.to_vec2());
                            if cell.contains(bounds.center()) {
                                assert!(
                                    !text.galley.elided && cell.contains_rect(bounds),
                                    "{key}:{index} text clips at {width}: {bounds:?} in {cell:?}"
                                );
                                text_found = true;
                            }
                        }
                    });
                }
                assert!(text_found, "{key}:{index} has painted, readable text at {width}");
            }
            if dropdown {
                h.step(&json!({"click":{"handle":format!("prefs:navbar:{key}:{}", labels.len() - 1)}})).unwrap();
                h.frames(2);
            }
        }
    }
}

#[test]
fn navigation_bar_inspector_header_reserves_one_line_for_actions() {
    let t = crate::theme::Tokens::light();
    for width in [0.0, 80.0, 200.0, 520.0, 640.0, 760.0, 880.0] {
        for remove_width in [75.0, 160.0] {
            let bounds = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(width, t.control_height));
            let (name, buttons, compact) = super::navbar::inspector_header_layout(bounds, remove_width, &t);
            assert!(bounds.contains_rect(name));
            for rect in buttons {
                assert!(bounds.contains_rect(rect), "{width}: {rect:?} outside {bounds:?}");
                assert_eq!(rect.top(), bounds.top());
                assert_eq!(rect.bottom(), bounds.bottom());
                assert!(name.right() <= rect.left());
            }
            assert!((buttons.last().unwrap().right() - bounds.right()).abs() < 0.01);
            for pair in buttons.windows(2) {
                assert!(pair[0].right() <= pair[1].left());
            }
            if width >= 760.0 {
                assert!(!compact, "wide rows keep Remove's label");
            } else if width <= 80.0 {
                assert!(compact, "narrow rows keep Remove as an icon");
            }
            if !compact {
                assert_eq!(buttons[0].width(), t.control_height, "Move left is always icon-only");
                assert_eq!(buttons[1].width(), t.control_height, "Move right is always icon-only");
                assert_eq!(buttons[2].width(), remove_width);
            }
        }
    }
}

#[test]
fn navigation_bar_inspector_and_status_keep_full_width_for_every_selection() {
    for width in [480, 600, 880, 1030, 1200, 1600] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":width,"height":1100}));
        h.call("ui.prefs", json!({"open":"Interface/Navigation bar", "navigation_bar":{"slots":["pan","fit","visual_style","display","separator"]}}));
        h.frames(20);
        for slot in [None, Some("pan"), Some("fit"), Some("visual_style"), Some("display"), Some("separator")] {
            if let Some(slot) = slot {
                navbar_click(&mut h, &format!("prefs:navbar:slot:{slot}"));
                h.frames(3);
            }
            let body = crate::scenario::rect_of("prefs:page_body").unwrap();
            if slot.is_some() {
                let card = crate::scenario::rect_of("prefs:navbar:inspector").unwrap();
                assert!(
                    (card.left() - body.left()).abs() < 0.5 && (card.right() - body.right()).abs() < 0.5,
                    "{slot:?} at {width}: {card:?} vs {body:?}"
                );
            } else {
                assert!(crate::scenario::rect_of("prefs:navbar:inspector").is_none(), "inspector belongs to a selected slot");
            }
            let status = crate::scenario::rect_of("prefs:navbar:status").unwrap();
            let reset = crate::scenario::rect_of("prefs:navbar:reset").unwrap();
            let show = crate::scenario::rect_of("prefs:navbar:show").unwrap();
            assert!(status.left() >= body.left() && status.right() <= body.right() + 0.5, "header actions stay inside the page");
            let stage = crate::scenario::rect_of("prefs:navbar:stage").unwrap();
            let count = crate::scenario::rect_of("prefs:navbar:count").unwrap();
            assert!(status.bottom() <= stage.top(), "Show and Reset belong above the shared stage");
            let bar = crate::scenario::rect_of("prefs:navbar:bar").unwrap();
            assert!(stage.contains_rect(count), "the real slot count sits in the stage");
            assert!(!count.intersects(bar), "the count never covers the bar");
            let properties = crate::scenario::rect_of("dialog:row:Position").unwrap();
            assert!(stage.bottom() <= properties.top(), "the stage stays before properties");
            assert!(status.contains_rect(reset) && status.contains_rect(show));
            assert!(show.right() <= reset.left());
            assert_eq!(reset.center().y, show.center().y);
            if slot.is_some() {
                let header = crate::scenario::rect_of("prefs:navbar:inspector:header").unwrap();
                let name = crate::scenario::rect_of("prefs:navbar:inspector:name").unwrap();
                let actions: Vec<_> =
                    ["left", "right", "remove"].iter().map(|key| crate::scenario::rect_of(&format!("prefs:navbar:{key}")).unwrap()).collect();
                for rect in &actions {
                    assert!(header.contains_rect(*rect), "{slot:?} at {width}: {rect:?} outside {header:?}");
                    assert_eq!(rect.center().y, header.center().y);
                    assert!(name.right() <= rect.left());
                }
                assert_eq!(actions[0].width(), header.height(), "Move left stays icon-only");
                assert_eq!(actions[1].width(), header.height(), "Move right stays icon-only");
                assert_eq!(actions.last().unwrap().right(), header.right());
            }
            for (_, at) in crate::scenario::handles("prefs:navbar:add:") {
                assert!(at.x >= body.left() && at.x <= body.right(), "tray actions stay in their tiles");
            }
        }
    }
}

#[test]
fn interface_sections_share_the_page_content_width_before_and_after_navigation() {
    for width in [600, 880, 1030, 1200, 1496, 1600] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":width,"height":1100}));
        for (page, sections, rows) in [
            ("Appearance", &["theme", "scale", "colour_vision"][..], &["dialog:divider:scale_font", "dialog:row:Font"][..]),
            ("Layout", &["layout", "navigation_bar"][..], &["prefs:navbar:stage"][..]),
        ] {
            h.call("ui.prefs", json!({"open":page}));
            h.frames(20);
            for _ in 0..2 {
                let body = crate::scenario::rect_of("prefs:page_body").unwrap();
                let window = crate::scenario::rect_of("prefs:window").unwrap();
                let t = crate::theme::Tokens::get();
                let inset = t.space_24 - t.space_2 + crate::ui_kit::navigation_popup_frame().stroke.width;
                assert!((body.right() - (window.right() - inset)).abs() < 0.5, "{width}: {page}");
                for key in sections {
                    let section = crate::scenario::rect_of(&format!("prefs:section:{key}")).unwrap();
                    assert!(
                        (section.left() - body.left()).abs() < 0.5 && (section.right() - body.right()).abs() < 0.5,
                        "{key} at {width}: {section:?} vs {body:?}"
                    );
                }
                for key in rows {
                    let row = crate::scenario::rect_of(key).unwrap();
                    assert!((row.right() - body.right()).abs() < 0.5, "{key} at {width}: {row:?} vs {body:?}");
                }
                h.call("ui.scroll", json!({"x":body.center().x,"y":body.center().y,"delta":-600}));
                h.frames(20);
            }
        }
    }
}

#[test]
fn navigation_bar_preview_stage_follows_anchor_live() {
    use crate::navigation_bar::{Edge, Position};
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1100}));
    h.call("ui.prefs", json!({"open":"Interface/Navigation bar","navigation_bar":{"slots":["pan","fit","display"]}}));
    for (index, edge, position) in [
        (0, Edge::Top, Position::Left),
        (1, Edge::Top, Position::Centre),
        (2, Edge::Top, Position::Right),
        (3, Edge::Bottom, Position::Left),
        (4, Edge::Bottom, Position::Centre),
        (5, Edge::Bottom, Position::Right),
    ] {
        navbar_click(&mut h, &format!("prefs:navbar:anchor:{index}"));
        h.frames(3);
        let stage = crate::scenario::rect_of("prefs:navbar:stage").unwrap();
        let bar = crate::scenario::rect_of("prefs:navbar:bar").unwrap();
        let count = crate::scenario::rect_of("prefs:navbar:count").unwrap();
        assert_eq!((h.app.preferences.navigation_bar.edge, h.app.preferences.navigation_bar.position), (edge, position));
        assert_eq!(stage.height(), crate::ui_kit::NAV_STAGE_HEIGHT);
        let inset = crate::ui_kit::NAV_BORDER + crate::ui_kit::NAV_STAGE_INSET;
        match edge {
            Edge::Top => {
                assert_eq!(bar.top(), stage.top() + inset);
                assert!(count.top() >= bar.bottom(), "the count sits below a top bar");
            }
            Edge::Bottom => {
                assert_eq!(bar.bottom(), stage.bottom() - inset);
                assert!(count.bottom() <= bar.top(), "the count sits above a bottom bar");
            }
        }
        match position {
            Position::Left => assert_eq!(bar.left(), stage.left() + inset),
            Position::Centre => assert_eq!(bar.center().x, stage.center().x),
            Position::Right => assert_eq!(bar.right(), stage.right() - inset),
        }
    }
}

#[test]
fn navigation_bar_editor_drag_adds_reorders_and_removes() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1100}));
    h.call("ui.prefs", json!({"open":"Interface/Navigation bar", "navigation_bar":{"slots":["pan","fit","display"]}}));
    h.frames(20);
    let preview = crate::scenario::rect_of("prefs:navbar:preview").unwrap();
    h.step(&json!({"drag":[{"handle":"prefs:navbar:tile:spin"}, [preview.right()-8.0,preview.center().y]],"steps":12})).unwrap();
    h.frames(4);
    assert_eq!(h.app.preferences.navigation_bar.slots, ["pan", "fit", "display", "spin"]);
    let pan = crate::scenario::rect_of("prefs:navbar:slot:pan").unwrap();
    h.step(&json!({"drag":[{"handle":"prefs:navbar:slot:spin"}, [pan.left()+1.0,pan.center().y]],"steps":12})).unwrap();
    h.frames(4);
    assert_eq!(h.app.preferences.navigation_bar.slots, ["spin", "pan", "fit", "display"]);
    let preview = crate::scenario::rect_of("prefs:navbar:preview").unwrap();
    h.step(&json!({"drag":[{"handle":"prefs:navbar:slot:spin"}, [preview.center().x,preview.bottom()+80.0]],"steps":12})).unwrap();
    h.frames(4);
    assert_eq!(h.app.preferences.navigation_bar.slots, ["pan", "fit", "display"]);
    // Empty stage space is off the bar too; it must remove rather than append a moved slot.
    h.step(&json!({"drag":[{"handle":"prefs:navbar:slot:pan"}, [preview.right()-4.0,preview.center().y]],"steps":12})).unwrap();
    h.frames(4);
    assert_eq!(h.app.preferences.navigation_bar.slots, ["fit", "display"]);
}

#[test]
fn navigation_bar_menu_options_cancel_defaults_and_section_reset() {
    use crate::navigation_bar::NavigationBar;
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    let before = h.app.preferences.navigation_bar.clone();
    let options = json!({"slots":["visual_style","pan"],"slot_options":{"visual_style":{"shows":"icon_and_value"}},"size":"regular"});
    h.call("ui.prefs", json!({"open":"Interface/Navigation bar","navigation_bar":options}));
    h.app.run("view.navigation_bar", json!({"visible":false})).unwrap();
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert_eq!(h.app.preferences.navigation_bar, before);
    assert!(h.app.ui.show_navigation_bar);
    h.call("ui.prefs", json!({"open":"Interface/Navigation bar","navigation_bar":options}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert_eq!(h.app.preferences.navigation_bar, NavigationBar::default());
    h.call("ui.prefs", json!({"navigation_bar":options}));
    navbar_click(&mut h, "prefs:navbar:reset");
    assert_eq!(h.app.preferences.navigation_bar, NavigationBar::default());
    navbar_click(&mut h, "dialog:section:Navigation bar");
    assert!(crate::scenario::handle_at("prefs:navbar:slot:pan").is_none(), "the section collapsed");
    h.call("ui.prefs", json!({"open":"Interface/Navigation bar"}));
    h.frames(20);
    let preview = crate::scenario::rect_of("prefs:navbar:preview").unwrap();
    let body = crate::scenario::rect_of("prefs:page_body").unwrap();
    assert!(body.contains(preview.center()), "the route reopens and scrolls to the section");
}

#[test]
fn preferred_units_update_only_a_blank_unsaved_design() {
    for design in ["new", "saved", "content"] {
        let mut app = crate::SketApp::new(sketcraft_engine::Session::default(), Default::default());
        match design {
            "saved" => app.session.path = Some("empty.sketcraft".into()),
            "content" => {
                app.session.execute("sketch.create", &serde_json::json!({"plane": "XY"})).unwrap();
                app.session.mark_saved();
            }
            _ => {}
        }
        let dirty = app.session.is_dirty();
        let undo = app.session.undo.len();
        super::set(&mut app, &serde_json::json!({"default_units": "in"})).unwrap();
        assert_eq!(app.preferences.default_units, "in");
        assert_eq!(app.session.doc.units, if design == "new" { "in" } else { "mm" }, "{design}");
        assert_eq!(app.session.is_dirty(), dirty, "{design}");
        assert_eq!(app.session.undo.len(), undo, "{design}");
        if design == "new" {
            super::control(&mut app, &serde_json::json!({"open": "Design"})).unwrap();
            super::set(&mut app, &serde_json::json!({"default_units": "ft"})).unwrap();
            assert_eq!(app.session.doc.units, "ft");
            super::control(&mut app, &serde_json::json!({"button": "cancel"})).unwrap();
            assert_eq!(app.session.doc.units, "in");
            super::defaults(&mut app, super::PAGE_DESIGN);
            assert_eq!(app.session.doc.units, "mm");
            assert!(!app.session.is_dirty());
            assert!(app.session.undo.is_empty());
        }
    }
}

/// Scrolls vertically to the handle, then clicks after the wheel animation settles.
/// Edge-aligned controls are valid targets; scrolling cannot move them inward horizontally.
fn scroll_to_and_click(h: &mut crate::scenario::Harness, handle: &str) {
    for _ in 0..60 {
        let body = crate::scenario::rect_of("prefs:page_body").expect("the page body");
        let target = crate::scenario::handle_at(handle);
        if let Some(at) = target
            && body.contains(at)
        {
            // The wheel scrolls smoothly: let it come to rest, then click where the handle is now.
            h.frames(20);
            let at = crate::scenario::handle_at(handle).unwrap_or(at);
            if body.contains(egui::pos2(at.x, at.y)) {
                h.step(&serde_json::json!({"click": {"handle": handle}})).unwrap();
                h.frames(5);
                return;
            }
        }
        h.app.synthetic.push(egui::Event::PointerMoved(body.center()));
        h.app.synthetic.push(egui::Event::MouseWheel {
            source: egui::MouseWheelSource::Unknown,
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, if target.is_some_and(|at| at.y < body.top()) { 60.0 } else { -60.0 }),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.frames(2);
    }
    panic!("{handle} never came into view");
}

#[test]
fn model_navigation_is_opt_in_and_persists() {
    for to_cursor in [false, true] {
        let old = serde_json::from_value::<Prefs>(json!({"zoom_to_cursor": to_cursor})).unwrap();
        assert!(!old.zoom_stop_at_model, "older saved preferences keep legacy navigation");
        assert_eq!(old.zoom_to_cursor, to_cursor);
    }
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    assert!(!app.preferences.zoom_stop_at_model);
    let enabled = json!({"open": "Navigation", "zoom_stop_at_model": true});
    control(&mut app, &enabled).unwrap();
    control(&mut app, &json!({"button": "cancel"})).unwrap();
    assert!(!app.preferences.zoom_stop_at_model);
    let reply = control(&mut app, &enabled).unwrap();
    assert_eq!(reply["zoom_stop_at_model"], true);
    control(&mut app, &json!({"button": "apply"})).unwrap();
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert!(other.preferences.zoom_stop_at_model);
    assert!(other.preferences.zoom_to_cursor, "enabling model navigation leaves wheel zoom alone");
    control(&mut app, &json!({"button": "defaults"})).unwrap();
    assert!(!app.preferences.zoom_stop_at_model);
    assert!(app.preferences.zoom_to_cursor, "Navigation defaults still anchor the wheel");
    control(&mut app, &json!({"button": "cancel"})).unwrap();
    assert!(app.preferences.zoom_stop_at_model, "Cancel restores the last Apply");
}

#[test]
fn changing_preferences_page_cancels_shortcut_capture() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.prefs", json!({"open":"Keyboard Shortcuts"}));
    h.step(&json!({"click":{"handle":"keymap:filter"}})).unwrap();
    h.step(&json!({"text":"solid.extrude"})).unwrap();
    h.step(&json!({"click":{"handle":"keymap:capture:solid.extrude"}})).unwrap();
    h.step(&json!({"key":"M","pressed":true})).unwrap();
    h.step(&json!({"click":{"handle":"prefs:page:General"}})).unwrap();
    assert!(!crate::keymap::capturing(&h.app));
    assert_eq!(crate::keymap::effective(&h.app, "solid.extrude").as_deref(), Some("E"));
    h.frames(65);
    assert_eq!(crate::keymap::effective(&h.app, "solid.extrude").as_deref(), Some("E"));
}

#[test]
fn display_quality_cards_show_increasing_facet_detail() {
    let mut app = super::SketApp::new(Default::default(), Default::default());
    super::open_at(&mut app, super::PAGE_GRAPHICS);
    let ctx = crate::fonts::test_context();
    let mut shapes = Vec::new();
    for _ in 0..3 {
        let mut out = ctx.run_ui(
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0))), ..Default::default() },
            |ui| super::show(&mut app, ui.ctx()),
        );
        shapes = std::mem::take(&mut out.shapes);
        out.textures_delta.clear();
    }
    let mut previous = 0;
    for (label, segments) in [("Coarse", 8), ("Normal", 16), ("Fine", 32), ("Ultra", 64)] {
        let card = crate::scenario::rect_of(&format!("prefs:option:{label}")).expect("quality card");
        let facets = shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Path(path) = &shape.shape {
                    path.points.iter().all(|p| card.contains(*p)).then_some(path.points.len())
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0);
        assert!(facets > previous, "{label} must visibly add detail to the preceding quality: {facets} <= {previous}");
        assert_eq!(facets, segments + 1, "{label} uses the canonical faceting caption, with the closing vertex");
        previous = facets;
    }
}

#[test]
fn design_precision_controls_use_preferred_units_and_keep_document_and_transactions() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.prefs", json!({"language":"en", "theme":theme}));
        // A nonblank document keeps its units while the new-design defaults are edited.
        h.app.run("solid.box", json!({"length":10,"width":10,"height":10})).unwrap();
        let doc = h.app.session.doc.to_json();
        let original = h.app.preferences.clone();
        h.call("ui.prefs", json!({"open":"General"}));
        h.step(&json!({"click":{"handle":"prefs:page:Design"}})).unwrap();
        h.app.prefs_window.filter = "Units and precision".into();
        h.frames(5);
        for unit in UNITS {
            h.step(&json!({"click":{"handle":format!("prefs:default_units:{unit}")}})).unwrap();
            h.frames(2);
            let sample = format_length(12.5 * unit_scale(unit).0, unit, 2);
            assert!(h.widget(&sample).is_some(), "{theme}: {unit} uses the real unit-aware sample {sample}");
            assert_eq!(h.app.session.doc.to_json(), doc);
        }
        for (key, text) in [("length_decimals", "4"), ("angle_decimals", "3"), ("parameter_list_rows", "75")] {
            let handle = format!("prefs:{key}");
            super::reveal_preference(&mut h, &handle);
            h.step(&json!({"click":{"handle":handle},"double":true})).unwrap();
            h.step(&json!({"key":"A","cmd":true})).unwrap();
            h.step(&json!({"text":text})).unwrap();
            h.step(&json!({"key":"Enter"})).unwrap();
            h.frames(2);
        }
        assert_eq!((h.app.preferences.length_decimals, h.app.preferences.angle_decimals, h.app.preferences.parameter_list_rows), (4, 3, 75));
        assert!(h.widget(&format_length(12.5 * unit_scale("ft").0, "ft", 4)).is_some());
        assert!(h.widget("45.000°").is_some());
        assert_eq!(saved_prefs(&h.app)["preferences"]["length_decimals"], original.length_decimals);
        h.call("ui.prefs", json!({"button":"apply"}));
        let applied = h.app.preferences.clone();
        let mut restarted = SketApp::new(Default::default(), Default::default());
        restarted.load_prefs(&h.app.prefs());
        assert_eq!(
            (
                restarted.preferences.default_units.as_str(),
                restarted.preferences.length_decimals,
                restarted.preferences.angle_decimals,
                restarted.preferences.parameter_list_rows
            ),
            ("ft", 4, 3, 75)
        );
        h.call("ui.prefs", json!({"button":"defaults"}));
        assert_eq!((h.app.preferences.length_decimals, h.app.preferences.angle_decimals, h.app.preferences.parameter_list_rows), (2, 1, 50));
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert_eq!(h.app.preferences, applied);
        assert_eq!(h.app.session.doc.to_json(), doc);
    }
}

#[test]
fn design_unknown_program_dropdown_keeps_note_default_and_stand_up_dependency() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.prefs", json!({"language":"en","open":"Design"}));
    assert_eq!(h.app.preferences.stand_up_unknown, StandUpUnknown::Note);
    h.app.prefs_window.filter = "Files from unknown programs".into();
    h.frames(5);
    super::reveal_preference(&mut h, "prefs:stand_up_unknown");
    h.step(&json!({"click":{"handle":"prefs:stand_up_unknown"}})).unwrap();
    h.frames(3);
    h.step(&json!({"click":{"handle":"dialog:list:prefs.stand_up_unknown.ask"}})).unwrap();
    assert_eq!(h.app.preferences.stand_up_unknown, StandUpUnknown::Ask);
    h.app.prefs_window.filter = "Offer to stand up Y-up files".into();
    h.frames(5);
    super::reveal_preference(&mut h, "prefs:offer_stand_up");
    h.step(&json!({"click":{"handle":"prefs:offer_stand_up"}})).unwrap();
    assert!(!h.app.preferences.offer_stand_up);
    h.app.prefs_window.filter = "Files from unknown programs".into();
    h.frames(5);
    let tree = h.output.accesskit_update.as_ref().unwrap();
    let dropdown =
        tree.nodes.iter().find(|(_, node)| node.role() == egui::accesskit::Role::ComboBox && node.label() == Some("Files from unknown programs"));
    assert!(dropdown.expect("unknown-program dropdown").1.is_disabled());
    assert_eq!(h.app.preferences.stand_up_unknown, StandUpUnknown::Ask, "disabling the offer keeps the stored choice");
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(h.app.preferences.offer_stand_up);
    assert_eq!(h.app.preferences.stand_up_unknown, StandUpUnknown::Note);
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert_eq!(h.app.preferences.stand_up_unknown, StandUpUnknown::Note);
}

#[test]
fn tools_ghost_and_pointer_tiles_edit_only_their_state_and_preserve_transactions() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.prefs", json!({"language":"en","theme":theme,"quick_measure":true}));
        let original = h.app.preferences.clone();
        h.call("ui.prefs", json!({"open":"General"}));
        h.step(&json!({"click":{"handle":"prefs:page:Tools"}})).unwrap();
        let mut expected = serde_json::to_value(&original).unwrap();
        for (query, field) in [
            ("Show moved parts' old place as a ghost", "drive_joints_ghost"),
            ("Axis distances in Measure", "measure_axis_distances"),
            ("Direct interaction", "direct_interaction"),
        ] {
            h.app.prefs_window.filter = query.into();
            h.frames(5);
            let handle = format!("prefs:{field}");
            super::reveal_preference(&mut h, &handle);
            h.step(&json!({"click":{"handle":handle}})).unwrap();
            expected[field] = json!(!expected[field].as_bool().unwrap());
            assert_eq!(serde_json::to_value(&h.app.preferences).unwrap(), expected, "{theme}: {field} edits only its own preference");
        }
        h.app.prefs_window.filter = "Tool at the cursor".into();
        h.frames(5);
        super::reveal_preference(&mut h, "view:tool_cursor");
        h.step(&json!({"click":{"handle":"view:tool_cursor"}})).unwrap();
        assert!(h.app.ui.tool_cursor);
        assert_eq!(serde_json::to_value(&h.app.preferences).unwrap(), expected, "cursor stays app UI state");
        assert_eq!(saved_prefs(&h.app)["preferences"], serde_json::to_value(&original).unwrap());
        h.call("ui.prefs", json!({"button":"apply"}));
        let applied = h.app.preferences.clone();
        let mut restarted = SketApp::new(Default::default(), Default::default());
        restarted.load_prefs(&h.app.prefs());
        assert_eq!(restarted.preferences, applied);
        assert!(restarted.ui.tool_cursor);
        // The tool at the cursor is on by default (SketchUp's): turn it off again, so Restore
        // Defaults has it to turn on and Cancel to put back.
        h.frames(2);
        super::reveal_preference(&mut h, "view:tool_cursor");
        h.step(&json!({"click":{"handle":"view:tool_cursor"}})).unwrap();
        assert!(!h.app.ui.tool_cursor);
        h.call("ui.prefs", json!({"button":"apply"}));
        assert_eq!(h.app.preferences, applied, "the cursor tile edits no preference");
        h.call("ui.prefs", json!({"button":"defaults"}));
        assert!(h.app.preferences.drive_joints_ghost && h.app.preferences.measure_axis_distances);
        assert!(!h.app.preferences.direct_interaction && h.app.ui.tool_cursor);
        assert!(h.app.preferences.quick_measure, "Tools defaults leave Graphics quick measurement alone");
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert_eq!(h.app.preferences, applied);
        assert!(!h.app.ui.tool_cursor);
    }
}

#[test]
fn tools_extrude_view_controls_keep_their_dependency_without_clearing_the_saved_choice() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.prefs", json!({"language":"en","open":"Tools"}));
    h.app.prefs_window.filter = "Extrude".into();
    h.frames(5);
    for (key, isometric, returns) in [
        ("extrude_view_return", false, false),
        ("extrude_view_isometric", true, false),
        ("extrude_view_return", true, true),
        ("extrude_view_isometric", false, true),
        ("extrude_view_return", false, true),
    ] {
        let handle = format!("prefs:{key}");
        super::reveal_preference(&mut h, &handle);
        h.step(&json!({"click":{"handle":handle}})).unwrap();
        h.frames(2);
        assert_eq!((h.app.preferences.extrude_view_isometric, h.app.preferences.extrude_view_return), (isometric, returns));
        assert!(h.app.preferences.auto_operation);
        assert!(h.app.preferences.extrude_auto_to_object);
    }
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.extrude_view_isometric && !h.app.preferences.extrude_view_return);
}

#[test]
fn moved_preferences_reset_only_on_their_new_page() {
    use super::*;
    let fields = [
        ("default_units", json!("ft"), PAGE_DESIGN),
        ("length_decimals", json!(5), PAGE_DESIGN),
        ("angle_decimals", json!(4), PAGE_DESIGN),
        ("parameter_list_rows", json!(75), PAGE_DESIGN),
        ("step_values", json!(true), PAGE_DESIGN),
        ("value_boxes_in_view", json!(false), PAGE_DESIGN),
        ("up_axis", json!("y"), PAGE_DESIGN),
        ("offer_stand_up", json!(false), PAGE_DESIGN),
        ("stand_up_unknown", json!("ask"), PAGE_DESIGN),
        ("hide_origin", json!(true), PAGE_DESIGN),
        ("update_changed_links", json!(false), PAGE_DESIGN),
        ("double_click_components", json!(false), PAGE_DESIGN),
        ("insert_lists_open_designs", json!(false), PAGE_DESIGN),
        ("theme", json!("light"), PAGE_APPEARANCE),
        ("ui_scale", json!(1.25), PAGE_APPEARANCE),
        ("ui_font", json!("test font"), PAGE_APPEARANCE),
        ("layout", json!("compact"), PAGE_LAYOUT),
        ("ribbon_labels", json!(false), PAGE_LAYOUT),
        ("persistent_ribbon_pressed", json!(false), PAGE_LAYOUT),
        ("viewport_analysis_status", json!(false), PAGE_GRAPHICS),
        ("viewport_section_status", json!(false), PAGE_GRAPHICS),
        ("viewport_status_details", json!(false), PAGE_GRAPHICS),
        ("viewport_status_summary", json!(false), PAGE_GRAPHICS),
        ("sketch_tab_always", json!(true), PAGE_LAYOUT),
        ("sketchup_screen", json!(false), PAGE_LAYOUT),
        ("keep_work_on_tab_switch", json!(true), PAGE_LAYOUT),
        ("ribbon_reveal", json!(true), PAGE_LAYOUT),
        ("tab_double_click_renames", json!(false), PAGE_LAYOUT),
        ("dialog_buttons", json!("top"), PAGE_LAYOUT),
        ("toast_place", json!("centre"), PAGE_LAYOUT),
        ("ribbon_icon_grid", json!(true), PAGE_LAYOUT),
        ("ribbon_menus_follow_hover", json!(true), PAGE_LAYOUT),
        ("ribbon_menus_skip_pinned", json!(true), PAGE_LAYOUT),
        ("timeline_left", json!(true), PAGE_LAYOUT),
        ("timeline_under_browser", json!(true), PAGE_LAYOUT),
        ("history_sketches", json!("under_feature"), PAGE_LAYOUT),
        ("history_play_button", json!(false), PAGE_LAYOUT),
        ("history_group_joints", json!(true), PAGE_LAYOUT),
        ("history_bar_groups", json!(true), PAGE_LAYOUT),
        ("browser_float", json!(true), PAGE_LAYOUT),
        ("browser_collapsed", json!(true), PAGE_LAYOUT),
        ("browser_show", json!("hover"), PAGE_LAYOUT),
        ("dialog_place", json!("left_column"), PAGE_LAYOUT),
        ("color_vision", json!("protan"), PAGE_APPEARANCE),
        ("viewport_theme", json!("light"), PAGE_APPEARANCE),
        ("viewport_background", json!([255, 255, 255]), PAGE_APPEARANCE),
        ("view_confirm_corner", json!(true), PAGE_LAYOUT),
        ("status_bar", json!(true), PAGE_LAYOUT),
        ("browser_eye_left", json!(true), PAGE_LAYOUT),
        ("browser_components_open", json!(true), PAGE_LAYOUT),
        ("browser_rows_start_closed", json!(true), PAGE_LAYOUT),
        ("quick_measure", json!(true), PAGE_GRAPHICS),
        ("origin_plane_names", json!("top_front_right"), PAGE_DESIGN),
        ("direct_interaction", json!(true), PAGE_TOOLS),
        ("hole_snaps", json!(true), PAGE_TOOLS),
        ("hole_moves", json!(true), PAGE_TOOLS),
        ("extrude_arrow_face_stops", json!(false), PAGE_TOOLS),
        ("extrude_auto_to_object", json!(false), PAGE_TOOLS),
        ("extrude_view_isometric", json!(true), PAGE_TOOLS),
        ("extrude_view_return", json!(true), PAGE_TOOLS),
        ("auto_operation", json!(false), PAGE_TOOLS),
        ("placement_handles", json!(true), PAGE_TOOLS),
        ("assembly_move_handles", json!(false), PAGE_TOOLS),
        ("joint_rotation_coarse", json!(12.0), PAGE_TOOLS),
        ("joint_rotation_fine", json!(0.2), PAGE_TOOLS),
        ("drive_joints_ghost", json!(false), PAGE_TOOLS),
        ("measure_axis_distances", json!(false), PAGE_TOOLS),
        ("network_locations", json!(true), PAGE_GENERAL),
        ("network_hosts", json!(["files.studio"]), PAGE_GENERAL),
        ("nav", json!("blender"), PAGE_NAVIGATION),
        ("look_at_anim", json!(false), PAGE_NAVIGATION),
        ("selection_shift_add_only", json!(true), PAGE_SKETCH),
        ("select_other_hidden", json!(false), PAGE_SKETCH),
        ("select_hidden_edges", json!(true), PAGE_SKETCH),
        ("sketch_conflict_colour", json!(false), PAGE_SKETCH),
        ("hide_constraint_glyphs_zoomed_out", json!(false), PAGE_SKETCH),
        ("suppress_sketch_snap", json!(ModifierCombo::NONE), PAGE_SKETCH),
        ("sketch_state_icons", json!(false), PAGE_SKETCH),
        ("sketch_mirror_auto_line", json!(false), PAGE_SKETCH),
        ("sketch_tracking_guides", json!(false), PAGE_SKETCH),
        ("sketch_tracking_points", json!(false), PAGE_SKETCH),
        ("sketch_align_points", json!(true), PAGE_SKETCH),
        ("sketch_recent_snap", json!(true), PAGE_SKETCH),
        ("line_hover_tangent_arc", json!(true), PAGE_SKETCH),
        ("ribbon_click_ends_tool", json!(false), PAGE_SKETCH),
        ("msaa", json!(8), PAGE_GRAPHICS),
        ("single_surface_analysis", json!(false), PAGE_GRAPHICS),
    ];
    let defaults = serde_json::to_value(Prefs::default()).unwrap();
    for (field, value, owner) in fields {
        for page in 0..PAGES.len() {
            let mut data = defaults.clone();
            data[field] = value.clone();
            let mut prefs: Prefs = serde_json::from_value(data).unwrap();
            reset_preferences(&mut prefs, page);
            let result = serde_json::to_value(prefs).unwrap();
            assert_eq!(result[field], if page == owner { defaults[field].clone() } else { value.clone() }, "{field}, page {page}");
        }
    }
}

#[test]
fn page_aliases_and_transient_defaults_follow_the_new_groups() {
    use super::*;
    for name in ["Sketch", "Selection", "Sketch & selection"] {
        assert_eq!(page_index(name), Some(PAGE_SKETCH));
    }
    assert_eq!(page_index("Assistant"), Some(PAGE_ASSISTANT));
    assert_eq!(page_index("Shortcuts"), Some(PAGE_SHORTCUTS));
    let mut app = SketApp::new(Default::default(), Default::default());
    app.ui.agent_cursor.show = false;
    app.ui.pick_bodies = true;
    app.session.auto_project = false;
    defaults(&mut app, PAGE_GENERAL);
    assert!(!app.ui.agent_cursor.show);
    assert!(app.ui.pick_bodies);
    assert!(!app.session.auto_project);
    defaults(&mut app, PAGE_ASSISTANT);
    let expected = SketApp::new(Default::default(), Default::default()).ui.agent_cursor.show;
    assert_eq!(app.ui.agent_cursor.show, expected);
    defaults(&mut app, PAGE_SKETCH);
    assert!(!app.ui.pick_bodies);
    assert!(app.session.auto_project);
}

#[test]
fn preferences_structure_order_groups_and_unique_field_homes() {
    assert_eq!(
        PAGES,
        ["General", "Appearance", "Layout", "Graphics", "Design", "Sketch", "Tools", "Navigation", "Shortcuts", "Assistant", "Plugins"]
    );
    assert_eq!(WINDOW_PAGES.len(), PAGES.len());
    let groups: Vec<_> = WINDOW_PAGES.iter().enumerate().filter_map(|(i, p)| p.group.map(|g| (i, crate::i18n::tr(g)))).collect();
    assert_eq!(groups, [(PAGE_APPEARANCE, "Look"), (PAGE_DESIGN, "Work"), (PAGE_NAVIGATION, "Input"), (PAGE_ASSISTANT, "Extend")]);
    let mut fields = std::collections::BTreeSet::new();
    for (field, page) in FIELD_PAGES {
        assert!(fields.insert(field), "duplicate home for {field}");
        assert!(page.is_none_or(|p| p < PAGES.len()), "invalid page for {field}");
    }
    let saved = serde_json::to_value(Prefs::default()).unwrap();
    assert_eq!(fields.len(), saved.as_object().unwrap().len(), "every stored field has exactly one metadata entry");
    for (i, name) in PAGES.iter().enumerate() {
        assert_eq!(crate::i18n::tr(WINDOW_PAGES[i].label), *name);
        assert_eq!(WINDOW_PAGES[i].handle, format!("prefs:page:{name}"));
    }
}

#[test]
fn preferences_all_canonical_and_legacy_pages_open_and_publish_real_alias_handles() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1200}));
    for (name, index) in PAGES.iter().enumerate().map(|(i, n)| (*n, i)).chain(PAGE_ALIASES.iter().copied()) {
        assert_eq!(page_index(name), Some(index));
        let reply = h.call("ui.prefs", json!({"open":name}));
        assert!(reply["error"].is_null(), "{name}: {reply}");
        assert_eq!(reply["result"]["dialog"], PAGES[index]);
        open_at(&mut h.app, index);
        h.frames(4);
        assert_eq!(h.app.prefs_window.section, index);
        assert_eq!(crate::scenario::rect_of(&format!("prefs:page:{name}")), crate::scenario::rect_of(WINDOW_PAGES[index].handle));
        assert!(crate::scenario::rect_of(&format!("prefs:page:{name}")).is_some());
    }
    for (alias, index) in PAGE_ALIASES {
        open_at(&mut h.app, *index);
        h.frames(4);
        h.step(&json!({"click":{"handle":format!("prefs:page:{alias}")}})).unwrap();
        assert_eq!(h.app.prefs_window.section, *index, "{alias} is clickable");
    }
}

#[test]
fn preferences_moved_controls_have_one_drawn_home() {
    let cases = [
        ("Tool at the cursor", PAGE_TOOLS),
        ("Direct interaction", PAGE_TOOLS),
        ("Axis distances in Measure", PAGE_TOOLS),
        ("Move by click or drag", PAGE_TOOLS),
        ("Snap to centers", PAGE_TOOLS),
        ("Show moved parts' old place as a ghost", PAGE_TOOLS),
        ("Extrude to a picked target", PAGE_TOOLS),
        ("Turn the view to isometric when Extrude opens", PAGE_TOOLS),
        ("Go back to the earlier view when Extrude closes", PAGE_TOOLS),
        ("Placement handles", PAGE_TOOLS),
        ("Origin plane names", PAGE_DESIGN),
        ("Status bar", PAGE_LAYOUT),
        ("Color vision", PAGE_APPEARANCE),
        ("Quick measurement", PAGE_GRAPHICS),
        ("Analysis", PAGE_GRAPHICS),
        ("Section", PAGE_GRAPHICS),
        ("Live details", PAGE_GRAPHICS),
        ("Fold items", PAGE_GRAPHICS),
        ("Allow network locations in designs", PAGE_GENERAL),
        ("Print face highlight", PAGE_GENERAL),
    ];
    let mut found = vec![Vec::new(); cases.len()];
    let mut h = crate::scenario::Harness::new();
    h.app.preferences.language = "en".into();
    h.app.home.open = false;
    for page in 0..PAGES.len() {
        open_at(&mut h.app, page);
        h.frames(4);
        for (i, (label, _)) in cases.iter().enumerate() {
            if h.widget(label).is_some() {
                found[i].push(page);
            }
        }
    }
    for ((label, page), actual) in cases.iter().zip(found) {
        assert_eq!(actual, [*page], "{label} must have exactly one Preferences home");
    }
}

#[test]
fn preferences_session_resets_follow_their_new_pages_only() {
    for page in 0..PAGES.len() {
        let mut app = SketApp::new(Default::default(), Default::default());
        app.autosave_minutes = 9.0;
        app.ui.dark = false;
        app.prefs_window.follow.launch = Some("theme.json".into());
        app.ui.layout = crate::shell::LayoutMode::Docked;
        app.ui.show_navigation_bar = false;
        // Each value away from its page's default (SketchUp's: tool cursor on; no grid or ground
        // shadow, in perspective).
        app.ui.tool_cursor = false;
        app.ui.visual_style = 2;
        app.ui.show_grid = true;
        app.ui.ground_shadow = true;
        app.ui.perspective = false;
        app.session.auto_project = false;
        app.ui.pick_bodies = true;
        app.ui.agent_cursor.show = true;
        app.keymap.custom.insert("solid.extrude".into(), vec!["X".into()]);
        defaults(&mut app, page);
        assert_eq!(app.autosave_minutes, if page == PAGE_GENERAL { 5.0 } else { 9.0 });
        assert_eq!(app.ui.dark, page == PAGE_APPEARANCE);
        assert_eq!(app.prefs_window.follow.launch.is_none(), page == PAGE_APPEARANCE);
        assert_eq!(app.ui.layout == crate::shell::LayoutMode::Floating, page == PAGE_LAYOUT);
        assert_eq!(app.ui.show_navigation_bar, page == PAGE_LAYOUT);
        assert_eq!(app.ui.tool_cursor, page == PAGE_TOOLS);
        assert_eq!(app.ui.visual_style, if page == PAGE_GRAPHICS { 0 } else { 2 });
        assert_eq!(app.ui.show_grid, page != PAGE_GRAPHICS);
        assert_eq!(app.ui.ground_shadow, page != PAGE_GRAPHICS);
        assert_eq!(app.ui.perspective, page == PAGE_GRAPHICS);
        assert_eq!(app.session.auto_project, page == PAGE_SKETCH);
        assert_eq!(app.ui.pick_bodies, page != PAGE_SKETCH);
        assert_eq!(app.ui.agent_cursor.show, page != PAGE_ASSISTANT);
        assert_eq!(app.keymap.custom.is_empty(), page == PAGE_SHORTCUTS);
    }
}

/// The Ellipse tool draws its axes with "Draw ellipses with their axes" on (the default),
/// and the ellipse alone, as before, with it off.
#[test]
fn the_ellipse_tool_draws_its_axes_as_the_preference_says() {
    for (on, curves) in [(None, 3), (Some(false), 1), (Some(true), 3)] {
        let mut h = crate::scenario::Harness::new();
        if let Some(on) = on {
            set(&mut h.app, &serde_json::json!({"ellipse_axes": on})).unwrap();
        }
        assert_eq!(h.app.preferences.ellipse_axes, on.unwrap_or(true));
        h.app.run("sketch.create", serde_json::json!({"plane": "XY"})).unwrap();
        h.call("ui.view", serde_json::json!({"view": "top"}));
        h.step(&serde_json::json!({"start": "sketch.ellipse"})).unwrap();
        for at in [[0.0, 0.0], [20.0, 0.0], [0.0, 8.0]] {
            h.step(&serde_json::json!({"click": {"sketch": at}})).unwrap();
        }
        h.settle();
        let sid = h.app.session.active_sketch.unwrap();
        let sk = h.app.session.model.state().sketch(sid).unwrap().sketch.clone();
        assert_eq!(sk.curves.len(), curves, "preference {on:?}: {:?}", sk.curves);
    }
}

fn hole_preset_row(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name, "style": "counterbore", "extent": "distance",
        "diameter": "shaft_diameter + 0.2 mm", "depth": "0.5 in",
        "cb_diameter": "10 mm", "cb_depth": "3 mm", "cs_diameter": "10 mm",
        "cs_angle": "90 deg", "tip_angle": "118 deg", "tapped": true, "thread": "M5"
    })
}

#[test]
fn hole_presets_corrupt_rows_and_limits_do_not_discard_other_preferences() {
    use serde_json::json;
    let good = hole_preset_row("Workshop");
    let mut oversized = hole_preset_row("Too long");
    oversized["diameter"] = json!("x".repeat(513));
    let mut duplicate = good.clone();
    duplicate["name"] = json!("WORKSHOP");
    let p = super::lenient(&json!({"length_decimals":4,"hole_presets":[null,good,oversized,duplicate,{"name":"incomplete"}]}));
    assert_eq!(p.length_decimals, 4);
    let rows = serde_json::to_value(&p.hole_presets).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["name"], "Workshop");
    assert_eq!(rows[0]["diameter"], "shaft_diameter + 0.2 mm");
    assert_eq!(rows[0]["depth"], "0.5 in");
    let corrupt = super::lenient(&json!({"length_decimals":3,"hole_presets":{"unexpected":true}}));
    assert_eq!(corrupt.length_decimals, 3);
    assert_eq!(serde_json::to_value(corrupt.hole_presets).unwrap(), json!([]));
    let many: Vec<_> = (0..100).map(|i| hole_preset_row(&format!("Preset {i}"))).collect();
    let capped = super::lenient(&json!({"hole_presets":many}));
    assert_eq!(serde_json::to_value(capped.hole_presets).unwrap().as_array().unwrap().len(), 64);
}

#[test]
fn hole_presets_survive_application_preferences_roundtrip_and_page_resets() {
    use serde_json::json;
    let mut app = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    let mut row = hole_preset_row("Fixture");
    row["thread_options"] = json!({"full_length":false,"depth":"4 mm"});
    super::set(&mut app, &json!({"hole_presets":[row]})).unwrap();
    let expected = serde_json::to_value(&app.preferences.hole_presets).unwrap();
    assert_eq!(expected[0]["thread_options"]["depth"], "4 mm");
    for page in 0..super::PAGES.len() {
        super::reset_preferences(&mut app.preferences, page);
    }
    assert_eq!(serde_json::to_value(&app.preferences.hole_presets).unwrap(), expected);
    let saved = app.prefs();
    let mut other = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    other.load_prefs(&saved);
    assert_eq!(serde_json::to_value(&other.preferences.hole_presets).unwrap(), expected);
    assert!(other.session.doc.features.is_empty());
}

#[test]
fn hole_presets_keep_unknown_thread_options_for_safe_future_recall() {
    use serde_json::json;
    let mut row = hole_preset_row("Newer version");
    row["thread_options"] = json!({"starts":2});
    let p = super::lenient(&json!({"hole_presets":[row]}));
    let saved = serde_json::to_value(p.hole_presets).unwrap();
    assert_eq!(saved[0]["thread_options"]["starts"], 2, "unknown thread semantics must not silently become the defaults");
}

fn personal_sheet_rule_row(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name, "thickness": "1.6 mm", "k_factor": "0.42", "bend_radius": "Thickness",
        "relief_width": "Thickness", "relief_depth": "Thickness * 0.5", "corner_relief": "Thickness * 4.0",
        "corner_relief_shape": "round", "hem_gap": "0.01 mm", "gap": "Thickness", "angle_display": "included"
    })
}

#[test]
fn personal_sheet_rules_corrupt_rows_and_limits_do_not_discard_other_preferences() {
    use serde_json::json;
    let good = personal_sheet_rule_row("CRS 1.6");
    let mut oversized = personal_sheet_rule_row("Too long");
    oversized["thickness"] = json!("x".repeat(513));
    let mut duplicate = good.clone();
    duplicate["name"] = json!("crs 1.6");
    let p = super::lenient(&json!({"length_decimals":4,"personal_sheet_rules":[null,good,oversized,duplicate,{"name":"incomplete"}]}));
    assert_eq!(p.length_decimals, 4);
    assert_eq!(serde_json::to_value(&p.personal_sheet_rules).unwrap(), json!([personal_sheet_rule_row("CRS 1.6")]));
    let corrupt = super::lenient(&json!({"length_decimals":3,"personal_sheet_rules":{"unexpected":true}}));
    assert_eq!(corrupt.length_decimals, 3);
    assert_eq!(serde_json::to_value(corrupt.personal_sheet_rules).unwrap(), json!([]));
    let many: Vec<_> = (0..100).map(|i| personal_sheet_rule_row(&format!("Rule {i}"))).collect();
    let capped = super::lenient(&json!({"personal_sheet_rules":many}));
    assert_eq!(serde_json::to_value(capped.personal_sheet_rules).unwrap().as_array().unwrap().len(), 64);
}

#[test]
fn personal_sheet_rules_survive_application_preferences_roundtrip_and_page_resets() {
    use serde_json::json;
    let mut app = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    super::set(&mut app, &json!({"personal_sheet_rules":[personal_sheet_rule_row("CRS 1.6")]})).unwrap();
    let expected = serde_json::to_value(&app.preferences.personal_sheet_rules).unwrap();
    assert_eq!(expected[0]["angle_display"], "included");
    for page in 0..super::PAGES.len() {
        super::reset_preferences(&mut app.preferences, page);
    }
    assert_eq!(serde_json::to_value(&app.preferences.personal_sheet_rules).unwrap(), expected);
    let saved = app.prefs();
    let mut other = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    other.load_prefs(&saved);
    assert_eq!(serde_json::to_value(&other.preferences.personal_sheet_rules).unwrap(), expected);
    assert!(other.session.doc.sheet.rules.is_empty(), "a personal rule is not a rule of the design");
}

/// The interface font (#1290): a font that can be read, or empty for the built-in one; saved
/// with the others and reset by Restore Defaults.
#[test]
fn ui_font_is_checked_saved_and_restored() {
    use serde_json::json;
    let mut app = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    assert_eq!(app.preferences.ui_font, "");
    let family = sketcraft_engine::sketch::DEFAULT_FONT_FAMILY;
    let out = super::set(&mut app, &json!({ "ui_font": family })).unwrap();
    assert_eq!(out["ui_font"], family);
    let e = super::set(&mut app, &json!({"ui_font": "No Such Font 1290"})).unwrap_err();
    assert!(e.contains("ui_font"), "{e}");
    assert_eq!(app.preferences.ui_font, family, "a refused font leaves the choice as it was");
    let mut other = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    other.load_prefs(&app.prefs());
    assert_eq!(other.preferences.ui_font, family);
    super::defaults(&mut app, super::PAGE_APPEARANCE);
    assert_eq!(app.preferences.ui_font, "");
}

/// The web app's open tabs share their settings (#8644): a running tab loads what another
/// tab wrote over settings of its own. Everything it had changed is replaced, a pin or a
/// shortcut the other tab doesn't have included, so both tabs then save the same settings.
#[test]
fn settings_loaded_over_changed_ones_replace_them_all() {
    let new_app = || crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    let mut writer = new_app();
    writer.sbox.toggle_pin("sketch.line");
    writer.ui.dark = !writer.ui.dark;
    writer.keymap.custom.insert("view.fit".into(), vec!["Shift+F".into()]);
    let mut reader = new_app();
    reader.sbox.toggle_pin("feature.extrude");
    reader.keymap.custom.insert("view.home".into(), vec!["H".into()]);
    reader.toolbar_custom.panels.insert(format!("{}/SOLID/CREATE", crate::toolbar_custom::WORKSPACE), vec!["feature.extrude".into()]);
    reader.tree.collapsed.insert("Bodies".into());
    reader.home.recent.push("part.sketcraft".into());
    reader.preferences.ui_scale = 1.5;
    reader.load_prefs(&writer.prefs());
    assert_eq!(reader.sbox.pinned, ["sketch.line"], "the other tab's pins, not this tab's");
    assert!(!reader.keymap.custom.contains_key("view.home"));
    assert_eq!(reader.ui.dark, writer.ui.dark);
    assert_eq!(reader.prefs(), writer.prefs(), "both tabs save the same settings");
}

/// Bodies folders (#4217): Several bodies only by default, also in preferences saved before it
/// existed. Each value is saved and read back; `ui.prefs` refuses anything else and changes
/// nothing, and saved preferences read such a value as the default and keep the rest. The
/// Interface page's segments set it, Cancel puts it back and the page's Restore Defaults resets it.
#[test]
fn bodies_folders_is_saved_restored_and_refuses_unknown_values() {
    use super::*;
    let mut h = crate::scenario::Harness::new();
    assert_eq!(h.app.preferences.browser_bodies_folders, BodiesFolders::Several);
    assert_eq!(lenient(&json!({"theme": "light"})).browser_bodies_folders, BodiesFolders::Several, "saved before it existed");
    for (mode, id) in [(BodiesFolders::Always, "always"), (BodiesFolders::Several, "several"), (BodiesFolders::Never, "never")] {
        assert_eq!(control(&mut h.app, &json!({"browser_bodies_folders": id})).unwrap()["browser_bodies_folders"], id);
        let mut restored = SketApp::new(Default::default(), Default::default());
        restored.load_prefs(&h.app.prefs());
        assert_eq!(restored.preferences.browser_bodies_folders, mode, "{id}");
    }
    for bad in [json!("sometimes"), json!("Never"), json!(""), json!(2), json!(null), json!(["never"])] {
        let before = h.app.preferences.clone();
        assert!(set(&mut h.app, &json!({"browser_bodies_folders": bad})).is_err(), "{bad}");
        assert_eq!(h.app.preferences, before, "{bad}: a refused value changes nothing");
        let saved = lenient(&json!({"browser_bodies_folders": bad, "theme": "light"}));
        assert_eq!((saved.browser_bodies_folders, saved.theme.as_str()), (BodiesFolders::Several, "light"), "{bad}");
    }
    h.call("ui.prefs", json!({"open": "Layout"}));
    reveal_preference(&mut h, "dialog:Bodies folders:Always");
    h.step(&json!({"click": {"handle": "dialog:Bodies folders:Always"}})).unwrap();
    assert_eq!(h.app.preferences.browser_bodies_folders, BodiesFolders::Always);
    h.call("ui.prefs", json!({"button": "cancel"}));
    assert_eq!(h.app.preferences.browser_bodies_folders, BodiesFolders::Never, "Cancel puts it back");
    defaults(&mut h.app, PAGE_LAYOUT);
    assert_eq!(h.app.preferences.browser_bodies_folders, BodiesFolders::Several, "Interface's Restore Defaults");
}

/// Theme: System on a Linux desktop (#7944): the freedesktop preference wins, GNOME's
/// settings decide when it has none (its Light is `default`, so a plain theme name is
/// light), and nothing read leaves the window toolkit to say.
#[test]
fn the_desktops_colour_scheme_setting_decides_dark_or_light() {
    use super::desktop_prefers_dark as d;
    assert_eq!(d(Some(1), Some("prefer-light"), Some("Adwaita")), Some(true), "freedesktop: prefer dark");
    assert_eq!(d(Some(2), Some("prefer-dark"), Some("Adwaita-dark")), Some(false), "freedesktop: prefer light");
    assert_eq!(d(Some(0), Some("prefer-dark"), None), Some(true), "no preference: GNOME's");
    assert_eq!(d(Some(0), Some("'prefer-light'"), Some("Adwaita-dark")), Some(false), "gsettings quotes its answer");
    assert_eq!(d(Some(0), Some("default"), Some("Adwaita-dark")), Some(true), "a dark GTK theme");
    assert_eq!(d(None, None, Some("Yaru:dark")), Some(true));
    assert_eq!(d(Some(0), Some("default"), Some("Adwaita")), Some(false), "GNOME's Light");
    assert_eq!(d(Some(0), None, None), Some(false), "no preference at all: light, as GTK applications");
    assert_eq!(d(Some(u32::MAX), None, None), Some(false), "an unknown value is no preference");
    assert_eq!(d(None, None, Some("  ")), None, "nothing read");
    assert_eq!(d(None, None, None), None);
}

/// The host's reading of the desktop comes before the window toolkit's theme, which egui
/// reports as None in tests, and Theme: System follows it every frame (#7944).
#[test]
fn theme_system_follows_the_hosts_desktop_setting() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};
    let desktop = Arc::new(AtomicU8::new(0));
    let reading = desktop.clone();
    let services = crate::Services {
        desktop_dark: Some(Box::new(move || match reading.load(Ordering::Relaxed) {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        })),
        ..Default::default()
    };
    let mut app = SketApp::new(sketcraft_engine::Session::default(), services);
    app.preferences.theme = "system".into();
    app.ui.dark = true;
    let ctx = egui::Context::default();
    super::apply(&mut app, &ctx);
    assert!(app.ui.dark, "nothing known: the colours stay");
    desktop.store(2, Ordering::Relaxed);
    super::apply(&mut app, &ctx);
    assert!(!app.ui.dark, "the desktop turned light");
    desktop.store(1, Ordering::Relaxed);
    super::apply(&mut app, &ctx);
    assert!(app.ui.dark, "and dark again");
    app.preferences.theme = "light".into();
    app.ui.dark = false;
    super::apply(&mut app, &ctx);
    assert!(!app.ui.dark, "an explicit theme does not follow the desktop");
}

/// Compact output for MCP agents: off by default, a checkbox on the AI page (there with the
/// chat off too), reported to the bridged MCP server as `mcp.options`, reset by Restore Defaults.
#[test]
fn mcp_compact_output_is_an_ai_preference_the_server_reads() {
    use serde_json::json;
    let mut h = crate::scenario::Harness::new();
    assert_eq!(h.call("mcp.options", json!({}))["result"], json!({"compact": false}));
    h.call("ui.prefs", json!({"ai_features": false, "open": "AI"}));
    h.frames(3);
    assert!(crate::scenario::handle_at("prefs:mcp_compact").is_some(), "the checkbox is on the AI page with the chat off");
    // The page scrolls, so the setting is turned on as an agent would, not by a click.
    h.call("ui.prefs", json!({"mcp_compact": true}));
    h.frames(2);
    assert!(h.app.preferences.mcp_compact);
    assert_eq!(h.call("mcp.options", json!({}))["result"], json!({"compact": true}));
    h.call("ui.prefs", json!({"button": "defaults"}));
    assert_eq!(h.call("mcp.options", json!({}))["result"], json!({"compact": false}));
}

/// Theme files (#2255): a built-in one and an imported one turn on through `ui.prefs`, the
/// next frame draws with their colours (dark following the base), an export gives the theme
/// back complete, imported themes are saved with the preferences, Remove falls back to the
/// base and Cancel takes an import back.
#[test]
fn system_theme_export_uses_os_slot_without_selecting_it() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    app.preferences.theme = "light".into();
    app.ui.dark = false;
    assert_eq!(theme_file(&app, "system", true)["base"], "dark");
    assert_eq!(theme_file(&app, "system", false)["base"], "light");
    app.preferences.theme_day = "akari".into();
    app.preferences.theme_night = "onyx".into();
    assert_eq!(theme_file(&app, "system", false)["name"], "Akari");
    assert_eq!(theme_file(&app, "system", true)["name"], "Onyx");
    assert_eq!(app.preferences.theme, "light");
    assert!(!app.ui.dark);
}

/// The System theme leaves the native window to follow the operating system (#8983). A macOS
/// window told to be light or dark keeps that appearance and no longer reports the system
/// appearance changing, so the app stayed light after the system turned dark. Dark and Light
/// still give the window their own appearance, for its title bar.
#[test]
fn the_system_theme_leaves_the_window_to_follow_the_system() {
    let mut h = crate::scenario::Harness::new();
    // A frame with the system appearance `system`: the window themes the frame asked for.
    let frame = |h: &mut crate::scenario::Harness, system: egui::Theme| {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))),
            system_theme: Some(system),
            ..Default::default()
        };
        let app = &mut h.app;
        let mut out = h.ctx.run_ui(raw, |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        out.textures_delta.clear();
        let commands = out.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default();
        commands.into_iter().filter_map(|c| if let egui::ViewportCommand::SetTheme(t) = c { Some(t) } else { None }).collect::<Vec<_>>()
    };
    let frames = |h: &mut crate::scenario::Harness, system: egui::Theme| (0..3).flat_map(|_| frame(h, system)).collect::<Vec<_>>();
    let out = h.call("ui.prefs", json!({"theme": "system"}));
    assert_eq!(out["result"]["theme"], "system", "{out}");
    frames(&mut h, egui::Theme::Light);
    assert_eq!(h.app.prefs_window.window_theme, Some(egui::SystemTheme::SystemDefault));
    assert!(!h.ctx.options(|o| o.sync_window_theme), "egui would tell the window light or dark");
    assert!(!h.app.ui.dark);
    // The system turns dark: the app follows, and nothing is asked of the window.
    assert_eq!(frames(&mut h, egui::Theme::Dark), vec![]);
    assert!(h.app.ui.dark);
    assert_eq!(frames(&mut h, egui::Theme::Light), vec![]);
    assert!(!h.app.ui.dark);
    // An explicit theme gives the window its appearance, whatever the system's.
    let out = h.call("ui.prefs", json!({"theme": "light"}));
    assert_eq!(out["result"]["theme"], "light", "{out}");
    assert_eq!(frames(&mut h, egui::Theme::Dark), vec![]);
    assert_eq!(h.app.prefs_window.window_theme, Some(egui::SystemTheme::Light));
    assert!(!h.app.ui.dark);
}

/// The 3D view in the other theme's colours (#4146): `ui.prefs` sets it, the window keeps its
/// theme and the next frame draws the view from the other side, and it is saved with the
/// preferences. With a theme file on, the view takes the built-in Light or Dark colours.
#[test]
fn the_3d_view_takes_the_light_or_dark_themes_colours() {
    use crate::theme::{ColorVision, Tokens};
    use serde_json::json;
    let mut h = crate::scenario::Harness::new();
    for (theme, view, dark, view_dark) in
        [("light", "theme", false, false), ("dark", "light", true, false), ("dark", "dark", true, true), ("light", "dark", false, true)]
    {
        let out = h.call("ui.prefs", json!({"theme": theme, "viewport_theme": view}));
        assert_eq!((out["result"]["dark"].as_bool(), out["result"]["viewport_dark"].as_bool()), (Some(dark), Some(view_dark)), "{out}");
        h.frames(2);
        let t = Tokens::get();
        assert_eq!(t.panel, Tokens::themed(dark, ColorVision::Off).panel, "{theme}, {view}: the window");
        assert_eq!(t.viewport_top, Tokens::themed(view_dark, ColorVision::Off).viewport_top, "{theme}, {view}: the view");
    }

    let mut other = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    other.load_prefs(&h.app.prefs());
    assert_eq!((other.preferences.theme.as_str(), other.ui.dark, other.viewport_dark()), ("light", false, true));

    let out = h.call("ui.prefs", json!({"theme": "gruvbox-dark-hard", "viewport_theme": "light"}));
    assert_eq!((out["result"]["dark"].as_bool(), out["result"]["viewport_dark"].as_bool()), (Some(true), Some(false)), "{out}");
    h.frames(2);
    let hard = resolved_theme(&h.app.preferences, active_theme_file(&h.app).unwrap());
    assert_eq!((Tokens::get().panel, Tokens::get().viewport_top), (hard.panel, Tokens::light().viewport_top));

    // Asked for, an unknown value is refused; read from saved preferences, it is the default.
    assert!(h.call("ui.prefs", json!({"viewport_theme": "dim"}))["error"].is_string());
    let read: crate::theme::ViewportTheme = serde_json::from_value(json!("dim")).unwrap();
    assert_eq!(read, crate::theme::ViewportTheme::Theme);
}

/// The 3D view's background in one colour of the user's (#7688): `ui.prefs` sets it, the next
/// frame fills the view with it in either theme while the window keeps its colours, a picture
/// of the view has it too, and it is saved with the preferences. `null` gives back the theme's
/// gradient; a value that is not a colour is refused, and read from saved preferences it is
/// the gradient.
#[test]
fn the_3d_view_takes_a_background_colour_of_its_own() {
    use crate::theme::{ColorVision, Tokens};
    use serde_json::json;
    let mut h = crate::scenario::Harness::new();
    assert_eq!(h.app.preferences.viewport_background, None);
    for (theme, view_dark) in [("light", false), ("dark", true)] {
        let out = h.call("ui.prefs", json!({"theme": theme, "viewport_background": [255, 255, 255]}));
        assert!(out["error"].is_null(), "{out}");
        h.frames(2);
        let (t, white) = (Tokens::get(), egui::Color32::WHITE);
        assert_eq!((t.viewport_top, t.viewport_bottom, Tokens::view().viewport_top), (white, white, white), "{theme}");
        assert_eq!(t.panel, Tokens::themed(view_dark, ColorVision::Off).panel, "{theme}: the window");
        let scene = crate::control::view_scene(&h.app, sketcraft_engine::view::Guides::default()).unwrap();
        let white = sketcraft_engine::render::Rgb(255, 255, 255);
        assert_eq!(scene.background, Some((white, white)), "{theme}: a picture of the view");
    }

    let mut other = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    other.load_prefs(&h.app.prefs());
    assert_eq!(other.preferences.viewport_background, Some([255, 255, 255]));

    h.call("ui.prefs", json!({"viewport_background": null}));
    h.frames(2);
    let (t, dark) = (Tokens::get(), Tokens::dark());
    assert_eq!((t.viewport_top, t.viewport_bottom), (dark.viewport_top, dark.viewport_bottom), "the theme's gradient again");

    for bad in [json!([256, 0, 0]), json!("white"), json!([1, 2])] {
        assert!(h.call("ui.prefs", json!({"viewport_background": bad}))["error"].is_string(), "{bad}");
        assert_eq!(lenient(&json!({"viewport_background": bad})).viewport_background, None, "{bad}");
    }
}

/// The theme strip's Import… reads the theme file through `vfs`, like the other imports: in the
/// web app a picked file is kept there, where `std::fs` can't read it (#7643). On the disk it
/// is the same file; a folder scope active while it is read refuses a path outside it, which
/// shows the read goes through `vfs`.
#[test]
fn a_theme_pasted_or_dropped_on_the_interface_page_imports_and_other_pastes_pass_through() {
    use serde_json::json;
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    let theme = |name: &str| format!(r##"{{"format":"sketcraft-theme/1","name":"{name}","base":"dark","colors":{{"panel":"#102030"}}}}"##);
    let has = |h: &crate::scenario::Harness, id: &str| h.app.preferences.custom_themes.iter().any(|t| t.id == id);

    // Another page: a theme's text is left to the usual paste.
    h.call("ui.prefs", json!({"open":"General"}));
    h.frames(2);
    h.frame_with(vec![egui::Event::Paste(theme("Elsewhere"))]);
    assert!(!has(&h, "elsewhere"));

    // The Interface page: plain text is not a theme and changes nothing.
    h.call("ui.prefs", json!({"open":"Interface"}));
    h.frames(2);
    let before = h.app.preferences.clone();
    h.frame_with(vec![egui::Event::Paste("just some words".into())]);
    assert_eq!(h.app.preferences.custom_themes, before.custom_themes);

    // A theme's text imports and turns on, as Import… does with a file.
    h.frame_with(vec![egui::Event::Paste(theme("Pasted"))]);
    assert!(has(&h, "pasted"), "{:?}", h.app.preferences.custom_themes.iter().map(|t| &t.id).collect::<Vec<_>>());
    assert_eq!(h.app.preferences.theme, "pasted");

    // An invalid theme says why, and imports nothing.
    h.frame_with(vec![egui::Event::Paste(r#"{"format":"sketcraft-theme/1","name":"Bad","base":"sideways"}"#.into())]);
    assert!(!has(&h, "bad"));

    // A file dropped on the Import tile is a theme to import, not a design to open.
    h.frames(2);
    let tile = h.app.prefs_window.import_tile.expect("the Import tile is drawn on the Interface page");
    let dir = std::env::temp_dir().join(format!("sketcraft-theme-drop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("dropped.json");
    std::fs::write(&file, theme("Dropped")).unwrap();
    #[derive(Debug)]
    struct File(std::path::PathBuf);
    impl egui::DroppedFile for File {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            std::fs::read(&self.0).map_err(|e| e.to_string())
        }
    }
    let tabs = h.app.docs.count();
    h.frame_with(vec![egui::Event::PointerMoved(tile.center())]);
    h.drop_files(tile.center(), vec![std::sync::Arc::new(File(file)) as egui::DroppedFileHandle]);
    h.frames(2);
    assert!(has(&h, "dropped"));
    assert_eq!(h.app.docs.count(), tabs, "the drop opened no design");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn theme_import_reads_through_vfs() {
    use serde_json::json;
    use sketcraft_engine::io::vfs;
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.prefs", json!({"theme": "dark", "open": "Interface"}));
    h.frames(2);
    assert!(h.app.prefs_window.import_tile.is_some(), "Import is always visible");
    let (root, out) = (h.dir.join("root"), h.dir.join("out"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let file = json!({"format": "sketcraft-theme/1", "name": "Paper", "base": "light", "colors": {"panel": "#fffdf0"}});
    for (path, inside) in [(out.join("Paper.json"), false), (root.join("Paper.json"), true)] {
        std::fs::write(&path, file.to_string()).unwrap();
        let answer = path.to_string_lossy().to_string();
        h.app.services.pick_open = Some(Box::new(move |_| Box::pin(std::future::ready(Some(answer.clone())))));
        h.app.status = None;
        vfs::scoped(Some(vfs::PathScope::new([root.clone()])), || {
            h.step(&json!({"click":{"handle":"prefs:theme_import"}})).unwrap();
            h.frames(4);
        });
        let status = h.app.status.clone().map(|(text, ..)| text);
        if inside {
            assert_eq!(status.as_deref(), Some("Theme imported: Paper"));
            assert_eq!(h.app.preferences.theme, "paper");
        } else {
            assert!(status.as_deref().is_some_and(|s| s.contains("outside the folders this agent may use")), "{status:?}");
            assert!(h.app.preferences.custom_themes.is_empty(), "nothing imported");
        }
    }
}

#[test]
fn a_followed_theme_file_applies_reloads_keeps_the_last_good_theme_and_the_flag_is_not_saved() {
    use crate::theme::Tokens;
    use egui::Color32;
    use serde_json::json;
    let mut h = crate::scenario::Harness::new();
    assert_eq!(h.app.preferences.theme_follow, "", "off by default");
    let out = h.call("ui.prefs", json!({}));
    assert!(out["result"]["theme_followed"].is_null() && out["result"]["theme_follow_error"].is_null(), "{out}");

    let dir = std::env::temp_dir().join(format!("sketcraft-prefs-follow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("live.json");
    let path = file.to_string_lossy().to_string();
    let write = |panel: &str, extra: &str| {
        let text = format!(r##"{{"format":"sketcraft-theme/1","name":"Live","base":"light","colors":{{"panel":"{panel}"{extra}}}}}"##);
        std::fs::write(&file, text).unwrap();
    };
    write("#102030", "");
    let out = h.call("ui.prefs", json!({"theme_follow": path}));
    assert_eq!(
        (out["result"]["theme_follow"].as_str(), out["result"]["theme_followed"].as_str()),
        (Some(path.as_str()), Some(path.as_str())),
        "{out}"
    );
    assert!(out["result"]["theme_follow_error"].is_null(), "{out}");
    h.frames(2);
    assert_eq!(Tokens::get().panel, Color32::from_rgb(0x10, 0x20, 0x30));
    assert!(!h.app.ui.dark, "the file's base");

    // Another tool rewrites it: reloaded within the poll interval, without a restart.
    write("#405060", r##","accent":"#aa3300""##);
    h.frames(70);
    assert_eq!(Tokens::get().panel, Color32::from_rgb(0x40, 0x50, 0x60));

    // Broken or missing: the last good theme stays on and the reply says why.
    std::fs::write(&file, "{\"format\": \"sketcraft-theme/1\", \"na").unwrap();
    h.frames(70);
    assert_eq!(Tokens::get().panel, Color32::from_rgb(0x40, 0x50, 0x60));
    let out = h.call("ui.prefs", json!({}));
    assert!(out["result"]["theme_follow_error"].as_str().is_some_and(|e| e.contains("not a theme file")), "{out}");
    std::fs::remove_file(&file).unwrap();
    let out = h.call("ui.prefs", json!({"theme_follow": path}));
    assert!(out["result"]["theme_follow_error"].as_str().is_some_and(|e| e.contains("can't read")), "{out}");

    // Choosing a theme, or "", stops following.
    let out = h.call("ui.prefs", json!({"theme": "dark"}));
    assert!(out["result"]["theme_followed"].is_null() && out["result"]["theme_follow"] == "", "{out}");
    h.frames(2);
    assert_eq!(Tokens::get().panel, Tokens::dark().panel);

    // The launch flag follows a file for this run only: the saved preferences never name it.
    write("#708090", "");
    follow_theme_file_for_run(&mut h.app, &path).unwrap();
    h.frames(2);
    assert_eq!(Tokens::get().panel, Color32::from_rgb(0x70, 0x80, 0x90));
    assert_eq!(h.call("ui.prefs", json!({}))["result"]["theme_followed"].as_str(), Some(path.as_str()));
    assert_eq!(h.app.preferences.theme_follow, "");
    assert!(!h.app.prefs().contains("live.json"));
    h.call("ui.prefs", json!({"theme_follow": ""}));
    h.frames(2);
    assert_eq!(Tokens::get().panel, Tokens::dark().panel);

    // Hostile values are refused; the browser has no files to follow.
    assert!(h.call("ui.prefs", json!({"theme_follow": 5}))["error"].is_string());
    assert!(h.call("ui.prefs", json!({"theme_follow": "a\nb"}))["error"].is_string());
    assert!(h.call("ui.prefs", json!({"theme_follow": "x".repeat(5000)}))["error"].is_string());
    let web = set_theme_follow(&mut h.app, &json!(path), false).unwrap_err();
    assert!(web.contains("not available in the browser"), "{web}");
    assert_eq!(h.app.preferences.theme_follow, "");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn appearance_sections_and_carousel_use_real_data_and_stay_in_the_page() {
    for base in ["light", "dark"] {
        for screen in [740, 1600] {
            let mut h = crate::scenario::Harness::new();
            h.app.home.open = false;
            h.call("ui.resize", json!({"width":screen,"height":1100}));
            h.call("ui.prefs", json!({"theme":base,"language":"en","open":"Appearance"}));
            h.frames(4);
            let page = crate::scenario::rect_of("prefs:page_body").unwrap();
            let mut previous: Option<f32> = None;
            for key in ["theme", "viewport_logo", "scale", "colour_vision"] {
                let section = crate::scenario::rect_of(&format!("prefs:section:{key}")).unwrap();
                assert!(section.left() >= page.left() - 1.0 && section.right() <= page.right() + 1.0);
                if let Some(bottom) = previous {
                    assert!((section.top() - bottom - 38.0).abs() <= 1.0, "{base} width={screen}: {key}");
                }
                previous = Some(crate::scenario::rect_of(&format!("prefs:section:{key}:body")).unwrap().bottom());
            }
            let choices = theme_choices(&h.app);
            let count = format!("{} themes", choices.len());
            let mut painted_count = false;
            for shape in &h.shapes {
                crate::test_support::each_shape(&shape.shape, &mut |shape| {
                    if let egui::Shape::Text(text) = shape {
                        painted_count |= text.galley.job.text == count;
                    }
                });
            }
            assert!(painted_count, "count comes from actual themes");
            while crate::scenario::rect_of("prefs:theme_previous").is_some() {
                h.step(&json!({"click":{"handle":"prefs:theme_previous"}})).unwrap();
                h.frames(3);
            }
            let mut found = std::collections::BTreeSet::new();
            for _ in 0..choices.len() + 1 {
                for (id, _) in &choices {
                    if let Some(rect) = crate::scenario::rect_of(&format!("prefs:theme:{id}")) {
                        assert!(page.expand(1.0).contains_rect(rect), "{base} width={screen}: {id} {rect:?}");
                        found.insert(id.clone());
                    }
                }
                if crate::scenario::rect_of("prefs:theme_next").is_none() {
                    break;
                }
                h.step(&json!({"click":{"handle":"prefs:theme_next"}})).unwrap();
                h.frames(3);
            }
            assert_eq!(found, choices.into_iter().map(|(id, _)| id).collect());
            assert!(h.app.prefs_window.import_tile.is_some(), "Import stays visible while scrolling");
            assert_eq!(h.app.preferences.theme, base, "scrolling does not select a theme");
            assert_eq!(h.app.preferences.ui_scale, 1.0);
            assert_eq!(h.app.preferences.viewport_background, None);
            assert_eq!(h.app.preferences.viewport_theme, crate::theme::ViewportTheme::Theme);
            assert!(!h.app.preferences.logo_accent);
            assert!(h.app.preferences.theme_follow.is_empty() && h.app.preferences.ui_font.is_empty());
            assert_eq!(h.app.preferences.color_vision, crate::theme::ColorVision::Off);
            h.call("ui.prefs", json!({"button":"cancel"}));
        }
    }
}

#[test]
fn appearance_controls_edit_real_fields_and_cancel_restores_them() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1100}));
    h.call("ui.prefs", json!({"language":"en"}));
    h.call("ui.prefs", json!({"open":"Interface"}));
    h.frames(3);
    assert_eq!(h.app.prefs_window.section, PAGE_APPEARANCE, "legacy alias remains supported");
    let before = h.app.preferences.clone();
    for (handle, expected) in
        [("prefs:viewport_theme:light", crate::theme::ViewportTheme::Light), ("prefs:viewport_theme:dark", crate::theme::ViewportTheme::Dark)]
    {
        super::reveal_preference(&mut h, handle);
        h.step(&json!({"click":{"handle":handle}})).unwrap();
        h.frames(2);
        assert_eq!(h.app.preferences.viewport_theme, expected);
    }
    super::reveal_preference(&mut h, "dialog:Background:Color");
    h.step(&json!({"click":{"handle":"dialog:Background:Color"}})).unwrap();
    h.frames(2);
    assert_eq!(h.app.preferences.viewport_background, Some([255, 255, 255]));
    super::reveal_preference(&mut h, "prefs:viewport_background_hex");
    h.step(&json!({"click":{"handle":"prefs:viewport_background_hex"}})).unwrap();
    h.step(&json!({"key":"A","cmd":true})).unwrap();
    h.step(&json!({"text":"#123456"})).unwrap();
    h.frames(2);
    assert_eq!(h.app.preferences.viewport_background, Some([0x12, 0x34, 0x56]));
    super::reveal_preference(&mut h, "prefs:viewport_background_picker");
    h.step(&json!({"click":{"handle":"prefs:viewport_background_picker"}})).unwrap();
    h.frames(2);
    assert!(crate::scenario::rect_of("color:popover").is_some(), "the standard picker opens beside the hex editor");
    h.step(&json!({"click":{"handle":"color:spin:R:up"}})).unwrap();
    h.frames(2);
    assert_eq!(h.app.preferences.viewport_background, Some([0x13, 0x34, 0x56]), "picker edits the same RGB as hex");
    h.step(&json!({"click":{"handle":"prefs:viewport_background_hex"}})).unwrap();
    h.frames(2);
    assert!(h.widget("#133456").is_some(), "a picker edit refreshes the inline hex field");
    super::reveal_preference(&mut h, "dialog:Logo:Accent");
    h.step(&json!({"click":{"handle":"dialog:Logo:Accent"}})).unwrap();
    h.frames(2);
    assert!(h.app.preferences.logo_accent);
    for (percent, scale) in [(75, 0.75), (100, 1.0), (110, 1.1), (125, 1.25), (150, 1.5), (200, 2.0)] {
        let handle = format!("prefs:scale_preset:{percent}");
        // Rescaling may leave the presets above the current vertical scroll position.
        let page = crate::scenario::rect_of("prefs:page_body").unwrap();
        h.call("ui.scroll", json!({"x":page.center().x,"y":page.center().y,"delta":5000}));
        h.frames(20);
        super::reveal_preference(&mut h, &handle);
        h.step(&json!({"click":{"handle":handle}})).unwrap();
        h.frames(4);
        assert!((h.app.preferences.ui_scale - scale).abs() < 0.001);
    }
    h.call("ui.prefs", json!({"button":"cancel"}));
    h.frames(2);
    assert_eq!(h.app.preferences, before);
}

fn theme_options_fixture(base: &str, count: usize) -> serde_json::Value {
    let fixtures: serde_json::Value = serde_json::from_str(include_str!("../tests/fixtures/theme-options.json")).unwrap();
    fixtures[format!("{base}-{count}")].clone()
}

#[test]
fn theme_options_ui_counts_badges_more_and_reset_in_light_and_dark() {
    for base in ["light", "dark"] {
        for count in [0, 1, 3, 6, 12] {
            let mut h = crate::scenario::Harness::new();
            h.app.home.open = false;
            h.call("ui.resize", json!({"width":1600,"height":1100}));
            let fixture = theme_options_fixture(base, count);
            // The 12-cell fixture also declares a composite child, counted in the header.
            let option_count = fixture["options"].as_array().expect("fixture option definitions").len();
            let out = h.call("ui.prefs", json!({"theme_import":fixture,"open":"Interface"}));
            assert!(out["error"].is_null(), "{out}");
            h.frames(3);
            assert_eq!(crate::scenario::rect_of("prefs:theme_options_header").is_some(), count > 0, "{base} {count}");
            let caption = if option_count == 1 { "1 option".to_owned() } else { format!("{option_count} options") };
            let mut painted_count = 0;
            for shape in &h.shapes {
                crate::test_support::each_shape(&shape.shape, &mut |shape| {
                    if let egui::Shape::Text(text) = shape {
                        painted_count += usize::from(text.galley.job.text == caption);
                    }
                });
            }
            assert_eq!(painted_count, usize::from(count > 0), "{base}: the real option count paints the exact singular/plural caption once");
            let id = format!("options-{base}-{count}");
            assert_eq!(crate::scenario::rect_of(&format!("prefs:theme_options_badge:{id}")).is_some(), count > 0);
            assert_eq!(crate::scenario::handle_at("prefs:theme_options_more").is_some(), count == 12);
            let cells: std::collections::BTreeSet<_> = crate::scenario::rects("prefs:theme_option_cell:").into_iter().map(|(id, _)| id).collect();
            assert_eq!(cells.len(), count.min(10), "only ten top-level cells before More");
            assert_eq!(
                crate::scenario::rect_of("dialog:hint:theme_option_section").is_some(),
                count > 6,
                "dim section labels only above six entries"
            );
            assert!(crate::scenario::handle_at("prefs:theme_options_reset").is_none());
            if count == 0 {
                let viewport = crate::scenario::rect_of("prefs:section:viewport_logo").unwrap();
                let mut unknown = theme_options_fixture(base, count);
                unknown["options"] = json!([]);
                h.call("ui.prefs", json!({"theme_import":unknown}));
                h.frames(2);
                assert!(crate::scenario::rect_of("prefs:theme_options_header").is_none());
                assert!(crate::scenario::rect_of(&format!("prefs:theme_options_badge:{id}")).is_none());
                assert_eq!(crate::scenario::rect_of("prefs:section:viewport_logo").unwrap(), viewport, "empty declarations add no gap or notice");
            }
            if count == 12 {
                assert!(crate::scenario::rect_of("prefs:theme_option:outline").is_none());
                assert!(crate::scenario::rect_of("prefs:theme_option:extra").is_none(), "advanced is always under More");
                super::reveal_preference(&mut h, "prefs:theme_options_more");
                h.step(&json!({"click":{"handle":"prefs:theme_options_more"}})).unwrap();
                h.frames(2);
                assert!(crate::scenario::rect_of("prefs:theme_option:outline").is_some());
                assert!(crate::scenario::rect_of("prefs:theme_option:extra").is_some());
            }
            if count >= 3 {
                super::reveal_preference(&mut h, "prefs:theme_option_switch:Warm accent");
                h.step(&json!({"click":{"handle":"prefs:theme_option_switch:Warm accent"}})).unwrap();
                h.frames(2);
                assert_eq!(h.app.preferences.theme_options[&id]["warm"], true, "UI edits go through preference merging");
                assert!(crate::scenario::handle_at("prefs:theme_options_reset").is_some());
                super::reveal_preference(&mut h, "prefs:theme_options_reset");
                h.step(&json!({"click":{"handle":"prefs:theme_options_reset"}})).unwrap();
                h.frames(2);
                assert!(!h.app.preferences.theme_options.contains_key(&id));
                assert!(crate::scenario::handle_at("prefs:theme_options_reset").is_none());
            }
        }
    }
}

#[test]
fn theme_options_ui_short_composite_shares_row_with_narrow_control() {
    for base in ["light", "dark"] {
        for language in ["en", "de"] {
            for width in [1600, 520] {
                let mut h = crate::scenario::Harness::new();
                h.app.home.open = false;
                h.call("ui.resize", json!({"width":width,"height":1100}));
                let file = json!({"format":"sketcraft-theme/1","name":"Compact options","base":base,"options":[
                    {"id":"shadow","type":"bool","default":true,"label":{"en":"Shadow","de":"Schatten"}},
                    {"id":"softness","type":"range","default":5.5,"min":0,"max":10,"step":0.5,"unit":"px","parent":"shadow",
                        "label":{"en":"Softness","de":"Weichheit"}},
                    {"id":"edges","type":"range","default":5.5,"min":0,"max":10,"step":0.5,"unit":"px",
                        "label":{"en":"Edges","de":"Kanten"}}
                ]});
                let out = h.call("ui.prefs", json!({"theme_import":file,"open":"Interface","language":language}));
                assert!(out["error"].is_null(), "{out}");
                h.frames(3);
                let rect = |name: &str| crate::scenario::rect_of(name).unwrap();
                let parent = rect("prefs:theme_option:shadow");
                let child = rect("prefs:theme_option:softness");
                let composite = rect("prefs:theme_option_cell:shadow");
                let neighbour = rect("prefs:theme_option_cell:edges");
                let body = rect("prefs:page_body").expand(1.0);
                assert!(body.contains_rect(composite) && body.contains_rect(neighbour), "{base} {language} width={width}");
                assert!(composite.contains_rect(parent) && composite.contains_rect(child));
                assert!(!composite.intersects(neighbour));
                if width == 1600 {
                    assert!(
                        (composite.top() - neighbour.top()).abs() < 1.0,
                        "short composite must share a normal Preferences row: {composite:?} {neighbour:?}"
                    );
                    assert!(composite.right() < neighbour.left(), "declared left-to-right order");
                    assert!((parent.top() - child.top()).abs() < 1.0);
                    assert!(parent.right() < child.left(), "parent and child share one compact cell");
                } else {
                    assert!(neighbour.top() > composite.bottom(), "a genuinely narrow page falls back to a full composite row");
                }
                let labels = if language == "de" { ["Schatten", "Weichheit", "Kanten"] } else { ["Shadow", "Softness", "Edges"] };
                let parts = [
                    rect(&format!("dialog:label:{}", labels[0])),
                    rect(&format!("prefs:theme_option_switch:{}", labels[0])),
                    rect(&format!("dialog:label:{}", labels[1])),
                    rect(&format!("dialog:slider:{}", labels[1])),
                    rect("prefs:theme_option_readout:softness"),
                    rect(&format!("dialog:label:{}", labels[2])),
                    rect(&format!("dialog:slider:{}", labels[2])),
                    rect("prefs:theme_option_readout:edges"),
                ];
                for (i, part) in parts.iter().enumerate() {
                    assert!(body.contains_rect(*part), "{base} {language} width={width}: {part:?} outside {body:?}");
                    for other in parts.iter().skip(i + 1) {
                        assert!(!part.intersects(*other), "{base} {language} width={width}: {part:?} overlaps {other:?}");
                    }
                }
                assert!(rect(&format!("dialog:slider:{}", labels[1])).width() >= Tokens::get().space_24 * 2.0, "retain a usable slider track");
                let control = rect(&format!("dialog:control:{}", labels[2]));
                let track = rect(&format!("dialog:slider_track:{}", labels[2]));
                let readout = rect("prefs:theme_option_readout:edges");
                let t = Tokens::get();
                let natural_width = |text: &str| {
                    h.ctx.fonts_mut(|fonts| {
                        fonts.layout_no_wrap(text.to_owned(), egui::FontId::proportional(t.font_label), t.form_ink).size().x.ceil()
                    })
                };
                assert!((rect(&format!("dialog:label:{}", labels[2])).width() - natural_width(labels[2]) - t.space_4).abs() <= 1.0);
                assert!((readout.width() - natural_width("5.5px")).abs() <= 1.0, "readout must retain its natural width");
                let reserved_readout = ["0px", "10px", "5.5px", "0.5px", "9.5px"].into_iter().map(natural_width).fold(0.0, f32::max);
                assert!(neighbour.expand(1.0).contains_rect(control));
                assert!(control.contains_rect(track) && control.contains_rect(readout));
                assert!(
                    (track.width() - (control.width() - reserved_readout - t.space_8 - t.space_4)).abs() <= 1.0,
                    "standalone slider must fill the width left by its natural label and readout: {base} {language} width={width}: {control:?} {track:?} {readout:?}"
                );
            }
        }
    }
}

#[test]
fn theme_options_ui_wide_composite_dropdown_and_advanced_keep_order() {
    for base in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":1600,"height":1100}));
        h.call("ui.prefs", json!({"theme_import":theme_options_fixture(base, 12),"open":"Interface"}));
        h.frames(3);
        let rect = |id: &str| crate::scenario::rect_of(&format!("prefs:theme_option:{id}")).unwrap();
        assert!(rect("opacity").center().x < rect("warm").center().x);
        assert!((rect("opacity").top() - rect("warm").top()).abs() < 1.0);
        assert!(rect("palette").top() > rect("warm").top());
        assert!(rect("radius").top() > rect("palette").top(), "four choices occupy a full row");
        let composite = crate::scenario::rect_of("prefs:theme_option_cell:shadow").unwrap();
        assert!(composite.contains_rect(rect("shadow")) && composite.contains_rect(rect("softness")));
        assert!(crate::scenario::rect_of("prefs:theme_option_cell:softness").is_none(), "child is not an eleventh cell");
        super::reveal_preference(&mut h, "dialog:Palette:Warm");
        h.step(&json!({"click":{"handle":"dialog:Palette:Warm"}})).unwrap();
        h.frames(2);
        let id = format!("options-{base}-12");
        assert_eq!(h.app.preferences.theme_options[&id]["palette"], "v2");
        super::reveal_preference(&mut h, "prefs:theme_option_switch:Shadow");
        h.step(&json!({"click":{"handle":"prefs:theme_option_switch:Shadow"}})).unwrap();
        h.frames(2);
        assert_eq!(h.app.preferences.theme_options[&id]["shadow"], false);
        let saved = h.app.preferences.theme_options[&id].clone();
        let track = crate::scenario::rect_of("dialog:slider_track:Softness").unwrap();
        h.step(&json!({"click":[track.center().x,track.center().y]})).unwrap();
        h.frames(2);
        assert_eq!(h.app.preferences.theme_options[&id], saved, "inactive child is inert and retains its value");
        super::reveal_preference(&mut h, "prefs:theme_option:pattern");
        h.step(&json!({"click":{"handle":"dialog:Pattern"}})).unwrap();
        h.frames(2);
        assert!(crate::scenario::handle_at("dialog:Pattern:Plain").is_some(), "six choices use a dropdown");
        h.step(&json!({"click":{"handle":"dialog:Pattern:Plain"}})).unwrap();
        h.frames(2);
        assert_eq!(h.app.preferences.theme_options[&id]["pattern"], "v6");
        assert!(crate::scenario::rect_of("prefs:theme_option:extra").is_none());
        super::reveal_preference(&mut h, "prefs:theme_options_more");
        h.step(&json!({"click":{"handle":"prefs:theme_options_more"}})).unwrap();
        h.frames(2);
        assert!(rect("extra").top() > rect("compact").top());
        assert!((rect("extra").top() - rect("outline").top()).abs() < 1.0);
    }
}

#[test]
fn theme_options_ui_opacity_live_reset_and_cancel_use_the_existing_path() {
    for base in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        control(&mut h.app, &json!({"theme_import":theme_options_fixture(base, 1)})).unwrap();
        h.call("ui.prefs", json!({"open":"Interface"}));
        h.frames(3);
        let full = Tokens::get();
        let file = active_theme_file(&h.app).unwrap();
        let mut invalid_saved = h.app.preferences.clone();
        invalid_saved.theme_options.entry(file.id.clone()).or_default().insert("opacity".into(), json!(20));
        assert_eq!(resolved_theme(&invalid_saved, file), full, "saved preference fallback stays at the theme default");
        assert_eq!(invalid_saved.theme_options[&file.id]["opacity"], 20, "fallback must retain the raw saved value");
        super::reveal_preference(&mut h, "dialog:slider:Opacity");
        let slider = crate::scenario::rect_of("dialog:slider_track:Opacity").unwrap();
        h.step(&json!({"click":[slider.center().x,slider.center().y]})).unwrap();
        h.frames(2);
        let id = format!("options-{base}-1");
        let value = h.app.preferences.theme_options[&id]["opacity"].as_f64().unwrap();
        assert!((40.0..100.0).contains(&value));
        assert!(Tokens::get().panel.a() < full.panel.a());
        assert_eq!(Tokens::get().text, full.text);
        assert!(h.app.prefs_window.open, "editing must keep the same Preferences window");
        h.step(&json!({"click":{"handle":"prefs:theme_options_reset"}})).unwrap();
        h.frames(2);
        assert_eq!(Tokens::get(), full);
        h.call("ui.prefs", json!({"theme_options":{(id.clone()):{"opacity":60}}}));
        h.frames(2);
        assert!(Tokens::get().panel.a() < full.panel.a());
        h.call("ui.prefs", json!({"button":"cancel"}));
        h.frames(2);
        assert_eq!(Tokens::get(), full);
        assert!(!h.app.preferences.theme_options.contains_key(&id));
    }
}

#[test]
fn theme_options_provider_registry_filters_unknown_and_retains_unavailable() {
    use crate::theme_options::{Availability, Provider};
    let file = crate::theme_files::from_value(&json!({"format":"sketcraft-theme/1","name":"Providers","base":"dark","options":[
        {"id":"blur","type":"range","default":0},
        {"id":"tab_colours","type":"bool","default":true},
        {"id":"tab_fade_ms","type":"range","default":100},
        {"id":"dependent","type":"bool","default":true,"parent":"tab_colours"},
        {"id":"custom","type":"bool","default":false,"label":{"en":"Custom","de":"Eigene"}},
        {"id":"opacity","type":"range","default":100}
    ]}))
    .unwrap_err();
    assert!(file.contains("more than one child"));
    let file = crate::theme_files::from_value(&json!({"format":"sketcraft-theme/1","name":"Providers","base":"dark","options":[
        {"id":"blur","type":"range","default":0}, {"id":"tab_colours","type":"bool","default":true},
        {"id":"tab_fade_ms","type":"range","default":100},
        {"id":"custom","type":"bool","default":false,"label":{"en":"Custom","de":"Eigene"}},
        {"id":"opacity","type":"range","default":100}
    ]}))
    .unwrap();
    let rows = theme_option_rows(&file, "de", crate::theme_options::PROVIDERS, Availability::Available);
    assert_eq!(rows.iter().map(|r| r.definition.id.as_str()).collect::<Vec<_>>(), ["opacity", "blur", "tab_colours", "tab_fade_ms", "custom"]);
    assert_eq!(rows[0].label, "Deckkraft");
    assert_eq!(rows[4].label, "Eigene");
    assert_eq!(theme_option_rows(&file, "ja", crate::theme_options::PROVIDERS, Availability::Available)[4].label, "Custom", "English fallback");
    let providers = [Provider {
        id: "blur",
        label: "prefs.theme_option_opacity",
        help: "prefs.theme_option_opacity_help",
        availability: || Availability::Unavailable("prefs.theme_options_unavailable"),
        apply: |_, _| {},
    }];
    let rows = theme_option_rows(&file, "en", &providers, Availability::Available);
    assert_eq!(rows.iter().map(|r| r.definition.id.as_str()).collect::<Vec<_>>(), ["blur", "custom"]);
    assert_eq!(rows[0].reason.as_deref(), Some("Unavailable on this device"));
    // The internal fixture goes through the real kit. No fake provider is shipped.
    for dark in [false, true] {
        crate::theme::set_custom(None);
        crate::theme::set_dark(dark);
        let mut values = crate::theme_files::effective_values(&file, &BTreeMap::new()).0;
        let before = values.clone();
        let ctx = crate::fonts::test_context();
        let mut draw = |events| {
            crate::scenario::clear_handles();
            crate::test_support::frame(
                &ctx,
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    ui_kit::scope(ui, |ui| {
                        ui_kit::theme_options_list(ui, &file.id, &file.name, &rows, &mut values);
                    });
                },
            );
        };
        draw(vec![]);
        assert!(crate::scenario::rect_of("prefs:theme_option_reason:blur").is_some());
        let at = crate::scenario::handle_at("dialog:slider:Opacity").unwrap();
        draw(crate::test_support::pointer(at, true));
        draw(crate::test_support::pointer(at, false));
        assert_eq!(values, before, "unavailable controls stay visible and inert");
    }
}

#[test]
fn blur_slider_reserves_readout_meter_and_disabled_reason() {
    use crate::theme_files::OptionValue;
    use crate::theme_options::Availability;
    let source: serde_json::Value = serde_json::from_str(include_str!("../tests/fixtures/theme-blur.json")).unwrap();
    let file = crate::theme_files::from_value(&source).unwrap();
    for dark in [false, true] {
        crate::theme::set_custom(None);
        crate::theme::set_dark(dark);
        for available in [Availability::Available, Availability::Unavailable("prefs.theme_options_unavailable")] {
            let rows = theme_option_rows(&file, "en", crate::theme_options::PROVIDERS, available);
            for radius in [0.0, 1.0, 8.0, 9.0, 18.0, 33.0, 40.0, 18.25] {
                let ctx = crate::fonts::test_context();
                let mut values = crate::theme_files::effective_values(&file, &BTreeMap::new()).0;
                values.insert("blur".into(), OptionValue::Num(radius));
                let before = values.clone();
                let mut output = egui::FullOutput::default();
                for _ in 0..2 {
                    crate::scenario::clear_handles();
                    output = crate::test_support::frame(
                        &ctx,
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
                            ..Default::default()
                        },
                        |ui| {
                            ui_kit::scope(ui, |ui| {
                                ui_kit::theme_options_list(ui, &file.id, &file.name, &rows, &mut values);
                            })
                        },
                    );
                }
                assert_eq!(values, before, "drawing must preserve fractional intent");
                let meter = crate::scenario::rect_of("prefs:blur_cost").unwrap();
                let readout = crate::scenario::rect_of("prefs:theme_option_readout:blur").unwrap();
                let slider = crate::scenario::rect_of("dialog:slider_track:Blur behind panels").unwrap();
                assert!(slider.width() > 300.0, "standalone blur fills remaining row width");
                assert!(slider.right() <= readout.left() && readout.right() <= meter.left());
                assert_eq!(crate::scenario::rect_of("prefs:theme_option_reason:blur").is_some(), available != Availability::Available);
                let mut heights = Vec::new();
                for clipped in &output.shapes {
                    crate::test_support::each_shape(&clipped.shape, &mut |shape| {
                        if let egui::Shape::Rect(rect) = shape
                            && meter.contains_rect(rect.rect)
                        {
                            heights.push(rect.rect.height());
                        }
                    });
                }
                assert_eq!(heights, if radius == 0.0 { vec![2.0] } else { vec![4.0, 6.0, 8.0, 11.0, 14.0] });
            }
        }
    }
}

#[test]
fn blur_availability_preserves_saved_intent_and_parent_state() {
    use crate::theme_options::{Availability, Provider};
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    let source = json!({"format":"sketcraft-theme/1","name":"Blur parent","base":"dark","options":[
        {"id":"enabled","type":"bool","default":true}, {"id":"blur","type":"range","default":0,"parent":"enabled"},
        {"id":"amount","type":"range","default":5,"min":0,"max":10,"step":1,"parent":"enabled"}
    ]});
    control(&mut h.app, &json!({"theme_import":source,"theme_options":{"blur-parent":{"blur":18.25,"amount":7.5}}})).unwrap();
    let file = active_theme_file(&h.app).unwrap().clone();
    assert_eq!(file.options.iter().find(|o| o.id == "blur").unwrap().parent, None, "reserved blur ignores authored parenting");
    assert_eq!(file.options.iter().find(|o| o.id == "amount").unwrap().parent.as_deref(), Some("enabled"));
    let saved = h.app.preferences.theme_options.clone();
    let values = effective_theme_options(&h.app.preferences, &file).0;
    for reason in ["prefs.blur_software", "prefs.blur_web", "prefs.blur_reduce_transparency"] {
        assert_eq!(crate::theme_options::blur_radius(&file, &values, Availability::Unavailable(reason)), 0.0);
        assert_eq!(crate::theme_options::blur_radius(&file, &values, Availability::Available), 18.25);
    }
    assert_eq!(h.app.preferences.theme_options, saved);
    let providers = [
        Provider { id: "enabled", label: "", help: "", availability: || Availability::Unavailable("prefs.blur_software"), apply: |_, _| {} },
        Provider {
            id: "blur",
            label: "prefs.theme_option_blur",
            help: "prefs.theme_option_blur_help",
            availability: || Availability::Available,
            apply: |_, _| {},
        },
    ];
    assert_eq!(
        crate::theme_options::availability(&file, "amount", &providers, Availability::Available),
        Availability::Unavailable("prefs.blur_software")
    );
    assert_eq!(
        crate::theme_options::availability(&file, "blur", &providers, Availability::Available),
        Availability::Available,
        "standard blur remains independent of the unavailable custom parent"
    );
    let rows = theme_option_rows(&file, "en", &providers, Availability::Available);
    let parent = rows.iter().find(|r| r.definition.id == "enabled").unwrap();
    let child = rows.iter().find(|r| r.definition.id == "amount").unwrap();
    let blur = rows.iter().find(|r| r.definition.id == "blur").unwrap();
    assert!(parent.reason.is_some());
    assert_eq!(child.reason, parent.reason, "the genuine custom child inherits its parent's unavailable reason");
    assert!(blur.reason.is_none(), "the reserved blur control is unaffected by custom-parent availability");
    let reduced = std::rc::Rc::new(std::cell::Cell::new(true));
    let reading = reduced.clone();
    h.app.services.reduce_transparency = Some(Box::new(move |_| Some(reading.get())));
    assert_eq!(blur_availability(&h.app), Availability::Unavailable("prefs.blur_reduce_transparency"));
    let reply = control(&mut h.app, &json!({})).unwrap();
    assert_eq!(reply["theme_option_values"]["blur"], 18.25);
    h.frames(2);
    assert_eq!(h.app.blur.radius(), 18.25, "OS unavailability preserves requested intent");
    assert_eq!(h.app.preferences.theme_options, saved);
    assert_eq!(h.app.blur.counters().textures_created, 0);
    assert_eq!(h.app.blur.counters().markers_emitted, 0);
    reduced.set(false);
    assert_ne!(blur_availability(&h.app), Availability::Unavailable("prefs.blur_reduce_transparency"));
    h.frames(2);
    assert_eq!(h.app.blur.radius(), 18.25, "renderer retains raw intent even without a GPU");
    assert_eq!(h.app.blur.counters().textures_created, 0);
    control(&mut h.app, &json!({"theme_options":{"blur-parent":{"enabled":false}}})).unwrap();
    h.frames(2);
    assert_eq!(h.app.blur.radius(), 18.25, "an inactive custom parent cannot turn independent standard blur off");
    assert_eq!(crate::theme::blur_intent(&h.ctx).radius, 18.25);
    let reply = control(&mut h.app, &json!({})).unwrap();
    assert_eq!(reply["theme_option_values"]["enabled"], false);
    assert_eq!(reply["theme_option_values"]["amount"], 7.5, "an inactive custom child retains its saved value");
    assert_eq!(h.app.preferences.theme_options["blur-parent"]["blur"], 18.25);
    control(&mut h.app, &json!({"theme_options":{"blur-parent":{"enabled":true}}})).unwrap();
    h.frames(2);
    assert_eq!(h.app.blur.radius(), 18.25);
    assert_eq!(h.app.preferences.theme_options["blur-parent"]["amount"], 7.5);
    let counts = h.app.blur.counters();
    assert_eq!((counts.textures_created, counts.markers_emitted, counts.markers_drawn, counts.live_bytes), (0, 0, 0, 0));
}

#[test]
fn blur_notices_dismiss_by_problem_identity_and_preserve_raw_json() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":1600,"height":1100}));
    h.call("ui.prefs", json!({"theme_import":option_theme_file("Notice blur"),"open":"Interface"}));
    h.app.preferences.theme_options.insert("notice-blur".into(), BTreeMap::from([("blur".into(), json!({"future":18}))]));
    h.frames(3);
    assert!(crate::scenario::rect_of("prefs:theme_notice:blur").is_some());
    h.step(&json!({"click":{"handle":"prefs:theme_notice_dismiss:blur"}})).unwrap();
    h.frames(2);
    assert!(crate::scenario::rect_of("prefs:theme_notice:blur").is_none());
    assert_eq!(h.app.preferences.theme_options["notice-blur"]["blur"], json!({"future":18}));
    let reply = control(&mut h.app, &json!({})).unwrap();
    assert_eq!(reply["theme_option_notices"], json!([{"id":"blur","kind":"wrong_type"}]));
    h.app.preferences.theme_options.get_mut("notice-blur").unwrap().insert("blur".into(), json!({"future":19}));
    h.frames(2);
    assert!(crate::scenario::rect_of("prefs:theme_notice:blur").is_some(), "changed raw value must reappear");
    h.call(
        "ui.prefs",
        json!({"theme_import":{"format":"sketcraft-theme/1","name":"Future blur","base":"dark","requires":crate::theme_files::REQUIREMENTS_LEVEL + 1,
        "options":[{"id":"blur","type":"range","default":18}]}}),
    );
    h.frames(2);
    assert!(crate::scenario::rect_of("prefs:theme_notice:newer").is_some());
    assert!(crate::scenario::rect_of("prefs:theme_options_header").is_none());
    h.step(&json!({"click":{"handle":"prefs:theme_notice_dismiss:newer"}})).unwrap();
    h.frames(2);
    assert!(crate::scenario::rect_of("prefs:theme_notice:newer").is_none());
    assert!(active_theme_file(&h.app).unwrap().options.is_empty(), "unsupported tables were discarded, never auto-restored");
    h.app.preferences.custom_themes.iter_mut().find(|f| f.id == "future-blur").unwrap().requires = crate::theme_files::REQUIREMENTS_LEVEL + 2;
    h.frames(2);
    assert!(crate::scenario::rect_of("prefs:theme_notice:newer").is_some());
    for identity in 0..80 {
        h.app.blur_session.dismiss(identity);
    }
    assert!(!h.app.blur_session.dismissed(0), "dismissal storage is bounded");
    assert!(h.app.blur_session.dismissed(79));
}

fn option_theme_file(name: &str) -> serde_json::Value {
    json!({
        "format": "sketcraft-theme/1", "name": name, "base": "dark", "colors": {"panel": "#112233"},
        "options": [
            {"id":"enabled", "type":"bool", "default":false, "label":{"en":"Enabled", "de":"Aktiv"}, "help":{"en":"Use the overlay", "de":"Farben verwenden"}},
            {"id":"amount", "type":"range", "default":5, "min":0, "max":10, "step":1, "unit":"px", "advanced":true, "section":"Details"},
            {"id":"style", "type":"choice", "default":"warm", "parent":"enabled", "values":[{"id":"warm", "label":{"en":"Warm", "de":"Warmton"}}, {"id":"cool", "label":{"en":"Cool"}}]},
            {"id":"blur", "type":"range", "default":0}
        ],
        "overlays": [{"when":{"opt":"enabled", "eq":true}, "colors":{"panel":"#abcdef"}}]
    })
}

#[test]
fn sketch_checkbox_bindings_edit_only_their_own_fields() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.prefs", json!({"language":"en","theme":theme}));
        h.call("ui.prefs", json!({"open":"Sketch"}));
        let original = h.app.preferences.clone();
        let session = (h.app.session.auto_project, h.app.ui.pick_bodies);
        let mut expected = serde_json::to_value(&original).unwrap();
        for (field, label) in [
            ("sketch_tracking_points", "prefs.sketch.tracking_points"),
            ("sketch_tracking_guides", "prefs.sketch.tracking_guides"),
            ("sketch_recent_snap", "prefs.sketch.recent_snap"),
            ("sketch_align_points", "prefs.sketch.align_points"),
            ("line_drag_draws_one", "prefs.line_drag_draws_one"),
            ("line_hover_tangent_arc", "prefs.line_hover_tangent_arc"),
            ("center_arc_follows_cursor", "prefs.sketch.center_arc"),
            ("sketch_construction_lines", "prefs.sketch_construction_lines"),
            ("sketch_auto_dimensions", "prefs.sketch.auto_dimensions"),
            ("sketch_auto_locate", "prefs.sketch.auto_locate"),
            ("shortcut_cycles_modes", "prefs.sketch.shortcut_cycles_modes"),
            ("ribbon_click_ends_tool", "prefs.ribbon_click_ends_tool"),
            ("project_options_dialog", "prefs.sketch.project_options"),
            ("sketch_face_activates_part", "prefs.sketch_face_activates_part"),
            ("sketch_open_look_at", "prefs.sketch_open_look_at"),
            ("selection_shift_add_only", "prefs.shift_add_ctrl_toggle"),
            ("prefer_free_sketch_points", "prefs.sketch.prefer_free_points"),
            ("select_other_hidden", "prefs.sketch.select_other_hidden"),
            ("manipulator_free_alt", "prefs.alt_frees_manipulators"),
        ] {
            h.app.prefs_window.filter = h.app.tr(label).into();
            h.frames(5);
            let handle = format!("prefs:{field}");
            super::reveal_preference(&mut h, &handle);
            let legacy = match field {
                "sketch_auto_dimensions" => Some("Dimension new shapes"),
                "sketch_auto_locate" => Some("Also place them from the origin (fully constrained)"),
                "select_other_hidden" => Some("Select hidden geometry with Select Other"),
                _ => None,
            };
            if let Some(legacy) = legacy {
                assert_eq!(crate::scenario::rect_of(&format!("dialog:{legacy}")), crate::scenario::rect_of(&handle));
            }
            h.step(&json!({"click":{"handle":handle}})).unwrap();
            expected[field] = json!(!expected[field].as_bool().unwrap());
            assert_eq!(serde_json::to_value(&h.app.preferences).unwrap(), expected, "{theme}: {field}");
            assert_eq!((h.app.session.auto_project, h.app.ui.pick_bodies), session, "{field} leaves session values alone");
        }
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert_eq!(h.app.preferences, original);
    }
}

#[test]
fn sketch_cards_tiles_and_session_controls_preserve_apply_defaults_and_cancel() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.prefs", json!({"language":"en","theme":theme,"quick_measure":true}));
        // Opening the page must retain a non-default session, too.
        h.app.session.auto_project = false;
        h.app.ui.pick_bodies = true;
        let original = Snapshot::take(&h.app);
        h.call("ui.prefs", json!({"open":"Sketch"}));
        assert!(original.matches(&h.app));
        let mut expected = serde_json::to_value(&h.app.preferences).unwrap();
        for (key, choice) in
            [("previous", SketchExitView::Previous), ("keep", SketchExitView::Keep), ("unless_rotated", SketchExitView::UnlessRotated)]
        {
            h.app.prefs_window.filter = h.app.tr("prefs.sketch_exit_view").into();
            h.frames(5);
            let handle = format!("prefs:sketch_exit_view:{key}");
            super::reveal_preference(&mut h, &handle);
            h.step(&json!({"click":{"handle":handle}})).unwrap();
            expected["sketch_exit_view"] = serde_json::to_value(choice).unwrap();
            assert_eq!(h.app.preferences.sketch_exit_view, choice);
            assert_eq!(serde_json::to_value(&h.app.preferences).unwrap(), expected, "only the exit enum changes");
        }
        for (key, label) in [
            ("sketch_state_icons", "prefs.sketch.state_icons"),
            ("sketch_conflict_colour", "prefs.sketch.conflict_colour"),
            ("hide_constraint_glyphs_zoomed_out", "prefs.sketch.hide_glyphs"),
        ] {
            h.app.prefs_window.filter = h.app.tr(label).into();
            h.frames(5);
            let handle = format!("prefs:{key}");
            super::reveal_preference(&mut h, &handle);
            h.step(&json!({"click":{"handle":handle}})).unwrap();
            expected[key] = json!(false);
            assert_eq!(serde_json::to_value(&h.app.preferences).unwrap(), expected, "{key} changes only its field");
        }
        for (key, bodies) in [("faces_edges", false), ("bodies", true), ("faces_edges", false)] {
            h.app.prefs_window.filter = h.app.tr("prefs.sketch.click_selects").into();
            h.frames(5);
            let handle = format!("prefs:pick_bodies:{key}");
            super::reveal_preference(&mut h, &handle);
            h.step(&json!({"click":{"handle":handle}})).unwrap();
            assert_eq!(h.app.ui.pick_bodies, bodies);
            assert!(!h.app.session.auto_project);
            assert_eq!(serde_json::to_value(&h.app.preferences).unwrap(), expected);
        }
        h.app.prefs_window.filter = h.app.tr("ui.project_face_edges_sketch").into();
        h.frames(5);
        super::reveal_preference(&mut h, "prefs:auto_project");
        h.step(&json!({"click":{"handle":"prefs:auto_project"}})).unwrap();
        assert!(h.app.session.auto_project);
        assert!(!h.app.ui.pick_bodies);
        assert_eq!(serde_json::to_value(&h.app.preferences).unwrap(), expected);
        assert_eq!(saved_prefs(&h.app)["preferences"], serde_json::to_value(&original.prefs).unwrap(), "unapplied tile choices are held back");
        h.call("ui.prefs", json!({"button":"apply"}));
        let applied = Snapshot::take(&h.app);
        let mut reloaded = SketApp::new(Default::default(), Default::default());
        reloaded.load_prefs(&h.app.prefs());
        assert_eq!(reloaded.preferences, h.app.preferences, "card and tile values survive the existing saved representation");

        h.call("ui.prefs", json!({"button":"defaults"}));
        let reset = serde_json::to_value(&h.app.preferences).unwrap();
        let defaults = serde_json::to_value(Prefs::default()).unwrap();
        for (field, owner) in FIELD_PAGES {
            assert_eq!(
                &reset[*field],
                if *owner == Some(PAGE_SKETCH) { &defaults[*field] } else { &expected[*field] },
                "{field}: only Sketch resets"
            );
        }
        assert_eq!((h.app.session.auto_project, h.app.ui.pick_bodies), (true, false));
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert!(applied.matches(&h.app), "Cancel after Defaults returns to Apply");

        // The session fields also roll back when the page was opened with the opposite values.
        original.clone().restore(&mut h.app);
        h.call("ui.prefs", json!({"open":"Sketch","button":"defaults"}));
        assert_eq!((h.app.session.auto_project, h.app.ui.pick_bodies), (true, false));
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert!(original.matches(&h.app));
    }
}

#[test]
fn sketch_disabled_dependencies_keep_their_stored_choices() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.prefs", json!({"language":"en","open":"Sketch","sketch_auto_locate":true}));
    for (query, parent, child) in [
        ("prefs.sketch.drawing", "sketch_auto_dimensions", "sketch_auto_locate"),
        ("prefs.sketch.snapping_guides", "sketch_tracking_guides", "sketch_tracking_points"),
    ] {
        h.app.prefs_window.filter = h.app.tr(query).into();
        h.frames(5);
        for enabled in [false, true, false] {
            set(&mut h.app, &json!({(parent):enabled, (child):true})).unwrap();
            h.frames(5);
            let handle = format!("prefs:{child}");
            super::reveal_preference(&mut h, &handle);
            h.step(&json!({"click":{"handle":handle}})).unwrap();
            let prefs = serde_json::to_value(&h.app.preferences).unwrap();
            assert_eq!(prefs[parent], enabled);
            assert_eq!(prefs[child], !enabled, "a disabled {child} retains true; an enabled one toggles");
        }
    }
}

#[test]
fn sketch_shift_legend_updates_in_the_checkbox_release_frame() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.prefs", json!({"language":"en","theme":theme,"open":"Sketch"}));
        h.app.prefs_window.filter = h.app.tr("prefs.selecting").into();
        h.frames(5);
        super::reveal_preference(&mut h, "prefs:selection_shift_add_only");
        for enabled in [true, false] {
            let at = crate::scenario::rect_of("prefs:selection_shift_add_only").unwrap().center();
            h.frame_with(crate::test_support::pointer(at, true));
            h.frame_with(crate::test_support::pointer(at, false));
            assert_eq!(h.app.preferences.selection_shift_add_only, enabled);
            let mut actions = Vec::new();
            for shape in &h.shapes {
                crate::test_support::each_shape(&shape.shape, &mut |shape| {
                    if let egui::Shape::Text(text) = shape {
                        actions.push(text.galley.job.text.clone());
                    }
                });
            }
            assert_eq!(actions.iter().filter(|text| text.as_str() == h.app.tr("prefs.add_item")).count(), usize::from(enabled));
            assert_eq!(actions.iter().filter(|text| text.as_str() == h.app.tr("prefs.toggle_item")).count(), if enabled { 1 } else { 2 });
            let primary = crate::tool_modifiers::ctrl_name_for(crate::tool_modifiers::Platform::of(&h.ctx));
            assert!(actions.iter().any(|text| text == primary), "the live platform keycap remains visible");
        }
    }
}

#[test]
fn theme_options_raw_values_default_while_theme_api_clamps() {
    use crate::theme_files::{OptionNotice, OptionNoticeKind, OptionValue};
    use sketcraft_engine::schema::ui_prefs::ThemeOptionNoticeKind;
    let file = crate::theme_files::from_value(&option_theme_file("Options A")).unwrap();
    for (raw, clamped) in [(-1.0, 0.0), (20.0, 10.0)] {
        let (values, notices) = crate::theme_files::effective_values(&file, &BTreeMap::from([("amount".into(), OptionValue::Num(raw))]));
        assert_eq!(values["amount"], OptionValue::Num(clamped));
        assert_eq!(notices, [OptionNotice { id: "amount".into(), kind: OptionNoticeKind::Clamped }]);

        let mut prefs = Prefs::default();
        prefs.theme_options.insert(file.id.clone(), BTreeMap::from([("amount".into(), json!(raw))]));
        let saved = prefs.theme_options.clone();
        let (values, notices) = effective_theme_options(&prefs, &file);
        assert_eq!(values["amount"], OptionValue::Num(5.0));
        assert_eq!(notices.len(), 1);
        assert_eq!((notices[0].id.as_str(), notices[0].kind), ("amount", ThemeOptionNoticeKind::OutOfRange));
        assert_eq!(prefs.theme_options, saved);
    }
}

#[test]
fn tabs_preferences_validate_atomically_restore_safely_and_preserve_export_defaults() {
    for base in ["dark", "light"] {
        let file = crate::theme_files::tab_fixture(base);
        let source = serde_json::to_value(&file).unwrap();
        let mut h = crate::scenario::Harness::new();
        for invalid in [json!(false), json!({"sketch":{}}), json!({"SKETCH":{"error":"#ffffff"}}), json!({"SKETCH":{"panel":"bad"}})] {
            let mut damaged = source.clone();
            damaged["tabs"] = invalid;
            let before = h.app.preferences.clone();
            let error = control(&mut h.app, &json!({"custom_themes":[damaged],"theme":file.id,"ui_scale":1.5,"open":"Interface"})).unwrap_err();
            assert!(error.contains("tabs"), "{error}");
            assert_eq!(h.app.preferences, before);
            assert!(!h.app.prefs_window.open);
            h.app.load_prefs(&json!({"preferences":{"custom_themes":[damaged,source],"theme":file.id,"zoom_reverse":true}}).to_string());
            assert_eq!(h.app.preferences.custom_themes.len(), 2);
            assert!(h.app.preferences.custom_themes[0].tabs.is_empty());
            assert_eq!(h.app.preferences.custom_themes[1].tabs, file.tabs);
            assert!(h.app.preferences.zoom_reverse);
        }
        let out = control(
            &mut h.app,
            &json!({"custom_themes":[source],"theme":file.id,"theme_options":{&file.id:{"tab_colours":false,"tab_fade_ms":123}},"theme_export":true}),
        )
        .unwrap();
        assert_eq!(out["theme_file"]["tabs"], serde_json::to_value(&file.tabs).unwrap());
        assert_eq!(out["theme_file"]["options"][0]["default"], true);
        assert_eq!(out["theme_file"]["options"][1]["default"], 240.0);
        assert_eq!(out["theme_option_values"]["tab_fade_ms"], 123.0, "step is a control increment, never rounding");
        assert_eq!(out["theme_option_defs"].as_array().unwrap().iter().find(|o| o["id"] == "tab_fade_ms").unwrap()["step"], 20.0);
        let mut restored = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
        restored.load_prefs(&h.app.prefs());
        assert_eq!(restored.preferences.custom_themes, h.app.preferences.custom_themes);
        assert_eq!(restored.preferences.theme_options, h.app.preferences.theme_options);
    }
}

#[test]
fn tabs_followed_reload_updates_surfaces_without_losing_saved_options() {
    let mut h = crate::scenario::Harness::new();
    let mut source = crate::theme_files::export_file(&crate::theme_files::tab_fixture("dark"));
    let path = h.dir.join("tabs.json");
    std::fs::write(&path, source.to_string()).unwrap();
    control(&mut h.app, &json!({"theme_follow":path,"theme_options":{"file-tabs-dark":{"tab_fade_ms":0}}})).unwrap();
    assert!(h.call("engine.execute", json!({"command":"sketch.create","params":{"plane":"XY"}}))["ok"].as_bool().unwrap());
    h.frames(2);
    assert_eq!(Tokens::get().viewport_top, egui::Color32::from_rgb(0x10, 0x31, 0x28));
    source["tabs"]["SKETCH"]["viewport_top"] = json!("#1a4232");
    std::fs::write(&path, source.to_string()).unwrap();
    control(&mut h.app, &json!({"theme_follow":path})).unwrap();
    h.frames(2);
    assert_eq!(Tokens::get().viewport_top, egui::Color32::from_rgb(0x1a, 0x42, 0x32));
    assert_eq!(h.app.preferences.theme_options["file-tabs-dark"]["tab_fade_ms"], 0);
    source["tabs"]["SKETCH"]["warning"] = json!("#ffffff");
    std::fs::write(&path, source.to_string()).unwrap();
    assert!(control(&mut h.app, &json!({"theme_follow":path})).unwrap()["theme_follow_error"].as_str().unwrap().contains("warning"));
    h.frames(2);
    assert_eq!(Tokens::get().viewport_top, egui::Color32::from_rgb(0x1a, 0x42, 0x32));
    control(&mut h.app, &json!({"theme_follow":""})).unwrap();
    h.frames(2);
    assert_eq!(Tokens::get(), Tokens::dark());
}

#[test]
fn theme_options_direct_definitions_validate_and_restore_safely() {
    let mut source = option_theme_file("Options A");
    source["id"] = json!("options-a");
    let mut other = source.clone();
    other["id"] = json!("options-b");
    other["name"] = json!("Options B");
    let mut app = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    let invalid_tables = [
        ("options", json!((0..17).map(|i| json!({"id":format!("option_{i}"),"type":"bool","default":true})).collect::<Vec<_>>())),
        ("options", json!([{ "id":"amount","type":"range","default":5,"min":10,"max":0,"step":1 }])),
        ("options", json!([{ "id":"amount","type":"range","default":11,"min":0,"max":10,"step":1 }])),
        ("options", json!([{ "id":"amount","type":"range","default":5,"min":0,"max":10,"step":0 }])),
        ("options", json!([{ "id":"amount","type":"range","default":"bad","min":0,"max":10,"step":1 }])),
        ("options", json!([{ "id":"enabled","type":"unknown","default":true }])),
        ("options", json!([{ "id":"enabled","type":"bool","default":true,"parent":"missing" }])),
        ("options", json!([{ "id":"a","type":"bool","default":true,"parent":"b" },{ "id":"b","type":"bool","default":true,"parent":"a" }])),
        ("options", json!([{ "id":"style","type":"choice","default":"missing","values":[{"id":"a","label":{}}] }])),
        ("overlays", json!((0..33).map(|_| json!({"when":{"opt":"enabled","eq":true},"colors":{"panel":"#abcdef"}})).collect::<Vec<_>>())),
        ("overlays", json!([{ "when":{"opt":"enabled","eq":1},"colors":{"panel":"#abcdef"} }])),
        ("overlays", json!([{ "when":{"opt":"unknown","eq":true},"colors":{"panel":"#abcdef"} }])),
        ("overlays", json!([{ "when":{"opt":"enabled","eq":true},"radii":{"radius_sm":-1} }])),
    ];
    for (table, invalid) in invalid_tables {
        let mut malformed = source.clone();
        malformed[table] = invalid;
        assert!(crate::theme_files::from_value(&malformed).is_err(), "{malformed}");
        let before = app.preferences.clone();
        let error = control(&mut app, &json!({"custom_themes":[malformed],"ui_scale":1.5,"open":"Interface"})).unwrap_err();
        assert!(error.contains("custom_themes[0]"), "{error}");
        assert_eq!(app.preferences, before);
        assert!(!app.prefs_window.open);

        let mut restored = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
        restored.load_prefs(
            &json!({"preferences":{
                "custom_themes":[malformed,other],"theme":"options-a","zoom_reverse":true,
                "theme_options":{"options-a":{"amount":99}}
            }})
            .to_string(),
        );
        assert_eq!(restored.preferences.custom_themes.len(), 2);
        assert!(restored.preferences.custom_themes[0].options.is_empty());
        assert!(restored.preferences.custom_themes[0].overlays.is_empty());
        assert_eq!(restored.preferences.custom_themes[0].colors, restored.preferences.custom_themes[1].colors);
        assert!(!restored.preferences.custom_themes[1].options.is_empty());
        assert!(restored.preferences.zoom_reverse);
        apply(&mut restored, &egui::Context::default());
        assert_eq!(resolved_theme(&restored.preferences, &restored.preferences.custom_themes[0]).panel, egui::Color32::from_rgb(0x11, 0x22, 0x33));
        let out = control(&mut restored, &json!({"ui_scale":1.25})).unwrap();
        assert_eq!(out["theme_option_defs"], json!([]));
        assert_eq!(out["theme_option_values"], json!({}));
        assert_eq!(out["theme_options"]["options-a"]["amount"], 99);
        assert!(restored.preferences.zoom_reverse);
    }

    // Reserved fields ignored by import must also be ignored before serde sees an update.
    source["options"][3] = json!({"id":"blur","type":"range","default":999,"step":null,"unit":false,"label":null,"advanced":"ignored"});
    let expected = crate::theme_files::from_value(&source).unwrap();
    control(&mut app, &json!({"custom_themes":[source],"theme":"options-a"})).unwrap();
    assert_eq!(app.preferences.custom_themes, [expected]);
    app.preferences.custom_themes[0].options[1].kind =
        crate::theme_files::OptionKind::Range { default: 5.0, min: 10.0, max: 0.0, step: 1.0, unit: None };
    let out = control(&mut app, &json!({"ui_scale":1.25})).unwrap();
    assert_eq!(out["theme_option_defs"], json!([]));
    assert_eq!(app.preferences.custom_themes[0].colors["panel"], "#112233");
}

#[test]
fn theme_options_persist_isolate_reset_remove_and_export_definitions() {
    let mut h = crate::scenario::Harness::new();
    let out =
        control(&mut h.app, &json!({"theme_import":option_theme_file("Options A"), "theme_options":{"options-a":{"enabled":true,"amount":7.5}}}))
            .unwrap();
    assert_eq!(out["theme_option_values"]["enabled"], true);
    h.frames(2);
    assert_eq!(Tokens::get().panel, egui::Color32::from_rgb(0xab, 0xcd, 0xef));
    control(&mut h.app, &json!({"theme_import":option_theme_file("Options B"), "theme_options":{"options-b":{"amount":3}}})).unwrap();
    assert_eq!(control(&mut h.app, &json!({})).unwrap()["theme_option_values"]["enabled"], false);
    let out = control(&mut h.app, &json!({"theme":"options-a", "theme_export":true})).unwrap();
    assert_eq!(out["theme_option_values"]["amount"], 7.5);
    assert_eq!(out["theme_file"]["options"][0]["default"], false);
    assert_eq!(out["theme_file"]["colors"]["panel"], "#112233");
    assert_eq!(out["theme_file"]["overlays"][0]["colors"]["panel"], "#abcdef");
    let mut other = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    other.load_prefs(&h.app.prefs());
    assert_eq!(other.preferences.theme_options, h.app.preferences.theme_options);
    control(&mut other, &json!({"theme_options":{"options-a":{"amount":null}}})).unwrap();
    assert_eq!(control(&mut other, &json!({})).unwrap()["theme_option_values"]["amount"], 5.0);
    assert!(other.preferences.theme_options["options-a"].contains_key("enabled"));
    control(&mut other, &json!({"theme_options":{"options-a":{}}})).unwrap();
    assert!(!other.preferences.theme_options.contains_key("options-a"));
    assert!(other.preferences.theme_options.contains_key("options-b"));
    control(&mut h.app, &json!({"theme_remove":"options-a"})).unwrap();
    assert!(!h.app.preferences.theme_options.contains_key("options-a"));
    assert!(h.app.preferences.theme_options.contains_key("options-b"));
    control(&mut other, &json!({"open":"Interface", "button":"defaults"})).unwrap();
    assert!(other.preferences.theme_options.is_empty());
    h.frames(2);
    assert_eq!(Tokens::get().panel, Tokens::dark().panel);
    control(&mut h.app, &json!({"theme_import":option_theme_file("Options A"),"theme_options":{"options-a":{"enabled":true}},"open":"Interface"}))
        .unwrap();
    control(&mut h.app, &json!({"button":"apply"})).unwrap();
    control(&mut h.app, &json!({"button":"defaults"})).unwrap();
    h.frames(2);
    assert!(h.app.preferences.theme_options.is_empty());
    assert_eq!(Tokens::get().panel, Tokens::dark().panel);
    control(&mut h.app, &json!({"button":"cancel"})).unwrap();
    h.frames(2);
    assert_eq!(Tokens::get().panel, egui::Color32::from_rgb(0xab, 0xcd, 0xef));
}

#[test]
fn theme_options_saved_invalid_values_survive_with_defaults_and_notices() {
    let mut h = crate::scenario::Harness::new();
    control(&mut h.app, &json!({"theme_import":option_theme_file("Options A")})).unwrap();
    let mut saved = serde_json::to_value(&h.app.preferences).unwrap();
    saved["theme_options"] =
        json!({"options-a":{"enabled":[], "amount":99, "style":"dropped", "removed":{"future":true}}, "unknown-theme":{"anything":null}});
    saved["zoom_reverse"] = json!(true);
    saved["ui_scale"] = json!("bad saved field");
    h.app.preferences = lenient(&saved);
    assert!(h.app.preferences.zoom_reverse);
    let out = control(&mut h.app, &json!({"look_at_anim":false})).unwrap();
    assert_eq!(out["theme_options"], saved["theme_options"]);
    assert_eq!(out["theme_option_values"], json!({"enabled":false,"amount":5.0,"style":"warm","blur":0.0}));
    assert_eq!(
        out["theme_option_notices"],
        json!([
            {"id":"amount","kind":"out_of_range"}, {"id":"enabled","kind":"wrong_type"},
            {"id":"removed","kind":"unknown_option"}, {"id":"style","kind":"unknown_choice"}
        ])
    );
    let mut other = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    other.load_prefs(&h.app.prefs());
    assert_eq!(other.preferences.theme_options, h.app.preferences.theme_options);
    assert!(other.preferences.zoom_reverse);
}

#[test]
fn theme_options_control_refuses_entire_mixed_call_before_side_effects() {
    let mut h = crate::scenario::Harness::new();
    control(&mut h.app, &json!({"theme_import":option_theme_file("Options A")})).unwrap();
    h.app.stand_up = Some(vec!["Offered".into()]);
    h.app.stand_up_note = true;
    let before = h.app.prefs();
    for (options, key) in [
        (json!({"unknown":{"enabled":true}}), "theme_options.unknown"),
        (json!({"options-a":{"unknown":true}}), "theme_options.options-a.unknown"),
        (json!({"options-a":{"enabled":1}}), "theme_options.options-a.enabled"),
        (json!({"options-a":{"amount":10.0000001}}), "theme_options.options-a.amount"),
        (json!({"options-a":{"amount":-1}}), "theme_options.options-a.amount"),
        (json!({"options-a":{"style":"missing"}}), "theme_options.options-a.style"),
        (json!({"options-a":{"amount":8,"enabled":{}},"options-b":{"enabled":true}}), "theme_options.options-a.enabled"),
    ] {
        let err = control(
            &mut h.app,
            &json!({
                "theme_import":option_theme_file("Options B"), "theme_options":options,
                "theme":"options-b", "theme_follow":"", "open":"Interface", "button":"defaults", "stand_up":false,
                "msaa":4, "default_units":"inch", "up_axis":"y", "ui_scale":1.5
            }),
        )
        .unwrap_err();
        assert!(err.contains(key), "{err}");
        assert_eq!(h.app.prefs(), before, "{key}");
        assert!(!h.app.prefs_window.open && h.app.prefs_window.snapshot.is_none());
        assert_eq!(h.app.stand_up.as_deref(), Some(["Offered".to_string()].as_slice()));
        assert!(h.app.stand_up_note);
    }
    let err = control(&mut h.app, &json!({"theme_remove":"options-a", "theme_options":{"options-a":{}}})).unwrap_err();
    assert!(err.contains("theme_options.options-a"));
    assert_eq!(h.app.prefs(), before);
    assert!(control(&mut h.app, &json!({"theme_options":{"options-a":{"enabled":true}},"ui_scale":9})).is_err());
    assert_eq!(h.app.prefs(), before);
    assert!(control(&mut h.app, &json!({"theme_import":option_theme_file("Options B"),"button":"invalid"})).is_err());
    assert_eq!(h.app.prefs(), before);
    assert!(control(&mut h.app, &json!({"theme_options":null})).is_err());
}

#[test]
fn theme_options_active_reply_is_localized_ordered_and_matches_schema_types() {
    use sketcraft_engine::schema::ui_prefs::{PrefsReply, ThemeOptionKind};
    let mut h = crate::scenario::Harness::new();
    let out = control(&mut h.app, &json!({"theme_import":option_theme_file("Options A"), "language":"de"})).unwrap();
    let reply: PrefsReply = serde_json::from_value(out.clone()).unwrap();
    assert_eq!(reply.options.theme_option_defs.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(), ["blur", "enabled", "amount", "style"]);
    let defs = &reply.options.theme_option_defs;
    assert!(defs[0].standard && !defs[0].available);
    let crate::theme_options::Availability::Unavailable(reason) = blur_availability(&h.app) else { panic!("headless fixture has no GPU") };
    assert_eq!(defs[0].reason.as_deref(), Some(crate::i18n::Locale::for_language("de").text(reason)));
    assert_eq!((&defs[1].label, &defs[1].help), (&"Aktiv".to_string(), &"Farben verwenden".to_string()));
    assert!(defs[1].available && defs[1].reason.is_none());
    assert!(defs[2].advanced && defs[2].section.as_deref() == Some("Details"));
    let ThemeOptionKind::Choice { values, .. } = &defs[3].kind else { panic!("choice definition") };
    assert_eq!(values.iter().map(|v| v.label.as_str()).collect::<Vec<_>>(), ["Warmton", "Cool"]);
    assert_eq!(serde_json::to_value(reply).unwrap(), out);
    let out = control(&mut h.app, &json!({"theme":"dark"})).unwrap();
    assert_eq!(out["theme_option_defs"], json!([]));
    assert_eq!(out["theme_option_values"], json!({}));
    assert_eq!(out["theme_option_notices"], json!([]));
}

#[test]
fn theme_options_followed_file_uses_its_id_and_reload_keeps_saved_entries() {
    let mut h = crate::scenario::Harness::new();
    let path = h.dir.join("follow-options.json");
    let mut file = option_theme_file("Follow Options");
    file["base"] = json!("light");
    std::fs::write(&path, file.to_string()).unwrap();
    let out = control(&mut h.app, &json!({"theme_follow":path, "theme_options":{"file-follow-options":{"enabled":true,"amount":8}}})).unwrap();
    assert_eq!(out["theme"], "dark");
    assert_eq!(out["theme_option_values"]["enabled"], true);
    h.frames(2);
    assert_eq!(Tokens::get().panel, egui::Color32::from_rgb(0xab, 0xcd, 0xef));
    assert!(!h.app.ui.dark);
    let saved = h.app.prefs();
    let err = control(&mut h.app, &json!({"theme":"light", "theme_options":{"missing":{}}, "stand_up":false})).unwrap_err();
    assert!(err.contains("theme_options.missing"));
    assert_eq!(h.app.prefs(), saved);
    assert_eq!(active_theme_file(&h.app).unwrap().id, "file-follow-options");
    let other_path = h.dir.join("other-options.json");
    std::fs::write(&other_path, option_theme_file("Other Options").to_string()).unwrap();
    let err = control(&mut h.app, &json!({"theme_follow":other_path,"theme_options":{"file-other-options":{"enabled":42}}})).unwrap_err();
    assert!(err.contains("theme_options.file-other-options.enabled"), "{err}");
    assert_eq!(h.app.prefs(), saved);
    assert_eq!(active_theme_file(&h.app).unwrap().id, "file-follow-options");
    file["options"][1]["max"] = json!(6);
    file["options"][1]["default"] = json!(2);
    std::fs::write(&path, file.to_string()).unwrap();
    let out = control(&mut h.app, &json!({"theme_follow":path, "theme_export":true})).unwrap();
    assert_eq!(out["theme_option_values"]["amount"], 2.0);
    assert_eq!(out["theme_options"]["file-follow-options"]["amount"], 8);
    assert_eq!(out["theme_option_notices"], json!([{"id":"amount","kind":"out_of_range"}]));
    assert_eq!(out["theme_file"]["name"], "Follow Options");
    assert_eq!(out["theme_file"]["options"][1]["default"], 2.0);
    std::fs::write(&path, "incomplete").unwrap();
    let out = control(&mut h.app, &json!({"theme_follow":path})).unwrap();
    assert!(out["theme_follow_error"].is_string());
    assert_eq!(out["theme_option_values"]["enabled"], true);
    let out = control(&mut h.app, &json!({"theme_follow":""})).unwrap();
    assert_eq!(out["dark"], true);
    assert!(active_theme_file(&h.app).is_none());
    h.frames(2);
    assert_eq!(Tokens::get().panel, Tokens::dark().panel);
    let out = control(&mut h.app, &json!({"theme_follow":path})).unwrap();
    assert_eq!(out["theme_option_values"], json!({}), "stopping clears the last good file");
    control(&mut h.app, &json!({"theme_follow":""})).unwrap();
    std::fs::write(&path, file.to_string()).unwrap();
    follow_theme_file_for_run(&mut h.app, path.to_str().unwrap()).unwrap();
    h.frames(2);
    let out = control(&mut h.app, &json!({})).unwrap();
    assert_eq!(out["theme_follow"], "");
    assert_eq!(out["theme_option_values"]["enabled"], true);
    assert_eq!(Tokens::get().panel, egui::Color32::from_rgb(0xab, 0xcd, 0xef));
    assert!(!h.app.ui.dark);
    let saved = h.app.prefs();
    let launch = h.app.prefs_window.follow.launch.clone();
    assert!(control(&mut h.app, &json!({"theme":"dark","theme_options":{"missing":{}}})).is_err());
    assert_eq!(h.app.prefs(), saved);
    assert_eq!(h.app.prefs_window.follow.launch, launch);
    assert_eq!(active_theme_file(&h.app).unwrap().id, "file-follow-options");
    let out = control(&mut h.app, &json!({"theme":"dark"})).unwrap();
    assert_eq!(out["dark"], true);
    assert!(out["theme_followed"].is_null());
    assert!(h.app.prefs_window.follow.theme().is_none());
    h.frames(2);
    assert_eq!(Tokens::get().panel, Tokens::dark().panel);
}

#[test]
fn gruvbox_legacy_selection_migrates_on_load_and_preserves_saved_values() {
    for (old, family, contrast) in crate::theme_files::GRUVBOX_ALIASES {
        let saved = json!({"theme":old,"theme_options":{
            family:{"contrast":"standard","unrelated":{"keep":true}},
            old:{"contrast":"standard","future":42}
        }});
        let migrated = lenient(&saved);
        assert_eq!(migrated.theme, family);
        assert_eq!(migrated.theme_options[family]["contrast"], contrast);
        assert_eq!(migrated.theme_options[family]["unrelated"], json!({"keep":true}));
        assert_eq!(migrated.theme_options[family]["future"], 42);
        assert!(!migrated.theme_options.contains_key(old));
        assert_eq!(serde_json::to_value(migrate(migrated.clone())).unwrap(), serde_json::to_value(&migrated).unwrap());
        let mut h = crate::scenario::Harness::new();
        h.app.load_prefs(&json!({"preferences":saved}).to_string());
        assert_eq!(h.app.preferences.theme, family);
        assert_eq!(h.app.preferences.theme_options[family]["contrast"], contrast);
        let output: serde_json::Value = serde_json::from_str(&h.app.prefs()).unwrap();
        assert_eq!(output["preferences"]["theme"], family);
        assert_eq!(output["preferences"]["theme_options"][family]["contrast"], contrast);
    }
    // An unselected legacy map must not replace an existing canonical contrast.
    let migrated = lenient(&json!({"theme":"dark","theme_options":{
        "gruvbox-dark":{"contrast":"soft"},"gruvbox-dark-hard":{"contrast":"hard"}
    }}));
    assert_eq!(migrated.theme_options["gruvbox-dark"]["contrast"], "soft");
}

#[test]
fn gruvbox_control_legacy_ids_allow_explicit_choices_resets_and_atomic_errors() {
    let mut h = crate::scenario::Harness::new();
    for (old, family, contrast) in crate::theme_files::GRUVBOX_ALIASES {
        let out = control(&mut h.app, &json!({"theme":old})).unwrap();
        assert_eq!(out["theme"], family);
        assert_eq!(out["theme_option_values"]["contrast"], contrast);
        let ids = out["theme_ids"].as_array().unwrap();
        assert_eq!(ids.iter().filter(|id| id.as_str().is_some_and(|id| id.starts_with("gruvbox-"))).count(), 2);
        let out = control(&mut h.app, &json!({"theme":old,"theme_options":{family:{"contrast":"standard"}}})).unwrap();
        assert_eq!(out["theme_option_values"]["contrast"], "standard");
        let out = control(&mut h.app, &json!({"theme":old,"theme_options":{family:{"contrast":null}}})).unwrap();
        assert_eq!(out["theme_option_values"]["contrast"], "standard");
        let out = control(&mut h.app, &json!({"theme":old,"theme_options":{family:{}}})).unwrap();
        assert_eq!(out["theme_option_values"]["contrast"], "standard");
        let out = control(&mut h.app, &json!({"theme":old,"theme_options":{old:{"contrast":"hard"},family:{"contrast":"soft"}}})).unwrap();
        assert_eq!(out["theme_option_values"]["contrast"], "soft", "canonical explicit values win over alias entries");
        let before = h.app.prefs();
        assert!(control(&mut h.app, &json!({"theme":old,"open":"Interface","theme_options":{family:{"contrast":"invalid"}}})).is_err());
        assert_eq!(h.app.prefs(), before, "failed changes do not migrate or open the dialog");
        assert!(!h.app.prefs_window.open);
    }
}

#[test]
fn theme_files_import_apply_export_save_and_remove() {
    use crate::theme::{ColorVision, Tokens};
    use serde_json::json;
    let mut h = crate::scenario::Harness::new();
    let out = h.call("ui.prefs", json!({"theme": "gruvbox-dark-hard"}));
    assert_eq!(out["result"]["theme"], "gruvbox-dark", "{out}");
    assert_eq!(out["result"]["theme_option_values"]["contrast"], "hard");
    assert!(out["result"]["theme_ids"].as_array().is_some_and(|ids| ids.len() == 2 + crate::theme_files::builtin().len()), "{out}");
    h.frames(2);
    let hard = resolved_theme(&h.app.preferences, active_theme_file(&h.app).unwrap());
    assert!(h.app.ui.dark);
    assert_eq!(Tokens::get().panel, hard.panel);
    assert_eq!(
        crate::viewport::scene_palette(true, ColorVision::Off).background.0,
        sketcraft_engine::render::Rgb(hard.viewport_top.r(), hard.viewport_top.g(), hard.viewport_top.b())
    );

    let file = json!({"format": "sketcraft-theme/1", "name": "Paper", "base": "light", "colors": {"panel": "#fffdf0", "accent": "#aa3300"}, "borders":{"primary":{"width":2,"color":"#ffffff"},"tertiary":{"width":0,"color":"#000000"}}, "radii": {"radius_sm": 0, "control_radius": 0, "radius_xl": 16}});
    let out = h.call("ui.prefs", json!({"theme_import": file, "theme_export": true}));
    assert_eq!(out["result"]["theme_imported"], "paper", "{out}");
    assert_eq!((out["result"]["theme"].as_str(), out["result"]["dark"].as_bool()), (Some("paper"), Some(false)));
    let exported = &out["result"]["theme_file"];
    assert_eq!((exported["name"].as_str(), exported["base"].as_str()), (Some("Paper"), Some("light")));
    assert_eq!(exported["colors"]["panel"], "#fffdf0");
    assert_eq!(exported["radii"]["control_radius"], 0);
    assert_eq!(exported["radii"]["radius_xl"], 16);
    assert_eq!(exported["radii"]["radius_md"], 6);
    assert_eq!(exported["borders"]["primary"]["width"], 2.0);
    assert_eq!(exported["colors"]["text"].as_str().map(str::len), Some(7), "an export is complete: {exported}");
    h.frames(2);
    assert_eq!((Tokens::get().panel, Tokens::get().text), (egui::Color32::from_rgb(0xff, 0xfd, 0xf0), Tokens::light().text));
    assert_eq!((Tokens::get().control_radius, Tokens::get().radius_xl), (0, 16));
    let invalid =
        h.call("ui.prefs", json!({"theme_import": {"format": "sketcraft-theme/1", "name": "Bad", "base": "dark", "radii": {"control_radius": -1}}}));
    assert!(invalid["error"].as_str().is_some_and(|e| e.contains("radii.control_radius")));
    let out = h.call(
        "ui.prefs",
        json!({"theme_import": "{\"format\":\"sketcraft-theme/1\",\"name\":\"x\",\"base\":\"dark\",\"colors\":{\"nope\":\"#000000\"}}"}),
    );
    assert!(out["error"].as_str().is_some_and(|e| e.contains("nope")), "{out}");
    assert_eq!(h.app.preferences.theme, "paper", "a refused file changes nothing");
    assert!(h.call("ui.prefs", json!({"theme": "no-such-theme"}))["error"].is_string());

    let mut other = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    other.load_prefs(&h.app.prefs());
    assert_eq!((other.preferences.theme.as_str(), other.preferences.custom_themes.len()), ("paper", 1));
    assert_eq!(other.preferences.custom_themes[0].radii, h.app.preferences.custom_themes[0].radii);
    assert_eq!(other.preferences.custom_themes[0].borders, h.app.preferences.custom_themes[0].borders);

    let out = h.call("ui.prefs", json!({"theme_remove": "paper"}));
    assert_eq!((out["result"]["theme"].as_str(), out["result"]["dark"].as_bool()), (Some("light"), Some(false)), "{out}");
    assert!(h.app.preferences.custom_themes.is_empty());
    assert!(h.call("ui.prefs", json!({"theme_remove": "paper"}))["error"].is_string());

    h.call("ui.prefs", json!({"theme": "dark"}));
    h.call("ui.prefs", json!({"open": "Interface"}));
    h.call("ui.prefs", json!({"theme_import": file}));
    let out = h.call("ui.prefs", json!({"button": "cancel"}));
    assert_eq!(out["result"]["theme"], "dark", "{out}");
    assert!(h.app.preferences.custom_themes.is_empty(), "Cancel takes the import back");
    h.frames(2);
    assert_eq!(Tokens::get().panel, Tokens::dark().panel);
    assert_eq!((Tokens::get().control_radius, Tokens::get().radius_xl), (6, 10));
}

/// A theme tile's Export… writes the theme file through `vfs`, like the other saves, so the
/// web app hands it out as a download (#7192). On the disk it is the same file; a folder scope
/// active while it is written refuses a path outside it, which shows the write goes through
/// `vfs`.
#[test]
fn theme_export_writes_through_vfs() {
    use serde_json::json;
    use sketcraft_engine::io::vfs;
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.prefs", json!({"theme": "dark", "open": "Interface"}));
    h.frames(2);
    super::reveal_preference(&mut h, "prefs:theme_tile:dark");
    let (root, out) = (h.dir.join("root"), h.dir.join("out"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    for (path, inside) in [(root.join("Dark.json"), true), (out.join("Dark.json"), false)] {
        let answer = path.to_string_lossy().to_string();
        h.app.services.pick_save = Some(Box::new(move |_, _| Box::pin(std::future::ready(Some(answer.clone())))));
        h.app.status = None;
        vfs::scoped(Some(vfs::PathScope::new([root.clone()])), || {
            h.step(&json!({"click":{"handle":"prefs:theme_tile:dark"},"button":"right"})).unwrap();
            h.step(&json!({"click":{"handle":"menu:theme_export"}})).unwrap();
            h.frames(4);
        });
        let status = h.app.status.clone().map(|(text, ..)| text);
        if inside {
            assert_eq!(status, None);
            let file: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(file, super::theme_file(&h.app, "dark", true));
        } else {
            assert!(status.as_deref().is_some_and(|s| s.contains("outside the folders this agent may use")), "{status:?}");
            assert!(!path.exists(), "nothing written outside");
        }
    }
}

/// Picking a font closes the font list, not the Preferences window's body: that folded the
/// window to its title bar, and egui kept it folded between runs (#1800). A window folded by
/// an earlier release unfolds when Preferences opens.
#[test]
fn picking_a_font_keeps_the_preferences_window_unfolded() {
    use egui::collapsing_header::CollapsingState;
    use serde_json::json;
    let body = egui::Id::unique("sc_prefs").with("collapsing");
    let folded = |h: &crate::scenario::Harness| CollapsingState::load(&h.ctx, body).is_some_and(|s| !s.is_open());
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"open":"Interface"}));
    super::reveal_preference(&mut h, "prefs:ui_font");
    let combo = crate::scenario::handle_at("prefs:ui_font").unwrap();
    h.call("ui.click", json!({"x":combo.x,"y":combo.y}));
    let builtin = crate::scenario::handle_at("prefs:font_builtin").expect("the font list is open");
    h.call("ui.click", json!({"x":builtin.x,"y":builtin.y}));
    h.frames(2);
    assert!(!folded(&h), "picking a font must not fold the Preferences window");
    assert!(crate::scenario::handle_at("prefs:font_builtin").is_none(), "the font list closes");
    assert!(crate::scenario::rect_of("prefs:ok").is_some(), "the window's body is still drawn");
    h.call("ui.prefs", json!({"button":"cancel"}));

    // The state 0.14 and 0.15 saved: the window's body folded.
    let mut state = CollapsingState::load_with_default_open(&h.ctx, body, true);
    state.set_open(false);
    state.store(&h.ctx);
    h.call("ui.prefs", json!({"open":"Interface"}));
    h.frames(2);
    assert!(!folded(&h), "a folded Preferences window unfolds when it opens");
    assert!(crate::scenario::rect_of("prefs:ok").is_some());
}

#[test]
fn appearance_popups_own_escape_restore_focus_and_leave_cancel_to_the_next_escape() {
    for font in [true, false] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":1600,"height":1100}));
        h.call("ui.prefs", json!({"language":"en","theme":"dark"}));
        let before = h.app.preferences.clone();
        h.call("ui.prefs", json!({"open":"Appearance"}));
        h.frames(3);
        reveal_preference(&mut h, "prefs:viewport_theme:light");
        h.step(&json!({"click":{"handle":"prefs:viewport_theme:light"}})).unwrap();
        h.frames(2);
        assert_eq!(h.app.preferences.viewport_theme, crate::theme::ViewportTheme::Light);
        let (target, popup) = if font {
            ("prefs:ui_font", "dialog:Interface font search")
        } else {
            reveal_preference(&mut h, "dialog:Background:Color");
            h.step(&json!({"click":{"handle":"dialog:Background:Color"}})).unwrap();
            h.frames(2);
            ("prefs:viewport_background_picker", "color:popover")
        };
        reveal_preference(&mut h, target);
        let body = crate::scenario::rect_of("prefs:page_body").unwrap();
        let rect = crate::scenario::rect_of(target).unwrap();
        assert!(body.contains_rect(rect), "{target} is visible before clicking");
        let edited = h.app.preferences.clone();
        assert_ne!(edited, before);
        h.step(&json!({"click":{"handle":target}})).unwrap();
        h.frames(2);
        assert!(crate::scenario::handle_at(popup).is_some() || crate::scenario::rect_of(popup).is_some(), "{target} opens its real popup");
        h.step(&json!({"key":"Escape"})).unwrap();
        h.frames(2);
        assert!(h.app.prefs_window.open && h.app.prefs_window.section == PAGE_APPEARANCE);
        assert!(h.app.prefs_window.snapshot.is_some());
        assert_eq!(h.app.preferences, edited, "dismissing {target} retains live changes");
        assert!(crate::scenario::handle_at(popup).is_none() && crate::scenario::rect_of(popup).is_none());
        // No pointer input: the actual combo/swatch must have regained keyboard focus.
        h.step(&json!({"key":"Enter"})).unwrap();
        h.frames(2);
        assert!(crate::scenario::handle_at(popup).is_some() || crate::scenario::rect_of(popup).is_some(), "{target} reopens from its restored focus");
        h.step(&json!({"key":"Escape"})).unwrap();
        h.frames(2);
        assert!(h.app.prefs_window.open && h.app.prefs_window.snapshot.is_some());
        assert_eq!(h.app.preferences, edited);
        h.step(&json!({"key":"Escape"})).unwrap();
        h.frames(2);
        assert!(!h.app.prefs_window.open && h.app.prefs_window.snapshot.is_none());
        assert_eq!(h.app.preferences, before, "Escape with no popup cancels and restores the original snapshot");
    }
}

/// Letters typed in the font list's own Search box narrow the list. Preferences' search takes
/// typing only when no text field has the keyboard, so it must not take these.
#[test]
fn typing_in_the_font_lists_search_narrows_the_list_not_the_page() {
    use serde_json::json;
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"open":"Interface"}));
    // Found by searching, as the setting is far down the page.
    h.step(&json!({"text": "font"})).expect("search");
    h.frames(3);
    let combo = crate::scenario::handle_at("prefs:ui_font").expect("the search finds the font row");
    let body = crate::scenario::rect_of("prefs:page_body").expect("the Preferences page body");
    let picker = crate::scenario::rect_of("prefs:ui_font").expect("the font picker bounds");
    assert!(body.contains_rect(picker) && picker.contains(combo), "the font picker is inside the visible page before clicking");
    h.call("ui.click", json!({"x":combo.x,"y":combo.y}));
    h.frames(2);
    let search = format!("dialog:{} search", crate::i18n::tr_label("Interface font"));
    let field = crate::scenario::handle_at(&search).expect("the font list is open");
    h.call("ui.click", json!({"x":field.x,"y":field.y}));
    h.frames(2);
    h.step(&json!({"text": "Comic"})).expect("type");
    h.frames(2);
    assert_eq!(h.app.prefs_window.filter, "font", "the Preferences search took the letters");
    assert_eq!(h.app.prefs_window.font_filter, "Comic", "the font list's search keeps them");
    assert!(crate::scenario::handle_at(&search).is_some(), "the font list stays open");
    h.call("ui.prefs", json!({"button":"cancel"}));
}

#[test]
fn number_fields_take_a_decimal_point_or_a_decimal_comma() {
    // #1347
    assert_eq!(super::parse_number("2.5"), Some(2.5));
    assert_eq!(super::parse_number("2,5"), Some(2.5));
    assert_eq!(super::parse_number(" -0,25 "), Some(-0.25));
    assert_eq!(super::parse_number("\u{2212}1,5"), Some(-1.5));
    assert_eq!(super::parse_number("12"), Some(12.0));
    for bad in ["1.234,5", "1,2,3", ",", "abc", "", "inf", "NaN"] {
        assert_eq!(super::parse_number(bad), None, "{bad}");
    }
}

#[test]
fn render_legacy_image_defaults_round_trips_and_resets() {
    use serde_json::json;
    let mut app = crate::SketApp::new(Default::default(), Default::default());
    assert!(!app.preferences.render_legacy_image_defaults);
    let out = super::set(&mut app, &json!({"render_legacy_image_defaults":true})).unwrap();
    assert_eq!(out["render_legacy_image_defaults"], true);
    let mut restored = crate::SketApp::new(Default::default(), Default::default());
    restored.load_prefs(&app.prefs());
    assert!(restored.preferences.render_legacy_image_defaults);
    super::defaults(&mut restored, super::PAGE_GRAPHICS);
    assert!(!restored.preferences.render_legacy_image_defaults);
}

#[test]
fn render_previous_geometry_budget_round_trips_and_resets() {
    use serde_json::json;
    let mut app = crate::SketApp::new(Default::default(), Default::default());
    assert!(!app.preferences.render_previous_geometry_budget);
    let out = super::set(&mut app, &json!({"render_previous_geometry_budget":true})).unwrap();
    assert_eq!(out["render_previous_geometry_budget"], true);
    let mut restored = crate::SketApp::new(Default::default(), Default::default());
    restored.load_prefs(&app.prefs());
    assert!(restored.preferences.render_previous_geometry_budget);
    super::defaults(&mut restored, super::PAGE_GRAPHICS);
    assert!(!restored.preferences.render_previous_geometry_budget);
}

/// Display quality (#1287) is a preference like any other: set by name, checked, saved with
/// the others, back to Normal with Restore Defaults.
#[test]
fn display_quality_is_set_checked_and_restored() {
    use serde_json::json;
    let mut app = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    assert_eq!(app.preferences.display_quality, "normal");
    let out = super::set(&mut app, &json!({"display_quality": "fine"})).unwrap();
    assert_eq!(out["display_quality"], "fine");
    assert!(super::set(&mut app, &json!({"display_quality": "silky"})).unwrap_err().contains("coarse, normal, fine or ultra"));
    assert_eq!(app.preferences.display_quality, "fine");
    assert_eq!(super::lenient(&json!({"display_quality": "ultra"})).display_quality, "ultra");
    super::defaults(&mut app, super::PAGE_GRAPHICS);
    assert_eq!(app.preferences.display_quality, "normal");
}

/// The validation mode is once per feature by default; set by name or by the command, the
/// two stay one setting, the session computes under it, and Restore Defaults on Design brings
/// each_feature back. A saved unknown mode reads as the default.
#[test]
fn validation_defaults_to_each_feature_and_follows_the_command() {
    use serde_json::json;
    use sketcraft_engine::kernel::Validation;
    let mut app = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    assert_eq!(app.preferences.validation, Validation::EachFeature);
    assert_eq!(super::lenient(&json!({"validation": "sometimes"})).validation, Validation::EachFeature);
    assert!(super::set(&mut app, &json!({"validation": "never"})).unwrap_err().contains("each_step, each_feature or off"));
    let out = super::set(&mut app, &json!({"validation": "each_step"})).unwrap();
    assert_eq!(out["validation"], "each_step");
    assert_eq!(super::lenient(&out).validation, Validation::EachStep);
    // A command runs under the preference.
    app.run("diagnostics.validation", json!({})).unwrap();
    assert_eq!(app.session.validation(), Validation::EachStep);
    // The command sets the preference too.
    app.run("diagnostics.validation", json!({"mode": "off"})).unwrap();
    assert_eq!((app.preferences.validation, app.session.validation()), (Validation::Off, Validation::Off));
    super::defaults(&mut app, super::PAGE_DESIGN);
    assert_eq!(app.preferences.validation, Validation::EachFeature);
}

#[test]
fn sketch_halo_is_off_by_default_saved_and_restored() {
    use serde_json::json;
    let mut app = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    assert_eq!(serde_json::to_value(&app.preferences).unwrap()["sketch_halo"], false);
    assert_eq!(serde_json::to_value(super::lenient(&json!({}))).unwrap()["sketch_halo"], false);
    let saved = super::set(&mut app, &json!({"sketch_halo": true})).unwrap();
    assert_eq!(saved["sketch_halo"], true);
    assert_eq!(serde_json::to_value(super::lenient(&saved)).unwrap()["sketch_halo"], true);
    super::defaults(&mut app, super::PAGE_GRAPHICS);
    assert_eq!(serde_json::to_value(&app.preferences).unwrap()["sketch_halo"], false);
}

/// Supersampling (#1288): off by default, adaptive, set by factor and checked.
#[test]
fn supersampling_is_off_by_default_and_checked() {
    use serde_json::json;
    let mut app = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    assert_eq!((app.preferences.supersampling, app.preferences.supersampling_adaptive), (1.0, true));
    super::set(&mut app, &json!({"supersampling": 2.0})).unwrap();
    assert_eq!(app.preferences.supersampling, 2.0);
    assert!(super::set(&mut app, &json!({"supersampling": 3.0})).unwrap_err().contains("1 (off), 1.5 or 2"));
    super::set(&mut app, &json!({"supersampling": 1.5, "supersampling_adaptive": false})).unwrap();
    super::defaults(&mut app, super::PAGE_GRAPHICS);
    assert_eq!((app.preferences.supersampling, app.preferences.supersampling_adaptive), (1.0, true));
}

#[test]
fn a_value_this_version_does_not_understand_keeps_the_other_preferences() {
    use serde_json::json;
    // A navigator, a field of the wrong type and a key from another release.
    let saved = json!({"nav": "solidworks", "navigator_style": "hologram", "ui_scale": "big", "zoom_reverse": true, "from_the_future": 3});
    let p = super::lenient(&saved);
    assert_eq!(p.nav, "solidworks");
    assert!(p.zoom_reverse);
    assert_eq!(p.navigator_style, super::NavigatorStyle::Axes);
    assert_eq!(p.ui_scale, super::Prefs::default().ui_scale);
    assert_eq!(super::lenient(&json!("garbage")), super::Prefs::default());
    // Hand-edited toolbar tab lists keep only the tabs each can hold, each once (#5717).
    let mut hidden = vec![json!("VIEW"), json!("SHEET METAL"), json!("VIEW"), json!("SURFACE")];
    hidden.extend(std::iter::repeat_n(json!("NOT A TAB"), 10_000));
    let tabs = super::lenient(&json!({"hidden_toolbar_tabs": hidden}));
    assert_eq!(tabs.hidden_toolbar_tabs, ["VIEW", "SHEET METAL", "SURFACE"]);
    // Whole-app load: the other saved state is kept too, and nothing panics.
    let mut app = crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.load_prefs(&json!({"grid": false, "preferences": saved}).to_string());
    assert!(!app.ui.show_grid && app.preferences.nav == "solidworks" && app.preferences.zoom_reverse);
    for junk in ["", "{", "[]", "null", "{\"preferences\": 7, \"recent\": {}, \"shortcuts\": 1, \"toolbar\": \"x\"}"] {
        crate::SketApp::new(sketcraft_engine::Session::default(), crate::Services::default()).load_prefs(junk);
    }
}

#[test]
fn stored_prefs_fall_back_to_the_legacy_key() {
    let only_old = |k: &str| (k == super::LEGACY_STORAGE_KEY).then(|| "old".to_string());
    assert_eq!(super::stored(only_old).as_deref(), Some("old"));
    let both = |k: &str| Some(if k == super::STORAGE_KEY { "new" } else { "old" }.to_string());
    assert_eq!(super::stored(both).as_deref(), Some("new"));
    assert_eq!(super::stored(|_| None), None);
}

use super::*;
use crate::navigation::orbit;

/// Graphics › Window system (#3083): saved in `graphics.json` beside the other start-up
/// settings, refused when unknown, taken back by Cancel; on Linux the pane says which one runs
/// now and, for native Wayland, that some features are limited, with the link.
#[test]
fn the_window_system_is_saved_for_the_next_start() {
    use serde_json::json;
    let dir = std::env::temp_dir().join(format!("sc_window_system_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("graphics.json");
    std::fs::write(&path, r#"{"msaa":4,"backend":"vulkan"}"#).unwrap();
    let saved = || -> serde_json::Value { serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default()).unwrap_or_default() };
    let mut h = crate::scenario::Harness::new();
    h.app.services.graphics_path = Some(path.to_string_lossy().to_string());
    h.app.services.window_system = Some("x11");
    assert_eq!(h.app.preferences.window_system, "auto");
    let out = h.call("ui.prefs", json!({"window_system": "wayland"}));
    assert_eq!(out["result"]["window_system"], "wayland", "{out}");
    assert_eq!((saved()["window_system"].as_str(), saved()["backend"].as_str()), (Some("wayland"), Some("vulkan")), "the other settings stay");
    assert!(h.call("ui.prefs", json!({"window_system": "mir"}))["error"].as_str().is_some_and(|e| e.contains("window_system")));
    assert_eq!(h.app.preferences.window_system, "wayland", "a refused value changes nothing");

    h.call("ui.prefs", json!({"open": "Graphics"}));
    h.frames(2);
    assert_eq!(crate::scenario::handle_at("prefs:window_system").is_some(), cfg!(target_os = "linux"), "the row is on Linux only");
    h.call("ui.prefs", json!({"window_system": "x11"}));
    let out = h.call("ui.prefs", json!({"button": "cancel"}));
    assert_eq!(out["result"]["window_system"], "wayland", "Cancel takes the change back");
    assert_eq!(saved()["window_system"], "wayland", "and the file with it");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Preferences › General › Largest file to open (#4086): the defaults are the limits there always
/// were; `ui.prefs` raises both, up to 16 GB and never below the defaults; a saved value out of
/// range reads back in range, and General's Restore Defaults puts them back.
#[test]
fn the_largest_file_to_open_defaults_raises_and_refuses_out_of_range() {
    use serde_json::json;
    use sketcraft_engine::io::limits::{FileLimits, GB};
    let mut h = crate::scenario::Harness::new();
    assert_eq!((h.app.preferences.max_step_file_mb, h.app.preferences.max_design_file_mb), (512, 256));
    assert_eq!(super::file_limits(&h.app.preferences), FileLimits::DEFAULT);
    let out = h.call("ui.prefs", json!({"max_step_file_mb": 4096, "max_design_file_mb": 2048}));
    assert_eq!((out["result"]["max_step_file_mb"].as_u64(), out["result"]["max_design_file_mb"].as_u64()), (Some(4096), Some(2048)), "{out}");
    assert_eq!(super::file_limits(&h.app.preferences), FileLimits { step: 4 * GB, design: 2 * GB });
    for bad in [
        json!({"max_step_file_mb": 16385}),
        json!({"max_design_file_mb": 100}),
        json!({"max_step_file_mb": -1}),
        json!({"max_design_file_mb": "2GB"}),
    ] {
        assert!(h.call("ui.prefs", bad.clone())["error"].is_string(), "{bad}");
    }
    assert_eq!(super::file_limits(&h.app.preferences), FileLimits { step: 4 * GB, design: 2 * GB }, "a refused value changes nothing");
    let read = super::lenient(&json!({"max_step_file_mb": 999_999, "max_design_file_mb": 3}));
    assert_eq!((read.max_step_file_mb, read.max_design_file_mb), (16384, 256));

    h.call("ui.prefs", json!({"open": "General"}));
    h.frames(2);
    super::reveal_preference(&mut h, "prefs:max_step_file_mb");
    assert!(crate::scenario::rect_of("prefs:max_design_file_mb").is_some());
    h.call("ui.prefs", json!({"button": "defaults"}));
    assert_eq!(super::file_limits(&h.app.preferences), FileLimits::DEFAULT);
    h.call("ui.prefs", json!({"button": "cancel"}));
}

#[test]
fn general_files_controls_edit_real_limits_and_hosts_in_both_themes() {
    for theme in ["light", "dark"] {
        for width in [1468, 480] {
            let mut h = crate::scenario::Harness::new();
            h.app.home.open = false;
            h.call("ui.resize", json!({"width":width, "height":1100}));
            let configured = h.call(
                "ui.prefs",
                json!({"language":"en", "theme":theme, "network_locations":true, "network_hosts":["files.example", "nas.example"]}),
            );
            assert!(configured["error"].is_null(), "{theme} {width}: setting up Preferences failed: {configured}");
            let opened = h.call("ui.prefs", json!({"open":"General"}));
            assert!(opened["error"].is_null(), "{theme} {width}: opening General failed: {opened}");
            let initial_body = crate::scenario::rect_of("prefs:page_body").unwrap();
            for target in
                ["dialog:widget:Profile name", "prefs:profile:save", "prefs:profile:import", "prefs:profiles:held:row", "prefs:profiles:computer:row"]
            {
                let rect = crate::scenario::rect_of(target).unwrap();
                assert!(initial_body.contains_rect(rect), "{theme} {width}: {target} {rect:?} leaves {initial_body:?}");
            }
            if width == 480 {
                let name = crate::scenario::rect_of("dialog:widget:Profile name").unwrap();
                let save = crate::scenario::rect_of("prefs:profile:save").unwrap();
                let import = crate::scenario::rect_of("prefs:profile:import").unwrap();
                assert!(save.top() >= name.bottom(), "narrow Save wraps after the independent name entry");
                assert!((save.top() - import.top()).abs() < 0.5);
            }
            let painted = |h: &crate::scenario::Harness, label: &str| {
                let body = crate::scenario::rect_of("prefs:page_body").unwrap();
                h.shapes
                    .iter()
                    .find_map(|shape| {
                        if let egui::Shape::Text(text) = &shape.shape
                            && text.galley.job.text == label
                        {
                            let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                            if body.contains_rect(rect) && shape.clip_rect.contains_rect(rect) {
                                return Some(rect);
                            }
                        }
                        None
                    })
                    .unwrap_or_else(|| panic!("{theme} {width}: {label} is not fully painted in the page"))
            };
            // Reveal the live dropdown, not the full-width layout row's allocation rectangle.
            super::reveal_preference(&mut h, "dialog:widget:Interface language");
            let body = crate::scenario::rect_of("prefs:page_body").unwrap();
            let language = painted(&h, "Interface language");
            assert!((language.left() - body.left()).abs() < 0.75);
            assert!((crate::scenario::rect_of("dialog:control:Interface language").unwrap().left() - body.left() - 164.0).abs() < 0.5);
            super::reveal_preference(&mut h, "prefs:autosave_minutes");
            let autosave = painted(&h, "Autosave every");
            let autosave_help = crate::scenario::rect_of("dialog:row_help:Autosave every").unwrap();
            assert!((autosave.left() - body.left()).abs() < 0.75, "the painted autosave label shares the language label's left edge");
            assert!((autosave_help.left() - autosave.right() - 4.0).abs() < 1.0, "help follows the actual text, not the whole label column");
            assert!((crate::scenario::rect_of("prefs:autosave_minutes").unwrap().left() - body.left() - 164.0).abs() < 0.5);
            super::reveal_preference(&mut h, "prefs:autosave_timeline");
            let timeline = crate::scenario::rect_of("prefs:autosave_timeline").unwrap();
            let autosave_control = crate::scenario::rect_of("prefs:autosave_minutes").unwrap();
            assert!((timeline.left() - autosave_control.left()).abs() < 0.5);
            assert!((timeline.width() - 260.0_f32.min(body.right() - autosave_control.left())).abs() < 0.5);
            assert!(body.contains_rect(timeline));
            super::reveal_preference(&mut h, "prefs:max_step_file_mb");
            super::reveal_preference(&mut h, "dialog:row:Largest file to open");
            let field = crate::scenario::rect_of("prefs:max_step_file_mb").unwrap();
            let design = crate::scenario::rect_of("prefs:max_design_file_mb").unwrap();
            assert!(
                (field.width() - 72.0).abs() < 0.5 && (design.width() - 72.0).abs() < 0.5,
                "{theme}: both compound file editors retain their width"
            );
            let step_group = crate::scenario::rect_of("dialog:row:Largest file to open: STEP and import").unwrap();
            let design_group = crate::scenario::rect_of("dialog:row:Largest file to open: Design").unwrap();
            let step_caption = painted(&h, "STEP and import");
            let design_caption = painted(&h, "Design");
            let largest = painted(&h, "Largest file to open");
            assert!((step_caption.left() - field.left()).abs() < 0.75 && (design_caption.left() - design.left()).abs() < 0.75);
            assert!((largest.left() - body.left()).abs() < 0.75);
            assert!((largest.top() - step_caption.top() - 6.0).abs() <= 1.0, "the left label stays in its top-aligned 28 px cell");
            assert!(step_group.width() >= 90.0 && step_group.width() < 130.0, "{theme}: the subgroup follows its field, unit and caption");
            if width == 1468 {
                assert!((step_caption.top() - design_caption.top()).abs() < 0.5, "{theme}: the actual painted captions share a top baseline");
                assert!(
                    (field.top() - design.top()).abs() < 0.5 && field.right() < design.left(),
                    "{theme}: independent editors share one visual row"
                );
                assert!((design_group.left() - step_group.right() - 20.0).abs() < 0.5, "{theme}: compact pair gap");
            } else {
                assert!(design_group.top() > step_group.bottom(), "{theme}: the pair wraps at narrow widths");
                assert!((design_group.left() - step_group.left()).abs() < 0.5);
            }
            let group = crate::scenario::rect_of("dialog:row:Largest file to open").unwrap();
            assert!(group.contains_rect(field) && group.contains_rect(design));
            assert!(crate::scenario::rect_of("prefs:page_body").unwrap().contains_rect(group));
            h.step(&json!({"click":[field.center().x, field.center().y], "double":true})).unwrap();
            h.step(&json!({"key":"A", "cmd":true})).unwrap();
            h.step(&json!({"text":"4096"})).unwrap();
            h.step(&json!({"key":"Enter"})).unwrap();
            assert_eq!(h.app.preferences.max_step_file_mb, 4096);
            super::reveal_preference(&mut h, "prefs:max_design_file_mb");
            let field = crate::scenario::rect_of("prefs:max_design_file_mb").unwrap();
            h.step(&json!({"click":[field.center().x, field.center().y], "double":true})).unwrap();
            h.step(&json!({"key":"A", "cmd":true})).unwrap();
            h.step(&json!({"text":"2048"})).unwrap();
            h.step(&json!({"key":"Enter"})).unwrap();
            assert_eq!(h.app.preferences.max_design_file_mb, 2048);
            assert_eq!(super::file_limits(&h.app.preferences).step, 4096 * sketcraft_engine::io::limits::MB);
            let before = h.app.preferences.clone();
            for bad in [json!({"max_step_file_mb":16385}), json!({"max_design_file_mb":255})] {
                assert!(h.call("ui.prefs", bad)["error"].is_string());
                assert_eq!(h.app.preferences, before, "invalid file limits fail atomically");
            }
            super::reveal_preference(&mut h, "prefs:network_host:files.example:remove");
            h.step(&json!({"click":{"handle":"prefs:network_host:files.example:remove"}})).unwrap();
            assert_eq!(h.app.preferences.network_hosts, ["nas.example"]);
            h.step(&json!({"widget":"Allow network locations in designs"})).unwrap();
            h.frames(3);
            assert!(!h.app.preferences.network_locations);
            assert_eq!(h.app.preferences.network_hosts, ["nas.example"], "hiding the chips retains the hosts");
            assert!(crate::scenario::rect_of("prefs:network_host:nas.example:remove").is_none());
            super::reveal_preference(&mut h, "prefs:print_face_highlight");
            let highlight = painted(&h, "Print face highlight");
            let highlight_help = crate::scenario::rect_of("dialog:row_help:Print face highlight").unwrap();
            assert!((highlight.left() - body.left()).abs() < 0.75, "the painted highlight label shares the language/autosave left edge");
            assert!((highlight_help.left() - highlight.right() - 4.0).abs() < 1.0);
            assert!((crate::scenario::rect_of("prefs:print_face_highlight").unwrap().left() - body.left() - 164.0).abs() < 0.5);
            let track = crate::scenario::rect_of("dialog:slider_track:Print face highlight").unwrap();
            // The published rectangle includes the circular handle's endpoint insets.
            let input_range = track.x_range().shrink(track.height() / 2.5);
            h.step(&json!({"click":[egui::lerp(input_range, 0.75), track.center().y]})).unwrap();
            assert!(
                (h.app.preferences.print_face_highlight - 0.75).abs() < 0.06,
                "{theme} {width}: highlight={} track={track:?} input_range={input_range:?}",
                h.app.preferences.print_face_highlight
            );
            super::reveal_preference(&mut h, "prefs:check_now");
            assert!(body.contains_rect(crate::scenario::rect_of("prefs:check_now").unwrap()));
            assert!(h.app.prefs_window.open);
            h.step(&json!({"click":{"handle":"prefs:cancel"}})).unwrap();
            assert_eq!((h.app.preferences.max_step_file_mb, h.app.preferences.max_design_file_mb), (DEFAULT_STEP_MB, DEFAULT_DESIGN_MB));
            assert_eq!(h.app.preferences.network_hosts, ["files.example", "nas.example"]);
            assert!(h.app.preferences.network_locations);
            assert_eq!(h.app.preferences.print_face_highlight, 0.2);
        }
    }
}

#[test]
fn general_autosave_rows_apply_cancel_and_defaults_keep_the_effective_interval() {
    use std::time::Duration;
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.step(&json!({"autosave":true})).unwrap();
    h.app.autosave_minutes = 7.0;
    super::set_autosave(&mut h.app);
    h.app.preferences.language = "en".into();
    h.app.preferences.nav = "tinkercad".into();
    h.app.preferences.reopen_designs = false;
    h.app.run("settings.profile.save", json!({"name":"Working"})).unwrap();
    let profiles = h.app.settings_profiles.list.clone();
    h.call("ui.prefs", json!({"open":"General"}));
    super::reveal_preference(&mut h, "prefs:autosave_minutes");
    super::reveal_preference(&mut h, "prefs:autosave_timeline");
    let number = crate::scenario::rect_of("prefs:autosave_minutes").unwrap();
    let timeline = crate::scenario::rect_of("prefs:autosave_timeline").unwrap();
    assert!((timeline.width() - 260.0).abs() < 0.5, "the timeline stays bounded in the control column");
    assert!((timeline.left() - number.left()).abs() < 0.5);
    assert!(timeline.top() >= number.bottom());
    assert!(crate::scenario::rect_of("prefs:page_body").unwrap().contains_rect(timeline));
    h.step(&json!({"click":{"handle":"prefs:autosave_minutes"}, "double":true})).unwrap();
    h.step(&json!({"key":"A", "cmd":true})).unwrap();
    h.step(&json!({"text":"2.5"})).unwrap();
    h.step(&json!({"key":"Enter"})).unwrap();
    assert_eq!(h.app.autosave_minutes, 2.5);
    assert_eq!(h.app.autosave.as_ref().unwrap().interval, Duration::from_secs(150));
    assert_eq!(saved_prefs(&h.app)["autosave_minutes"], 7.0, "an unapplied value is not persisted");
    h.step(&json!({"click":{"handle":"prefs:apply"}})).unwrap();
    assert_eq!(saved_prefs(&h.app)["autosave_minutes"], 2.5);
    super::reveal_preference(&mut h, "prefs:autosave_minutes");
    h.step(&json!({"click":{"handle":"prefs:autosave_minutes"}, "double":true})).unwrap();
    h.step(&json!({"key":"A", "cmd":true})).unwrap();
    h.step(&json!({"text":"0"})).unwrap();
    h.step(&json!({"key":"Enter"})).unwrap();
    assert_eq!(h.app.autosave_minutes, 0.0);
    assert!(h.app.autosave.as_ref().unwrap().interval > Duration::from_secs(365 * 24 * 60 * 60));
    h.step(&json!({"click":{"handle":"prefs:cancel"}})).unwrap();
    assert_eq!(h.app.autosave_minutes, 2.5);
    assert_eq!(h.app.autosave.as_ref().unwrap().interval, Duration::from_secs(150));
    h.call("ui.prefs", json!({"open":"General"}));
    assert!(h.widget(h.app.tr("prefs.restore_page_defaults")).is_some(), "the translated Preferences caption is visible");
    assert!(h.widget(h.app.tr("ui.restore_defaults")).is_none(), "the generic caption is absent from Preferences");
    h.step(&json!({"widget":"Restore page defaults"})).unwrap();
    assert_eq!(h.app.autosave_minutes, 5.0);
    assert_eq!(h.app.autosave.as_ref().unwrap().interval, Duration::from_secs(300));
    assert_eq!(h.app.preferences.language, crate::i18n::SYSTEM);
    assert!(h.app.preferences.reopen_designs);
    assert_eq!(h.app.preferences.nav, "tinkercad", "General's reset leaves other pages alone");
    assert_eq!(h.app.settings_profiles.list, profiles);
    assert_eq!(h.app.settings_profiles.active.as_deref(), Some("Working"));
    h.step(&json!({"click":{"handle":"prefs:cancel"}})).unwrap();
    assert_eq!(h.app.preferences.language, "en");
    assert!(!h.app.preferences.reopen_designs);
    assert_eq!(h.app.autosave_minutes, 2.5);
    assert_eq!(h.app.autosave.as_ref().unwrap().interval, Duration::from_secs(150));
}

#[test]
fn graphics_card_picker_selects_named_cards_and_restores_automatic() {
    let dir = std::env::temp_dir().join(format!("sketcraft-graphics-picker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = crate::scenario::Harness::new();
    h.app.services.graphics_path = Some(dir.join("graphics.json").to_string_lossy().into_owned());
    h.app.services.graphics_adapters = vec![("Discrete small".into(), "Discrete small (DiscreteGpu, Vulkan)".into())];
    h.call("ui.prefs", json!({"open":"Graphics"}));
    super::reveal_preference(&mut h, "prefs:graphics_card");
    assert!(h.app.preferences.graphics_adapter.is_empty());
    h.step(&json!({"click":{"handle":"prefs:graphics_card"}})).unwrap();
    h.step(&json!({"click":{"handle":"dialog:list:graphics_card:0"}})).unwrap();
    assert_eq!(h.app.preferences.graphics_adapter, "Discrete small");
    h.step(&json!({"click":{"handle":"prefs:graphics_card"}})).unwrap();
    h.step(&json!({"click":{"handle":"dialog:list:graphics_card:auto"}})).unwrap();
    assert!(h.app.preferences.graphics_adapter.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn graphics_card_preference_persists_cancels_and_resets_without_changing_backend() {
    let dir = std::env::temp_dir().join(format!("sketcraft-graphics-card-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("graphics.json");
    std::fs::write(&path, r#"{"backend":"vulkan","msaa":4}"#).unwrap();
    let services = crate::Services { graphics_path: Some(path.to_string_lossy().into()), ..Default::default() };
    let mut app = SketApp::new(sketcraft_engine::Session::default(), services);
    let snapshot = Snapshot::take(&app);
    set(&mut app, &json!({"graphics_adapter":"Discrete large"})).unwrap();
    let read = || serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(read()["adapter"], "Discrete large");
    assert_eq!(read()["backend"], "vulkan");
    let saved = serde_json::to_value(&app.preferences).unwrap();
    assert_eq!(lenient(&saved).graphics_adapter, "Discrete large");
    snapshot.restore(&mut app);
    assert_eq!(read()["adapter"], "");
    set(&mut app, &json!({"graphics_adapter":"Discrete small"})).unwrap();
    defaults(&mut app, PAGE_GRAPHICS);
    assert_eq!(read()["adapter"], "");
    assert_eq!(read()["backend"], "vulkan");
    assert!(set(&mut app, &json!({"graphics_adapter":"x".repeat(1025)})).is_err());
    assert!(set(&mut app, &json!({"graphics_adapter":"card\nname"})).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

/// The settings `SketApp::prefs` would write right now, parsed.
fn saved_prefs(app: &SketApp) -> serde_json::Value {
    serde_json::from_str(&app.prefs()).unwrap()
}

#[test]
fn unapplied_changes_are_not_saved_while_the_dialog_is_open() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    assert_eq!(saved_prefs(&app)["preferences"]["theme"], "dark");
    assert_eq!(saved_prefs(&app)["preferences"]["sketch_tab_always"], false);
    // Open the dialog: the snapshot is taken as `show` and the control channel take it.
    app.prefs_window.open = true;
    app.prefs_window.snapshot = Some(Snapshot::take(&app));
    // The Light tile and the Sketch tab box, clicked without Apply.
    set(&mut app, &json!({"theme": "light", "sketch_tab_always": true})).unwrap();
    assert_eq!(app.preferences.theme, "light", "the live app has the change");
    assert!(!app.ui.dark);
    let held = saved_prefs(&app);
    assert_eq!(held["preferences"]["theme"], "dark", "a change that was not applied must not be saved");
    assert_eq!(held["preferences"]["sketch_tab_always"], false);
    assert_eq!(held["dark"], true, "the Light tile's dark flag is not saved either");
    // Apply makes the change the baseline: from now on it is saved.
    app.prefs_window.snapshot = Some(Snapshot::take(&app));
    let applied = saved_prefs(&app);
    assert_eq!(applied["preferences"]["theme"], "light");
    assert_eq!(applied["dark"], false);
    // A second unapplied change is held back too, and Cancel puts it back.
    set(&mut app, &json!({"theme": "dark"})).unwrap();
    assert_eq!(saved_prefs(&app)["preferences"]["theme"], "light", "the second change waits for Apply too");
    close(&mut app, true);
    let after = saved_prefs(&app);
    assert_eq!(after["preferences"]["theme"], "light", "Cancel restores the applied state");
    assert_eq!(after["dark"], false);
    assert_eq!(after["preferences"]["sketch_tab_always"], true, "Apply's change stays");
}

#[test]
fn settings_the_dialog_cannot_change_keep_saving_while_it_is_open() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.prefs_window.open = true;
    app.prefs_window.snapshot = Some(Snapshot::take(&app));
    // A recent file joins while the dialog is open: not a dialog setting, so it is saved.
    app.home.recent.insert(0, "bracket.solid".to_string());
    assert_eq!(saved_prefs(&app)["recent"][0], "bracket.solid");
}

#[test]
fn shortcut_changes_are_not_saved_while_the_dialog_is_open() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.keymap.custom.insert("file.new".into(), vec!["Ctrl+N".into()]);
    app.prefs_window.open = true;
    app.prefs_window.snapshot = Some(Snapshot::take(&app));
    app.keymap.custom.insert("file.save".into(), vec!["Ctrl+S".into()]);
    let saved = saved_prefs(&app);
    assert_eq!(saved["shortcuts"]["file.new"], "Ctrl+N", "the shortcut from before the dialog is saved");
    assert!(saved["shortcuts"].get("file.save").is_none(), "a shortcut rebound without Apply is not saved");
}

#[test]
fn control_channel_holds_unapplied_changes_out_of_the_saved_settings() {
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"open": "Layout"}));
    h.call("ui.prefs", json!({"sketch_tab_always": true}));
    let saved = saved_prefs(&h.app);
    assert!(h.app.preferences.sketch_tab_always);
    assert_eq!(saved["preferences"]["sketch_tab_always"], false, "the control channel's change waits for Apply");
    h.call("ui.prefs", json!({"button": "apply"}));
    assert_eq!(saved_prefs(&h.app)["preferences"]["sketch_tab_always"], true, "Apply makes it the saved state");
    h.call("ui.prefs", json!({"sketch_tab_always": false}));
    h.call("ui.prefs", json!({"button": "cancel"}));
    assert_eq!(saved_prefs(&h.app)["preferences"]["sketch_tab_always"], true, "Cancel puts back what Apply left");
}

#[test]
fn startup_msaa_check_keeps_the_default_unless_fallback_is_confirmed() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.msaa_supported = vec![1, 4, 8];
    for requested in [1, 4, 8] {
        app.preferences.msaa = requested;
        app.msaa_compatibility = false;
        check_msaa(&mut app, requested);
        assert_eq!(app.preferences.msaa, requested, "a passing check preserves the choice, including the 4x default");
        assert!(app.settings_reset.is_empty());
    }
    app.preferences.msaa = 4;
    app.msaa_compatibility = true;
    check_msaa(&mut app, 4);
    assert_eq!(app.preferences.msaa, 1, "Graphics must show the confirmed fallback so it can be changed back");
    assert!(app.msaa_supported.contains(&4), "the original choice stays available");
    assert!(app.msaa_compatibility, "Graphics explains the fallback");
    assert!(set(&mut app, &serde_json::json!({ "msaa": 4 })).is_ok());
    assert_eq!(app.preferences.msaa, 4);
}

#[test]
fn unsupported_saved_msaa_is_reset_to_the_default_with_a_notice() {
    let dir = std::env::temp_dir().join(format!("sc_msaa_reset_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("graphics.json");
    let services = crate::Services { graphics_path: Some(path.to_string_lossy().to_string()), ..Default::default() };
    let mut app = SketApp::new(sketcraft_engine::Session::default(), services);
    // WebGPU's guarantee for the surface format: 8x saved, the device can only do 1x and 4x.
    app.msaa_supported = vec![1, 4];
    app.preferences.msaa = 8;
    std::fs::write(&path, r#"{"msaa":8,"backend":"vulkan"}"#).unwrap();
    check_msaa(&mut app, 8);
    assert_eq!(app.preferences.msaa, 4);
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default()).unwrap_or_default();
    assert_eq!(saved["msaa"], 4);
    assert_eq!(saved["backend"], "vulkan", "the backend preference is kept");
    assert_eq!(app.settings_reset, [SettingReset::Msaa { requested: 8, reset: 4, supported: vec![1, 4] }]);
    assert_eq!(
        app.settings_reset[0].message(),
        "Graphics › Anti-aliasing: 8× isn't supported by this graphics device; it is now 4× (supported: 1×, 4×)."
    );
    {
        let _de = crate::i18n::scope("de");
        assert_eq!(
            app.settings_reset[0].message(),
            "Grafik › Kantenglättung: 8× wird von diesem Grafikgerät nicht unterstützt; jetzt 4× (unterstützt: 1×, 4×)."
        );
    }
    // Below the default when the device can't do that either.
    app.settings_reset.clear();
    app.msaa_supported = vec![1, 2];
    check_msaa(&mut app, 8);
    assert_eq!(app.preferences.msaa, 2);
    // Supported, off, or no GPU: nothing changes (OpenGL's 1x policy isn't a reset either).
    for (supported, requested) in [(vec![1, 2, 4, 8], 8), (vec![1, 4], 1), (vec![], 8), (vec![1, 4], 4)] {
        app.settings_reset.clear();
        app.preferences.msaa = requested;
        app.msaa_supported = supported;
        check_msaa(&mut app, requested);
        assert!(app.settings_reset.is_empty());
        assert_eq!(app.preferences.msaa, requested);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn control_channel_rejects_msaa_the_device_cannot_do_and_bad_saved_msaa_loads_as_default() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.msaa_supported = vec![1, 4];
    assert!(set(&mut app, &serde_json::json!({ "msaa": 8 })).is_err());
    assert!(set(&mut app, &serde_json::json!({ "msaa": 1 })).is_ok());
    app.load_prefs(&serde_json::json!({ "preferences": { "msaa": 16 } }).to_string());
    assert_eq!(app.preferences.msaa, Prefs::default().msaa);
}

/// "Let AI agents connect" is on for a new install and for preferences saved before it existed;
/// an explicit off stays off.
#[test]
fn agent_control_is_on_unless_saved_off() {
    assert!(Prefs::default().agent_control);
    let mut app = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    app.preferences.agent_control = false;
    app.load_prefs(&serde_json::json!({ "preferences": { "msaa": 4 } }).to_string());
    assert!(app.preferences.agent_control);
    app.load_prefs(&serde_json::json!({ "preferences": { "agent_control": false } }).to_string());
    assert!(!app.preferences.agent_control);
}

#[test]
fn length_fields_read_bare_numbers_in_design_units_and_honour_explicit_units() {
    let inch = unit_scale("in").0;
    let close = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-9);
    assert!(close(parse_length_mm("2", inch), 50.8));
    assert!(close(parse_length_mm(" 10 mm ", inch), 10.0));
    assert!(close(parse_length_mm("0.5 in", 1.0), 12.7));
    assert!(close(parse_length_mm("1 in + 5 mm", unit_scale("cm").0), 30.4));
    assert!(close(parse_length_mm("3", unit_scale("cm").0), 30.0));
    assert!(close(parse_length_mm("3", 1.0), 3.0));
    assert_eq!(parse_length_mm("10 deg", inch), None);
    assert_eq!(parse_length_mm("abc", inch), None);
    assert_eq!(parse_length_mm("", inch), None);
    assert_eq!(parse_length_mm("1e400", 1.0), None);
}

#[test]
fn navigator_picker_preserves_axes_and_follows_preferences_transactions() {
    let old: Prefs = serde_json::from_value(json!({"nav":"solidworks"})).unwrap();
    assert_eq!(old.navigator_style, NavigatorStyle::Axes);
    let mut h = crate::scenario::Harness::new();
    assert!(crate::scenario::handle_at("axis_view:z").is_some());
    assert!(crate::scenario::handle_at("navigation_cube:front").is_none());
    let camera = h.app.cam;
    let design = h.app.session.doc.to_json();
    let nav = h.app.preferences.nav.clone();
    h.call("ui.prefs", json!({"open":"Navigation"}));
    super::reveal_preference(&mut h, "prefs:navigator");
    let cube = crate::scenario::rect_of("prefs:navigator:Cube").unwrap().center();
    assert!(crate::scenario::rect_of("prefs:navigator:Signed axes").is_some());
    h.call("ui.click", json!({"x":cube.x,"y":cube.y}));
    assert_eq!(h.app.preferences.navigator_style, NavigatorStyle::Cube);
    assert!(crate::scenario::handle_at("navigation_cube:front").is_some());
    assert_eq!(h.app.preferences.nav, nav);
    assert_eq!(h.app.cam, camera);
    assert_eq!(h.app.session.doc.to_json(), design);
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert_eq!(restored.preferences.navigator_style, NavigatorStyle::Axes, "a change that was not applied is not saved (#9730)");
    let before = h.app.preferences.clone();
    assert!(control(&mut h.app, &json!({"navigator_style":"unknown", "nav":"solidworks"})).is_err());
    assert_eq!(h.app.preferences, before, "invalid settings must fail atomically");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert_eq!(h.app.preferences.navigator_style, NavigatorStyle::Axes);
    assert!(crate::scenario::handle_at("navigation_cube:front").is_none());
    h.call("ui.prefs", json!({"open":"Navigation","navigator_style":"cube"}));
    h.call("ui.prefs", json!({"button":"apply"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert_eq!(h.app.preferences.navigator_style, NavigatorStyle::Axes);
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert_eq!(h.app.preferences.navigator_style, NavigatorStyle::Cube);
    h.call("ui.prefs", json!({"navigator_style":"axes", "look_at_anim":false}));
    h.call("ui.view", json!({"view":"front"}));
    let axis = crate::scenario::handle_at("axis_view:-y").unwrap();
    h.call("ui.click", json!({"x":axis.x,"y":axis.y}));
    assert!(h.app.cam.back().dist(Vec3::Y) < 1e-6, "original axes retain click-again reversal");
    assert_eq!(h.app.session.doc.to_json(), design);
}

/// The texts one frame paints.
fn painted_texts(h: &mut crate::scenario::Harness) -> Vec<String> {
    let output = h.ctx.run_ui(egui::RawInput { screen_rect: Some(h.ctx.content_rect()), focused: true, ..Default::default() }, |ui| {
        h.app.logic(ui.ctx());
        h.app.ui(ui);
    });
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}

/// The shipped ribbon uses the same localized tab names as workspace metadata.
#[test]
fn ribbon_names_the_tabs_as_workspace_metadata_does() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    h.call("ui.resize", json!({"width":2400,"height":1000}));
    for (lang, want) in [
        ("en", ["Model", "Surface", "Mesh", "Form", "Sheet Metal", "Render"]),
        ("de", ["Modell", "Fläche", "Mesh", "Form", "Blech", "Rendern"]),
        ("fr", ["Modèle", "Surface", "Maillage", "Forme", "Tôlerie", "Rendu"]),
    ] {
        h.app.preferences.language = lang.into();
        h.frames(3);
        let _scope = crate::i18n::scope(lang);
        for (id, name) in ["MODEL", "SURFACE", "MESH", "FORM", "SHEET METAL", "RENDER"].into_iter().zip(want) {
            assert_eq!(crate::workspace::tab_name(id), name, "{lang}");
            assert_eq!(crate::toolbar::ribbon_label(&h.app, id), name, "{lang}");
        }
        let mut painted = Vec::new();
        for shape in &h.shapes {
            crate::test_support::each_shape(&shape.shape, &mut |shape| {
                if let egui::Shape::Text(text) = shape {
                    painted.push(text.galley.job.text.clone());
                }
            });
        }
        for id in crate::workspace::visible_tabs(&h.app) {
            let name = crate::toolbar::ribbon_label(&h.app, id);
            assert!(crate::scenario::handle_at(&format!("tab:{id}")).is_some(), "{lang}: real ribbon tab {id}");
            assert!(painted.contains(&name), "{lang}: the ribbon paints {painted:?}, not {name}");
        }
    }
}

/// Origin plane names (#6491): Top, Front and Right are names shown in the browser, the
/// dialogs' chips and the plane choices; the planes' ids, the commands and the design keep XY,
/// XZ and YZ. Saved, refused when unknown, and taken back by Cancel and Restore Defaults.
#[test]
fn origin_plane_names_change_only_the_names_shown() {
    use OriginPlaneNames::{TopFrontRight, Xyz};
    let old: Prefs = serde_json::from_value(json!({"nav":"solidworks"})).unwrap();
    assert_eq!(old.origin_plane_names, Xyz, "older preferences keep XY, XZ and YZ");
    assert_eq!(lenient(&json!({"origin_plane_names":"hologram","zoom_reverse":true})).origin_plane_names, Xyz);
    assert_eq!(OriginPlaneNames::PLANES.map(|p| Xyz.label(p)), ["XY", "XZ", "YZ"]);
    assert_eq!(OriginPlaneNames::PLANES.map(|p| TopFrontRight.label(p)), ["Top", "Front", "Right"]);
    assert_eq!(TopFrontRight.label("Plane1"), "Plane1", "construction planes keep their own names");

    let mut h = crate::scenario::Harness::new();
    let sketch = h.app.run("sketch.create", json!({"plane":"XY"})).unwrap()["sketch"].as_u64().unwrap();
    h.app.run("sketch.rectangle.two_point", json!({"p0":[0,0],"p1":[20,10]})).unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    h.frames(2);
    h.step(&json!({"click":{"chevron":"Origin"}})).unwrap();
    let rows = || crate::browser::drawn_rows().into_iter().map(|(label, ..)| label).collect::<Vec<_>>();
    assert!(OriginPlaneNames::PLANES.iter().all(|p| rows().iter().any(|r| r == p)), "{:?}", rows());
    let design = h.app.session.doc.to_json();
    h.call("ui.prefs", json!({"open":"Design"}));
    super::reveal_preference(&mut h, "prefs:origin_plane_names:top_front_right");
    h.step(&json!({"click":{"handle":"prefs:origin_plane_names:top_front_right"}})).unwrap();
    assert_eq!(h.app.preferences.origin_plane_names, TopFrontRight);
    h.call("ui.prefs", json!({"button":"ok"}));
    h.frames(2);
    for (id, name) in [("XY", "Top"), ("XZ", "Front"), ("YZ", "Right")] {
        assert!(rows().iter().any(|r| r == name) && !rows().iter().any(|r| r == id), "{:?}", rows());
    }
    assert_eq!(h.app.session.doc.to_json(), design, "a name shown changes nothing in the design");

    // Picked from its row, the plane is named in the dialog's chip and runs by its id.
    h.call("ui.start", json!({"command":"construct.plane.offset"}));
    h.step(&json!({"click":{"browser":"Front"}})).unwrap();
    let picked = h.app.dialog.as_ref().and_then(|d| d.inputs.first()).map(|i| i.items.clone());
    assert_eq!(picked, Some(vec![sketcraft_engine::Sel::Plane { name: "XZ".into() }]));
    assert!(painted_texts(&mut h).iter().any(|t| t == "Front"), "the chip shows the plane's name");
    h.step(&json!({"widget":"OK"})).unwrap();
    h.frames(2);
    let made = h.app.session.doc.features.last().map(|f| serde_json::to_value(&f.kind).unwrap_or_default()).unwrap_or_default();
    assert!(made.to_string().contains("\"XZ\""), "the command took the id: {made}");
    // The sketch plane choices of Redefine Sketch Plane: by name, and they run by id.
    crate::browser::redefine_sketch(&mut h.app, sketch);
    h.frames(2);
    let texts = painted_texts(&mut h);
    assert!(["Use Top plane", "Use Front plane", "Use Right plane"].iter().all(|t| texts.iter().any(|x| x == t)), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "Use XY plane"));
    crate::sketch_attachment_ui::clear(&mut h.app);

    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert_eq!(restored.preferences.origin_plane_names, TopFrontRight, "the choice is saved");
    let before = h.app.preferences.clone();
    assert!(control(&mut h.app, &json!({"origin_plane_names":"top", "nav":"solidworks"})).is_err());
    assert_eq!(h.app.preferences, before, "an unknown naming fails atomically");
    h.call("ui.prefs", json!({"open":"Design","origin_plane_names":"xyz"}));
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert_eq!(h.app.preferences.origin_plane_names, TopFrontRight, "Cancel takes it back");
    h.call("ui.prefs", json!({"open":"Design"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert_eq!(h.app.preferences.origin_plane_names, Xyz, "Restore Defaults: XY, XZ and YZ");
    h.frames(2);
    assert!(OriginPlaneNames::PLANES.iter().all(|p| rows().iter().any(|r| r == p)), "{:?}", rows());
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert_eq!(h.app.preferences.origin_plane_names, TopFrontRight);
}

/// The snap-suppress choice and Direct interaction read in the user's language: the classic
/// choice's "or" was English in every language, and Direct interaction had no message.
#[test]
fn modifier_choice_and_direct_interaction_are_translated() {
    let _japanese = crate::i18n::scope("ja");
    let either = crate::tool_modifiers::ModifierCombo::CTRL_ALT.shown_label();
    assert!(either.contains(" または ") && !either.contains(" or "), "{either}");
    assert_ne!(crate::tool_modifiers::ModifierCombo::NONE.shown_label(), "None");
    for english in [
        "Direct interaction",
        "Work on the model directly in the viewport. Press a flat face or a profile and pull: up extrudes, down into a body cuts, and sideways movement sets the step size",
    ] {
        assert_ne!(crate::i18n::tr_label(english), english);
    }
}

#[test]
fn layout_cards_and_toolbar_chips_preserve_transactions_in_both_themes() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":1200,"height":1100}));
        h.call("ui.prefs", json!({"theme":theme,"quick_measure":true,"invert_curved_roll_arrows":true}));
        h.call("ui.prefs", json!({"open":"Layout"}));
        let before = h.app.preferences.clone();
        let doc = h.app.session.doc.to_json();
        for (key, left, under, browser) in
            [("left", true, false, false), ("under_browser", true, true, false), ("browser", false, false, true), ("bottom", false, false, false)]
        {
            scroll_to_and_click(&mut h, &format!("prefs:history:{key}"));
            assert_eq!(
                (h.app.preferences.timeline_left, h.app.preferences.timeline_under_browser, h.app.preferences.history_in_browser),
                (left, under, browser)
            );
        }
        for (key, floating) in [("card", true), ("column", false), ("card", true)] {
            scroll_to_and_click(&mut h, &format!("prefs:browser:{key}"));
            assert_eq!(h.app.preferences.browser_float, floating);
        }
        for tab in crate::workspace::choosable_tabs() {
            scroll_to_and_click(&mut h, &format!("prefs:toolbar_tab:{tab}"));
            assert!(!crate::workspace::tab_turned_on(&h.app, tab));
            scroll_to_and_click(&mut h, &format!("prefs:toolbar_tab:{tab}"));
            assert!(crate::workspace::tab_turned_on(&h.app, tab));
        }
        assert_eq!(h.app.session.doc.to_json(), doc);
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert_eq!(h.app.preferences, before, "Cancel restores every Layout edit");
        h.call("ui.prefs", json!({"open":"Layout"}));
        scroll_to_and_click(&mut h, "prefs:browser:card");
        h.call("ui.prefs", json!({"button":"apply"}));
        h.call("ui.prefs", json!({"button":"defaults"}));
        assert!(!h.app.preferences.browser_float);
        assert!(h.app.preferences.quick_measure && h.app.preferences.invert_curved_roll_arrows, "Layout reset retains Graphics and Navigation");
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert!(h.app.preferences.browser_float, "Cancel after Apply returns to the applied state");
    }
}

#[test]
fn history_preferences_widget_click_reveals_scrolled_content_and_keeps_folds_closed() {
    let steps: serde_json::Value = serde_json::from_str(include_str!("../tests/scenarios/history_sketches_under_feature.json")).unwrap();
    let steps = steps.as_array().unwrap();
    let transition = steps.iter().position(|step| step["params"]["history_sketches"] == "under_feature").unwrap();
    let mut h = crate::scenario::Harness::new();
    for step in steps.iter().take(transition) {
        h.step(step).unwrap();
    }
    h.call("ui.prefs", json!({"open":"Layout"}));
    let page = crate::scenario::rect_of("prefs:page_body").unwrap();
    h.call("ui.scroll", json!({"x":page.center().x,"y":page.center().y,"delta":-10000}));
    h.frames(60);
    let at = h.widget("Under their feature").unwrap();
    let page = crate::scenario::rect_of("prefs:page_body").unwrap();
    assert!(!page.contains(egui::pos2(at[0] as f32, at[1] as f32)), "the real control begins scrolled out of view");
    assert!((at[1] as f32) < page.top(), "scrolling to the bottom puts the target above the viewport");
    assert_eq!(h.app.preferences.history_sketches, HistorySketches::InOrder);
    h.step(&json!({"widget":"Under their feature", "near":"Sketches in History"})).unwrap();
    assert_eq!(h.app.preferences.history_sketches, HistorySketches::UnderFeature);
    let at = h.widget("Under their feature").unwrap();
    assert!(crate::scenario::rect_of("prefs:page_body").unwrap().contains(egui::pos2(at[0] as f32, at[1] as f32)));
    assert!(h.app.tree.expanded.is_empty(), "revealing a preference does not open history folds");
    let page_before = crate::scenario::rect_of("prefs:page_body");
    h.step(&json!({"widget":"Apply"})).unwrap();
    assert!(h.app.prefs_window.open);
    assert_eq!(crate::scenario::rect_of("prefs:page_body"), page_before);
    h.step(&json!({"widget":"OK"})).unwrap();
    h.step(&steps[transition + 1]).unwrap();
    assert!(!h.app.prefs_window.open);
    for step in steps.iter().skip(transition + 2) {
        h.step(step).unwrap();
    }
    h.call("ui.prefs", json!({"open":"Layout"}));
    h.step(&json!({"widget":"Navigation"})).unwrap();
    assert_eq!(h.app.prefs_window.section, PAGE_NAVIGATION, "sidebar navigation remains a real click");
    h.step(&json!({"widget":"Cancel"})).unwrap();
    assert!(!h.app.prefs_window.open);
}

#[test]
fn history_sketches_defaults_in_order_and_follows_preferences_transactions() {
    let old: Prefs = serde_json::from_value(json!({"timeline_left": true})).unwrap();
    assert_eq!(old.history_sketches, HistorySketches::InOrder, "designs and settings from before keep the creation order");
    assert_eq!(lenient(&json!({"history_sketches": "sideways"})).history_sketches, HistorySketches::InOrder);
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"timeline_left": true, "timeline_under_browser": true}));
    let design = h.app.session.doc.to_json();
    h.call("ui.prefs", json!({"open": "Layout"}));
    let row = h.app.tr("prefs.history_sketches").to_string();
    let under = h.app.tr("prefs.history_sketches_under_feature").to_string();
    let handle = format!("dialog:{row}:{under}");
    super::reveal_preference(&mut h, &handle);
    let at = crate::scenario::handle_at(&handle).unwrap();
    h.call("ui.click", json!({"x": at.x, "y": at.y}));
    assert_eq!(h.app.preferences.history_sketches, HistorySketches::UnderFeature);
    assert_eq!(h.app.session.doc.to_json(), design, "the preference changes only the History");
    h.call("ui.prefs", json!({"button": "cancel"}));
    assert_eq!(h.app.preferences.history_sketches, HistorySketches::InOrder);

    let reply = h.call("ui.prefs", json!({"history_sketches": "under_feature"}));
    assert_eq!(reply["result"]["history_sketches"], "under_feature");
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert_eq!(restored.preferences.history_sketches, HistorySketches::UnderFeature);
    let before = h.app.preferences.clone();
    assert!(control(&mut h.app, &json!({"history_sketches": "sideways"})).is_err());
    assert_eq!(h.app.preferences, before, "an unknown value changes nothing");
    h.call("ui.prefs", json!({"open": "Layout"}));
    h.call("ui.prefs", json!({"button": "defaults"}));
    assert_eq!(h.app.preferences.history_sketches, HistorySketches::InOrder);
    h.call("ui.prefs", json!({"button": "cancel"}));
    assert_eq!(h.app.preferences.history_sketches, HistorySketches::UnderFeature);
}

#[test]
fn dialog_buttons_default_bottom_and_follow_preferences_transactions() {
    use crate::ui_kit::DialogButtons;
    let old: Prefs = serde_json::from_value(json!({"ribbon_labels": false})).unwrap();
    assert_eq!(old.dialog_buttons, DialogButtons::Bottom, "settings from before keep the footer");
    assert_eq!(lenient(&json!({"dialog_buttons": "sideways"})).dialog_buttons, DialogButtons::Bottom);
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"open": "Layout"}));
    super::reveal_preference(&mut h, "prefs:dialog_buttons:top");
    h.step(&json!({"click": {"handle": "prefs:dialog_buttons:top"}})).unwrap();
    assert_eq!(h.app.preferences.dialog_buttons, DialogButtons::Top);
    h.call("ui.prefs", json!({"button": "cancel"}));
    assert_eq!(h.app.preferences.dialog_buttons, DialogButtons::Bottom);

    let reply = h.call("ui.prefs", json!({"dialog_buttons": "top"}));
    assert_eq!(reply["result"]["dialog_buttons"], "top");
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert_eq!(restored.preferences.dialog_buttons, DialogButtons::Top);
    let before = h.app.preferences.clone();
    assert!(control(&mut h.app, &json!({"dialog_buttons": "sideways"})).is_err());
    assert_eq!(h.app.preferences, before, "an unknown value changes nothing");
    h.call("ui.prefs", json!({"open": "Layout"}));
    h.call("ui.prefs", json!({"button": "defaults"}));
    assert_eq!(h.app.preferences.dialog_buttons, DialogButtons::Bottom);
    h.call("ui.prefs", json!({"button": "cancel"}));
    assert_eq!(h.app.preferences.dialog_buttons, DialogButtons::Top);
}

#[test]
fn navigation_legend_follows_all_presets_and_runtime_options_without_mutating_them() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    for preset in NAV_PRESETS {
        choose_nav_preset(&mut app.preferences, preset);
        let before = app.preferences.clone();
        let rows = navigation::mapping_rows(&app);
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|(keys, action)| !keys.is_empty() && !action.is_empty()));
        assert_eq!(app.preferences, before);
        if preset.id == "creo" {
            assert!(preset.reverse_drag_zoom);
            assert_eq!(preset.mode(NavButton::Middle, Modifiers::NONE), Some(NavMode::Orbit));
            assert!(rows.iter().any(|(keys, action)| keys.iter().any(|key| key == "Ctrl/Cmd") && action == crate::i18n::tr("prefs.gesture_zoom")));
        }
        if preset.id == TOUCHPAD_NAV {
            assert!(
                rows.iter()
                    .any(|(keys, action)| keys == &[crate::i18n::tr("prefs.shift").to_owned()] && action == crate::i18n::tr("prefs.gesture_pan"))
            );
            assert!(rows.iter().any(|(keys, action)| keys == &["Alt/Option".to_owned()] && action == crate::i18n::tr("prefs.gesture_orbit")));
            assert!(!app.preferences.trackpad_pans());
        }
    }
    choose_nav_preset(&mut app.preferences, nav_preset(crate::mouse_gesture::MOUSE_MODE));
    app.preferences.mouse_gestures.enabled = true;
    let rows = navigation::mapping_rows(&app);
    assert!(rows.iter().any(|(_, action)| action == crate::i18n::tr("prefs.context_menu_ring")));
    assert!(
        !rows.iter().any(
            |(keys, action)| keys.contains(&crate::i18n::tr("prefs.button_right").to_owned()) && action == crate::i18n::tr("prefs.gesture_orbit")
        )
    );
}

#[test]
fn navigation_dropdown_escape_retains_live_edits_then_cancels_the_snapshot() {
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":1600,"height":1100}));
        h.call("ui.prefs", json!({"language":"en","theme":theme}));
        let before = h.app.preferences.clone();
        h.call("ui.prefs", json!({"open":"Navigation","zoom_reverse":true}));
        let edited = h.app.preferences.clone();
        scroll_to_and_click(&mut h, "prefs:nav");
        let popup = "dialog:list:creo";
        assert!(crate::scenario::handle_at(popup).is_some());
        h.step(&json!({"key":"Escape"})).unwrap();
        h.frames(2);
        assert!(h.app.prefs_window.open && h.app.prefs_window.snapshot.is_some());
        assert_eq!(h.app.preferences, edited);
        assert!(crate::scenario::handle_at(popup).is_none());
        h.step(&json!({"key":"Enter"})).unwrap();
        h.frames(2);
        assert!(crate::scenario::handle_at(popup).is_some(), "Escape restored focus to the mapping trigger");
        h.step(&json!({"key":"Escape"})).unwrap();
        h.frames(2);
        h.step(&json!({"key":"Escape"})).unwrap();
        h.frames(2);
        assert!(!h.app.prefs_window.open);
        assert_eq!(h.app.preferences, before);
    }
}

#[test]
fn orbit_picture_cards_keep_legacy_migration_and_preferences_transactions() {
    let legacy = migrate(serde_json::from_value::<Prefs>(json!({"orbit_cursor":true})).unwrap());
    assert_eq!(legacy.orbit_around, OrbitAround::Cursor);
    assert!(!legacy.orbit_cursor);
    for theme in ["light", "dark"] {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.call("ui.resize", json!({"width":1600,"height":1100}));
        h.call("ui.prefs", json!({"language":"en","theme":theme,"open":"Navigation"}));
        let camera = h.app.cam;
        for (key, value) in [("Origin", OrbitAround::Origin), ("View center", OrbitAround::ViewCentre), ("Under the cursor", OrbitAround::Cursor)] {
            scroll_to_and_click(&mut h, &format!("prefs:orbit_around:{key}"));
            assert_eq!(h.app.preferences.orbit_around, value);
            assert_eq!(h.app.cam, camera, "choosing the pivot does not turn the camera");
            assert!(!h.app.preferences.orbit_cursor);
        }
        h.call("ui.prefs", json!({"button":"apply"}));
        h.call("ui.prefs", json!({"button":"defaults"}));
        assert_eq!(h.app.preferences.orbit_around, OrbitAround::ViewCentre);
        h.call("ui.prefs", json!({"button":"cancel"}));
        assert_eq!(h.app.preferences.orbit_around, OrbitAround::Cursor);
    }
}

#[test]
fn orbit_cursor_wrap_defaults_on_and_can_be_turned_off() {
    let old: Prefs = serde_json::from_value(json!({"nav":"solidworks"})).unwrap();
    assert!(old.orbit_cursor_wrap, "saved preferences from before the option wrap");
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"open":"Navigation"}));
    h.step(&json!({"click":{"handle":"prefs:orbit_cursor_wrap"}})).unwrap();
    assert!(!h.app.preferences.orbit_cursor_wrap);
    h.call("ui.prefs", json!({"button":"apply"}));
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(!restored.preferences.orbit_cursor_wrap);
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(h.app.preferences.orbit_cursor_wrap);
    h.call("ui.prefs", json!({"orbit_cursor_wrap":false,"button":"apply"}));
    assert!(!h.app.preferences.orbit_cursor_wrap);
}

#[test]
fn free_orbit_defaults_off_and_follows_preferences_transactions() {
    let old: Prefs = serde_json::from_value(json!({"nav":"solidworks"})).unwrap();
    assert!(!old.free_orbit);
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"open":"Navigation"}));
    h.app.preferences.free_orbit_upright = true;
    h.frames(2);
    scroll_to_and_click(&mut h, "prefs:free_orbit_upright");
    assert!(h.app.preferences.free_orbit_upright, "the disabled upright control preserves its saved value");
    assert!(!h.app.preferences.free_orbit);
    scroll_to_and_click(&mut h, "prefs:free_orbit");
    assert!(h.app.preferences.free_orbit);
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(!restored.preferences.free_orbit, "a change that was not applied is not saved (#9730)");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.free_orbit);
    h.call("ui.prefs", json!({"open":"Navigation","free_orbit":true,"button":"apply"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(!h.app.preferences.free_orbit);
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(h.app.preferences.free_orbit);
}

#[test]
fn radial_context_menu_defaults_off_and_follows_preferences_transactions() {
    assert!(!Prefs::default().radial_context_menu);
    let old: Prefs = serde_json::from_value(json!({"nav":"solidworks","zoom_reverse":true})).unwrap();
    assert!(!old.radial_context_menu && old.zoom_reverse && old.nav == "solidworks");
    let mut h = crate::scenario::Harness::new();
    h.call("ui.prefs", json!({"open":"Navigation"}));
    for _ in 0..16 {
        let page = crate::scenario::rect_of("dialog:page_body").expect("Navigation page body");
        if h.widget("Radial context menu").is_some_and(|[x, y]| page.contains(egui::pos2(x as f32, y as f32))) {
            break;
        }
        h.call("ui.scroll", json!({"x":page.center().x,"y":page.center().y,"delta":-100}));
        h.frames(20);
    }
    h.step(&json!({"widget":"Radial context menu"})).unwrap();
    assert!(h.app.preferences.radial_context_menu, "the actual checkbox enables radial menus");
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(!restored.preferences.radial_context_menu, "a change that was not applied is not saved (#9730)");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.radial_context_menu, "Cancel restores the default-off setting");
    h.call("ui.prefs", json!({"open":"Navigation","radial_context_menu":true}));
    h.call("ui.prefs", json!({"button":"apply"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(!h.app.preferences.radial_context_menu, "Navigation defaults disable the ring");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(h.app.preferences.radial_context_menu, "Cancel restores the last Apply");
}

#[test]
fn single_surface_analysis_live_cancel_restores_policy_without_restarting_effects() {
    let mut app = SketApp::new(Default::default(), Default::default());
    super::control(&mut app, &json!({"single_surface_analysis":false})).unwrap();
    for (command, params) in [
        ("solid.box", json!({"length":10,"width":10,"height":10})),
        ("inspect.zebra", json!({"stripes":14.5})),
        ("inspect.section", json!({"plane":"XY"})),
        ("inspect.continuity", json!({"edges":[[5,0,0]],"samples":5})),
    ] {
        app.run(command, params).unwrap();
    }
    app.session.mark_saved();
    let doc = app.session.doc.clone();
    let undo = app.session.undo.len();
    assert_eq!(app.session.status_items().len(), 3);
    super::control(&mut app, &json!({"open":"Graphics"})).unwrap();
    super::control(&mut app, &json!({"single_surface_analysis":true})).unwrap();
    assert!(app.session.single_surface_analysis());
    assert_eq!(app.session.analysis, None, "turning on applies immediately and ends older shading");
    assert_eq!(app.session.status_items()[0].command, "inspect.continuity");
    super::control(&mut app, &json!({"button":"cancel"})).unwrap();
    assert!(!app.preferences.single_surface_analysis && !app.session.single_surface_analysis());
    assert_eq!(app.session.analysis, None, "Cancel restores the policy, not ended view effects");
    assert_eq!(app.session.continuity.len(), 1);
    assert_eq!(app.session.status_items().len(), 2);
    app.run("inspect.zebra", json!({"stripes":14.5})).unwrap();
    assert!(app.session.analysis.is_some() && !app.session.continuity.is_empty());
    assert_eq!(app.session.status_items().len(), 3, "the restored legacy policy permits reactivation");
    assert!(std::sync::Arc::ptr_eq(&doc, &app.session.doc));
    assert!(!app.session.is_dirty());
    assert_eq!(app.session.undo.len(), undo);
}

#[test]
fn persistent_ribbon_pressed_preference_checkbox_control_persists_cancels_and_resets() {
    assert!(Prefs::default().persistent_ribbon_pressed);
    assert!(lenient(&json!({"theme":"light"})).persistent_ribbon_pressed);
    assert!(lenient(&json!({"persistent_ribbon_pressed":"no"})).persistent_ribbon_pressed);
    let mut h = crate::scenario::Harness::new();
    let design = h.app.session.doc.to_json();
    let revision = h.app.session.revision;
    h.call("ui.prefs", json!({"open":"Layout"}));
    super::reveal_preference(&mut h, "prefs:persistent_ribbon_pressed");
    let checkbox = crate::scenario::handle_at("prefs:persistent_ribbon_pressed").unwrap();
    h.call("ui.click", json!({"x":checkbox.x,"y":checkbox.y}));
    assert!(!h.app.preferences.persistent_ribbon_pressed);
    assert_eq!(h.call("ui.prefs", json!({}))["result"]["persistent_ribbon_pressed"], false);
    let mut restored = SketApp::new(Default::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(restored.preferences.persistent_ribbon_pressed, "a change that was not applied is not saved (#9730)");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(h.app.preferences.persistent_ribbon_pressed);
    h.call("ui.prefs", json!({"open":"Layout","persistent_ribbon_pressed":false,"button":"apply"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(h.app.preferences.persistent_ribbon_pressed);
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.persistent_ribbon_pressed);
    for malformed in [json!("false"), json!(null), json!(0), json!([])] {
        assert!(super::set(&mut h.app, &json!({"persistent_ribbon_pressed":malformed})).is_err());
        assert!(!h.app.preferences.persistent_ribbon_pressed);
    }
    assert_eq!(h.app.session.revision, revision);
    assert_eq!(h.app.session.doc.to_json(), design);
}

#[test]
fn viewport_analysis_status_preference_checkbox_control_persists_cancels_and_resets() {
    assert!(Prefs::default().viewport_analysis_status);
    assert!(lenient(&json!({"theme":"light"})).viewport_analysis_status);
    assert!(lenient(&json!({"viewport_analysis_status":"no"})).viewport_analysis_status);
    let mut h = crate::scenario::Harness::new();
    let design = h.app.session.doc.to_json();
    let revision = h.app.session.revision;
    h.call("ui.prefs", json!({"open":"Graphics"}));
    super::reveal_preference(&mut h, "prefs:viewport_analysis_status");
    let checkbox = crate::scenario::handle_at("prefs:viewport_analysis_status").unwrap();
    h.call("ui.click", json!({"x":checkbox.x,"y":checkbox.y}));
    assert!(!h.app.preferences.viewport_analysis_status);
    assert_eq!(h.call("ui.prefs", json!({}))["result"]["viewport_analysis_status"], false);
    let mut restored = SketApp::new(Default::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(restored.preferences.viewport_analysis_status, "a change that was not applied is not saved (#9730)");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(h.app.preferences.viewport_analysis_status);
    h.call("ui.prefs", json!({"open":"Graphics","viewport_analysis_status":false,"button":"apply"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(h.app.preferences.viewport_analysis_status);
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.viewport_analysis_status);
    for malformed in [json!("false"), json!(null), json!(0), json!([])] {
        assert!(super::set(&mut h.app, &json!({"viewport_analysis_status":malformed})).is_err());
        assert!(!h.app.preferences.viewport_analysis_status);
    }
    assert_eq!(h.app.session.revision, revision);
    assert_eq!(h.app.session.doc.to_json(), design);
}

#[test]
fn viewport_analysis_status_is_independent_of_session_policy_in_background_documents() {
    let mut app = SketApp::new(Default::default(), Default::default());
    super::set(&mut app, &json!({"single_surface_analysis":false,"persistent_ribbon_pressed":false})).unwrap();
    app.run("inspect.zebra", json!({})).unwrap();
    let mut other = sketcraft_engine::Session::default();
    other.execute("inspect.draft", &json!({})).unwrap();
    crate::documents::open_tab(&mut app, other).unwrap();
    let before: Vec<_> = std::iter::once(&app.session).chain(app.docs.background()).map(|s| (s.status_items(), s.revision, s.undo.len())).collect();
    super::control(&mut app, &json!({"open":"Graphics","viewport_analysis_status":false})).unwrap();
    assert!(!app.preferences.viewport_analysis_status);
    assert!(!app.preferences.persistent_ribbon_pressed);
    assert!(std::iter::once(&app.session).chain(app.docs.background()).all(|s| !s.single_surface_analysis()));
    super::control(&mut app, &json!({"button":"cancel"})).unwrap();
    assert!(app.preferences.viewport_analysis_status);
    let after: Vec<_> = std::iter::once(&app.session).chain(app.docs.background()).map(|s| (s.status_items(), s.revision, s.undo.len())).collect();
    assert_eq!(before, after, "a presentation preference never changes effects in any document");
}

#[test]
fn viewport_section_status_preference_checkbox_control_persists_cancels_and_resets() {
    assert!(Prefs::default().viewport_section_status);
    assert!(lenient(&json!({"theme":"light"})).viewport_section_status);
    assert!(lenient(&json!({"viewport_section_status":"no"})).viewport_section_status);
    let mut h = crate::scenario::Harness::new();
    let design = h.app.session.doc.to_json();
    let revision = h.app.session.revision;
    h.call("ui.prefs", json!({"open":"Graphics"}));
    super::reveal_preference(&mut h, "prefs:viewport_section_status");
    let checkbox = crate::scenario::handle_at("prefs:viewport_section_status").unwrap();
    h.call("ui.click", json!({"x":checkbox.x,"y":checkbox.y}));
    assert!(!h.app.preferences.viewport_section_status);
    assert_eq!(h.call("ui.prefs", json!({}))["result"]["viewport_section_status"], false);
    let mut restored = SketApp::new(Default::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(restored.preferences.viewport_section_status, "a change that was not applied is not saved (#9730)");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(h.app.preferences.viewport_section_status);
    h.call("ui.prefs", json!({"open":"Graphics","viewport_section_status":false,"button":"apply"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(h.app.preferences.viewport_section_status);
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.viewport_section_status);
    for malformed in [json!("false"), json!(null), json!(0), json!([])] {
        assert!(super::set(&mut h.app, &json!({"viewport_section_status":malformed})).is_err());
        assert!(!h.app.preferences.viewport_section_status);
    }
    assert_eq!(h.app.session.revision, revision);
    assert_eq!(h.app.session.doc.to_json(), design);
}

#[test]
fn viewport_status_details_preference_checkbox_control_persists_cancels_and_resets() {
    assert!(Prefs::default().viewport_status_details);
    assert!(lenient(&json!({"theme":"light"})).viewport_status_details);
    assert!(lenient(&json!({"viewport_status_details":"no"})).viewport_status_details);
    let mut h = crate::scenario::Harness::new();
    let design = h.app.session.doc.to_json();
    let revision = h.app.session.revision;
    h.call("ui.prefs", json!({"open":"Graphics"}));
    super::reveal_preference(&mut h, "prefs:viewport_status_details");
    let checkbox = crate::scenario::handle_at("prefs:viewport_status_details").unwrap();
    h.call("ui.click", json!({"x":checkbox.x,"y":checkbox.y}));
    assert!(!h.app.preferences.viewport_status_details);
    assert_eq!(h.call("ui.prefs", json!({}))["result"]["viewport_status_details"], false);
    let mut restored = SketApp::new(Default::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(restored.preferences.viewport_status_details, "a change that was not applied is not saved (#9730)");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(h.app.preferences.viewport_status_details);
    h.call("ui.prefs", json!({"open":"Graphics","viewport_status_details":false,"button":"apply"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(h.app.preferences.viewport_status_details);
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.viewport_status_details);
    for malformed in [json!("false"), json!(null), json!(0), json!([])] {
        assert!(super::set(&mut h.app, &json!({"viewport_status_details":malformed})).is_err());
        assert!(!h.app.preferences.viewport_status_details);
    }
    assert_eq!(h.app.session.revision, revision);
    assert_eq!(h.app.session.doc.to_json(), design);
}

#[test]
fn viewport_status_summary_preference_checkbox_control_persists_cancels_and_resets() {
    assert!(Prefs::default().viewport_status_summary);
    assert!(lenient(&json!({"theme":"light"})).viewport_status_summary);
    assert!(lenient(&json!({"viewport_status_summary":"no"})).viewport_status_summary);
    let mut h = crate::scenario::Harness::new();
    let design = h.app.session.doc.to_json();
    let revision = h.app.session.revision;
    h.call("ui.prefs", json!({"open":"Graphics"}));
    super::reveal_preference(&mut h, "prefs:viewport_status_summary");
    let checkbox = crate::scenario::handle_at("prefs:viewport_status_summary").unwrap();
    h.call("ui.click", json!({"x":checkbox.x,"y":checkbox.y}));
    assert!(!h.app.preferences.viewport_status_summary);
    assert_eq!(h.call("ui.prefs", json!({}))["result"]["viewport_status_summary"], false);
    let mut restored = SketApp::new(Default::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(restored.preferences.viewport_status_summary, "a change that was not applied is not saved (#9730)");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(h.app.preferences.viewport_status_summary);
    h.call("ui.prefs", json!({"open":"Graphics","viewport_status_summary":false,"button":"apply"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(h.app.preferences.viewport_status_summary);
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.viewport_status_summary);
    for malformed in [json!("false"), json!(null), json!(0), json!([])] {
        assert!(super::set(&mut h.app, &json!({"viewport_status_summary":malformed})).is_err());
        assert!(!h.app.preferences.viewport_status_summary);
    }
    assert_eq!(h.app.session.revision, revision);
    assert_eq!(h.app.session.doc.to_json(), design);
}

#[test]
fn viewport_section_status_is_independent_of_session_policy_in_background_documents() {
    let mut app = SketApp::new(Default::default(), Default::default());
    super::set(&mut app, &json!({"single_surface_analysis":false,"persistent_ribbon_pressed":false})).unwrap();
    app.run("inspect.zebra", json!({})).unwrap();
    app.run("inspect.section", json!({"plane":"XY","at":4})).unwrap();
    let mut other = sketcraft_engine::Session::default();
    other.execute("inspect.draft", &json!({})).unwrap();
    other.execute("inspect.section", &json!({"plane":"XZ","at":-3})).unwrap();
    crate::documents::open_tab(&mut app, other).unwrap();
    let before: Vec<_> = std::iter::once(&app.session).chain(app.docs.background()).map(|s| (s.status_items(), s.revision, s.undo.len())).collect();
    super::control(&mut app, &json!({"open":"Graphics","viewport_section_status":false})).unwrap();
    assert!(!app.preferences.viewport_section_status);
    assert!(!app.preferences.persistent_ribbon_pressed);
    assert!(std::iter::once(&app.session).chain(app.docs.background()).all(|s| !s.single_surface_analysis()));
    super::control(&mut app, &json!({"button":"cancel"})).unwrap();
    assert!(app.preferences.viewport_section_status);
    let after: Vec<_> = std::iter::once(&app.session).chain(app.docs.background()).map(|s| (s.status_items(), s.revision, s.undo.len())).collect();
    assert_eq!(before, after, "a presentation preference never changes effects in any document");
}

#[test]
fn single_surface_analysis_preference_checkbox_persists_cancels_and_resets() {
    assert!(Prefs::default().single_surface_analysis);
    let old: Prefs = serde_json::from_value(json!({"theme":"light"})).unwrap();
    assert!(old.single_surface_analysis);
    assert!(lenient(&json!({"single_surface_analysis":"no"})).single_surface_analysis);
    let mut h = crate::scenario::Harness::new();
    let design = h.app.session.doc.to_json();
    let revision = h.app.session.revision;
    h.call("ui.prefs", json!({"open":"Graphics"}));
    super::reveal_preference(&mut h, "prefs:single_surface_analysis");
    let checkbox = crate::scenario::handle_at("prefs:single_surface_analysis").unwrap();
    h.call("ui.click", json!({"x":checkbox.x,"y":checkbox.y}));
    assert!(!h.app.preferences.single_surface_analysis);
    assert!(!h.app.session.single_surface_analysis());
    let mut restored = SketApp::new(Default::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(restored.preferences.single_surface_analysis, "a change that was not applied is not saved (#9730)");
    assert!(restored.session.single_surface_analysis(), "loaded settings apply before any command");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(h.app.preferences.single_surface_analysis && h.app.session.single_surface_analysis());
    h.call("ui.prefs", json!({"open":"Graphics","single_surface_analysis":false,"button":"apply"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(h.app.preferences.single_surface_analysis && h.app.session.single_surface_analysis());
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.single_surface_analysis && !h.app.session.single_surface_analysis());
    for malformed in [json!("false"), json!(null), json!(0), json!([])] {
        assert!(super::set(&mut h.app, &json!({"single_surface_analysis":malformed})).is_err());
        assert!(!h.app.preferences.single_surface_analysis && !h.app.session.single_surface_analysis());
    }
    assert_eq!(h.app.session.revision, revision, "policy-only edits with no effects need no view revision");
    assert_eq!(h.app.session.doc.to_json(), design);
    assert!(!h.app.session.is_dirty());
    assert!(h.app.session.undo.is_empty());
}

#[test]
fn single_surface_analysis_preference_syncs_background_designs_and_resets_effects() {
    fn legacy_session() -> sketcraft_engine::Session {
        let mut s = sketcraft_engine::Session::default();
        s.set_single_surface_analysis(false);
        s.execute("solid.box", &json!({"length":10,"width":10,"height":10})).unwrap();
        s.execute("inspect.zebra", &json!({"stripes":14.5})).unwrap();
        s.execute("inspect.section", &json!({"plane":"XY"})).unwrap();
        s.execute("inspect.continuity", &json!({"edges":[[5,0,0]],"samples":5})).unwrap();
        s.mark_saved();
        s
    }
    let mut app = SketApp::new(Default::default(), Default::default());
    super::set(&mut app, &json!({"single_surface_analysis":false})).unwrap();
    app.session = legacy_session();
    crate::documents::open_tab(&mut app, legacy_session()).unwrap();
    assert_eq!(app.docs.count(), 2);
    assert!(!app.session.single_surface_analysis());
    assert_eq!(app.session.status_items().len(), 3);
    assert!(app.docs.background().all(|s| !s.single_surface_analysis() && s.status_items().len() == 3));
    let doc = app.session.doc.clone();
    let undo = app.session.undo.len();
    let declarations = app.session.status_items();
    let revision = app.session.revision;
    app.preferences.single_surface_analysis = true;
    app.run("view.status_items", json!({})).unwrap();
    assert!(!app.session.single_surface_analysis(), "a raw query cannot apply a desynchronised preference");
    assert_eq!(app.session.status_items(), declarations);
    assert_eq!(app.session.revision, revision);
    assert!(app.session.analysis.is_some() && !app.session.continuity.is_empty());
    assert!(app.docs.background().all(|s| !s.single_surface_analysis() && s.status_items().len() == 3));
    app.preferences.single_surface_analysis = false;
    super::defaults(&mut app, super::PAGE_GENERAL);
    assert!(!app.session.single_surface_analysis(), "another page leaves the policy alone");
    super::defaults(&mut app, super::PAGE_GRAPHICS);
    for s in std::iter::once(&app.session).chain(app.docs.background()) {
        assert!(s.single_surface_analysis());
        assert_eq!(s.analysis, None, "latest Continuity wins on reset");
        assert_eq!(s.continuity.len(), 1);
        assert_eq!(s.status_items().len(), 2);
        assert_eq!(s.status_items()[0].command, "inspect.continuity");
        assert!(s.section.is_some());
        assert!(!s.is_dirty());
    }
    assert!(std::sync::Arc::ptr_eq(&doc, &app.session.doc));
    assert_eq!(app.session.undo.len(), undo);
    // Query entry never applies a pending policy change; the explicit frame hook does.
    app.preferences.single_surface_analysis = false;
    app.run("view.status_items", json!({})).unwrap();
    assert!(app.session.single_surface_analysis());
    super::apply(&mut app, &egui::Context::default());
    assert!(!app.session.single_surface_analysis());
    assert!(app.docs.background().all(|s| !s.single_surface_analysis()));
    app.preferences.single_surface_analysis = true;
    super::apply(&mut app, &egui::Context::default());
    assert!(app.session.single_surface_analysis());
    assert!(app.docs.background().all(|s| s.single_surface_analysis()));
}

#[test]
fn cosmetic_threads_default_off_and_follow_preferences_transactions() {
    assert!(!Prefs::default().cosmetic_threads);
    let old: Prefs = serde_json::from_value(json!({"theme":"light","msaa":4})).unwrap();
    assert!(!old.cosmetic_threads && old.theme == "light" && old.msaa == 4);
    let malformed = lenient(&json!({"cosmetic_threads":"yes","theme":"light"}));
    assert!(!malformed.cosmetic_threads && malformed.theme == "light");
    let mut h = crate::scenario::Harness::new();
    let design = h.app.session.doc.to_json();
    h.call("ui.prefs", json!({"open":"Graphics"}));
    super::reveal_preference(&mut h, "prefs:cosmetic_threads");
    let checkbox = crate::scenario::handle_at("prefs:cosmetic_threads").unwrap();
    h.call("ui.click", json!({"x":checkbox.x,"y":checkbox.y}));
    assert!(h.app.preferences.cosmetic_threads, "the actual checkbox enables the optional markings");
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(!restored.preferences.cosmetic_threads, "a change that was not applied is not saved (#9730)");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.cosmetic_threads);
    h.call("ui.prefs", json!({"open":"Graphics","cosmetic_threads":true,"button":"apply"}));
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(!h.app.preferences.cosmetic_threads, "Graphics defaults restore the default-off policy");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(h.app.preferences.cosmetic_threads, "Cancel restores the last Apply");
    h.call("ui.prefs", json!({"cosmetic_threads":false}));
    restored.load_prefs(&h.app.prefs());
    assert!(!restored.preferences.cosmetic_threads, "an explicitly disabled value also survives reload");
    assert_eq!(h.app.session.doc.to_json(), design);
    assert!(h.app.session.undo.is_empty());
}

#[test]
fn hidden_edge_preference_defaults_persists_and_uses_real_checkbox() {
    let mut h = crate::scenario::Harness::new();
    assert!(!Prefs::default().select_hidden_edges);
    assert!(!serde_json::from_value::<Prefs>(json!({})).unwrap().select_hidden_edges);
    h.call("ui.prefs", json!({"open":"Sketch"}));
    super::reveal_preference(&mut h, "prefs:select_hidden_edges");
    h.step(&json!({"click":{"handle":"prefs:select_hidden_edges"}})).unwrap();
    assert!(h.app.preferences.select_hidden_edges);
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.select_hidden_edges);
    h.call("ui.prefs", json!({"open":"Sketch","select_hidden_edges":true,"button":"apply"}));
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(restored.preferences.select_hidden_edges);
    h.call("ui.prefs", json!({"button":"defaults"}));
    assert!(!h.app.preferences.select_hidden_edges);
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(h.app.preferences.select_hidden_edges);
}

#[test]
fn ground_shadow_placement_follows_preferences_transactions() {
    assert!(!Prefs::default().ground_shadow_follows_model);
    let old: Prefs = serde_json::from_value(json!({"msaa":4})).unwrap();
    assert!(!old.ground_shadow_follows_model);
    assert!(!lenient(&json!({"ground_shadow_follows_model":"yes"})).ground_shadow_follows_model);
    let mut h = crate::scenario::Harness::new();
    h.app.ui.ground_shadow = true;
    let camera = h.app.cam;
    let design = h.app.session.doc.to_json();
    h.call("ui.prefs", json!({"open":"Graphics"}));
    super::reveal_preference(&mut h, "prefs:ground_shadow_follows_model");
    let checkbox = crate::scenario::handle_at("prefs:ground_shadow_follows_model").unwrap();
    h.call("ui.click", json!({"x":checkbox.x,"y":checkbox.y}));
    assert!(h.app.preferences.ground_shadow_follows_model);
    let mut restored = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    restored.load_prefs(&h.app.prefs());
    assert!(!restored.preferences.ground_shadow_follows_model, "an unapplied change is not saved");
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(!h.app.preferences.ground_shadow_follows_model);
    h.call("ui.prefs", json!({"ground_shadow_follows_model":true}));
    assert!(!h.app.prefs_window.open, "programmatic changes do not open Preferences");
    restored.load_prefs(&h.app.prefs());
    assert!(restored.preferences.ground_shadow_follows_model);
    h.call("ui.prefs", json!({"open":"Graphics", "button":"defaults"}));
    assert!(!h.app.preferences.ground_shadow_follows_model);
    h.call("ui.prefs", json!({"button":"cancel"}));
    assert!(h.app.preferences.ground_shadow_follows_model, "Cancel restores the saved choice");
    h.call("ui.prefs", json!({"open":"Graphics", "button":"defaults"}));
    h.call("ui.prefs", json!({"button":"apply"}));
    restored.load_prefs(&h.app.prefs());
    assert!(!restored.preferences.ground_shadow_follows_model, "Graphics defaults can be saved");
    assert_eq!(h.app.cam, camera);
    assert_eq!(h.app.session.doc.to_json(), design);
    assert!(h.app.session.undo.is_empty());
}

#[test]
fn tool_modifier_prefs_default_classic_and_persist() {
    let d = Prefs::default();
    assert_eq!(d.suppress_sketch_snap, ModifierCombo::CTRL_ALT);
    assert!(d.manipulator_free_alt);
    assert!(!d.selection_shift_add_only);
    let old: Prefs = serde_json::from_value(json!({"nav": "solidworks"})).unwrap();
    assert_eq!(old.suppress_sketch_snap, ModifierCombo::CTRL_ALT);
    assert!(!old.selection_shift_add_only);
    // Classic default: Ctrl/Cmd or Alt each suppress (`command` is primary on all hosts).
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    let ctrl = Modifiers { ctrl: true, command: true, ..Default::default() };
    let alt = Modifiers { alt: true, ..Default::default() };
    let shift = Modifiers { shift: true, ..Default::default() };
    assert!(suppress_sketch_snap_held(&app, ctrl));
    assert!(suppress_sketch_snap_held(&app, alt));
    assert!(!suppress_sketch_snap_held(&app, shift));
    control(
        &mut app,
        &json!({"open": "Sketch", "suppress_sketch_snap": {"shift": false, "ctrl": true, "alt": false}, "manipulator_free_alt": false}),
    )
    .unwrap();
    control(&mut app, &json!({"button": "apply"})).unwrap();
    control(&mut app, &json!({"open": "Selection", "selection_shift_add_only": true})).unwrap();
    control(&mut app, &json!({"button": "apply"})).unwrap();
    assert_eq!(app.preferences.suppress_sketch_snap, ModifierCombo::CTRL);
    assert!(!app.preferences.manipulator_free_alt);
    assert!(app.preferences.selection_shift_add_only);
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert_eq!(other.preferences.suppress_sketch_snap, ModifierCombo::CTRL);
    assert!(!other.preferences.manipulator_free_alt);
    assert!(other.preferences.selection_shift_add_only);
    control(&mut app, &json!({"open": "Sketch", "button": "defaults"})).unwrap();
    assert_eq!(app.preferences.suppress_sketch_snap, ModifierCombo::CTRL_ALT);
    assert!(app.preferences.manipulator_free_alt);
    assert!(!app.preferences.selection_shift_add_only, "the merged Sketch & selection page resets both sets of modifiers");
    control(&mut app, &json!({"button": "cancel"})).unwrap();
    assert_eq!(app.preferences.suppress_sketch_snap, ModifierCombo::CTRL);
}

#[test]
fn wheel_smoothing_persists_and_follows_dialog_buttons() {
    assert!(serde_json::from_value::<Prefs>(json!({})).unwrap().smooth_wheel_zoom);
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    control(&mut app, &json!({"open":"Navigation", "smooth_wheel_zoom":false})).unwrap();
    control(&mut app, &json!({"button":"cancel"})).unwrap();
    assert!(app.preferences.smooth_wheel_zoom);
    control(&mut app, &json!({"open":"Navigation", "smooth_wheel_zoom":false})).unwrap();
    control(&mut app, &json!({"button":"apply"})).unwrap();
    control(&mut app, &json!({"smooth_wheel_zoom":true, "button":"cancel"})).unwrap();
    assert!(!app.preferences.smooth_wheel_zoom);
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert!(!other.preferences.smooth_wheel_zoom);
    control(&mut app, &json!({"open":"Navigation", "button":"defaults"})).unwrap();
    assert!(app.preferences.smooth_wheel_zoom);
    control(&mut app, &json!({"button":"cancel"})).unwrap();
    assert!(!app.preferences.smooth_wheel_zoom);
}

#[test]
fn touchpad_preset_persists_and_obeys_preference_transactions() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    control(&mut app, &json!({"open":"Navigation", "nav":"touchpad"})).unwrap();
    assert_eq!(app.preferences.nav, "touchpad");
    assert!(app.preferences.trackpad_pan, "choosing Touchpad must not overwrite the saved scroll preference");
    control(&mut app, &json!({"button":"cancel"})).unwrap();
    assert_eq!(app.preferences.nav, "fusion");
    control(&mut app, &json!({"open":"Navigation", "nav":"touchpad", "button":"apply"})).unwrap();
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert_eq!(other.preferences.nav, "touchpad");
    control(&mut app, &json!({"button":"defaults"})).unwrap();
    assert_eq!(app.preferences.nav, "fusion");
    control(&mut app, &json!({"button":"cancel"})).unwrap();
    assert_eq!(app.preferences.nav, "touchpad");
}

#[test]
fn choosing_the_solidworks_preset_frees_the_right_button_and_fits_on_middle_double_click() {
    let none = Modifiers::default();
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    app.preferences.middle_double_click = "off".into();
    set(&mut app, &json!({"nav": "solidworks"})).unwrap();
    assert!(app.preferences.right_drag_orbits, "a saved SolidWorks preset keeps its right-drag orbit");
    assert_eq!(nav_mode(&app, NavButton::Right, none), Some(NavMode::Orbit));
    choose_nav_preset(&mut app.preferences, nav_preset("fusion"));
    let p = &app.preferences;
    assert_eq!((p.nav.as_str(), p.zoom_reverse, p.middle_double_click.as_str(), p.right_drag_orbits), ("fusion", false, "off", true));
    choose_nav_preset(&mut app.preferences, nav_preset("solidworks"));
    let p = &app.preferences;
    assert_eq!((p.nav.as_str(), p.zoom_reverse, p.middle_double_click.as_str(), p.right_drag_orbits), ("solidworks", true, "fit", false));
    assert_eq!(nav_mode(&app, NavButton::Right, none), None, "right drag is left to the context menu (#6492)");
    assert_eq!(nav_mode(&app, NavButton::Middle, none), Some(NavMode::Orbit));
    set(&mut app, &json!({"nav": "openscad"})).unwrap();
    assert_eq!(nav_mode(&app, NavButton::Right, none), Some(NavMode::Pan), "only an orbiting right drag is affected");
    set(&mut app, &json!({"right_drag_orbits": true, "nav": "solidworks"})).unwrap();
    assert_eq!(nav_mode(&app, NavButton::Right, none), Some(NavMode::Orbit), "turned back on, the right drag orbits again");
}

#[test]
fn creo_preset_maps_navigation_and_persists() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    assert_eq!(app.preferences.nav, "fusion");
    choose_nav_preset(&mut app.preferences, nav_preset("creo"));
    assert_eq!(app.preferences.nav, "creo");
    assert!(!app.preferences.zoom_reverse);
    for (mods, mode) in
        [(Modifiers::NONE, NavMode::Orbit), (Modifiers::SHIFT, NavMode::Pan), (Modifiers::CTRL, NavMode::Zoom), (Modifiers::COMMAND, NavMode::Zoom)]
    {
        assert_eq!(nav_mode(&app, NavButton::Middle, mods), Some(mode));
    }
    for button in [NavButton::Left, NavButton::Right] {
        assert_eq!(nav_mode(&app, button, Modifiers::NONE), None);
    }
    assert!(nav_preset("creo").reverse_drag_zoom);
    for preset in NAV_PRESETS.iter().filter(|p| p.id != "creo") {
        assert!(!preset.reverse_drag_zoom, "{} keeps its drag zoom direction", preset.id);
    }
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert_eq!(other.preferences.nav, "creo");
    assert_eq!(nav_mode(&other, NavButton::Middle, Modifiers::NONE), Some(NavMode::Orbit));
}

#[test]
fn creo_navigation_settings_show_localised_bindings() {
    let _language = crate::i18n::scope("de");
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    control(&mut app, &json!({"open":"Navigation", "nav":"creo", "language":"de"})).unwrap();
    let ctx = crate::fonts::test_context();
    let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 1000.0));
    let mut textures = std::collections::HashMap::new();
    let mut draw = || {
        let raw = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
        let mut output = ctx.run_ui(raw, |ui| show(&mut app, ui.ctx()));
        crate::ui_kit::test_textures_update(&mut textures, &output.textures_delta);
        output.textures_delta.clear();
        output
    };
    for _ in 0..3 {
        draw();
    }
    let output = draw();
    for label in ["Creo", crate::i18n::tr("prefs.gesture_orbit"), crate::i18n::tr("prefs.gesture_pan"), crate::i18n::tr("prefs.gesture_zoom")] {
        assert!(crate::test_support::text_shapes(&output).any(|text| text.galley.text().contains(label)), "missing binding label: {label}");
    }
    assert_ne!(nav_preset("creo").translated_help(), nav_preset("creo").help);
    if let Ok(dir) = std::env::var("SKETCRAFT_CREO_EVIDENCE") {
        std::fs::create_dir_all(&dir).unwrap();
        crate::ui_kit::test_raster(&ctx, output.shapes, output.pixels_per_point, screen, &textures)
            .save(std::path::Path::new(&dir).join("creo-navigation-de.png"))
            .unwrap();
    }
}

#[test]
fn creo_preset_obeys_preference_transactions() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    control(&mut app, &json!({"open":"Navigation", "nav":"creo"})).unwrap();
    assert_eq!(app.preferences.nav, "creo");
    control(&mut app, &json!({"button":"cancel"})).unwrap();
    assert_eq!(app.preferences.nav, "fusion");
    control(&mut app, &json!({"open":"Navigation", "nav":"creo", "button":"apply"})).unwrap();
    control(&mut app, &json!({"button":"defaults"})).unwrap();
    assert_eq!(app.preferences.nav, "fusion");
    control(&mut app, &json!({"button":"cancel"})).unwrap();
    assert_eq!(app.preferences.nav, "creo");
}

#[test]
fn openscad_preset_maps_navigation_and_persists() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    set(&mut app, &json!({"nav": OPENSCAD_NAV})).unwrap();
    for button in [NavButton::Middle, NavButton::Right] {
        assert_eq!(nav_mode(&app, button, Modifiers::default()), Some(NavMode::Pan));
        assert_eq!(nav_mode(&app, button, Modifiers { shift: true, ..Default::default() }), Some(NavMode::Zoom));
    }
    assert_eq!(nav_mode(&app, NavButton::Left, Modifiers::default()), Some(NavMode::Orbit));
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert_eq!(other.preferences.nav, OPENSCAD_NAV);
}

#[test]
fn pan_reverse_x_defaults_off_persists_and_flips_the_sign() {
    assert!(!serde_json::from_value::<Prefs>(json!({})).unwrap().pan_reverse_x);
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    assert_eq!(pan_sign_x(&app), 1.0);
    set(&mut app, &json!({"pan_reverse_x": true})).unwrap();
    assert_eq!(pan_sign_x(&app), -1.0);
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert!(other.preferences.pan_reverse_x);
}

#[test]
fn orbit_reverse_defaults_off_persists_and_flips_the_sign() {
    assert!(!serde_json::from_value::<Prefs>(json!({})).unwrap().orbit_reverse);
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    assert_eq!(orbit_sign(&app), 1.0);
    set(&mut app, &json!({"orbit_reverse": true})).unwrap();
    assert_eq!(orbit_sign(&app), -1.0);
    assert_eq!((pan_sign_x(&app), pan_sign_y(&app)), (1.0, 1.0));
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert!(other.preferences.orbit_reverse);
}

#[test]
fn pan_reverse_y_defaults_off_persists_and_flips_the_sign() {
    assert!(!serde_json::from_value::<Prefs>(json!({})).unwrap().pan_reverse_y);
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    assert_eq!(pan_sign_y(&app), 1.0);
    set(&mut app, &json!({"pan_reverse_y": true})).unwrap();
    assert_eq!(pan_sign_y(&app), -1.0);
    assert_eq!(pan_sign_x(&app), 1.0);
    set(&mut app, &json!({"pan_reverse_x": true, "pan_reverse_y": false})).unwrap();
    assert_eq!(pan_sign_x(&app), -1.0);
    assert_eq!(pan_sign_y(&app), 1.0);
    set(&mut app, &json!({"pan_reverse_y": true})).unwrap();
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert!(other.preferences.pan_reverse_y);
}

#[test]
fn trackpad_pan_defaults_on_persists_and_follows_dialog_buttons() {
    assert!(serde_json::from_value::<Prefs>(json!({})).unwrap().trackpad_pan);
    assert!(Prefs::default().trackpad_pan);
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    control(&mut app, &json!({"open":"Navigation", "trackpad_pan":false})).unwrap();
    control(&mut app, &json!({"button":"cancel"})).unwrap();
    assert!(app.preferences.trackpad_pan);
    control(&mut app, &json!({"open":"Navigation", "trackpad_pan":false})).unwrap();
    control(&mut app, &json!({"button":"apply"})).unwrap();
    assert!(!app.preferences.trackpad_pan);
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert!(!other.preferences.trackpad_pan);
    control(&mut app, &json!({"open":"Navigation", "button":"defaults"})).unwrap();
    assert!(app.preferences.trackpad_pan);
    control(&mut app, &json!({"button":"cancel"})).unwrap();
    assert!(!app.preferences.trackpad_pan);
}

#[test]
fn blender_preset_moves_the_camera_with_the_middle_button_only() {
    // #1090: orbit, pan and zoom all on the middle button; no left or right drag binding, so
    // those stay selection and the context menu (Ctrl+left on empty space orbits in every
    // preset, outside the bindings).
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    set(&mut app, &json!({"nav": "blender"})).unwrap();
    let none = Modifiers::default();
    let shift = Modifiers { shift: true, ..Default::default() };
    let ctrl = Modifiers { ctrl: true, ..Default::default() };
    assert_eq!(nav_mode(&app, NavButton::Middle, none), Some(NavMode::Orbit));
    assert_eq!(nav_mode(&app, NavButton::Middle, shift), Some(NavMode::Pan));
    assert_eq!(nav_mode(&app, NavButton::Middle, ctrl), Some(NavMode::Zoom));
    for mods in [none, shift, ctrl] {
        assert_eq!(nav_mode(&app, NavButton::Left, mods), None);
        assert_eq!(nav_mode(&app, NavButton::Right, mods), None);
    }
    assert_eq!(zoom_sign(&app), 1.0, "the wheel turns the usual way");
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert_eq!(other.preferences.nav, "blender");
}

#[test]
fn presets_map_the_mouse_and_prefs_round_trip() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    let shift = Modifiers { shift: true, ..Default::default() };
    let ctrl = Modifiers { ctrl: true, ..Default::default() };
    let none = Modifiers::default();
    assert!(matches!(nav_mode(&app, NavButton::Middle, none), Some(NavMode::Pan)));
    assert!(matches!(nav_mode(&app, NavButton::Middle, shift), Some(NavMode::Orbit)));
    set(&mut app, &json!({"nav": "solidworks"})).unwrap();
    assert!(matches!(nav_mode(&app, NavButton::Middle, none), Some(NavMode::Orbit)));
    assert!(matches!(nav_mode(&app, NavButton::Middle, ctrl), Some(NavMode::Pan)));
    set(&mut app, &json!({"nav": "inventor", "zoom_reverse": true})).unwrap();
    assert!(matches!(nav_mode(&app, NavButton::Middle, ctrl), Some(NavMode::Zoom)));
    assert_eq!(zoom_sign(&app), -1.0);
    assert!(matches!(nav_mode(&app, NavButton::Right, none), Some(NavMode::Orbit)), "right drag orbits everywhere");
    assert_eq!(nav_mode(&app, NavButton::Left, none), None, "left drag selects");
    let shift_alt = Modifiers { shift: true, alt: true, ..Default::default() };
    assert_eq!(nav_mode(&app, NavButton::Left, shift_alt), None, "only Alias navigates with the left button");
    set(&mut app, &json!({"nav": "alias"})).unwrap();
    assert_eq!(nav_mode(&app, NavButton::Left, shift_alt), Some(NavMode::Orbit));
    assert_eq!(nav_mode(&app, NavButton::Middle, shift_alt), Some(NavMode::Pan));
    assert_eq!(nav_mode(&app, NavButton::Right, shift_alt), Some(NavMode::Zoom));
    assert_eq!(nav_mode(&app, NavButton::Left, none), None, "a plain left drag still selects");
    assert_eq!(nav_mode(&app, NavButton::Left, shift), None);
    assert_eq!(nav_mode(&app, NavButton::Right, none), Some(NavMode::Orbit));
    assert!(set(&mut app, &json!({"nav": "no-such-preset"})).is_err());
    assert!(set(&mut app, &json!({"default_units": "furlong"})).is_err());
    assert!(set(&mut app, &json!({"nope": 1})).is_err());
    // Kept between runs.
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert_eq!(other.preferences, app.preferences);
    assert_eq!(format_length(25.4, "in", 3), "1.000 in");
}

/// #6474, #2742: Maya moves the camera with Alt and any button, HFSS with Alt or Shift+Alt and
/// the left button. A plain left drag still selects, and HFSS leaves Ctrl+left to the drag
/// from empty space every preset has, so a Ctrl+click on geometry still adds to the selection.
#[test]
fn maya_and_hfss_presets_navigate_with_alt() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    let none = Modifiers::default();
    let alt = Modifiers { alt: true, ..Default::default() };
    let shift_alt = Modifiers { shift: true, alt: true, ..Default::default() };
    let ctrl = Modifiers { ctrl: true, ..Default::default() };
    set(&mut app, &json!({"nav": "maya"})).unwrap();
    assert_eq!(nav_mode(&app, NavButton::Left, alt), Some(NavMode::Orbit));
    assert_eq!(nav_mode(&app, NavButton::Middle, alt), Some(NavMode::Pan));
    assert_eq!(nav_mode(&app, NavButton::Right, alt), Some(NavMode::Zoom));
    assert_eq!(nav_mode(&app, NavButton::Middle, none), Some(NavMode::Pan));
    assert_eq!(nav_mode(&app, NavButton::Left, none), None, "a plain left drag selects");
    assert_eq!(nav_mode(&app, NavButton::Right, none), None, "a plain right click opens the context menu");
    set(&mut app, &json!({"nav": "hfss"})).unwrap();
    assert_eq!(nav_mode(&app, NavButton::Left, alt), Some(NavMode::Pan));
    assert_eq!(nav_mode(&app, NavButton::Left, shift_alt), Some(NavMode::Zoom));
    assert_eq!(nav_mode(&app, NavButton::Left, none), None);
    assert_eq!(nav_mode(&app, NavButton::Left, ctrl), None);
    assert_eq!(zoom_sign(&app), 1.0);
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert_eq!(other.preferences.nav, "hfss");
}

/// The dialog: changes apply at once, Cancel puts back what was there when it opened, Apply
/// keeps them, Restore Defaults resets one page, and the shortcuts menus open its page.
#[test]
fn preferences_dialog_cancel_apply_and_defaults() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    control(&mut app, &json!({"open": "general"})).unwrap();
    assert!(app.prefs_window.open && app.prefs_window.section == 0);
    control(&mut app, &json!({"theme": "light", "ui_scale": 1.25, "zoom_to_cursor": false})).unwrap();
    assert!(!app.ui.dark, "light applies at once");
    app.keymap.custom.insert("solid.extrude".into(), vec!["Shift+E".into()]);
    let v = control(&mut app, &json!({"button": "cancel"})).unwrap();
    assert_eq!(v["dialog"], serde_json::Value::Null);
    assert!(app.ui.dark && app.preferences.theme == "dark" && app.preferences.ui_scale == 1.0 && app.preferences.zoom_to_cursor);
    assert!(app.keymap.custom.is_empty(), "a rebinding made while the dialog was open is undone too");

    control(&mut app, &json!({"open": "Navigation", "nav": "alias", "look_at_anim": false})).unwrap();
    control(&mut app, &json!({"button": "apply"})).unwrap();
    control(&mut app, &json!({"nav": "tinkercad"})).unwrap();
    control(&mut app, &json!({"button": "cancel"})).unwrap();
    assert_eq!(app.preferences.nav, "alias", "Cancel goes back to the last Apply");
    let to = {
        let mut c = app.cam;
        c.set_view(sketcraft_engine::render::StandardView::Top);
        c
    };
    app.animate_to(to);
    assert!(app.cam_anim.is_none() && app.cam == to, "no animation: the view jumps");

    control(&mut app, &json!({"open": "Navigation"})).unwrap();
    control(&mut app, &json!({"button": "defaults"})).unwrap();
    assert_eq!(app.preferences.nav, "fusion");
    assert!(app.preferences.look_at_anim);
    control(&mut app, &json!({"button": "ok"})).unwrap();
    assert!(!app.prefs_window.open);

    // File and Help › Keyboard Shortcuts open the same dialog at its page.
    app.keymap.open = true;
    let ctx = crate::fonts::test_context();
    let mut out = ctx.run_ui(Default::default(), |ui| show(&mut app, ui.ctx()));
    out.textures_delta.clear();
    assert!(app.prefs_window.open && app.prefs_window.section == PAGE_SHORTCUTS && !app.keymap.open);
    assert!(control(&mut app, &json!({"open": "NoSuch"})).is_err());
    assert!(set(&mut app, &json!({"up_axis": "x"})).is_err());
    assert!(set(&mut app, &json!({"theme": "sepia"})).is_err());
    assert!(set(&mut app, &json!({"language": "xx"})).is_err());
}

/// Apply is disabled while nothing changed since the dialog opened or since the last Apply,
/// enabled by a change, and disabled again by Apply (#8561).
#[test]
fn apply_is_enabled_only_while_something_changed() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    let ctx = crate::fonts::test_context();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0))),
        focused: true,
        ..Default::default()
    };
    let draw = |app: &mut SketApp, events: Vec<egui::Event>| {
        crate::test_support::frame(&ctx, egui::RawInput { events, ..input() }, |ui| show(app, ui.ctx()));
        crate::scenario::count_of("dialog:apply:enabled")
    };
    let v = control(&mut app, &json!({"open": "Navigation"})).unwrap();
    assert_eq!(v["apply_enabled"], false, "nothing changed yet");
    assert_eq!(draw(&mut app, vec![]), Some(0.0), "just opened: Apply is disabled");

    let v = control(&mut app, &json!({"zoom_to_cursor": false})).unwrap();
    assert_eq!(v["apply_enabled"], true);
    assert_eq!(draw(&mut app, vec![]), Some(1.0), "after a change: Apply is enabled");
    let v = control(&mut app, &json!({"zoom_to_cursor": true})).unwrap();
    assert_eq!(v["apply_enabled"], false, "a change put back by hand leaves nothing to apply");
    control(&mut app, &json!({"zoom_to_cursor": false})).unwrap();

    // Clicking Apply keeps the change and leaves nothing to apply.
    draw(&mut app, vec![]);
    let apply = crate::scenario::handle_at("dialog:apply").unwrap();
    draw(&mut app, crate::test_support::pointer(apply, true));
    draw(&mut app, crate::test_support::pointer(apply, false));
    assert!(app.prefs_window.open && !app.preferences.zoom_to_cursor, "Apply keeps the dialog and the change");
    assert_eq!(draw(&mut app, vec![]), Some(0.0), "after Apply: Apply is disabled");
    // A disabled Apply does nothing when clicked.
    draw(&mut app, crate::test_support::pointer(apply, true));
    draw(&mut app, crate::test_support::pointer(apply, false));
    assert_eq!(draw(&mut app, vec![]), Some(0.0));

    // Every setting Cancel would put back counts, the shortcuts and view settings included;
    // update bookkeeping (not a setting, and Cancel keeps it) does not.
    app.preferences.update.last_check += 1.0;
    assert_eq!(draw(&mut app, vec![]), Some(0.0), "update bookkeeping is not a change");
    app.keymap.custom.insert("solid.extrude".into(), vec!["Shift+E".into()]);
    assert_eq!(draw(&mut app, vec![]), Some(1.0), "a rebinding is a change");
    let v = control(&mut app, &json!({"button": "apply"})).unwrap();
    assert_eq!(v["apply_enabled"], false);
    app.ui.show_grid = !app.ui.show_grid;
    assert_eq!(draw(&mut app, vec![]), Some(1.0), "a view setting is a change");

    // Cancel puts it back and closes; opened again, there is nothing to apply.
    let v = control(&mut app, &json!({"button": "cancel"})).unwrap();
    assert_eq!(v["apply_enabled"], false);
    let v = control(&mut app, &json!({"open": "General"})).unwrap();
    assert_eq!(v["apply_enabled"], false);
}

#[test]
fn language_preference_switches_and_persists() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    // A first start follows the system (#10138), which is English in a test.
    assert_eq!(app.preferences.language, crate::i18n::SYSTEM);
    assert_eq!(app.tr("ui.preferences"), "Preferences");
    assert_eq!(app.tr("ui.extrude"), "Extrude");

    // Switch language to Japanese
    set(&mut app, &json!({"language": "ja", "timeline_left": true})).unwrap();
    app.preferences.timeline_left_browser_collapsed = true;
    assert_eq!(app.preferences.language, "ja");
    assert!(app.preferences.timeline_left);
    assert_eq!(app.tr("ui.preferences"), "環境設定");
    assert_eq!(app.tr("ui.extrude"), "押し出し");

    // Preferences round-trip
    let mut other = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    other.load_prefs(&app.prefs());
    assert_eq!(other.preferences.language, "ja");
    assert!(other.preferences.timeline_left);
    assert!(other.preferences.timeline_left_browser_collapsed);
    assert_eq!(other.tr("ui.preferences"), "環境設定");
}

/// A STEP file opened Z-up is offered to be stood up: a quarter turn about X, undoable.
#[test]
fn a_y_up_step_file_can_be_stood_up() {
    let dir = std::env::temp_dir().join(format!("sketcraft-standup-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("tall_in_y.step").to_string_lossy().to_string();
    let mut src = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    src.run("solid.box", json!({"length": 10, "width": 40, "height": 5})).unwrap();
    src.run("file.export", json!({"path": path})).unwrap();

    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    app.open_path(&path);
    assert!(app.stand_up.is_none(), "SketCraft's own exports already have Z up");
    // Another originating system is offered (asked in the dialog, without looking at how the
    // flat box rests).
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("'SketCraft','SketCraft'", "'SketCraft','Example CAD'")).unwrap();
    app.preferences.stand_up_unknown = StandUpUnknown::Ask;
    app.open_path(&path);
    let offered = app.stand_up.clone().unwrap_or_default();
    assert_eq!(offered.len(), 1, "the file's body is offered");
    let size = |app: &SketApp| sketcraft_engine::view::bounds(&app.session).unwrap();
    assert!((size(&app).max.y - size(&app).min.y - 40.0).abs() < 1e-6);
    control(&mut app, &json!({"stand_up": true})).unwrap();
    let b = size(&app);
    assert!((b.max.z - b.min.z - 40.0).abs() < 1e-6, "Y became Z: {b:?}");
    assert!(app.stand_up.is_none());
    app.run("edit.undo", json!({})).unwrap();
    assert!((size(&app).max.y - size(&app).min.y - 40.0).abs() < 1e-6, "one undo step");

    // Not offered when turned off, or with Y up as the default orientation.
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    app.preferences.offer_stand_up = false;
    app.open_path(&path);
    assert!(app.stand_up.is_none());
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    app.preferences.up_axis = "y".into();
    app.open_path(&path);
    assert!(app.stand_up.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Orbiting about the world origin keeps the origin where it was drawn, even when Pan has
/// moved it off to the side.
#[test]
fn orbiting_around_the_origin_keeps_it_still() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    app.run("solid.box", json!({"length": 40, "width": 40, "height": 40})).unwrap();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    app.fit_view();
    assert_eq!(app.preferences.orbit_around, OrbitAround::ViewCentre, "the view centre stays the default");
    app.preferences.orbit_around = OrbitAround::Origin;
    app.cam.pan(60.0, -25.0, 600.0);
    let was = (app.cam.yaw, app.cam.pitch);
    let before = crate::viewport::projection(&app, rect).to_screen(Vec3::ZERO).unwrap();
    orbit(&mut app, rect, 40.0, 25.0, None);
    let after = crate::viewport::projection(&app, rect).to_screen(Vec3::ZERO).unwrap();
    assert!(after.distance(before) < 2.0, "{before:?} -> {after:?}");
    assert_ne!((app.cam.yaw, app.cam.pitch), was, "the view must turn");
}

/// Preferences saved before the origin option had `orbit_cursor`: that choice survives, and
/// the control channel still accepts the old key.
#[test]
fn saved_cursor_preference_migrates_to_the_origin_option() {
    assert_eq!(lenient(&json!({"orbit_cursor": true})).orbit_around, OrbitAround::Cursor);
    assert_eq!(lenient(&json!({"orbit_cursor": false})).orbit_around, OrbitAround::ViewCentre);
    assert_eq!(lenient(&json!({})).orbit_around, OrbitAround::ViewCentre);
    assert_eq!(lenient(&json!({"orbit_around": "origin"})).orbit_around, OrbitAround::Origin);

    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    set(&mut app, &json!({"orbit_around": "origin"})).unwrap();
    assert_eq!(app.preferences.orbit_around, OrbitAround::Origin);
    set(&mut app, &json!({"orbit_cursor": true})).unwrap();
    assert_eq!(app.preferences.orbit_around, OrbitAround::Cursor, "the old key still moves the choice");
    assert!(!app.preferences.orbit_cursor, "and is folded away");
}

#[test]
fn the_agent_chat_limit_is_off_unless_chosen() {
    assert_eq!(Prefs::default().agent_chat_turn_limit, 0);
    assert_eq!(migrate(lenient(&json!({"agent_chat_turn_limit": 24}))).agent_chat_turn_limit, 0, "the old default reads as no limit");
    assert_eq!(lenient(&json!({"agent_chat_turn_limit": 10})).agent_chat_turn_limit, 10);
}

#[test]
fn orbiting_around_the_cursor_keeps_that_point_still() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    app.run("solid.box", json!({"length": 40, "width": 40, "height": 40})).unwrap();
    app.preferences.orbit_around = OrbitAround::Cursor;
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    app.fit_view();
    let at = egui::pos2(500.0, 250.0);
    let pivot = pivot_under(&app, &crate::viewport::projection(&app, rect), at);
    orbit(&mut app, rect, 40.0, 25.0, Some(at));
    let s = crate::viewport::projection(&app, rect).to_screen(pivot).unwrap();
    assert!(s.distance(at) < 2.0, "{s:?}");
}

/// A drag that starts on an edge turns around the edge's point under the cursor, not its
/// middle, and that point stays still on screen.
#[test]
fn orbiting_from_an_edge_turns_around_the_point_under_the_cursor() {
    use crate::viewport::Hit;
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    app.run("solid.box", json!({"length": 200, "width": 40, "height": 40})).unwrap();
    app.preferences.orbit_around = OrbitAround::Cursor;
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    app.fit_view();
    let proj = crate::viewport::projection(&app, rect);
    let st = crate::open_meshing::session_world(&app.session);
    let m = st.bodies[0].mesh();
    // A spot a fifth of the way along a visible edge, well away from its middle on screen.
    let found = m.edges.iter().filter(|e| e.len() >= 2).find_map(|e| {
        let (a, b) = (e[0], e[e.len() - 1]);
        let at = proj.to_screen(a + (b - a) * 0.2)?;
        let mid = proj.to_screen(crate::viewport::polyline_mid(e))?;
        let first =
            crate::viewport::pick(&app, &proj, at).into_iter().find(|h| matches!(h, Hit::Face { .. } | Hit::Edge { .. } | Hit::Vertex { .. }));
        (rect.contains(at) && at.distance(mid) > 100.0 && matches!(first, Some(Hit::Edge { .. }))).then_some((at, mid))
    });
    let (at, mid) = found.unwrap();
    let pivot = pivot_under(&app, &proj, at);
    let on_screen = proj.to_screen(pivot).unwrap();
    assert!(on_screen.distance(at) < 6.0, "the pivot is under the cursor: {on_screen:?} vs {at:?}");
    assert!(on_screen.distance(mid) > 90.0, "not the edge's middle");
    orbit(&mut app, rect, 40.0, 25.0, Some(at));
    assert_eq!(app.viewport.orbit_pivot, Some(pivot));
    let after = crate::viewport::projection(&app, rect).to_screen(pivot).unwrap();
    assert!(after.distance(on_screen) < 2.0, "{on_screen:?} -> {after:?}");
}

/// On a sketch line too: the line's point under the cursor, not the view's middle.
#[test]
fn orbiting_from_a_sketch_line_turns_around_the_point_under_the_cursor() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    app.run("sketch.line", json!({"points": [[-100, 0], [100, 0]]})).unwrap();
    app.run("sketch.finish", json!({})).unwrap();
    app.preferences.orbit_around = OrbitAround::Cursor;
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    app.fit_view();
    let proj = crate::viewport::projection(&app, rect);
    let on_line = Vec3::new(-60.0, 0.0, 0.0);
    let at = proj.to_screen(on_line).unwrap();
    let pivot = pivot_under(&app, &proj, at);
    assert!(pivot.dist(on_line) < 2.0, "{pivot:?}");
}

/// A drag that starts over empty space turns around the visible geometry's centre, not a
/// point far off to the side; with nothing visible it turns around the view's centre.
#[test]
fn orbiting_from_empty_space_turns_around_the_visible_geometry() {
    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    app.run("solid.box", json!({"length": 40, "width": 40, "height": 40})).unwrap();
    app.preferences.orbit_around = OrbitAround::Cursor;
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    app.fit_view();
    let corner = egui::pos2(10.0, 10.0);
    let proj = crate::viewport::projection(&app, rect);
    let hits = crate::viewport::pick(&app, &proj, corner);
    use crate::viewport::Hit;
    assert!(!hits.iter().any(|h| matches!(h, Hit::Face { .. } | Hit::Edge { .. } | Hit::Vertex { .. })), "the corner is empty space");
    let centre = crate::viewport::view_bounds(&app).center();
    orbit(&mut app, rect, 40.0, 25.0, Some(corner));
    assert_eq!(app.viewport.orbit_pivot, Some(centre));

    let mut app = SketApp::new(sketcraft_engine::Session::default(), Default::default());
    app.preferences.orbit_around = OrbitAround::Cursor;
    app.cam.target = sketcore_record::Vec3::new(5.0, -3.0, 2.0);
    orbit(&mut app, rect, 40.0, 25.0, Some(corner));
    assert_eq!(app.viewport.orbit_pivot, Some(sketcore_record::Vec3::new(5.0, -3.0, 2.0)));
}

/// The Y up preference turns the view: the camera's up axis follows it, startup included.
#[test]
fn up_axis_preference_turns_the_view() {
    use sketcraft_engine::render::UpAxis;
    let mut app = SketApp::new(sketcraft_engine::Session::default(), crate::Services::default());
    let ctx = crate::fonts::test_context();
    apply(&mut app, &ctx);
    assert_eq!(app.cam.up, UpAxis::Z);
    app.preferences.up_axis = "y".into();
    apply(&mut app, &ctx);
    assert_eq!((app.ui.up_axis, app.cam.up), (UpAxis::Y, UpAxis::Y));
    // Home looks at the model with Y up: the eye is above the XZ ground.
    assert!(app.cam.back().y > 0.3, "{:?}", app.cam.back());
    app.preferences.up_axis = "z".into();
    apply(&mut app, &ctx);
    assert_eq!(app.cam.up, UpAxis::Z);
}
