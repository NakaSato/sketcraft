//! Assembly motion dialogs: Contact Sets, Motion Study (keyframes on a timeline with playback),
//! Exploded View (a scale slider) and Configurations (a table of rows with Activate). What they
//! show while open (a study's pose, an explosion) is display only: pending occurrence moves,
//! computed on a scratch copy of the design and put back when the dialog closes.

use crate::ui_kit::text_row;
use std::collections::BTreeMap;

use egui::RichText;
use serde_json::{Value, json};
use sketcraft_engine::Session;
use sketcraft_engine::doc::Mat;

use crate::SketApp;
use crate::dialogs::{Dialog, Kind, form_row};
use crate::selection::SelInput;
use crate::theme::Tokens;

#[path = "configurations_ui.rs"]
mod configurations_ui;
use configurations_ui::configurations_rows;

#[path = "configuration_rules_ui.rs"]
mod configuration_rules_ui;

#[derive(Clone, Debug)]
pub enum Mo {
    Explode {
        name: String,
        scale: f64,
        shown: Option<(u64, u64)>,
        saved: Option<BTreeMap<u64, Mat>>,
        error: Option<String>,
    },
    Configurations {
        new_row: String,
        error: Option<String>,
        rule_editor: Box<configuration_rules_ui::Editor>,
    },
}

impl Mo {
    pub fn title(&self) -> &'static str {
        match self {
            Mo::Explode { .. } => "EXPLODED VIEW",
            Mo::Configurations { .. } => "CONFIGURATIONS",
        }
    }

    /// A table that needs the wide dialog.
    pub fn wide(&self) -> bool {
        matches!(self, Mo::Configurations { .. })
    }
}

/// The dialog for a motion command.
pub fn start(app: &SketApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    let n = |prefix: &str, list: &str| -> String {
        let taken: Vec<String> = app
            .session
            .scratch()
            .execute(list, &json!({}))
            .ok()
            .and_then(|v| v.as_object().and_then(|o| o.values().find_map(|x| x.as_array().cloned())))
            .unwrap_or_default()
            .iter()
            .filter_map(|x| x.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();
        (1..).map(|k| format!("{prefix}{k}")).find(|c| !taken.contains(c)).unwrap_or_default()
    };
    let (mo, inputs) = match id {
        "explode.create" => (Mo::Explode { name: n("Exploded View", "explode.list"), scale: 1.0, shown: None, saved: None, error: None }, vec![]),
        "config.table" | "config.configure" => (Mo::Configurations { new_row: String::new(), error: None, rule_editor: Default::default() }, vec![]),
        _ => return None,
    };
    Some((Kind::Motion(mo), inputs))
}

// ---- pure mappings (tested) ----

/// What a dialog opens on when the control channel's `ui.start` passes `params`: none of these
/// takes any.
pub fn preset(_s: &Session, k: &mut Mo, _p: &Value) -> Result<(), String> {
    Err(format!("{} takes no params yet", k.title()))
}

/// The configurations table: columns and rows (name, value per column).
#[derive(Debug)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<(String, Vec<String>)>,
    pub active: Option<String>,
}

pub fn table(v: &Value) -> Table {
    let columns: Vec<String> =
        v.get("columns").and_then(Value::as_array).into_iter().flatten().filter_map(|c| c.as_str().map(str::to_string)).collect();
    let cell = |x: Option<&Value>| match x {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => {
            if *b {
                "suppressed".into()
            } else {
                "on".into()
            }
        }
        Some(Value::Null) | None => String::new(),
        Some(o) => o.to_string(),
    };
    let rows = v
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let name = r.get("name")?.as_str()?.to_string();
            let vals = r.get("values");
            Some((name, columns.iter().map(|c| cell(vals.and_then(|m| m.get(c)))).collect()))
        })
        .collect();
    Table { columns, rows, active: v.get("active").and_then(Value::as_str).map(str::to_string) }
}

/// The unit a value of this kind is shown in.
fn unit_of(k: sketcraft_engine::doc::expr::Kind) -> Option<&'static str> {
    match k {
        sketcraft_engine::doc::expr::Kind::Length => Some("mm"),
        sketcraft_engine::doc::expr::Kind::Angle => Some("deg"),
        _ => None,
    }
}

/// A column key ("param:width", "suppress:Fillet1") as (kind, name).
pub fn column_parts(c: &str) -> (&str, &str) {
    c.split_once(':').unwrap_or(("param", c))
}

/// The value a table cell edit sends: suppress columns take booleans, parameters expressions.
pub fn cell_value(column: &str, text: &str) -> Value {
    match column_parts(column).0 {
        "suppress" => match text.trim().to_ascii_lowercase().as_str() {
            "suppressed" | "true" | "yes" | "1" | "off" => json!(true),
            "unsuppressed" | "false" | "no" | "0" | "on" => json!(false),
            _ => json!(text),
        },
        _ => json!(text),
    }
}

// ---- display ----

/// Show `moves` as pending occurrence moves (display only), keeping what was there before.
fn display(app: &mut SketApp, saved: &mut Option<BTreeMap<u64, Mat>>, moves: BTreeMap<u64, Mat>) {
    if saved.is_none() {
        *saved = Some(app.session.pending_moves.clone());
    }
    app.session.pending_moves = moves;
    app.session.revision += 1;
}

fn restore(app: &mut SketApp, saved: &mut Option<BTreeMap<u64, Mat>>) {
    if let Some(m) = saved.take() {
        app.session.pending_moves = m;
        app.session.revision += 1;
    }
}

/// The dialog closed (`applied`: by OK): what it showed goes away, unless OK made it the view.
pub fn closed(app: &mut SketApp, d: &mut Dialog, applied: bool) {
    let Kind::Motion(k) = &mut d.kind else { return };
    match k {
        // OK shows the new view itself (explode.show).
        Mo::Explode { saved, .. } if !applied => restore(app, saved),
        _ => {}
    }
}

/// The explosion at `scale`, computed on a scratch copy.
fn explode_pose(app: &SketApp, scale: f64) -> Result<BTreeMap<u64, Mat>, String> {
    let mut sc = app.session.scratch();
    sc.execute("explode.create", &json!({"name": "__preview", "scale": scale})).map_err(|e| e.to_string())?;
    sc.execute("explode.show", &json!({ "view": "__preview" })).map_err(|e| e.to_string())?;
    Ok(sc.pending_moves.clone())
}

// ---- rows ----

/// The dialog's rows; true when Enter was pressed in a value.
pub fn rows(app: &mut SketApp, ui: &mut egui::Ui, k: &mut Mo, inputs: &mut [SelInput]) -> bool {
    let t = Tokens::get();
    let enter = false;
    match k {
        Mo::Explode { name, scale, shown, saved, error } => {
            text_row(ui, "Name", name, "");
            form_row(ui, "Explode", |ui| {
                crate::ui_kit::slider(ui, scale, 0.0..=2.0, Default::default());
            });
            let key = ((*scale * 1000.0).round() as u64, 0);
            if *shown != Some(key) {
                match explode_pose(app, *scale) {
                    Ok(m) => {
                        display(app, saved, m);
                        *error = None;
                    }
                    Err(e) => *error = Some(e),
                }
                *shown = Some(key);
            }
            if let Some(e) = error {
                ui.vertical(|ui| {
                    ui.label(RichText::new(crate::i18n::tr_label(e)).color(t.error));
                });
            }
        }
        Mo::Configurations { new_row, error, rule_editor } => configurations_rows(app, ui, new_row, error, rule_editor),
    }
    let _ = inputs;
    enter
}

/// The commands OK runs.
pub fn commands(_app: &SketApp, k: &Mo, _inputs: &[SelInput]) -> Result<Vec<(String, Value)>, String> {
    Ok(match k {
        Mo::Explode { name, scale, .. } => {
            vec![("explode.create".into(), json!({"name": name, "scale": scale})), ("explode.show".into(), json!({ "view": name }))]
        }
        Mo::Configurations { .. } => Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a dialog shows goes away when it closes; an exploded view applied with OK stays.
    #[test]
    fn shown_poses_are_put_back() {
        let mut app = SketApp::new(Session::default(), crate::Services::default());
        app.session.pending_moves.insert(9, sketcraft_engine::doc::IDENTITY);
        let before = app.session.pending_moves.clone();
        let mut m = sketcraft_engine::doc::IDENTITY;
        m[3] = [5.0, 0.0, 0.0, 1.0];
        let shown: BTreeMap<u64, Mat> = [(3, m)].into_iter().collect();
        for (applied, kept) in [(false, false), (true, true)] {
            let mut saved = None;
            display(&mut app, &mut saved, shown.clone());
            assert_eq!(app.session.pending_moves, shown);
            let mo = Mo::Explode { name: "E".into(), scale: 1.0, shown: None, saved, error: None };
            let mut d = Dialog::new(Kind::Motion(mo), vec![]);
            closed(&mut app, &mut d, applied);
            assert_eq!(app.session.pending_moves == shown, kept, "{applied}");
            app.session.pending_moves = before.clone();
        }
    }

    #[test]
    fn configuration_table_and_cells() {
        let v = json!({"columns": ["param:width", "suppress:Fillet1"], "rows": [
            {"name": "Default", "values": {"param:width": "80 mm", "suppress:Fillet1": false}},
            {"name": "Long", "values": {"param:width": "120 mm", "suppress:Fillet1": true}},
        ], "active": "Long"});
        let t = table(&v);
        assert_eq!(t.columns.len(), 2);
        assert_eq!(t.rows[1], ("Long".to_string(), vec!["120 mm".to_string(), "suppressed".to_string()]));
        assert_eq!(t.active.as_deref(), Some("Long"));
        assert_eq!(column_parts("suppress:Fillet1"), ("suppress", "Fillet1"));
        assert_eq!(cell_value("suppress:Fillet1", "Suppressed"), json!(true));
        assert_eq!(cell_value("suppress:Fillet1", "on"), json!(false));
        assert_eq!(cell_value("param:width", "90 mm"), json!("90 mm"));
    }

    fn click_rule_button(h: &mut crate::scenario::Harness, name: &str) {
        // The table header also contains this name. Its TextRun may be emitted
        // after the popup, so select the actual choice rather than matching text.
        let bounds = h
            .output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .rev()
            .find_map(|(_, n)| {
                (n.role() == egui::accesskit::Role::Button && !n.is_disabled() && n.label() == Some(name)).then(|| n.bounds()).flatten()
            })
            .expect("rule button");
        h.step(&json!({"click": [(bounds.x0 + bounds.x1) / 2.0, (bounds.y0 + bounds.y1) / 2.0]})).unwrap();
    }

    fn select_rule_parameter(h: &mut crate::scenario::Harness, name: &str) {
        click_rule_button(h, name);
        assert!(h.widget("Choose…").is_none(), "parameter choice should close and update the dropdown");
    }

    // Kit rows publish identities independently of translated text. Long forms scroll through
    // the same wheel events as a person; assertions still verify the saved rules and geometry.
    // A table row scrolled out of view is not built, so its handle is missing until the form
    // scrolls back up to the table above the rules.
    fn reveal_config_handle(h: &mut crate::scenario::Harness, handle: &str) {
        for _ in 0..24 {
            let header = crate::scenario::rect_of("dialog:header").unwrap();
            let footer = crate::scenario::rect_of("dialog:footer").unwrap();
            let at = crate::scenario::handle_at(handle);
            if handle.starts_with("config:activate:") && at.is_none() {
                h.call("ui.scroll", json!({"x":header.left()+Tokens::get().space_6,"y":(header.bottom()+footer.top())*0.5,"delta":160.0}));
                h.frames(6);
                continue;
            }
            let at = at.unwrap_or_else(|| panic!("missing {handle}: {:?}", h.app.dialog));
            if at.y > header.bottom() + 8.0 && at.y < footer.top() - 8.0 {
                return;
            }
            h.call(
                "ui.scroll",
                json!({"x":header.left()+Tokens::get().space_6,"y":(header.bottom()+footer.top())*0.5,"delta":if at.y >= footer.top()-Tokens::get().space_8 {-160.0} else {160.0}}),
            );
            h.frames(6);
        }
        panic!(
            "could not reveal {handle}: {:?}, header {:?}, footer {:?}",
            crate::scenario::handle_at(handle),
            crate::scenario::rect_of("dialog:header"),
            crate::scenario::rect_of("dialog:footer")
        );
    }

    fn click_config_handle(h: &mut crate::scenario::Harness, handle: &str) {
        reveal_config_handle(h, handle);
        h.step(&json!({"click":{"handle":handle}})).unwrap();
    }

    fn configuration_steps(h: &mut crate::scenario::Harness, steps: &Value) -> Result<(), String> {
        for (index, step) in steps.as_array().ok_or("steps must be an array")?.iter().enumerate() {
            let handle = match step["widget"].as_str() {
                Some("Save Rule") => Some("config:save_rule"),
                Some("Preview Row") => Some("config:preview_row"),
                Some("Apply") => Some("config:activate:Default"),
                Some("Drive parameter") => Some("dialog:Then"),
                Some("Preview: Default") => Some("dialog:section:Preview: Default"),
                _ => step
                    .get("click")
                    .and_then(|c| c.get("handle"))
                    .and_then(Value::as_str)
                    .filter(|h| ["dialog:Value", "dialog:Minimum", "dialog:Maximum"].contains(h)),
            };
            if let Some(handle) = handle {
                click_config_handle(h, handle);
            } else {
                h.step(step).map_err(|error| format!("step {index}: {error}"))?;
            }
        }
        Ok(())
    }

    #[test]
    fn configuration_compound_condition_and_rule_order_are_editable_in_dialog() {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.resize", json!({"width":1400,"height":1100}));
        configuration_steps(
            &mut h,
            &json!([
                {"call":"engine.execute","params":{"command":"parameters.change","params":{"name":"length","expression":"40 mm"}}},
                {"call":"engine.execute","params":{"command":"config.configure","params":{"params":["length"]}}},
                {"start":"config.table"},{"widget":"Add or edit rule"},
                {"widget":"Always"},{"click":{"handle":"dialog:When:All"}},{"widget":"Always"},{"click":{"handle":"dialog:When:Parameter"}},
                {"widget":"Choose…"}
            ]),
        )
        .unwrap();
        select_rule_parameter(&mut h, "length");
        click_config_handle(&mut h, "config:save_rule");
        assert!(!h.app.session.doc.configs.rules.is_empty(), "{:?}", h.app.dialog);
        assert!(
            matches!(&h.app.session.doc.configs.rules[0].condition,sketcraft_engine::doc::config::RuleCondition::All{conditions} if matches!(&conditions[0],sketcraft_engine::doc::config::RuleCondition::Parameter{..})),
            "{:?}",
            h.app.dialog
        );
        configuration_steps(&mut h, &json!([
            {"call":"engine.execute","params":{"command":"config.rule","params":{"rule":{"id":"last","name":"Last drive","condition":{"type":"always"},"effect":{"type":"drive_parameter","parameter":"length","expression":"20 mm"}}}}},
            {"widget":"Preview Row"},{"widget":"Preview: Default"}
        ])).unwrap();
        reveal_config_handle(&mut h, "config:preview:param:length");
        assert!(h.widget("length = 20 mm").is_some());
        assert_eq!(h.app.session.doc.param("length").unwrap().expr, "40 mm");
        click_config_handle(&mut h, "config:rule:last:up");
        click_config_handle(&mut h, "config:preview_row");
        assert_eq!(h.app.session.doc.configs.rules[0].id, "last");
        reveal_config_handle(&mut h, "config:preview:param:length");
        assert!(h.widget("length = 10").is_some());
        assert_eq!(h.app.session.doc.param("length").unwrap().expr, "40 mm");
    }

    #[test]
    fn configuration_rule_editor_creates_previews_edits_and_applies_driven_value() {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.resize", json!({"width":1400,"height":1000}));
        configuration_steps(
            &mut h,
            &json!([
                {"call":"engine.execute","params":{"command":"parameters.change","params":{"name":"length","expression":"40 mm"}}},
                {"call":"engine.execute","params":{"command":"solid.box","params":{"length":"length","width":10,"height":10}}},
                {"call":"engine.execute","params":{"command":"config.configure","params":{"params":["length"]}}},
                {"start":"config.table"},
                {"widget":"Add or edit rule"},
                {"widget":"Choose…"}
            ]),
        )
        .unwrap();
        select_rule_parameter(&mut h, "length");
        configuration_steps(
            &mut h,
            &json!([
                {"widget":"Save Rule"}
            ]),
        )
        .unwrap();
        assert_eq!(h.app.session.doc.configs.rules.len(), 1);
        assert_eq!(h.app.session.doc.param("length").unwrap().expr, "40 mm");
        let before = h.app.session.doc.clone();
        configuration_steps(&mut h, &json!([{"widget":"Preview Row"}])).unwrap();
        assert_eq!(h.app.session.doc, before);
        configuration_steps(&mut h, &json!([{"widget":"Apply"}])).unwrap();
        assert_eq!(h.app.session.doc.param("length").unwrap().expr, "10");
        assert_eq!(h.app.session.doc.eval("length", sketcraft_engine::doc::expr::Kind::Length).unwrap(), 10.0);
        assert!(h.app.session.doc.configuration_matches());
        configuration_steps(
            &mut h,
            &json!([
                {"click":{"handle":"dialog:Value"}},{"key":"A","cmd":true},{"text":"20 mm"},{"widget":"Save Rule"},
                {"widget":"Apply"}
            ]),
        )
        .unwrap();
        assert_eq!(h.app.session.doc.param("length").unwrap().expr, "20 mm");
        assert_eq!(h.app.session.doc.configs.rows[0].cells[0], "40 mm");
        let before = h.app.session.doc.clone();
        configuration_steps(
            &mut h,
            &json!([
                {"widget":"Drive parameter"},{"widget":"Range"},
                {"click":{"handle":"dialog:Minimum"}},{"key":"A","cmd":true},{"text":"100 mm"},{"widget":"Save Rule"}
            ]),
        )
        .unwrap();
        assert_eq!(h.app.session.doc, before, "failed rule edit is atomic");
        assert!(
            h.widget("configuration rules: rule `New rule` restricts the final value").is_some()
                || h.output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .any(|(_, n)| n.label().or(n.value()).is_some_and(|text| text.contains("restricts the final value")))
        );
    }

    #[test]
    fn configuration_component_and_face_pickers_start_fresh_and_clear_inherited() {
        for (component, width) in [(true, 1400), (false, 900)] {
            let mut h = crate::scenario::Harness::new();
            h.call("ui.resize", json!({"width":width,"height":720}));
            if component {
                h.call("engine.execute", json!({"command":"component.create","params":{"name":"Part"}}));
            }
            h.run(&json!([
                {"call":"engine.execute","params":{"command":"solid.box","params":{"length":10,"width":10,"height":10,"body_name":"Block"}}},
                {"start":"config.table"}, {"widget":"Add Column"}
            ]))
            .unwrap();
            if component {
                h.step(&json!({"widget":"Part (component appearance)"})).unwrap();
                assert!(h.widget("Component: Part").is_some());
            } else {
                h.run(&json!([{"widget":"Face Appearance"},{"widget":"Faces on Block"},{"widget":"+z"}])).unwrap();
                assert!(h.widget("Face: Block / +z").is_some());
            }
            assert_eq!(h.app.session.doc.configs.rows.len(), 1);
            h.run(&json!([{"widget":"Library"},{"widget":"ABS - Red"},{"widget":"Apply"}])).unwrap();
            if component {
                assert_eq!(h.app.session.doc.appearances.components.values().next().unwrap().name, "ABS - Red");
            } else {
                assert_eq!(h.app.session.doc.appearances.faces[0].look.name, "ABS - Red");
                assert!(h.app.session.doc.appearances.faces[0].strict_name);
            }
            h.run(&json!([{"widget":"Library"},{"widget":"Follow inherited appearance"},{"widget":"Apply"}])).unwrap();
            assert!(h.app.session.doc.appearances.components.is_empty());
            assert!(h.app.session.doc.appearances.faces.is_empty());
            crate::scenario::check(&mut h, &json!({"windows_fit":{"max_width":Tokens::get().form_card_width * 3.0}})).unwrap();
        }
    }

    #[test]
    fn configuration_annulus_picker_exposes_both_certificates_despite_shared_legacy_name() {
        let mut h = crate::scenario::Harness::new();
        h.call("ui.resize", json!({"width":1400,"height":900}));
        h.run(&json!([
            {"call":"engine.execute","params":{"command":"sketch.create","params":{"plane":"XY"}}},
            {"call":"engine.execute","params":{"command":"sketch.circle.center","params":{"center":[0,0],"radius":10,"id":"outer"}}},
            {"call":"engine.execute","params":{"command":"sketch.circle.center","params":{"center":[0,0],"radius":9.999,"id":"inner"}}},
            {"call":"engine.execute","params":{"command":"sketch.finish","params":{}}},
            {"call":"engine.execute","params":{"command":"solid.extrude","params":{"distance":10,"profiles":[{"point":[9.9995,0]}],"body_name":"Ring"}}},
            {"start":"config.table"},{"widget":"Add Column"},{"widget":"Face Appearance"},{"widget":"Faces on Ring"}
        ]))
        .unwrap();
        assert!(h.widget("side:outer").is_some());
        assert!(h.widget("side:inner").is_some());
        h.run(&json!([{"widget":"side:outer"},{"widget":"Library"},{"widget":"ABS - Red"},{"widget":"Apply"}])).unwrap();
        let sketcraft_engine::doc::config::Column::FaceAppearance(target) = &h.app.session.doc.configs.columns[0] else { panic!() };
        assert!(matches!(&target.certificate, Some(sketcraft_engine::doc::config::FaceCertificate::ExtrudeSide { curve, .. }) if curve == "outer"));
        let state = h.app.session.model.state();
        let body = state.body("Ring").unwrap();
        // The outer circle, one closed curve, is one wall face.
        let indices = h.app.session.doc.configuration_face_indices(body, target).unwrap();
        assert_eq!(indices.len(), 1);
        let painted = h.app.session.doc.face_colors(body);
        assert_eq!(painted.len(), 1);
        assert!(painted.iter().all(|(index, _)| indices.contains(index)));
    }

    #[test]
    fn configuration_style_picker_applies_then_inherits_material() {
        let steps: Value = serde_json::from_str(include_str!("../tests/scenarios/s80_configuration_styles.json")).unwrap();
        let mut harness = crate::scenario::Harness::new();
        for step in steps.as_array().unwrap() {
            harness.step(step).unwrap();
            match step.get("shot").and_then(Value::as_str) {
                Some("configuration_red") => {
                    let look = harness.app.session.doc.appearances.bodies.get("Block").unwrap();
                    assert_eq!(look.color, [196, 40, 40]);
                    assert!(harness.app.session.doc.configuration_matches());
                }
                Some("configuration_default") => {
                    assert!(!harness.app.session.doc.appearances.bodies.contains_key("Block"));
                    let state = harness.app.session.model.state();
                    let body = state.body("Block").unwrap();
                    let look = harness.app.session.doc.body_look("Block", body.feature).unwrap();
                    assert_eq!(look.name, "Steel");
                    assert_eq!(look.color, [158, 164, 172]);
                    assert!(harness.app.session.doc.configuration_matches());
                }
                _ => {}
            }
        }
    }

    #[test]
    fn configuration_table_actions_stay_on_one_line_at_normal_and_narrow_widths() {
        for width in [1400, 900] {
            let mut harness = crate::scenario::Harness::new();
            harness.call("ui.resize", json!({"width": width, "height": 720}));
            harness
                .run(&json!([
                    {"call":"engine.execute","params":{"command":"solid.box","params":{"length":40,"width":20,"height":10,"body_name":"Block"}}},
                    {"start":"config.table"}
                ]))
                .unwrap();
            assert_single_line(&harness, "Configuration");
            for name in ["span", "thickness", "depth", "clearance"] {
                harness.call("engine.execute", json!({"command":"parameters.change","params":{"name":name,"expression":"10 mm"}}));
            }
            harness
                .run(&json!([
                    {"call":"engine.execute","params":{"command":"config.configure","params":{
                        "params":["span","thickness","depth","clearance"],"materials":["Block"],"appearances":["Block"]
                    }}},
                    {"call":"engine.execute","params":{"command":"config.row","params":{"name":"Default","values":{"param:span":"20 mm"}}}}
                ]))
                .unwrap();
            harness.frames(4);
            for label in ["Configuration", "Active", "Apply", "Add configuration", "Add Column"] {
                assert_single_line(&harness, label);
            }
            crate::scenario::check(&mut harness, &json!({"windows_fit":{"max_width":Tokens::get().form_card_width * 3.0}})).unwrap();
            for name in ["span", "thickness", "depth", "clearance"] {
                harness.call("engine.execute", json!({"command":"config.column","params":{"param":name,"delete":true}}));
            }
            harness.call("engine.execute", json!({"command":"config.column","params":{"appearance":"Block","delete":true}}));
            assert_single_line(&harness, "Material: Block");
            harness.call("engine.execute", json!({"command":"config.column","params":{"material":"Block","delete":true}}));
            harness.call("engine.execute", json!({"command":"config.column","params":{"feature":"Box1"}}));
            assert_single_line(&harness, "Suppress: Box1");
        }
        fn assert_single_line(harness: &crate::scenario::Harness, label: &str) {
            let tree = harness.output.accesskit_update.as_ref().unwrap();
            let bounds = tree
                .nodes
                .iter()
                .filter(|(_, n)| n.label() == Some(label) || n.value() == Some(label))
                .filter_map(|(_, n)| n.bounds())
                .collect::<Vec<_>>();
            assert!(
                !bounds.is_empty(),
                "missing {label}; labels/values: {:?}",
                tree.nodes.iter().filter_map(|(_, n)| n.label().or(n.value())).collect::<Vec<_>>()
            );
            assert!(bounds.iter().all(|b| b.y1 - b.y0 <= Tokens::get().control_height as f64), "{label} wrapped: {bounds:?}");
        }
    }

}
