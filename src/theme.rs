//! Textmode theme: black background, sand/peach pixel text, one-pixel frames with small square knobs.

use alacritty_terminal::vte::ansi::{Color, NamedColor};
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};

pub const BG: Color32 = Color32::from_rgb(0, 0, 0);
pub const FG: Color32 = Color32::from_rgb(233, 180, 127); // sand / peach
pub const FG2: Color32 = Color32::from_rgb(185, 138, 92);
pub const DIM: Color32 = Color32::from_rgb(107, 82, 54);
pub const LINE: Color32 = Color32::from_rgb(90, 69, 48);
pub const ACC: Color32 = Color32::from_rgb(255, 210, 63); // lightning yellow
pub const HOT: Color32 = Color32::from_rgb(255, 122, 61);
pub const INK: Color32 = Color32::from_rgb(13, 10, 7);
pub const SELECTION: Color32 = Color32::from_rgb(70, 52, 30);

pub const FONT: &str = "VT323";

/// ANSI palette warmed to sit on the sand/black scheme.
const NAMED: [Color32; 16] = [
    Color32::from_rgb(30, 24, 18),
    Color32::from_rgb(255, 96, 70),
    Color32::from_rgb(154, 211, 106),
    Color32::from_rgb(255, 210, 63),
    Color32::from_rgb(120, 170, 255),
    Color32::from_rgb(240, 130, 200),
    Color32::from_rgb(120, 220, 200),
    Color32::from_rgb(233, 180, 127),
    Color32::from_rgb(107, 82, 54),
    Color32::from_rgb(255, 140, 110),
    Color32::from_rgb(200, 240, 150),
    Color32::from_rgb(255, 232, 140),
    Color32::from_rgb(165, 200, 255),
    Color32::from_rgb(250, 170, 220),
    Color32::from_rgb(170, 240, 225),
    Color32::from_rgb(255, 240, 215),
];

pub fn color(c: Color, bold: bool) -> Color32 {
    match c {
        Color::Named(n) => named(n, bold),
        Color::Spec(rgb) => Color32::from_rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(i) => indexed(i),
    }
}

fn named(n: NamedColor, bold: bool) -> Color32 {
    use NamedColor::*;
    let i = match n {
        Black => 0, Red => 1, Green => 2, Yellow => 3, Blue => 4, Magenta => 5, Cyan => 6, White => 7,
        BrightBlack => 8, BrightRed => 9, BrightGreen => 10, BrightYellow => 11, BrightBlue => 12,
        BrightMagenta => 13, BrightCyan => 14, BrightWhite => 15,
        Foreground => return if bold { NAMED[15] } else { FG },
        Background => return BG,
        Cursor => return FG,
        DimBlack | DimRed | DimGreen | DimYellow | DimBlue | DimMagenta | DimCyan | DimWhite | DimForeground => return FG2,
        BrightForeground => return NAMED[15],
    };
    let i = if bold && i < 8 { i + 8 } else { i };
    NAMED[i]
}

pub fn indexed(i: u8) -> Color32 {
    match i {
        0..=15 => NAMED[i as usize],
        16..=231 => {
            let i = i - 16;
            let (r, g, b) = (i / 36, (i % 36) / 6, i % 6);
            let f = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color32::from_rgb(f(r), f(g), f(b))
        }
        _ => {
            let v = 8 + (i - 232) * 10;
            Color32::from_rgb(v, (v as f32 * 0.8) as u8, (v as f32 * 0.6) as u8)
        }
    }
}

pub fn mono(px: f32) -> FontId {
    FontId::new(px, egui::FontFamily::Name(FONT.into()))
}

/// One-pixel frame with a label on the top edge and two small square knobs, like the reference UI.
pub fn frame(painter: &egui::Painter, rect: Rect, label: &str) {
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0f32, LINE));
    let k = 5.0;
    painter.rect_filled(Rect::from_center_size(Pos2::new(rect.right() - 14.0, rect.top()), Vec2::splat(k)), 0.0, LINE);
    painter.rect_filled(Rect::from_center_size(Pos2::new(rect.left() + 14.0, rect.bottom()), Vec2::splat(k)), 0.0, LINE);
    if !label.is_empty() {
        let font = mono(18.0);
        let text = format!(" {label} ");
        let galley = painter.layout_no_wrap(text.clone(), font.clone(), FG2);
        let pos = Pos2::new(rect.left() + 10.0, rect.top() - galley.size().y / 2.0);
        painter.rect_filled(Rect::from_min_size(pos, galley.size() + Vec2::new(30.0, 0.0)), 0.0, BG);
        painter.galley(pos, galley, FG2);
        let tail_x = pos.x + painter.layout_no_wrap(text, font, FG2).size().x;
        painter.text(Pos2::new(tail_x, rect.top()), Align2::LEFT_CENTER, "■─■", mono(18.0), LINE);
    }
}

/// `[K]Label` chip. Active chips are filled with the text colour, like a selected key on the reference site.
pub fn chip(ui: &mut egui::Ui, key: &str, label: &str, active: bool) -> egui::Response {
    let font = mono(19.0);
    let text = format!("[{key}]{label}");
    let galley = ui.painter().layout_no_wrap(text.clone(), font.clone(), FG);
    let size = galley.size() + Vec2::new(14.0, 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let p = ui.painter();
    if active {
        p.rect_filled(rect, 0.0, FG);
    } else {
        p.rect_stroke(rect, 0.0, Stroke::new(1.0f32, if resp.hovered() { FG2 } else { LINE }));
    }
    let (bracket, keyc, labelc) = if active { (INK, Color32::BLACK, INK) } else { (FG2, ACC, FG) };
    let mut x = rect.left() + 7.0;
    let y = rect.center().y;
    for (s, c) in [("[", bracket), (key, keyc), ("]", bracket), (label, labelc)] {
        let r = p.text(Pos2::new(x, y), Align2::LEFT_CENTER, s, font.clone(), c);
        x = r.right();
    }
    resp
}

/// `key ········ value` row with a dotted leader.
pub fn leader_row(ui: &mut egui::Ui, key: &str, value: &str, value_color: Color32, selected: bool) {
    let font = mono(19.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 21.0), egui::Sense::hover());
    let p = ui.painter();
    let mut x = rect.left();
    if selected {
        let r = p.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, ">", font.clone(), ACC);
        x = r.right() + 4.0;
    }
    let kr = p.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, key, font.clone(), FG2);
    let vr = p.text(Pos2::new(rect.right(), rect.center().y), Align2::RIGHT_CENTER, value, font, value_color);
    let mut dx = kr.right() + 6.0;
    let y = rect.center().y + 5.0;
    while dx < vr.left() - 6.0 {
        p.rect_filled(Rect::from_min_size(Pos2::new(dx, y), Vec2::splat(1.5)), 0.0, DIM);
        dx += 4.0;
    }
}

/// Dithered "skyline" history bars, like a textmode city.
pub fn skyline(ui: &mut egui::Ui, data: &std::collections::VecDeque<f32>, height: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 0.0, Color32::from_rgb(5, 4, 3));
    p.rect_stroke(r, 0.0, Stroke::new(1.0f32, LINE));
    let n = 30usize;
    let w = (r.width() - 8.0) / n as f32;
    let skip = data.len().saturating_sub(n);
    for (i, v) in data.iter().skip(skip).enumerate() {
        let h = ((v / 100.0).clamp(0.0, 1.0) * (r.height() - 8.0)).max(2.0);
        let x = r.left() + 4.0 + i as f32 * w;
        let bar = Rect::from_min_max(Pos2::new(x, r.bottom() - 4.0 - h), Pos2::new(x + w - 2.0, r.bottom() - 4.0));
        p.rect_filled(bar, 0.0, FG2);
        // dither: punch dark lines through the bar, offset per column
        let mut y = bar.top() + if i % 2 == 0 { 1.0 } else { 2.5 };
        while y < bar.bottom() {
            p.hline(bar.x_range(), y, Stroke::new(1.0f32, Color32::from_rgb(27, 20, 12)));
            y += 3.0;
        }
        if i % 2 == 1 {
            let mut xx = bar.left() + 1.0;
            while xx < bar.right() {
                p.vline(xx, bar.y_range(), Stroke::new(1.0f32, Color32::from_rgb(27, 20, 12)));
                xx += 3.0;
            }
        }
    }
}
