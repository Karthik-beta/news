//! Board spike: an eframe window themed from design/tokens/tokens.json, following the Windows
//! light/dark setting, optionally starting minimised, with an optional tray icon and toast.
//! `run` is the whole app so it can be linked into `news.exe` as `news board` for the size test.
//!
//! Flags: --minimized  --exit-after-first-frame  --report FILE  --theme light|dark|system
//!        --toast (feature "toast")  --tray (feature "tray")

use std::time::Instant;

use eframe::egui::{self, Color32, Theme, ThemePreference};
use serde_json::{Value, json};

const TOKENS: &str = include_str!("../../../../design/tokens/tokens.json");

fn hex(s: &str) -> Color32 {
    let s = s.trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(s.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    Color32::from_rgb(p(0), p(2), p(4))
}

/// One palette per mode, from the colour group of tokens.json (light = $value, dark = news.dark).
pub struct Palette {
    pub paper: Color32,
    pub wash: Color32,
    pub rule: Color32,
    pub ink: Color32,
    pub ink2: Color32,
    pub brand: Color32,
    pub control_border: Color32,
    pub focus: Color32,
    pub selection: Color32,
}

fn palette(tokens: &Value, dark: bool) -> Palette {
    let c = |name: &str| {
        let t = &tokens["color"][name];
        let v = if dark { t["$extensions"]["news.dark"].as_str() } else { None };
        hex(v.or(t["$value"].as_str()).unwrap_or("#ff00ff"))
    };
    Palette {
        paper: c("paper"),
        wash: c("wash"),
        rule: c("rule"),
        ink: c("ink"),
        ink2: c("ink-2"),
        brand: c("brand"),
        control_border: c("control-border"),
        focus: c("focus"),
        selection: c("selection"),
    }
}

fn visuals(p: &Palette, dark: bool) -> egui::Visuals {
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.paper;
    v.window_fill = p.paper;
    v.faint_bg_color = p.wash;
    v.extreme_bg_color = p.wash;
    v.override_text_color = Some(p.ink);
    v.hyperlink_color = p.focus;
    v.selection.bg_fill = p.selection;
    v.selection.stroke.color = p.ink;
    v.widgets.noninteractive.bg_stroke.color = p.rule;
    v.widgets.inactive.bg_stroke.color = p.control_border;
    v.widgets.inactive.bg_stroke.width = 1.0;
    v.widgets.hovered.bg_stroke.color = p.focus;
    v
}

fn process_age_ms() -> f64 {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let ft = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
    // SAFETY: plain out-parameters.
    unsafe {
        let z = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut c, mut e, mut k, mut u, mut now) = (z, z, z, z, z);
        GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u);
        GetSystemTimeAsFileTime(&mut now);
        (ft(now).saturating_sub(ft(c))) as f64 / 10_000.0
    }
}

fn has_console_window() -> bool {
    // SAFETY: no arguments.
    !unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() }.is_null()
}

struct Board {
    frames: u32,
    minimized_start: bool,
    exit_after_first: bool,
    report: Option<String>,
    t_main: Instant,
    age_at_main_ms: f64,
    light: Palette,
    dark: Palette,
    #[cfg(feature = "tray")]
    _tray: Option<tray_icon::TrayIcon>,
}

impl eframe::App for Board {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.frames == 0 && self.minimized_start {
            // Created hidden; minimise, then show, so the window never flashes open.
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            if self.exit_after_first {
                self.write_report(ctx, "logic");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        #[cfg(feature = "tray")]
        while let Ok(ev) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click { .. } = ev {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        self.frames += 1;
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |ui| {
            let theme = ctx.theme();
            let p = if theme == Theme::Dark { &self.dark } else { &self.light };
            ui.label(egui::RichText::new("RUNS & APPROVALS").color(p.brand).strong().size(12.0));
            ui.heading("Board");
            ui.label(egui::RichText::new("Bureau service: offline (spike)").color(p.ink2));
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Theme:");
                let mut pref = ctx.options(|o| o.theme_preference);
                ui.selectable_value(&mut pref, ThemePreference::System, "System");
                ui.selectable_value(&mut pref, ThemePreference::Light, "Light");
                ui.selectable_value(&mut pref, ThemePreference::Dark, "Dark");
                ctx.set_theme(pref);
            });
            ui.label(format!("Windows says: {:?}; showing: {:?}", ctx.system_theme(), theme));
            let _ = ui.button("Start service");
            ui.hyperlink_to("Open run 81234", "https://example.invalid/");
        });
        if self.frames <= 1 && self.exit_after_first && !self.minimized_start {
            self.write_report(&ctx, "ui");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl Board {
    fn write_report(&self, ctx: &egui::Context, stage: &str) {
        let r = json!({
            "stage": stage,
            "process_age_at_first_frame_ms": process_age_ms(),
            "process_age_at_main_ms": self.age_at_main_ms,
            "main_to_first_frame_ms": self.t_main.elapsed().as_secs_f64() * 1000.0,
            "system_theme": format!("{:?}", ctx.system_theme()),
            "active_theme": format!("{:?}", ctx.theme()),
            "console_window": has_console_window(),
            "minimized_start": self.minimized_start,
        });
        match &self.report {
            Some(f) => {
                let _ = std::fs::write(f, r.to_string());
            }
            None => println!("{r}"),
        }
    }
}

pub fn run(args: &[String]) -> i32 {
    let age_at_main_ms = process_age_ms();
    let t_main = Instant::now();
    let has = |f: &str| args.iter().any(|a| a == f);
    let val = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1).cloned());
    let tokens: Value = serde_json::from_str(TOKENS).unwrap_or(Value::Null);
    let minimized_start = has("--minimized");
    let pref = match val("--theme").as_deref() {
        Some("light") => ThemePreference::Light,
        Some("dark") => ThemePreference::Dark,
        _ => ThemePreference::System,
    };
    #[cfg(feature = "toast")]
    if has("--toast") {
        let r = tauri_winrt_notification::Toast::new(tauri_winrt_notification::Toast::POWERSHELL_APP_ID)
            .title("News Board (spike)")
            .text1("Gate waiting: Edition plan for 16 Nov")
            .on_activated(|_| Ok(()))
            .show();
        eprintln!("toast: {r:?}");
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("News Board")
            .with_inner_size([720.0, 480.0])
            .with_visible(!minimized_start),
        ..Default::default()
    };
    let report = val("--report");
    let exit_after_first = has("--exit-after-first-frame");
    let _want_tray = has("--tray");
    let r = eframe::run_native(
        "News Board",
        options,
        Box::new(move |cc| {
            let light = palette(&tokens, false);
            let dark = palette(&tokens, true);
            cc.egui_ctx.set_visuals_of(Theme::Light, visuals(&light, false));
            cc.egui_ctx.set_visuals_of(Theme::Dark, visuals(&dark, true));
            cc.egui_ctx.set_theme(pref);
            #[cfg(feature = "tray")]
            let tray = if _want_tray {
                let ctx = cc.egui_ctx.clone();
                tray_icon::TrayIconEvent::set_event_handler(Some(move |ev| {
                    let _ = ev;
                    ctx.request_repaint();
                }));
                let px: Vec<u8> = (0..16 * 16).flat_map(|_| [0xC8, 0x21, 0x3A, 0xFF]).collect();
                tray_icon::Icon::from_rgba(px, 16, 16)
                    .ok()
                    .and_then(|icon| tray_icon::TrayIconBuilder::new().with_tooltip("News Board").with_icon(icon).build().ok())
            } else {
                None
            };
            Ok(Box::new(Board {
                frames: 0,
                minimized_start,
                exit_after_first,
                report,
                t_main,
                age_at_main_ms,
                light,
                dark,
                #[cfg(feature = "tray")]
                _tray: tray,
            }))
        }),
    );
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("board: {e}");
            1
        }
    }
}
