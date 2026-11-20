//! The Windows window: eframe on the glow renderer (spike item 5). It starts hidden and, on the first
//! `logic` call, minimises and shows itself, so a log-on start paints nothing. Toasts are raised here
//! from the event stream when Board is not in front, and a click on one opens Board on that item.

use crate::app::Board;
use crate::args::{Args, OpenTarget};
use crate::link::{Link, PipeConnect};
use crate::secrets::Credman;
use crate::theme::{self, Preference};
use crate::toast;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

struct Shell {
    board: Board,
    minimized: bool,
    shown: bool,
    clicks: Receiver<String>,
    click_tx: std::sync::mpsc::Sender<String>,
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.shown {
            self.shown = true;
            if self.minimized {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        self.board.poll();
        let focused = ctx.input(|i| i.focused);
        for t in std::mem::take(&mut self.board.toasts) {
            if !focused {
                let (tx, ctx2) = (self.click_tx.clone(), ctx.clone());
                toast::show(&t, move |open| {
                    let _ = tx.send(open);
                    ctx2.request_repaint();
                });
            }
        }
        while let Ok(open) = self.clicks.try_recv() {
            if let Ok(t) = OpenTarget::parse(&open) {
                self.board.open(&t);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::ui::show(&mut self.board, ui);
    }
}

pub fn run(args: Args) -> i32 {
    // At log-on `news.exe` starts with a console of its own; drop it so only Board's window shows.
    news_win::detach_console_if_alone();
    let viewport = egui::ViewportBuilder::default()
        .with_title("News Board")
        .with_inner_size([1100.0, 780.0])
        .with_visible(false);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let result = eframe::run_native(
        "News Board",
        options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx, Preference::System);
            let ctx = cc.egui_ctx.clone();
            let link = Link::spawn(Arc::new(PipeConnect), move || ctx.request_repaint());
            let mut board = Board::new(link, Box::new(Credman));
            if let Some(t) = &args.open {
                board.open(t);
            }
            let (click_tx, clicks) = channel();
            Ok(Box::new(Shell {
                board,
                minimized: args.minimized,
                shown: false,
                clicks,
                click_tx,
            }))
        }),
    );
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Board could not open its window: {e}");
            1
        }
    }
}
