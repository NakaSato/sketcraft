use super::*;

#[test]
fn translated_workspace_captions_fit_with_room_for_the_arrow() {
    let mut h = crate::scenario::Harness::new();
    h.app.home.open = false;
    for layout in ["floating", "compact"] {
        h.call("ui.set", json!({"layout": layout}));
        for labels in [true, false] {
            h.app.preferences.ribbon_labels = labels;
            for language in ["en", "ru", "de", "ja", "ko", "uk"] {
                h.call("ui.prefs", json!({"language": language}));
                h.frames(3);
                let ws = crate::scenario::rect_of("workspace_switcher").unwrap();
                let caption = h.app.tr("ui.ribbon_workspace");
                let text = h
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == caption => Some(text),
                        _ => None,
                    })
                    .unwrap();
                let bounds = Rect::from_min_size(text.pos, text.galley.size());
                assert!(bounds.left() >= ws.left() + 4.0, "{language}, {layout}, labels={labels}: {bounds:?} outside {ws:?}");
                assert!(bounds.right() + 12.0 <= ws.right() - 4.0, "{language}, {layout}, labels={labels}: no room for the arrow");
                if language == "en" {
                    assert!(!text.galley.elided, "the English caption still fits whole");
                }
            }
        }
    }
}

fn tab_test_themes() -> Vec<Tokens> {
    let mut themes = Vec::new();
    for dark in [false, true] {
        for vision in crate::theme::ColorVision::ALL {
            themes.push(Tokens::themed(dark, vision));
        }
    }
    themes.extend(crate::theme_files::builtin().iter().map(crate::theme_files::resolve));
    themes
}

#[test]
fn a_selected_contextual_tab_stands_out_and_an_unselected_one_is_outlined() {
    for t in tab_test_themes() {
        for tab in ["SKETCH", "SHEET METAL"] {
            let accent = crate::theme::TabAccent::for_tab(&t, tab);
            for hovered in [false, true] {
                let active = ribbon_tab_style(&t, tab, true, true, hovered);
                let inactive = ribbon_tab_style(&t, tab, true, false, hovered);
                assert_eq!(active, RibbonTabStyle { fill: accent.color, ink: accent.on_fill(&t), outline: Color32::TRANSPARENT });
                assert_eq!(inactive, RibbonTabStyle { outline: accent.color, ..ribbon_tab_style(&t, tab, false, false, hovered) });
                assert!(crate::theme::contrast_ratio(active.ink, active.fill) >= 4.5, "{tab}: selected text must remain readable");
                // The selected contextual tab is the solid pill; the unselected one never is.
                assert!(
                    crate::theme::contrast_ratio(active.fill, t.toolbar)
                        > crate::theme::contrast_ratio(if hovered { t.hover } else { t.toolbar }, t.toolbar),
                    "{tab}: the selected tab must be the more prominent one"
                );
            }
        }
    }
}

#[test]
fn ordinary_tabs_keep_their_selected_hovered_and_idle_colors() {
    for t in tab_test_themes() {
        for tab in ["MODEL", "SURFACE", "SHEET METAL"] {
            let accent = crate::theme::TabAccent::for_tab(&t, tab);
            for hovered in [false, true] {
                let none = Color32::TRANSPARENT;
                assert_eq!(ribbon_tab_style(&t, tab, false, true, hovered), RibbonTabStyle { fill: accent.soft, ink: accent.ink, outline: none });
                let idle = if hovered { t.hover } else { none };
                assert_eq!(ribbon_tab_style(&t, tab, false, false, hovered), RibbonTabStyle { fill: idle, ink: t.text, outline: none });
            }
        }
    }
}

#[test]
fn ribbon_weight_is_painted_once_at_a_pixel_aligned_origin() {
    for scale in [1.0, 1.5, 2.0] {
        let ctx = crate::fonts::test_context();
        ctx.set_pixels_per_point(scale);
        let ink = Tokens::get().text_dim;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ribbon_text(ui, pos2(100.3, 40.7), "Selected tab", 13.0, ink, true);
        });
        output.textures_delta.clear();
        let text: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(text) if text.galley.text() == "Selected tab" => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(text.len(), 1, "ribbon labels must never be drawn twice");
        let text = text[0];
        assert_eq!(text.galley.job.sections[0].format.font_id, crate::fonts::semibold(13.0));
        assert_eq!(text.galley.job.sections[0].format.color, ink);
        assert!((text.pos.x * scale - (text.pos.x * scale).round()).abs() < 0.001);
        assert!((text.pos.y * scale - (text.pos.y * scale).round()).abs() < 0.001);
    }
}

/// Every promoted button's label, in every language, sits between the button's icon and its
/// bottom edge, so the hover highlight holds all of it (#8391). One line stays where it was.
#[test]
fn every_promoted_label_fits_in_its_button_below_the_icon() {
    let ctx = crate::fonts::test_context();
    let mut two_lines = 0;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter().clone();
        let specs = sketcraft_engine::command_specs();
        for section in crate::workspace::SECTIONS {
            let (commands, count) = crate::workspace::layout(section.tab, section.panel, &specs).unwrap();
            for command in commands.iter().take(count) {
                for catalog in crate::i18n::LOCALES {
                    let locale = crate::i18n::Locale::for_language(catalog.id);
                    let label = locale.label(crate::workspace::ribbon_button_label(command));
                    let whole = Rect::from_min_size(pos2(40.0, 2.0), vec2(ribbon_button_width(&painter, label), BUTTON_HEIGHT));
                    let galley = ribbon_button_galley(&painter, label, whole.width() - BUTTON_LABEL_PADDING, Color32::WHITE);
                    let top = ribbon_label_top(whole, galley.size().y);
                    let at = format!("{} in {}: {label:?}", command.id, catalog.id);
                    // The icon is 26 points square, centred 16 points below the button's top.
                    assert!(top >= whole.top() + 16.0 + 13.0, "{at} starts at {top} over the icon");
                    assert!(top + galley.size().y <= whole.bottom() - BUTTON_LABEL_BOTTOM + 0.01, "{at} runs past the button");
                    if galley.rows.len() == 1 {
                        assert_eq!(top, whole.top() + BUTTON_LABEL_TOP, "{at}");
                    } else {
                        two_lines += 1;
                    }
                }
            }
        }
    });
    output.textures_delta.clear();
    assert!(two_lines > 0, "some labels take two lines");
}

#[test]
fn every_promoted_label_wraps_only_at_word_boundaries() {
    let ctx = crate::fonts::test_context();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter().clone();
        let specs = sketcraft_engine::command_specs();
        for section in crate::workspace::SECTIONS {
            let (commands, count) = crate::workspace::layout(section.tab, section.panel, &specs).unwrap();
            for command in commands.iter().take(count) {
                for catalog in crate::i18n::LOCALES {
                    let locale = crate::i18n::Locale::for_language(catalog.id);
                    let label = locale.label(crate::workspace::ribbon_button_label(command));
                    let width = ribbon_button_width(&painter, label) - BUTTON_LABEL_PADDING;
                    let galley = ribbon_button_galley(&painter, label, width, Color32::WHITE);
                    assert!(galley.rows.len() <= 2, "{}: {label}", command.id);
                    assert!(galley.size().x <= width + 0.01, "{}: {label}: {} > {width}", command.id, galley.size().x);
                    if crate::bidi::has_rtl(label) {
                        // epaint adds zero-width glyphs to right-to-left rows, so their text
                        // is not the source text: check the lines the galley is built from.
                        let lines = ribbon_button_lines(&painter, label, width);
                        let first = lines[0].strip_prefix('…').unwrap_or(&lines[0]);
                        assert!(label.ends_with(first), "{}: {label} -> {lines:?}", command.id);
                        if let [first, second] = lines.as_slice()
                            && !first.starts_with('…')
                            && !second.starts_with('…')
                        {
                            // Both lines whole: they split the label at a space or after a slash.
                            assert!(
                                *label == format!("{second} {first}") || *label == format!("{second}{first}"),
                                "{}: {label} -> {lines:?}",
                                command.id
                            );
                        }
                        continue;
                    }
                    let first = galley.rows.first().unwrap().text();
                    let visible = first.strip_suffix('…').unwrap_or(&first);
                    assert!(label.starts_with(visible), "{}: {label} -> {first}", command.id);
                    if let Some(second) = galley.rows.get(1) {
                        let second = second.text();
                        let next = second.strip_suffix('…').unwrap_or(&second);
                        // The second line must start at a legal source boundary,
                        // including when the first line was ellipsized.
                        assert!(
                            label.char_indices().any(|(at, ch)| {
                                let end = at + ch.len_utf8();
                                (ch.is_whitespace() || ch == '/' || cjk_word_break(label, end))
                                    && label[end..].trim_start().starts_with(next)
                                    && (first.ends_with('…') || label[..end].trim() == first)
                            }),
                            "{}: {label} -> {first:?}, {second:?} breaks inside a word",
                            command.id
                        );
                    }
                }
            }
        }
        for label in ["Rectangular", "Dimension", "Coincident", "Chamfer", "Revolve"] {
            let width = ribbon_button_width(&painter, label);
            assert_eq!(ribbon_button_lines(&painter, label, width - BUTTON_LABEL_PADDING), vec![label]);
            assert!((BUTTON_MIN_WIDTH..=BUTTON_MAX_WIDTH).contains(&width));
        }
        let finish = sketcraft_engine::find_command("sketch.finish").unwrap();
        let label = crate::workspace::ribbon_button_label(finish);
        let width = ribbon_button_width(&painter, label);
        assert_eq!(ribbon_button_lines(&painter, label, width - BUTTON_LABEL_PADDING), vec!["Finish", "Sketch"]);
        // Exercise full labels and pinned commands too, not just compact defaults.
        for (label, width, expected) in [
            ("Coincident", 50.0, vec!["Coincident"]),
            ("Rectangular Pattern", 50.0, vec!["Rectangular", "Pattern"]),
            ("Move/Copy", 40.0, vec!["Move/", "Copy"]),
        ] {
            let lines = ribbon_button_lines(&painter, label, width);
            // Exact glyph widths depend on the bundled font: verify the source
            // word stays on its own line, with only an optional suffix omitted.
            assert_eq!(lines.len(), expected.len(), "{label}: {lines:?}");
            for (actual, word) in lines.iter().zip(expected) {
                assert!(actual == word || actual.strip_suffix('…').is_some_and(|prefix| word.starts_with(prefix)), "{label}: {lines:?}");
            }
        }
        let lines = ribbon_button_lines(&painter, "one two three four five", 50.0);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'));
        let lines = ribbon_button_lines(&painter, "長い日本語のラベル 続き", 50.0);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with('…'));
        // Hebrew comes in display order (`bidi::visual`): the reader starts at the right end,
        // so "סקיצה חדשה" (New Sketch, stored as "חדשה סקיצה") puts סקיצה on the first line.
        let label = crate::bidi::visual("סקיצה חדשה").into_owned();
        let width = ribbon_label_width(&painter, "סקיצה").max(ribbon_label_width(&painter, "חדשה")) + 1.0;
        assert!(ribbon_label_width(&painter, &label) > width);
        assert_eq!(ribbon_button_lines(&painter, &label, width), vec!["סקיצה", "חדשה"]);
        let lines = ribbon_button_lines(&painter, &crate::bidi::visual("אחת שתיים שלוש ארבע חמש"), 50.0);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].starts_with('…'), "{lines:?}");
    });
    output.textures_delta.clear();
    // Japanese is measured with the bundled Japanese face, as a Japanese UI draws it: without it
    // the glyphs are missing and their width says nothing.
    let ctx = crate::fonts::test_context();
    let cjk = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/biz-udp-gothic/BIZUDPGothic-Regular.ttf"));
    assert!(crate::fonts::install_cjk_font_for(&ctx, cjk.to_vec(), "ja"));
    ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter().clone();
        let ja = crate::i18n::Locale::for_language("ja");
        for (id, expected) in [
            ("sketch.create", vec!["新規", "スケッチ"]),
            ("config.table", vec!["構成", "テーブル"]),
            ("construct.plane.offset", vec!["オフセット", "平面"]),
            ("surface.loft", vec!["サーフェス", "ロフト"]),
            ("mesh.convert", vec!["メッシュを", "変換"]),
            ("component.create", vec!["新規", "コンポ"]),
            ("joint.origin", vec!["ジョイント", "原点"]),
            ("joint.drive", vec!["ジョイントを", "駆動"]),
        ] {
            let command = sketcraft_engine::find_command(id).unwrap();
            let label = ja.label(crate::workspace::ribbon_button_label(command));
            let width = ribbon_button_width(&painter, label) - BUTTON_LABEL_PADDING;
            assert_eq!(ribbon_button_lines(&painter, label, width), expected, "{id}");
        }
        let comp = sketcraft_engine::find_command("component.create").unwrap();
        assert_eq!(ja.label(comp.label), "新規コンポーネント");
    });
    output.textures_delta.clear();
}
