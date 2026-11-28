//! Board's window: the navigation, the offline banner and the three screens of Bureau §8.1 step 1.
//! It draws `Board`'s state and calls its methods; all colours and sizes come from `theme`. Structure
//! is rules and spacing, not boxes: a 2 px ink rule opens each section, hairlines separate rows.

use crate::app::{Board, Conn, LabelMode};
use crate::args::Screen;
use crate::logic::{FormOpen, Pick, READING_ROWS, local_time, progress_text, short_sha};
use crate::theme::{self, Preference, Tones, border, layout, size, space};
use egui::{Align, Button, Layout, RichText, ScrollArea, TextEdit, Ui};
use serde_json::Value;

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or_default()
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn tones(ui: &Ui) -> Tones {
    theme::tones(ui.ctx().theme())
}

/// The state of the whole window for one frame.
pub fn show(b: &mut Board, ui: &mut Ui) {
    egui::Panel::top("nav").show(ui, |ui| nav(b, ui));
    egui::Panel::bottom("feedback")
        .show_separator_line(true)
        .show(ui, |ui| feedback(b, ui));
    egui::CentralPanel::default().show(ui, |ui| {
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(layout::CONTAINER);
                banner(b, ui);
                match b.screen {
                    Screen::Runs => runs(b, ui),
                    Screen::Settings => settings(b, ui),
                    Screen::Docker => docker(b, ui),
                    Screen::Labels => labelling(b, ui),
                }
            });
    });
}

fn nav(b: &mut Board, ui: &mut Ui) {
    let t = tones(ui);
    ui.add_space(space::S3);
    ui.horizontal(|ui| {
        ui.label(RichText::new("News Board").size(size::HEADLINE_S).strong());
        ui.add_space(space::S7);
        for (screen, label) in [
            (Screen::Runs, "Runs & approvals"),
            (Screen::Settings, "Settings & schedules"),
            (Screen::Docker, "Docker health"),
            (Screen::Labels, "Labelling"),
        ] {
            let on = b.screen == screen;
            // The current page is underlined in crimson (design/components.md, "current nav").
            let r = ui.selectable_label(on, label);
            if on {
                let y = r.rect.bottom() + 1.0;
                ui.painter().hline(
                    r.rect.x_range(),
                    y,
                    egui::Stroke::new(border::FOCUS, t.brand),
                );
            }
            if r.clicked() {
                b.screen = screen;
                if screen == Screen::Docker && b.online() {
                    b.refresh_docker();
                }
                if screen == Screen::Labels {
                    b.refresh_labels();
                }
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let before = b.pref;
            egui::ComboBox::from_id_salt("theme")
                .selected_text(b.pref.label())
                .show_ui(ui, |ui| {
                    for p in [Preference::System, Preference::Light, Preference::Dark] {
                        ui.selectable_value(&mut b.pref, p, p.label());
                    }
                });
            if b.pref != before {
                theme::install(ui.ctx(), b.pref);
            }
        });
    });
    ui.add_space(space::S2);
}

fn feedback(b: &mut Board, ui: &mut Ui) {
    let t = tones(ui);
    ui.add_space(space::S2);
    ui.horizontal(|ui| match &b.feedback {
        Some(f) => {
            let c = if f.ok { t.ink_2 } else { t.error };
            ui.label(RichText::new(&f.text).color(c));
            if ui.small_button("Dismiss").clicked() {
                b.feedback = None;
            }
        }
        None => {
            let line = if b.in_flight > 0 {
                "Working…"
            } else {
                s(&b.status, "text")
            };
            ui.label(RichText::new(line).color(t.ink_3).size(size::SOURCE));
        }
    });
    ui.add_space(space::S2);
}

fn banner(b: &mut Board, ui: &mut Ui) {
    let t = tones(ui);
    match b.conn.clone() {
        Conn::Online { build_ok: true, .. } => {}
        Conn::Online {
            build_ok: false, ..
        } => {
            ui.label(
                RichText::new(
                    "Board and the service are different builds. Restart Board before you act.",
                )
                .color(t.error),
            );
            ui.separator();
        }
        Conn::Connecting => {
            ui.label(RichText::new("Connecting to the Bureau service…").color(t.ink_2));
            ui.separator();
        }
        Conn::Offline { reason, scm } => {
            ui.label(
                RichText::new("The Bureau service is offline")
                    .size(size::HEADLINE_S)
                    .color(t.error),
            );
            ui.label(&reason);
            if let Some(n) = scm {
                ui.label(RichText::new(n).color(t.ink_2));
            }
            if ui.button("Start service").clicked() {
                b.start_service();
            }
            ui.separator();
        }
    }
}

/// A section opens with a 2 px ink rule (design/layout.md) and a heading.
fn section(ui: &mut Ui, title: &str) {
    let t = tones(ui);
    ui.add_space(space::S5);
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, border::SECTION), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, t.ink);
    ui.label(RichText::new(title).size(size::HEADLINE_S).strong());
}

fn weak(ui: &mut Ui, text: &str) {
    let t = tones(ui);
    ui.label(RichText::new(text).color(t.ink_3).size(size::SOURCE));
}

// ---- Runs & approvals ------------------------------------------------------------------------------

fn runs(b: &mut Board, ui: &mut Ui) {
    if !b.status.is_null() {
        let next = s(&b.status["next"], "reason");
        if !next.is_empty() {
            ui.add_space(space::S3);
            ui.label(RichText::new(next).size(size::BODY_S));
        }
    }
    detail(b, ui);
    waiting(b, ui);
    running(b, ui);
    lineup(b, ui);
    start_form(b, ui);
    problems(b, ui);
}

fn waiting(b: &mut Board, ui: &mut Ui) {
    let items = arr(&b.status["waiting"]).to_vec();
    section(ui, "Waiting for you");
    if items.is_empty() {
        weak(ui, "Nothing is waiting.");
        return;
    }
    for w in items {
        let run = s(&w, "short_id").to_owned();
        ui.add_space(space::S3);
        ui.label(RichText::new(s(&w["summary"], "text")).size(size::BODY_S));
        match s(&w, "kind") {
            "gate" => {
                let g = w["gate"].clone();
                weak(
                    ui,
                    &format!(
                        "{} {}, gate {}, round {}, since {}, shows hash {}",
                        s(&w, "preset"),
                        run,
                        s(&g, "gate"),
                        g["round"],
                        local_time(s(&g, "since")),
                        short_sha(s(&g, "subject_sha")),
                    ),
                );
                if !g["preview"].is_null() {
                    ui.collapsing("What you are approving", |ui| {
                        let text = serde_json::to_string_pretty(&g["preview"]).unwrap_or_default();
                        ui.label(text);
                    });
                }
                let key = format!("{run}.{}", s(&g, "gate"));
                let note = b.forms.gate_notes.entry(key).or_default();
                ui.add(
                    TextEdit::singleline(note)
                        .hint_text("Note (needed to request changes)")
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    let can = b.can_act();
                    if ui.add_enabled(can, Button::new("Approve")).clicked() {
                        b.answer_gate(&g, "approve");
                    }
                    if ui
                        .add_enabled(can, Button::new("Request changes"))
                        .clicked()
                    {
                        b.answer_gate(&g, "changes");
                    }
                    if ui.add_enabled(can, Button::new("Reject")).clicked() {
                        b.answer_gate(&g, "reject");
                    }
                    if ui.button("Details").clicked() {
                        b.select_run(&run);
                    }
                });
            }
            "paused" => {
                weak(
                    ui,
                    &format!(
                        "{} {} is paused ({})",
                        s(&w, "preset"),
                        run,
                        s(&w, "paused_reason")
                    ),
                );
                ui.horizontal(|ui| {
                    if ui.add_enabled(b.can_act(), Button::new("Resume")).clicked() {
                        b.run_control("run.resume", &run, true);
                    }
                    if ui.button("Details").clicked() {
                        b.select_run(&run);
                    }
                });
            }
            _ => {
                if ui.button("Details").clicked() {
                    b.select_run(&run);
                }
            }
        }
        ui.separator();
    }
}

fn running(b: &mut Board, ui: &mut Ui) {
    let items = arr(&b.status["running"]).to_vec();
    section(ui, "Running");
    if items.is_empty() {
        weak(ui, "No run is running.");
        return;
    }
    for r in items {
        let run = s(&r, "short_id").to_owned();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} {}", s(&r, "preset_name"), run)).strong());
            let step = &r["step"];
            if !step.is_null() {
                let prog = progress_text(&r["progress"])
                    .map(|p| format!(", {p}"))
                    .unwrap_or_default();
                ui.label(format!(
                    "step {} of {}: {}{}",
                    step["seq"],
                    r["steps"],
                    s(step, "title"),
                    prog
                ));
            }
            ui.label(RichText::new(s(&r, "status")).color(tones(ui).ink_2));
            if r["stalled"] == true {
                ui.label(RichText::new("stalled").color(tones(ui).error));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let can = b.can_act();
                if ui.add_enabled(can, Button::new("Cancel")).clicked() {
                    b.run_control("run.cancel", &run, false);
                }
                if ui.add_enabled(can, Button::new("Pause")).clicked() {
                    b.run_control("run.pause", &run, true);
                }
                if ui.button("Details").clicked() {
                    b.select_run(&run);
                }
            });
        });
        ui.separator();
    }
    confirm(b, ui);
}

fn confirm(b: &mut Board, ui: &mut Ui) {
    if let Some(c) = b.confirm.clone() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&c.label).strong());
            if ui.button("Yes").clicked() {
                b.confirmed();
            }
            if ui.button("No").clicked() {
                b.confirm = None;
            }
        });
    }
}

fn lineup(b: &mut Board, ui: &mut Ui) {
    let items = arr(&b.status["queued"]).to_vec();
    section(ui, "Lineup");
    if items.is_empty() {
        weak(ui, "No heavy run is waiting for the slot.");
        return;
    }
    for e in items {
        let run = s(&e, "short_id").to_owned();
        ui.horizontal(|ui| {
            ui.label(format!(
                "{}. {} {} ({}, held by {}{})",
                e["position"],
                s(&e, "preset"),
                run,
                s(&e, "origin"),
                s(&e, "held_by"),
                if e["start_by"].is_null() {
                    String::new()
                } else {
                    format!(", start by {}", local_time(s(&e, "start_by")))
                }
            ));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add_enabled(b.can_act(), Button::new("Remove")).clicked() {
                    b.remove_from_lineup(&run, false);
                }
            });
        });
    }
    confirm(b, ui);
}

fn start_form(b: &mut Board, ui: &mut Ui) {
    section(ui, "Start a run");
    if b.presets.is_empty() {
        weak(ui, "The presets load when the service is connected.");
        return;
    }
    let names: Vec<(String, String)> = b
        .presets
        .iter()
        .map(|p| (s(p, "id").to_owned(), s(p, "name").to_owned()))
        .collect();
    let current = names
        .iter()
        .find(|(id, _)| *id == b.forms.preset)
        .map_or("Choose", |(_, n)| n.as_str())
        .to_owned();
    ui.horizontal(|ui| {
        ui.label("Preset");
        egui::ComboBox::from_id_salt("preset")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (id, name) in &names {
                    if ui
                        .selectable_value(&mut b.forms.preset, id.clone(), name)
                        .clicked()
                    {
                        b.forms.args.clear();
                    }
                }
            });
    });
    let preset = b.forms.preset.clone();
    if let Some(decl) = b.preset(&preset).cloned() {
        for a in arr(&decl["args"]) {
            let name = s(a, "name").to_owned();
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{name}{}",
                    if a["required"] == true {
                        " (required)"
                    } else {
                        ""
                    }
                ));
                let text = b.forms.args.entry(name.clone()).or_default();
                match s(a, "type") {
                    "boolean" => {
                        let mut on = text == "true";
                        if ui.checkbox(&mut on, "").changed() {
                            *text = on.to_string();
                        }
                    }
                    "enum" => {
                        egui::ComboBox::from_id_salt(format!("arg-{name}"))
                            .selected_text(if text.is_empty() {
                                "default"
                            } else {
                                text.as_str()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(text, String::new(), "default");
                                for c in arr(&a["choices"]) {
                                    let c = c.as_str().unwrap_or_default();
                                    ui.selectable_value(text, c.to_owned(), c);
                                }
                            });
                    }
                    _ => {
                        ui.add(TextEdit::singleline(text).desired_width(280.0));
                    }
                }
                weak(ui, s(a, "doc"));
            });
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(b.can_act(), Button::new("Start")).clicked() {
                b.start_run(&preset, None);
            }
            weak(
                ui,
                if decl["heavy"] == true {
                    "Heavy: uses the one heavy slot."
                } else {
                    ""
                },
            );
        });
    }
    if let Some(o) = b.offer.clone() {
        ui.add_space(space::S3);
        ui.label(RichText::new(&o.text).strong());
        ui.horizontal(|ui| {
            if ui.button("Queue it").clicked() {
                b.queue_offered();
            }
            if ui.button("Pause the running one and start this").clicked() {
                b.pause_and_start();
            }
            if ui.button("Never mind").clicked() {
                b.offer = None;
            }
        });
    }
}

fn problems(b: &mut Board, ui: &mut Ui) {
    let p = b.status["problems"].clone();
    let failed = arr(&p["failed_runs"]);
    let sources = arr(&p["sources"]);
    let refused = arr(&p["refused_control"]);
    if failed.is_empty() && sources.is_empty() && refused.is_empty() {
        return;
    }
    section(ui, "Problems");
    for f in failed {
        ui.label(format!(
            "{} {} {} {}",
            s(f, "preset"),
            s(f, "short_id"),
            s(f, "status"),
            s(f, "reason")
        ));
    }
    for x in sources {
        ui.label(format!(
            "Source {} is {}: {}",
            s(x, "source_id"),
            s(x, "health"),
            s(x, "reason")
        ));
    }
    for r in refused {
        // A refused control attempt is a security signal: shown, never hidden.
        ui.label(
            RichText::new(format!(
                "{}: {} was refused ({})",
                local_time(s(r, "at")),
                s(r, "cmd"),
                s(r, "refusal")
            ))
            .color(tones(ui).error),
        );
    }
}

fn detail(b: &mut Board, ui: &mut Ui) {
    let Some(id) = b.selected_run.clone() else {
        return;
    };
    section(ui, &format!("Run {id}"));
    match b.detail.clone() {
        None => weak(ui, "Loading…"),
        Some(d) => {
            ui.label(s(&d, "text"));
            for st in arr(&d["steps"]) {
                ui.horizontal(|ui| {
                    ui.label(format!("{}. {}", st["seq"], s(st, "title")));
                    ui.label(RichText::new(s(st, "status")).color(tones(ui).ink_2));
                    if let Some(p) = progress_text(&st["progress"]) {
                        weak(ui, &p);
                    }
                    if !st["summary"].is_null() {
                        weak(ui, s(&st["summary"], "text"));
                    }
                });
            }
            if let Some(open) = b.open_form() {
                huddle_form(b, ui, &open);
            }
            if !d["wrap"].is_null() {
                ui.add_space(space::S3);
                ui.label(RichText::new("Wrap").strong());
                ui.label(s(&d["wrap"], "text"));
            }
        }
    }
    if ui.button("Close").clicked() {
        b.close_detail();
    }
}

/// Huddle's calibration form: the pairwise questions, the Search Console status and the reading notes.
/// `form.submit` carries the round and the hash of this form as displayed.
fn huddle_form(b: &mut Board, ui: &mut Ui, open: &FormOpen) {
    b.sync_form(open);
    section(ui, "Your answers");
    weak(
        ui,
        &format!(
            "{} Round {}, shows hash {}.",
            open.text,
            open.round,
            short_sha(&open.sha)
        ),
    );
    ui.add_space(space::S3);
    if !open.questions.is_empty() {
        ui.label(RichText::new("Which story deserved a page first?").strong());
    }
    for (i, q) in open.questions.iter().enumerate() {
        ui.add_space(space::S2);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{}.", i + 1));
            if let Some(pick) = b.forms.huddle.picks.get_mut(i) {
                ui.selectable_value(pick, Pick::A, s(&q["a"], "title"));
                ui.label("or");
                ui.selectable_value(pick, Pick::B, s(&q["b"], "title"));
                ui.selectable_value(pick, Pick::Skip, "Skip");
            }
        });
    }
    ui.add_space(space::S3);
    ui.label(RichText::new("Search Console status").strong());
    ui.add(
        TextEdit::multiline(&mut b.forms.huddle.search_console)
            .hint_text("What the Coverage and Performance reports say this week")
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    ui.add_space(space::S3);
    ui.label(RichText::new("Pages you read").strong());
    for (page, note) in &mut b.forms.huddle.reading {
        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(page)
                    .hint_text("Page id or slug")
                    .desired_width(220.0),
            );
            ui.add(
                TextEdit::singleline(note)
                    .hint_text("What you found")
                    .desired_width(f32::INFINITY),
            );
        });
    }
    if b.forms.huddle.reading.len() < READING_ROWS && ui.small_button("Add a page").clicked() {
        b.forms.huddle.reading.push((String::new(), String::new()));
    }
    ui.add_space(space::S3);
    let answered = b
        .forms
        .huddle
        .picks
        .iter()
        .filter(|p| **p != Pick::Skip)
        .count();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(b.can_act(), Button::new("Send answers"))
            .clicked()
        {
            b.submit_form();
        }
        weak(
            ui,
            &format!(
                "{answered} of {} questions answered; a skipped one sends no label.",
                open.questions.len()
            ),
        );
    });
}

// ---- Labelling -------------------------------------------------------------------------------------

/// Karthik's labels for the A12 judges and for G3 (Bureau §9.1): one failure mode or the story pairs at a
/// time, one item on screen, a click or a key per label. Items show no verdict, seeded fault or cosine.
fn labelling(b: &mut Board, ui: &mut Ui) {
    ui.add_space(space::S3);
    weak(
        ui,
        "Label stored outputs for the A12 judges, one failure mode at a time, and story pairs for G3. \
         Nothing here shows what a judge said, which fault was seeded or how close two stories are.",
    );
    if b.labelling.data.is_null() {
        ui.add_space(space::S3);
        weak(ui, "Loading… (items come from `news eval draft`)");
        return;
    }
    let judges = arr(&b.labelling.data["judges"]).to_vec();
    let pairs = b.labelling.data["pairs"].clone();
    section(ui, "What to label");
    ui.horizontal_wrapped(|ui| {
        for j in &judges {
            let mode = LabelMode::Judge(s(j, "id").to_owned());
            let on = b.labelling.mode == mode;
            let label = format!("{} {} of {}", s(j, "name"), j["labelled"], j["target"]);
            if ui.selectable_label(on, label).clicked() {
                b.choose_labels(mode);
            }
        }
        let on = b.labelling.mode == LabelMode::Pairs;
        let label = format!("Story pairs {} of {}", pairs["labelled"], pairs["target"]);
        if ui.selectable_label(on, label).clicked() {
            b.choose_labels(LabelMode::Pairs);
        }
    });
    match b.labelling.mode.clone() {
        LabelMode::Judge(id) => {
            let Some(j) = judges.iter().find(|j| s(j, "id") == id) else {
                return;
            };
            section(ui, s(j, "name"));
            ui.label(s(j, "question"));
            weak(
                ui,
                &format!(
                    "{} waiting. Pass means the text has none of this failure.",
                    j["waiting"]
                ),
            );
            let Some(item) = b.front_label().cloned() else {
                weak(
                    ui,
                    "Nothing waits for this judge. `news eval draft` stores more.",
                );
                return;
            };
            ui.add_space(space::S3);
            ui.label(RichText::new(s(&item, "text")).size(size::BODY));
            let facts = arr(&item["facts"]);
            if !facts.is_empty() {
                ui.collapsing("Recorded figures", |ui| {
                    for f in facts {
                        weak(ui, f.as_str().unwrap_or_default());
                    }
                });
            }
            label_buttons(b, ui, ("Pass", egui::Key::P), ("Fail", egui::Key::F));
        }
        LabelMode::Pairs => {
            section(ui, "Same story or different?");
            ui.label("Do these two describe the same story, so they belong on one page?");
            weak(ui, &format!("{} waiting.", pairs["waiting"]));
            let Some(item) = b.front_label().cloned() else {
                weak(ui, "No pair waits. `news eval draft` stores more.");
                return;
            };
            ui.add_space(space::S3);
            ui.columns(2, |cols| {
                let (a, st) = (&item["a"], &item["b"]);
                cols[0].label(RichText::new(s(a, "title")).strong());
                weak(&mut cols[0], &format!("An item from {}", s(a, "source")));
                cols[0].label(s(a, "text"));
                cols[1].label(RichText::new(s(st, "title")).strong());
                weak(&mut cols[1], "A story, with some of its items:");
                for m in arr(&st["members"]) {
                    cols[1].label(m.as_str().unwrap_or_default());
                }
            });
            label_buttons(
                b,
                ui,
                ("Same story", egui::Key::S),
                ("Different", egui::Key::D),
            );
        }
    }
}

/// The two label buttons and Skip; the keys do the same while no text box has the keyboard. Neither
/// answer is coloured: a label is not good or bad, up or down.
fn label_buttons(b: &mut Board, ui: &mut Ui, yes: (&str, egui::Key), no: (&str, egui::Key)) {
    ui.add_space(space::S3);
    let keys = !ui.ctx().egui_wants_keyboard_input();
    let pressed = |k: egui::Key| keys && ui.input(|i| i.key_pressed(k));
    let (key_yes, key_no, key_skip) = (pressed(yes.1), pressed(no.1), pressed(egui::Key::K));
    let can = b.can_act();
    ui.horizontal(|ui| {
        let (mut y, mut n) = (false, false);
        y |= ui
            .add_enabled(can, Button::new(format!("{} ({:?})", yes.0, yes.1)))
            .clicked();
        n |= ui
            .add_enabled(can, Button::new(format!("{} ({:?})", no.0, no.1)))
            .clicked();
        if ui.button("Skip (K)").clicked() || key_skip {
            b.skip_label();
        }
        if y || key_yes {
            b.label_front(true);
        } else if n || key_no {
            b.label_front(false);
        }
    });
}

// ---- Settings & schedules --------------------------------------------------------------------------

fn settings(b: &mut Board, ui: &mut Ui) {
    if b.standing.is_null() {
        ui.add_space(space::S5);
        weak(ui, "Settings load when the service is connected.");
        secrets(b, ui);
        return;
    }
    let items = arr(&b.standing["items"]).to_vec();
    let version = b.standing["version"].clone();
    ui.add_space(space::S3);
    weak(
        ui,
        &format!(
            "Standing version {version}. Every save is one new version with who, when, and old and new values."
        ),
    );

    let mut group = String::new();
    for it in items
        .iter()
        .filter(|i| !s(i, "key").starts_with("schedules."))
    {
        let key = s(it, "key").to_owned();
        let g = key.split('.').next().unwrap_or_default().to_owned();
        if g != group {
            section(ui, &g);
            group = g;
        }
        setting_row(b, ui, it, &key);
    }

    section(ui, "Sources on or off");
    weak(
        ui,
        "A switch here overrides the source's Contacts file; a source you have not switched follows the file.",
    );
    ui.horizontal(|ui| {
        ui.label("Source id");
        ui.add(TextEdit::singleline(&mut b.forms.new_source).desired_width(160.0));
        for (label, on) in [("On", "true"), ("Off", "false")] {
            if ui.button(label).clicked() {
                let id = b.forms.new_source.trim().to_owned();
                if !id.is_empty() {
                    b.forms
                        .edits
                        .insert(format!("sources.{id}.enabled"), on.to_owned());
                    b.forms.new_source.clear();
                }
            }
        }
    });
    let pending: Vec<String> = b
        .forms
        .edits
        .iter()
        .filter(|(k, _)| {
            k.starts_with("sources.") && !items.iter().any(|i| s(i, "key") == k.as_str())
        })
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    for line in pending {
        weak(ui, &format!("Pending: {line}"));
    }

    section(ui, "Schedules");
    weak(
        ui,
        "Each schedule is {preset, args, cron, enabled}; the cron runs in the time zone above.",
    );
    for it in items
        .iter()
        .filter(|i| s(i, "key").starts_with("schedules."))
    {
        let key = s(it, "key").to_owned();
        ui.label(RichText::new(&key).strong());
        let current = serde_json::to_string_pretty(&it["value"]).unwrap_or_default();
        let text = b.forms.edits.entry(key.clone()).or_insert(current);
        ui.add(
            TextEdit::multiline(text)
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            if ui.small_button("Revert").clicked() {
                b.forms.edits.remove(&key);
            }
            if ui.small_button("History").clicked() {
                b.show_history(Some(key.clone()));
            }
        });
    }
    ui.horizontal(|ui| {
        ui.label("New schedule id");
        ui.add(TextEdit::singleline(&mut b.forms.new_schedule).desired_width(160.0));
        if ui.button("Add").clicked() {
            let id = b.forms.new_schedule.trim().to_owned();
            if !id.is_empty() {
                let preset = if b.forms.preset.is_empty() { "edition".to_owned() } else { b.forms.preset.clone() };
                b.forms.edits.insert(
                    format!("schedules.{id}"),
                    format!("{{\"preset\": \"{preset}\", \"args\": {{}}, \"cron\": \"0 11 * * *\", \"enabled\": false}}"),
                );
                b.forms.new_schedule.clear();
            }
        }
    });
    // A new schedule is not in `items` yet: list its draft.
    let drafts: Vec<String> = b
        .forms
        .edits
        .keys()
        .filter(|k| k.starts_with("schedules.") && !items.iter().any(|i| s(i, "key") == k.as_str()))
        .cloned()
        .collect();
    for k in drafts {
        ui.label(RichText::new(format!("{k} (new)")).strong());
        if let Some(text) = b.forms.edits.get_mut(&k) {
            ui.add(
                TextEdit::multiline(text)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
        }
    }

    ui.add_space(space::S5);
    let n = b.forms.edits.len();
    ui.horizontal(|ui| {
        ui.add(
            TextEdit::singleline(&mut b.forms.note)
                .hint_text("Note for this version (optional)")
                .desired_width(320.0),
        );
        if ui
            .add_enabled(
                b.can_act() && n > 0,
                Button::new(format!("Save {n} change(s)")),
            )
            .clicked()
        {
            b.save_settings();
        }
        if ui.add_enabled(n > 0, Button::new("Discard")).clicked() {
            b.forms.edits.clear();
        }
    });

    history(b, ui);
    secrets(b, ui);
    service(b, ui);
}

fn setting_row(b: &mut Board, ui: &mut Ui, it: &Value, key: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).strong());
        let edited = b.forms.edits.contains_key(key);
        let shown = value_text(&it["value"]);
        let ty = s(it, "type").to_owned();
        match ty.as_str() {
            "boolean" => {
                let mut on = b
                    .forms
                    .edits
                    .get(key)
                    .map_or(it["value"] == true, |t| t == "true");
                if ui.checkbox(&mut on, "").changed() {
                    b.forms.edits.insert(key.to_owned(), on.to_string());
                }
            }
            "enum" => {
                let cur = b
                    .forms
                    .edits
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| shown.clone());
                egui::ComboBox::from_id_salt(format!("set-{key}"))
                    .selected_text(&cur)
                    .show_ui(ui, |ui| {
                        for c in arr(&it["choices"]) {
                            let c = c.as_str().unwrap_or_default();
                            if ui.selectable_label(cur == c, c).clicked() {
                                b.forms.edits.insert(key.to_owned(), c.to_owned());
                            }
                        }
                    });
            }
            _ => {
                let mut text = b
                    .forms
                    .edits
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| shown.clone());
                if ui
                    .add(TextEdit::singleline(&mut text).desired_width(180.0))
                    .changed()
                {
                    b.forms.edits.insert(key.to_owned(), text);
                }
            }
        }
        weak(ui, &format!("default {}", value_text(&it["default"])));
        if edited && ui.small_button("Revert").clicked() {
            b.forms.edits.remove(key);
        }
        if it["is_set"] == true && ui.small_button("Reset to default").clicked() {
            b.forms
                .edits
                .insert(key.to_owned(), crate::logic::RESET.to_owned());
        }
        if ui.small_button("History").clicked() {
            b.show_history(Some(key.to_owned()));
        }
    });
    weak(ui, s(it, "doc"));
}

/// A setting's value as text: strings bare, `null` as empty (no value).
fn value_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(x) => x.clone(),
        other => other.to_string(),
    }
}

fn history(b: &mut Board, ui: &mut Ui) {
    let title = b
        .history_key
        .as_ref()
        .map_or_else(|| "History".to_owned(), |k| format!("History of {k}"));
    section(ui, &title);
    if b.history_key.is_some() && ui.small_button("Show every key").clicked() {
        b.show_history(None);
    }
    for v in arr(&b.history["items"]) {
        ui.label(
            RichText::new(format!(
                "Version {} by {}, {}{}",
                v["version"],
                s(v, "who"),
                local_time(s(v, "created_at")),
                v["note"]
                    .as_str()
                    .map(|n| format!(": {n}"))
                    .unwrap_or_default()
            ))
            .strong(),
        );
        for c in arr(&v["changes"]) {
            weak(
                ui,
                &format!("{}: {} to {}", s(c, "key"), c["old"], c["new"]),
            );
        }
    }
}

fn secrets(b: &mut Board, ui: &mut Ui) {
    section(ui, "Secrets");
    weak(
        ui,
        "Stored in Windows Credential Manager under your account. They never go over the Hotline, into PostgreSQL or a file.",
    );
    for (secret, set) in b.secrets_set() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(secret.label).strong());
            weak(ui, if set { "set" } else { "not set" });
            let entry = b.forms.secrets.entry(secret.name).or_default();
            ui.add(
                TextEdit::singleline(entry.buffer())
                    .password(true)
                    .desired_width(220.0),
            );
            if ui.button("Save").clicked() {
                b.save_secret(secret.name);
            }
            if set && ui.button("Remove").clicked() {
                b.remove_secret(secret.name);
            }
        });
        weak(ui, &format!("Read by {}", secret.used_by));
    }
}

fn service(b: &mut Board, ui: &mut Ui) {
    section(ui, "Service");
    if let Conn::Online { ping, .. } = b.conn.clone() {
        ui.label(s(&ping, "text"));
        weak(
            ui,
            &format!(
                "Build {} · settings version {}",
                s(&ping, "build"),
                ping["settings_version"]
            ),
        );
    }
    ui.horizontal(|ui| {
        if ui.add_enabled(b.can_act(), Button::new("Update the service")).clicked() {
            b.update_service();
        }
        weak(ui, "Waits for AI calls and publishes to finish, swaps in news.exe.new and restarts. Restart Board afterwards.");
    });
}

// ---- Docker health ---------------------------------------------------------------------------------

fn docker(b: &mut Board, ui: &mut Ui) {
    let t = tones(ui);
    section(ui, "Docker health");
    if b.docker.is_null() {
        weak(ui, "Docker status loads when the service is connected.");
        return;
    }
    let d = b.docker.clone();
    if d["engine"]["reachable"] == false {
        ui.label(
            RichText::new(format!(
                "Docker is not reachable: {}",
                s(&d["engine"], "reason")
            ))
            .color(t.error),
        );
        return;
    }
    for c in arr(&d["containers"]) {
        let name = s(c, "name").to_owned();
        ui.horizontal(|ui| {
            ui.label(RichText::new(&name).strong());
            let health = match s(c, "health") {
                "none" => String::new(),
                h => format!(" ({h})"),
            };
            ui.label(format!("{}{}", s(c, "state"), health));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if c["managed"] == true {
                    let running = matches!(s(c, "state"), "running" | "starting");
                    let can = b.can_act();
                    if ui
                        .add_enabled(can && running, Button::new("Stop"))
                        .clicked()
                    {
                        b.docker_control(&name, false);
                    }
                    if ui
                        .add_enabled(can && !running, Button::new("Start"))
                        .clicked()
                    {
                        b.docker_control(&name, true);
                    }
                } else {
                    weak(ui, "shared: health only");
                }
            });
        });
        ui.separator();
    }
    if ui.button("Refresh").clicked() {
        b.refresh_docker();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::link::{BoxStream, Connect, Link};
    use crate::secrets::Store;
    use news_hotline::BoxFut;
    use std::sync::Arc;

    struct Nothing;
    impl Connect for Nothing {
        fn connect(&self) -> BoxFut<'_, Result<BoxStream, String>> {
            Box::pin(async { Err("The Bureau service is not running.".to_owned()) })
        }
    }
    struct NoStore;
    impl Store for NoStore {
        fn write(&self, _: &str, _: &str) -> std::io::Result<()> {
            Ok(())
        }
        fn exists(&self, _: &str) -> bool {
            false
        }
        fn delete(&self, _: &str) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Every screen draws, in both themes, offline and with data, without panicking.
    #[test]
    fn every_screen_draws_in_both_themes_offline_and_with_data() {
        let ctx = egui::Context::default();
        theme::install(&ctx, Preference::Light);
        let mut b = Board::new(Link::spawn(Arc::new(Nothing), || {}), Box::new(NoStore));
        b.status = serde_json::json!({"text": "Quiet.", "next": {"reason": "Nothing is due."},
            "waiting": [{"kind": "gate", "short_id": "r_1", "preset": "edition", "since": "2026-11-12T10:00:00Z",
                "summary": {"text": "Plan waiting."},
                "gate": {"short_id": "r_1", "gate": "plan", "round": 1, "subject_sha": "9c1e5d7a22bb",
                         "since": "2026-11-12T10:00:00Z", "preview": {"pages": 3}}}],
            "running": [{"short_id": "r_2", "preset_name": "Flash", "status": "running", "steps": 5,
                "step": {"seq": 2, "title": "Fetch"}, "progress": {"done": 3, "total": 12}}],
            "queued": [{"position": 1, "short_id": "r_3", "preset": "edition", "origin": "schedule", "held_by": "slot"}],
            "problems": {"failed_runs": [], "sources": [], "refused_control": []}});
        b.standing = serde_json::json!({"version": 2, "items": [
            {"key": "chief.stall_minutes", "type": "integer", "value": 15, "default": 15, "is_set": false, "doc": "d"},
            {"key": "quiet_hours.enabled", "type": "boolean", "value": false, "default": false, "is_set": true, "doc": "d"},
            {"key": "schedules.edition", "type": "schedule", "value": {"preset": "edition"}, "default": null, "is_set": true, "doc": "d"}]});
        b.docker = serde_json::json!({"engine": {"reachable": true, "reason": null}, "containers": [
            {"name": "news-postgres", "role": "postgres", "managed": true, "state": "running", "health": "healthy"},
            {"name": "ch", "role": "clickhouse", "managed": false, "state": "running", "health": "healthy"}]});
        // A run waiting at Huddle's form, and the labelling screen on each of its modes.
        b.selected_run = Some("r_hud1".to_owned());
        b.detail = Some(
            serde_json::json!({"run": {"short_id": "r_hud1", "preset": "huddle"},
            "text": "Huddle r_hud1 waits.", "wrap": null,
            "steps": [{"seq": 2, "id": "calibration", "title": "Ranking calibration", "kind": "form",
                       "status": "running",
                       "summary": {"text": "1 pairwise question", "round": 1, "form_sha": "x",
                                   "form": {"text": "1 pairwise question", "questions": [
                                       {"a": {"story_id": 4, "title": "Fed holds"},
                                        "b": {"story_id": 9, "title": "Apple results"}}]}}}]}),
        );
        b.labelling.data = serde_json::json!({"kind": "judge", "judge": "wrong_direction",
            "items": [{"item": "j1", "text": "Revenue rose.", "facts": ["Revenue: 7 USD (2026)"]}],
            "judges": [{"id": "wrong_direction", "name": "Wrong direction", "question": "Q?",
                        "labelled": 2, "waiting": 1, "target": 100}],
            "pairs": {"labelled": 0, "waiting": 1, "target": 200}, "text": "x"});
        let judge_data = b.labelling.data.clone();
        for theme_pref in [Preference::Light, Preference::Dark] {
            theme::install(&ctx, theme_pref);
            for screen in [
                Screen::Runs,
                Screen::Settings,
                Screen::Docker,
                Screen::Labels,
            ] {
                b.screen = screen;
                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| show(&mut b, ui));
                out.textures_delta.clear();
            }
            b.labelling.mode = crate::app::LabelMode::Pairs;
            b.labelling.data = serde_json::json!({"kind": "pair", "judge": null,
                "items": [{"item": "g3-c1-s2", "a": {"title": "Fed holds", "source": "wire-a", "text": "Rates."},
                           "b": {"title": "Fed decision", "members": ["Fed holds rates"]}}],
                "judges": [], "pairs": {"labelled": 0, "waiting": 1, "target": 200}, "text": "x"});
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| show(&mut b, ui));
            out.textures_delta.clear();
            b.labelling.mode = crate::app::LabelMode::default();
            b.labelling.data = judge_data.clone();
        }
    }
}
