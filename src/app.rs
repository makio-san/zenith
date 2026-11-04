//! Zenith application: tabs, terminal rendering, mouse selection, keyboard, sidebar, palette.

use std::time::Instant;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};
use egui::{Align2, Color32, Event, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};

use crate::input;
use crate::pixelcat::Cat;
use crate::stats::{self, Shared, fmt_bytes, fmt_rate, fmt_uptime};
use crate::term::{Session, Size};
use crate::theme::{self, chip, frame, leader_row, mono, skyline};

struct Tab {
    session: Session,
    name: String,
}

struct Palette {
    open: bool,
    query: String,
    sel: usize,
}

#[derive(PartialEq, Clone, Copy)]
enum SidePage {
    Stats,
    About,
}

pub struct App {
    tabs: Vec<Tab>,
    active: usize,
    counter: usize,
    stats: Shared,
    start: Instant,
    show_side: bool,
    side_page: SidePage,
    font_px: f32,
    palette: Palette,
    toast: Option<(String, f64)>,
    /// Terminal geometry of the last frame, for mouse → cell mapping.
    grid: Option<(Rect, f32, f32, usize, usize)>,
    selecting: bool,
    last_strike: f64,
    mouse_btn: Option<u8>,
    last_mouse_cell: Option<(usize, usize)>,
    demo_sent: bool,
}

const ACTIONS: &[(&str, &str)] = &[
    ("New tab", "⌘T"),
    ("Close tab", "⌘W"),
    ("Copy selection", "⌘C"),
    ("Toggle sidebar", "⌘S"),
    ("Bigger font", "⌘+"),
    ("Smaller font", "⌘-"),
    ("Print system info", "zenith info"),
    ("Lightning strike", "⚡"),
];

const WELCOME: &str = concat!(
    "\x1b[38;5;220m  /\\_/\\ \x1b[0m   \x1b[1;38;5;220mZENITH\x1b[0m ⚡\r\n",
    "\x1b[38;5;220m ( -.- )z\x1b[0m   \x1b[2m[K] palette  [S] sidebar  [T] new tab\x1b[0m\r\n",
    "\x1b[38;5;220m  > ^ <  \x1b[0m\r\n\r\n"
);

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        install_fonts(&ctx);
        let mut style = (*ctx.style()).clone();
        style.visuals.dark_mode = true;
        style.visuals.panel_fill = theme::BG;
        style.visuals.window_fill = theme::BG;
        style.visuals.override_text_color = Some(theme::FG);
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, theme::LINE);
        style.visuals.selection.bg_fill = theme::SELECTION;
        style.spacing.item_spacing = Vec2::new(6.0, 4.0);
        ctx.set_style(style);

        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            counter: 0,
            stats: stats::start(ctx.clone()),
            start: Instant::now(),
            show_side: true,
            side_page: SidePage::Stats,
            font_px: 22.0,
            palette: Palette { open: false, query: String::new(), sel: 0 },
            toast: None,
            grid: None,
            selecting: false,
            last_strike: -10.0,
            mouse_btn: None,
            last_mouse_cell: None,
            demo_sent: false,
        };
        app.new_tab(&ctx);
        app
    }

    fn t(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), self.t()));
    }

    fn new_tab(&mut self, ctx: &egui::Context) {
        self.counter += 1;
        let banner = if self.counter == 1 { WELCOME } else { "" };
        match Session::spawn(ctx.clone(), Size { cols: 100, rows: 30 }, banner) {
            Ok(session) => {
                let shell = std::env::var("SHELL").unwrap_or_default();
                let shell = shell.rsplit('/').next().unwrap_or("sh").to_string();
                self.tabs.push(Tab { session, name: shell });
                self.active = self.tabs.len() - 1;
            }
            Err(e) => self.toast(format!("Could not start shell: {e}")),
        }
    }

    fn close_tab(&mut self, i: usize, ctx: &egui::Context) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.new_tab(ctx);
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }

    fn write_active(&self, bytes: &[u8]) {
        if let Some(t) = self.tabs.get(self.active) {
            t.session.write(bytes);
        }
    }

    /// Keyboard and paste input: also jumps back to the live screen if the user had scrolled up.
    fn user_input(&self, bytes: &[u8]) {
        if let Some(t) = self.tabs.get(self.active) {
            if let Ok(mut term) = t.session.term.lock() {
                if term.grid().display_offset() != 0 {
                    term.scroll_display(Scroll::Bottom);
                }
            }
            t.session.write(bytes);
        }
    }

    /// Terminal cell (column, row) under a screen position, if it is inside the text area.
    fn cell_at(&self, pos: Pos2) -> Option<(usize, usize)> {
        let (inner, cw, rh, cols, rows) = self.grid?;
        if !inner.contains(pos) {
            return None;
        }
        let col = (((pos.x - inner.left()) / cw) as usize).min(cols.saturating_sub(1));
        let row = (((pos.y - inner.top()) / rh) as usize).min(rows.saturating_sub(1));
        Some((col, row))
    }

    fn mouse_report(&self, mode: TermMode, code: u8, col: usize, row: usize, press: bool, m: Modifiers) {
        if let Some(bytes) = input::encode_mouse(mode, code, col, row, press, m) {
            self.write_active(&bytes);
        }
    }

    fn feed_active(&self, text: &str) {
        if let Some(t) = self.tabs.get(self.active) {
            if let Ok(mut term) = t.session.term.lock() {
                use alacritty_terminal::vte::ansi::Processor;
                let mut p: Processor = Processor::new();
                for &b in text.as_bytes() {
                    p.advance(&mut *term, b);
                }
            }
        }
    }

    fn print_info(&mut self) {
        let s = self.stats.lock().unwrap().clone();
        let y = |t: &str| format!("\x1b[38;5;220m{t}\x1b[0m");
        let ram = if s.mem_total > 0 { s.mem_used as f32 / s.mem_total as f32 * 100.0 } else { 0.0 };
        let text = format!(
            "\r\n{}\x1b[1;38;5;220mZENITH\x1b[0m ⚡\r\n{}OS    {}\r\n{}CPU   {} · {} cores · {:.0}%\r\n            GPU   {}\r\n            RAM   {} · {:.0}% used\r\n\r\n",
            y("    /\\_/\\   "),
            y("   ( -.- )z "), s.os,
            y("    > ^ <   "), s.cpu_name, s.cores, s.cpu,
            s.gpu_name,
            fmt_bytes(s.mem_total), ram,
        );
        self.feed_active(&text);
    }

    fn copy_selection(&mut self, ctx: &egui::Context) -> bool {
        let Some(t) = self.tabs.get(self.active) else { return false };
        let Ok(mut term) = t.session.term.lock() else { return false };
        let text = term.selection_to_string();
        match text {
            Some(s) if !s.trim().is_empty() => {
                ctx.copy_text(s);
                term.selection = None;
                drop(term);
                self.toast("Copied");
                true
            }
            _ => false,
        }
    }

    fn strike(&mut self) {
        self.last_strike = self.t();
    }

    fn run_action(&mut self, name: &str, ctx: &egui::Context) {
        match name {
            "New tab" => self.new_tab(ctx),
            "Close tab" => self.close_tab(self.active, ctx),
            "Copy selection" => {
                if !self.copy_selection(ctx) {
                    self.toast("Nothing selected");
                }
            }
            "Toggle sidebar" => self.show_side = !self.show_side,
            "Bigger font" => self.font_px = (self.font_px + 1.0).min(40.0),
            "Smaller font" => self.font_px = (self.font_px - 1.0).max(12.0),
            "Print system info" => self.print_info(),
            "Lightning strike" => self.strike(),
            _ => {}
        }
    }

    // ---------- Keyboard ----------
    fn handle_input(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        let mode = self
            .tabs
            .get(self.active)
            .and_then(|t| t.session.term.lock().ok().map(|term| *term.mode()))
            .unwrap_or(TermMode::empty());
        let app_cursor = mode.contains(TermMode::APP_CURSOR);
        let live_mods = ctx.input(|i| i.modifiers);

        for ev in events {
            match ev {
                Event::Text(s) => {
                    if !self.palette.open {
                        self.user_input(s.as_bytes());
                    }
                }
                Event::Paste(s) => {
                    if !self.palette.open {
                        self.user_input(&input::wrap_paste(&s, mode.contains(TermMode::BRACKETED_PASTE)));
                    }
                }
                Event::Key { key, pressed: true, modifiers, .. } => {
                    if modifiers.command {
                        self.shortcut(key, modifiers, ctx);
                        continue;
                    }
                    if self.palette.open {
                        self.palette_key(key, ctx);
                        continue;
                    }
                    if let Some(bytes) = key_bytes(key, modifiers, app_cursor) {
                        self.user_input(&bytes);
                    }
                }
                Event::Screenshot { image, .. } => {
                    if let Ok(path) = std::env::var("ZENITH_SHOT") {
                        let [w, h] = image.size;
                        let raw: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                        if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, raw) {
                            let _ = img.save(&path);
                        }
                        std::process::exit(0);
                    }
                }
                Event::MouseWheel { delta, modifiers, .. } => {
                    let lines = (delta.y / 2.0).round() as i32;
                    if lines == 0 || self.palette.open {
                        continue;
                    }
                    let steps = lines.unsigned_abs().min(5);
                    if mode.intersects(TermMode::MOUSE_MODE) && !modifiers.shift {
                        // The running program (vim, tmux, htop...) owns the mouse: report wheel buttons.
                        if let Some((c, r)) = ctx.input(|i| i.pointer.hover_pos()).and_then(|p| self.cell_at(p)) {
                            let code = if lines > 0 { 64 } else { 65 };
                            for _ in 0..steps {
                                self.mouse_report(mode, code, c, r, true, modifiers);
                            }
                        }
                    } else if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) && !modifiers.shift {
                        // Full-screen apps without mouse mode (less, man): the wheel acts as arrow keys.
                        let seq: &[u8] = match (lines > 0, app_cursor) {
                            (true, true) => b"\x1bOA",
                            (true, false) => b"\x1b[A",
                            (false, true) => b"\x1bOB",
                            (false, false) => b"\x1b[B",
                        };
                        for _ in 0..steps {
                            self.write_active(seq);
                        }
                    } else if let Some(t) = self.tabs.get(self.active) {
                        if let Ok(mut term) = t.session.term.lock() {
                            term.scroll_display(Scroll::Delta(lines));
                        }
                    }
                }
                Event::PointerButton { pos, button, pressed, modifiers } => {
                    if !mode.intersects(TermMode::MOUSE_MODE) || modifiers.shift || self.palette.open {
                        if !pressed {
                            self.mouse_btn = None;
                        }
                        continue;
                    }
                    let Some((c, r)) = self.cell_at(pos) else { continue };
                    let code = match button {
                        egui::PointerButton::Primary => 0,
                        egui::PointerButton::Middle => 1,
                        egui::PointerButton::Secondary => 2,
                        _ => continue,
                    };
                    self.mouse_btn = if pressed { Some(code) } else { None };
                    self.last_mouse_cell = Some((c, r));
                    self.mouse_report(mode, code, c, r, pressed, modifiers);
                }
                Event::PointerMoved(pos) => {
                    if !mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) || live_mods.shift || self.palette.open {
                        continue;
                    }
                    let Some((c, r)) = self.cell_at(pos) else { continue };
                    if self.last_mouse_cell == Some((c, r)) {
                        continue;
                    }
                    let code = match self.mouse_btn {
                        Some(b) => Some(b + 32),
                        None if mode.contains(TermMode::MOUSE_MOTION) => Some(35),
                        None => None,
                    };
                    if let Some(code) = code {
                        self.last_mouse_cell = Some((c, r));
                        self.mouse_report(mode, code, c, r, true, live_mods);
                    }
                }
                _ => {}
            }
        }
    }

    fn shortcut(&mut self, key: Key, m: Modifiers, ctx: &egui::Context) {
        match key {
            Key::T => self.new_tab(ctx),
            Key::W => self.close_tab(self.active, ctx),
            Key::S | Key::B => self.show_side = !self.show_side,
            Key::K => {
                self.palette.open = !self.palette.open;
                self.palette.query.clear();
                self.palette.sel = 0;
            }
            Key::Equals | Key::Plus => self.font_px = (self.font_px + 1.0).min(40.0),
            Key::Minus => self.font_px = (self.font_px - 1.0).max(12.0),
            Key::L => self.write_active(b"\x0c"),
            Key::C if !m.shift => {
                // ⌘C copies the selection; with nothing selected it sends an interrupt like a classic terminal.
                if !self.copy_selection(ctx) {
                    self.write_active(b"\x03");
                }
            }
            Key::A => {
                if let Some(t) = self.tabs.get(self.active) {
                    if let Ok(mut term) = t.session.term.lock() {
                        let rows = term.screen_lines() as i32;
                        let cols = term.columns();
                        let mut sel = Selection::new(SelectionType::Lines, Point::new(Line(-(term.history_size() as i32)), Column(0)), Side::Left);
                        sel.update(Point::new(Line(rows - 1), Column(cols.saturating_sub(1))), Side::Right);
                        term.selection = Some(sel);
                    }
                }
            }
            Key::Num1 | Key::Num2 | Key::Num3 | Key::Num4 | Key::Num5 | Key::Num6 | Key::Num7 | Key::Num8 | Key::Num9 => {
                let n = key.name().chars().last().and_then(|c| c.to_digit(10)).unwrap_or(1) as usize;
                if n <= self.tabs.len() {
                    self.active = n - 1;
                }
            }
            _ => {}
        }
    }

    fn palette_items(&self) -> Vec<&'static (&'static str, &'static str)> {
        let q = self.palette.query.to_lowercase();
        ACTIONS.iter().filter(|(n, _)| n.to_lowercase().contains(&q)).collect()
    }

    fn palette_key(&mut self, key: Key, ctx: &egui::Context) {
        let items = self.palette_items();
        match key {
            Key::Escape => self.palette.open = false,
            Key::ArrowDown => self.palette.sel = (self.palette.sel + 1).min(items.len().saturating_sub(1)),
            Key::ArrowUp => self.palette.sel = self.palette.sel.saturating_sub(1),
            Key::Enter => {
                if let Some((name, _)) = items.get(self.palette.sel) {
                    let name = *name;
                    self.palette.open = false;
                    self.run_action(name, ctx);
                }
            }
            _ => {}
        }
    }

    // ---------- Mouse → grid ----------
    fn point_at(&self, pos: Pos2, offset: i32) -> Option<(Point, Side)> {
        let (inner, cw, rh, cols, rows) = self.grid?;
        let fx = ((pos.x - inner.left()) / cw).max(0.0);
        let col = (fx.floor() as usize).min(cols.saturating_sub(1));
        let side = if fx.fract() > 0.5 { Side::Right } else { Side::Left };
        let line = (((pos.y - inner.top()) / rh).floor() as i32).clamp(0, rows as i32 - 1);
        Some((Point::new(Line(line - offset), Column(col)), side))
    }

    // ---------- Terminal rendering ----------
    fn paint_terminal(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::BG);

        let font = mono(self.font_px);
        let (cw, rh) = ui.ctx().fonts(|f| (f.glyph_width(&font, 'M'), f.row_height(&font)));
        let inner = rect.shrink2(Vec2::new(16.0, 12.0));
        let cols = (inner.width() / cw).floor().max(2.0) as usize;
        let rows = (inner.height() / rh).floor().max(1.0) as usize;
        self.grid = Some((inner, cw, rh, cols, rows));

        let t = self.t();
        let blink_on = (t * 2.0).fract() < 0.55;
        let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
        let press_origin = ui.ctx().input(|i| i.pointer.press_origin());

        let Some(tab) = self.tabs.get(self.active) else { return };
        tab.session.resize_shared(Size { cols, rows });
        let Ok(mut term) = tab.session.term.lock() else { return };
        let offset = term.grid().display_offset() as i32;
        let mode = *term.mode();
        let mods = ui.ctx().input(|i| i.modifiers);
        // When a program has turned on mouse reporting, it owns the mouse; Shift gives selection back to us.
        let reporting = mode.intersects(TermMode::MOUSE_MODE) && !mods.shift;

        // ⌘-hover highlights a link, ⌘-click opens it.
        let mut link: Option<(i32, usize, usize, String)> = None;
        if mods.command {
            if let Some((c, r)) = pointer.and_then(|p| self.cell_at(p)) {
                let ncols = term.columns().min(cols);
                let line = Line(r as i32 - offset);
                let text: Vec<char> = (0..ncols).map(|x| term.grid()[Point::new(line, Column(x))].c).collect();
                link = input::find_url(&text, c).map(|(a, b, u)| (r as i32, a, b, u));
            }
        }
        let mut opened = false;
        if resp.clicked() && mods.command {
            if let Some((_, _, _, url)) = &link {
                input::open_url(url);
                opened = true;
            }
        }
        if link.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        if reporting || opened {
            // The running program owns the mouse, or a link was just opened: no selection change.
        } else if resp.triple_clicked() {
            if let Some((p, s)) = pointer.and_then(|p| self.point_at(p, offset)) {
                term.selection = Some(Selection::new(SelectionType::Lines, p, s));
            }
        } else if resp.double_clicked() {
            if let Some((p, s)) = pointer.and_then(|p| self.point_at(p, offset)) {
                term.selection = Some(Selection::new(SelectionType::Semantic, p, s));
            }
        } else if resp.drag_started() {
            if let Some((p, s)) = press_origin.and_then(|p| self.point_at(p, offset)) {
                term.selection = Some(Selection::new(SelectionType::Simple, p, s));
                self.selecting = true;
            }
        } else if resp.dragged() && self.selecting {
            if let Some((p, s)) = pointer.and_then(|p| self.point_at(p, offset)) {
                if let Some(sel) = term.selection.as_mut() {
                    sel.update(p, s);
                }
            }
        } else if resp.drag_stopped() {
            self.selecting = false;
        } else if resp.clicked() {
            term.selection = None;
        }

        let content = term.renderable_content();
        let cursor = content.cursor;
        let selection = content.selection;

        let mut run = String::new();
        let mut run_col = 0usize;
        let mut run_line = -1i32;
        let mut run_color = theme::FG;
        let flush = |run: &mut String, line: i32, col: usize, color: Color32| {
            if run.is_empty() {
                return;
            }
            let pos = Pos2::new(inner.left() + col as f32 * cw, inner.top() + line as f32 * rh);
            painter.text(pos, Align2::LEFT_TOP, run.as_str(), font.clone(), color);
            run.clear();
        };

        for cell in content.display_iter {
            let line = cell.point.line.0;
            let col = cell.point.column.0;
            let flags = cell.flags;
            if flags.contains(Flags::WIDE_CHAR_SPACER) || flags.contains(Flags::HIDDEN) {
                continue;
            }
            let (mut fg, mut bg) = (cell.fg, cell.bg);
            if flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let bold = flags.contains(Flags::BOLD);
            let mut fgc = theme::color(fg, bold);
            if flags.contains(Flags::DIM) {
                fgc = theme::FG2;
            }
            let cell_rect = Rect::from_min_size(Pos2::new(inner.left() + col as f32 * cw, inner.top() + line as f32 * rh), Vec2::new(cw, rh));
            let selected = selection.map_or(false, |r| r.contains(Point::new(Line(line - offset), Column(col))));
            if selected {
                painter.rect_filled(cell_rect, 0.0, theme::SELECTION);
            } else if !matches!(bg, Color::Named(NamedColor::Background)) {
                painter.rect_filled(cell_rect, 0.0, theme::color(bg, false));
            }
            if flags.contains(Flags::UNDERLINE) {
                painter.hline(cell_rect.x_range(), cell_rect.bottom() - 1.5, Stroke::new(1.0f32, fgc));
            }
            let c = cell.c;
            let same = line == run_line && col == run_col + run.chars().count() && fgc == run_color;
            if !same {
                flush(&mut run, run_line, run_col, run_color);
                run_line = line;
                run_col = col;
                run_color = fgc;
            }
            if c == ' ' && run.is_empty() {
                run_col = col + 1;
                continue;
            }
            run.push(c);
        }
        flush(&mut run, run_line, run_col, run_color);

        if let Some((r, a, b, _)) = &link {
            let y = inner.top() + (*r as f32 + 1.0) * rh - 1.0;
            painter.hline((inner.left() + *a as f32 * cw)..=(inner.left() + (*b as f32 + 1.0) * cw), y, Stroke::new(1.5f32, theme::ACC));
        }

        if cursor.shape != CursorShape::Hidden && blink_on {
            let line = cursor.point.line.0 + offset;
            if line >= 0 && (line as usize) < rows {
                let pos = Pos2::new(inner.left() + cursor.point.column.0 as f32 * cw, inner.top() + line as f32 * rh);
                let r = Rect::from_min_size(pos, Vec2::new(cw, rh));
                match cursor.shape {
                    CursorShape::Beam => {
                        painter.rect_filled(Rect::from_min_size(pos, Vec2::new(2.0, rh)), 0.0, theme::FG);
                    }
                    CursorShape::Underline => {
                        painter.rect_filled(Rect::from_min_size(Pos2::new(pos.x, pos.y + rh - 2.0), Vec2::new(cw, 2.0)), 0.0, theme::FG);
                    }
                    _ => {
                        painter.rect_filled(r, 0.0, theme::FG);
                        let under = term.grid()[cursor.point].c;
                        if under != ' ' {
                            painter.text(pos, Align2::LEFT_TOP, under, font.clone(), theme::INK);
                        }
                    }
                }
            }
        }
    }

    // ---------- Sidebar ----------
    fn paint_side(&mut self, ui: &mut egui::Ui) {
        let s = self.stats.lock().unwrap().clone();
        let t = self.t();
        let striking = t - self.last_strike < 0.7;
        let load = s.cpu / 100.0;

        ui.horizontal(|ui| {
            if chip(ui, "1", "Stats", self.side_page == SidePage::Stats).clicked() {
                self.side_page = SidePage::Stats;
            }
            if chip(ui, "2", "About", self.side_page == SidePage::About).clicked() {
                self.side_page = SidePage::About;
            }
        });
        ui.add_space(10.0);

        // Pixel cat + brand
        let (cat_rect, cat_resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 100.0), Sense::click());
        if cat_resp.clicked() {
            self.strike();
        }
        let cat_box = Rect::from_min_size(cat_rect.min, Vec2::new(116.0, 100.0));
        Cat::paint(ui.painter(), cat_box, t, load, striking);
        let tx = cat_box.right() + 10.0;
        ui.painter().text(Pos2::new(tx, cat_rect.top() + 22.0), Align2::LEFT_CENTER, "ZENITH ⚡", mono(30.0), theme::ACC);
        let mood = if striking { "zenitsu is startled!" } else if load > 0.45 { "zenitsu is awake" } else { "zenitsu is asleep" };
        ui.painter().text(Pos2::new(tx, cat_rect.top() + 50.0), Align2::LEFT_CENTER, mood, mono(18.0), theme::FG2);
        ui.painter().text(Pos2::new(tx, cat_rect.top() + 70.0), Align2::LEFT_CENTER, chrono::Local::now().format("%H:%M · %a %d %b").to_string(), mono(18.0), theme::DIM);
        ui.add_space(14.0);

        match self.side_page {
            SidePage::Stats => self.paint_stats(ui, &s),
            SidePage::About => self.paint_about(ui),
        }

        // Bottom chips
        let bottom = ui.available_rect_before_wrap();
        let mut b = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(bottom.left(), bottom.bottom() - 30.0), bottom.max)).layout(egui::Layout::left_to_right(egui::Align::Center)));
        if chip(&mut b, "S", "Hide sidebar", false).clicked() {
            self.show_side = false;
        }
        if chip(&mut b, "K", "Palette", false).clicked() {
            self.palette.open = true;
            self.palette.query.clear();
            self.palette.sel = 0;
        }
    }

    fn paint_stats(&mut self, ui: &mut egui::Ui, s: &stats::Stats) {
        let ram_pct = if s.mem_total > 0 { s.mem_used as f32 / s.mem_total as f32 * 100.0 } else { 0.0 };
        let hot = |v: f32| if v > 85.0 { theme::HOT } else { theme::FG };

        framed(ui, "System", 170.0, |ui| {
            leader_row(ui, "CPU", &format!("{:.0}%", s.cpu), hot(s.cpu), true);
            leader_row(ui, "RAM", &format!("{} / {}", fmt_bytes(s.mem_used), fmt_bytes(s.mem_total)), hot(ram_pct), false);
            leader_row(ui, "GPU", &s.gpu.map_or("—".into(), |g| format!("{g:.0}%")), theme::FG, false);
            leader_row(ui, "Temp", &s.temp.map_or("—".into(), |t| format!("{t:.0}°C")), hot(if s.temp.unwrap_or(0.0) > 85.0 { 100.0 } else { 0.0 }), false);
            leader_row(ui, "Net", &format!("↓{} ↑{}", fmt_rate(s.rx), fmt_rate(s.tx)), theme::FG, false);
            leader_row(ui, "Disk", &format!("{} / {}", fmt_bytes(s.disk_used), fmt_bytes(s.disk_total)), theme::FG, false);
            leader_row(ui, "Up", &fmt_uptime(s.uptime), theme::FG, false);
        });

        framed(ui, "Cores", 44.0, |ui| {
            let blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
            let mut x = r.left();
            for v in &s.per_core {
                let i = ((v / 100.0).clamp(0.0, 0.999) * 8.0) as usize;
                let c = if i == 0 { theme::DIM } else { theme::FG };
                let rr = ui.painter().text(Pos2::new(x, r.center().y), Align2::LEFT_CENTER, blocks[i], FontId::monospace(22.0), c);
                x = rr.right() + 2.0;
            }
            ui.painter().text(Pos2::new(r.right(), r.center().y), Align2::RIGHT_CENTER, format!("{} · {}", s.cpu_name, s.cores), mono(16.0), theme::DIM);
        });

        framed(ui, "CPU history", 96.0, |ui| skyline(ui, &s.cpu_hist, 80.0));
        framed(ui, "Memory history", 96.0, |ui| skyline(ui, &s.ram_hist, 80.0));
    }

    fn paint_about(&mut self, ui: &mut egui::Ui) {
        framed(ui, "About", 230.0, |ui| {
            let lines = [
                ("Zenith", theme::ACC),
                ("a textmode terminal, written in Rust.", theme::FG),
                ("", theme::FG),
                ("Named after Zenitsu, a yellow cat who", theme::FG2),
                ("sleeps through everything and still", theme::FG2),
                ("strikes like lightning when it matters.", theme::FG2),
                ("", theme::FG),
                ("egui · alacritty_terminal · sysinfo", theme::DIM),
                ("font: VT323 (SIL OFL 1.1)", theme::DIM),
            ];
            for (l, c) in lines {
                ui.label(egui::RichText::new(l).font(mono(18.0)).color(c));
            }
        });
        framed(ui, "Keys", 175.0, |ui| {
            for (k, v) in [("⌘T", "new tab"), ("⌘W", "close tab"), ("⌘1-9", "switch tab"), ("⌘S", "sidebar"), ("⌘K", "palette"), ("⌘C", "copy selection"), ("⌘-click", "open link")] {
                leader_row(ui, k, v, theme::FG, false);
            }
        });
    }

    // ---------- Status line ----------
    fn paint_status(&mut self, ui: &mut egui::Ui) {
        let cpu = self.stats.lock().unwrap().cpu;
        let t = self.t();
        let striking = t - self.last_strike < 0.7;
        let cat = if striking { "( °o° )⚡" } else if cpu > 45.0 { "( o.o )" } else { "( -.- )z" };
        let name = self.tabs.get(self.active).map(|t| t.name.clone()).unwrap_or_default();
        let (cols, rows) = self.grid.map(|g| (g.3, g.4)).unwrap_or((0, 0));
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(cat).font(mono(19.0)).color(if striking { theme::ACC } else { theme::FG }));
            ui.label(egui::RichText::new("zenitsu").font(mono(18.0)).color(theme::DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new("⚡").font(FontId::proportional(14.0)).color(theme::ACC));
                ui.label(egui::RichText::new(format!("{name} · {cols}×{rows}")).font(mono(18.0)).color(theme::FG2));
            });
        });
    }

    // ---------- Command palette ----------
    fn paint_palette(&mut self, ctx: &egui::Context) {
        if !self.palette.open {
            return;
        }
        let items: Vec<(&str, &str)> = self.palette_items().into_iter().map(|(a, b)| (*a, *b)).collect();
        let mut run: Option<&str> = None;
        egui::Area::new("palette".into()).anchor(Align2::CENTER_TOP, Vec2::new(0.0, 110.0)).show(ctx, |ui| {
            let w = 460.0;
            let h = 70.0 + items.len() as f32 * 21.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
            ui.painter().rect_filled(rect.expand(4.0), 0.0, theme::BG);
            ui.painter().rect_filled(rect, 0.0, theme::BG);
            frame(ui.painter(), rect, "Palette");
            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(12.0, 12.0))).layout(egui::Layout::top_down(egui::Align::Min)));
            inner.horizontal(|ui| {
                ui.label(egui::RichText::new(">").font(mono(21.0)).color(theme::ACC));
                let edit = egui::TextEdit::singleline(&mut self.palette.query)
                    .font(mono(21.0))
                    .text_color(theme::FG)
                    .hint_text("type a command")
                    .frame(false)
                    .desired_width(f32::INFINITY);
                ui.add(edit).request_focus();
            });
            inner.add_space(6.0);
            for (i, (name, hint)) in items.iter().enumerate() {
                let sel = i == self.palette.sel;
                let r = inner.allocate_rect(Rect::from_min_size(inner.cursor().min, Vec2::new(w - 24.0, 21.0)), Sense::click());
                let mut row_ui = inner.new_child(egui::UiBuilder::new().max_rect(r.rect).layout(egui::Layout::top_down(egui::Align::Min)));
                leader_row(&mut row_ui, name, hint, if sel { theme::ACC } else { theme::FG }, sel);
                if r.clicked() {
                    run = Some(name);
                }
            }
        });
        if let Some(name) = run {
            self.palette.open = false;
            self.run_action(name, ctx);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut i = 0;
        while i < self.tabs.len() {
            if self.tabs[i].session.exited() {
                self.close_tab(i, ctx);
            } else {
                i += 1;
            }
        }
        for tab in &mut self.tabs {
            if let Some(t) = tab.session.title() {
                tab.name = if t.chars().count() > 18 { format!("{}…", t.chars().take(17).collect::<String>()) } else { t };
            }
        }
        self.handle_input(ctx);
        let t = self.t();

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::BG))
            .show(ctx, |ui| {
                let full = ui.available_rect_before_wrap();
                let side_w = if self.show_side { 300.0 } else { 0.0 };

                if self.show_side {
                    let side_rect = Rect::from_min_max(full.min, Pos2::new(full.left() + side_w, full.bottom()));
                    ui.painter().vline(side_rect.right(), side_rect.y_range(), Stroke::new(1.0f32, theme::LINE));
                    let mut side_ui = ui.new_child(egui::UiBuilder::new().max_rect(side_rect.shrink2(Vec2::new(12.0, 12.0))).layout(egui::Layout::top_down(egui::Align::Min)));
                    self.paint_side(&mut side_ui);
                }

                let main = Rect::from_min_max(Pos2::new(full.left() + side_w, full.top()), full.max);
                // Top bar: tab chips
                let top = Rect::from_min_size(main.min, Vec2::new(main.width(), 44.0));
                ui.painter().hline(top.x_range(), top.bottom(), Stroke::new(1.0f32, theme::LINE));
                let mut top_ui = ui.new_child(egui::UiBuilder::new().max_rect(top.shrink2(Vec2::new(12.0, 8.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                let mut go = None;
                let mut close = None;
                for i in 0..self.tabs.len() {
                    let name = self.tabs[i].name.clone();
                    let r = chip(&mut top_ui, &(i + 1).to_string(), &name, i == self.active);
                    if r.clicked() {
                        go = Some(i);
                    }
                    if r.secondary_clicked() || r.middle_clicked() {
                        close = Some(i);
                    }
                }
                if chip(&mut top_ui, "+", "", false).on_hover_text("New tab ⌘T").clicked() {
                    self.new_tab(ctx);
                }
                top_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if chip(ui, "⚡", "", false).on_hover_text("Lightning strike").clicked() {
                        self.strike();
                    }
                    if !self.show_side && chip(ui, "S", "Sidebar", false).clicked() {
                        self.show_side = true;
                    }
                });
                if let Some(i) = go {
                    self.active = i;
                }
                if let Some(i) = close {
                    self.close_tab(i, ctx);
                }

                // Status line at the bottom
                let status_h = 30.0;
                let status = Rect::from_min_max(Pos2::new(main.left(), main.bottom() - status_h), main.max);
                ui.painter().hline(status.x_range(), status.top(), Stroke::new(1.0f32, theme::LINE));
                let mut status_ui = ui.new_child(egui::UiBuilder::new().max_rect(status.shrink2(Vec2::new(12.0, 4.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                self.paint_status(&mut status_ui);

                // Terminal in between
                let term_rect = Rect::from_min_max(Pos2::new(main.left(), top.bottom() + 1.0), Pos2::new(main.right(), status.top() - 1.0));
                let mut term_ui = ui.new_child(egui::UiBuilder::new().max_rect(term_rect).layout(egui::Layout::top_down(egui::Align::Min)));
                self.paint_terminal(&mut term_ui);
            });

        self.paint_palette(ctx);

        if let Some((msg, at)) = self.toast.clone() {
            if t - at < 1.8 {
                egui::Area::new("toast".into()).anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -44.0)).show(ctx, |ui| {
                    let text = format!("[⚡] {msg}");
                    let galley = ui.painter().layout_no_wrap(text, mono(19.0), theme::ACC);
                    let (r, _) = ui.allocate_exact_size(galley.size() + Vec2::new(20.0, 10.0), Sense::hover());
                    ui.painter().rect_filled(r, 0.0, theme::BG);
                    ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0f32, theme::FG2));
                    ui.painter().galley(r.min + Vec2::new(10.0, 5.0), galley, theme::ACC);
                });
            } else {
                self.toast = None;
            }
        }

        // Dev helper: ZENITH_SHOT=/path.png makes the app save a frame of its own window and quit.
        // Dev helper: ZENITH_DEMO="cmd" types a command into the first tab one second after start.
        if let Ok(cmd) = std::env::var("ZENITH_DEMO") {
            if !self.demo_sent && t > 1.0 {
                self.demo_sent = true;
                self.write_active(format!("{cmd}\r").as_bytes());
            }
        }
        let shot_at: f64 = std::env::var("ZENITH_SHOT_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(2.4);
        if std::env::var_os("ZENITH_SHOT").is_some() && (shot_at..shot_at + 0.1).contains(&t) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

// ---------- Helpers ----------

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("VT323".to_owned(), egui::FontData::from_static(include_bytes!("../assets/VT323-Regular.ttf")));
    // Fallbacks for glyphs the pixel font lacks (block elements, arrows, emoji-like symbols)
    let mut family = vec!["VT323".to_owned()];
    for (name, path) in [("Menlo", "/System/Library/Fonts/Menlo.ttc"), ("Monaco", "/System/Library/Fonts/Monaco.ttf")] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(name.to_owned(), egui::FontData::from_owned(bytes));
            family.push(name.to_owned());
            fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, name.to_owned());
        }
    }
    family.extend(fonts.families[&egui::FontFamily::Monospace].iter().cloned());
    fonts.families.insert(egui::FontFamily::Name(theme::FONT.into()), family);
    fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "VT323".to_owned());
    ctx.set_fonts(fonts);
}

/// Labelled frame with fixed inner height.
fn framed(ui: &mut egui::Ui, label: &str, inner_h: f32, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(10.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), inner_h + 16.0), Sense::hover());
    frame(ui.painter(), rect, label);
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(10.0, 10.0))).layout(egui::Layout::top_down(egui::Align::Min)));
    body(&mut inner);
    ui.add_space(4.0);
}

fn key_bytes(key: Key, m: Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    let arrow = |c: u8| -> Vec<u8> { if app_cursor { vec![0x1b, b'O', c] } else { vec![0x1b, b'[', c] } };
    let b = match key {
        Key::Enter => vec![b'\r'],
        Key::Backspace => vec![0x7f],
        Key::Tab => vec![b'\t'],
        Key::Escape => vec![0x1b],
        Key::ArrowUp => arrow(b'A'),
        Key::ArrowDown => arrow(b'B'),
        Key::ArrowRight => arrow(b'C'),
        Key::ArrowLeft => arrow(b'D'),
        Key::Home => arrow(b'H'),
        Key::End => arrow(b'F'),
        Key::Delete => b"\x1b[3~".to_vec(),
        Key::PageUp => b"\x1b[5~".to_vec(),
        Key::PageDown => b"\x1b[6~".to_vec(),
        Key::Insert => b"\x1b[2~".to_vec(),
        Key::F1 => b"\x1bOP".to_vec(),
        Key::F2 => b"\x1bOQ".to_vec(),
        Key::F3 => b"\x1bOR".to_vec(),
        Key::F4 => b"\x1bOS".to_vec(),
        _ if m.ctrl => {
            let name = key.name();
            let mut chars = name.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            match c {
                'A'..='Z' => vec![c as u8 - b'A' + 1],
                '[' => vec![0x1b],
                '\\' => vec![0x1c],
                ']' => vec![0x1d],
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(b)
}
