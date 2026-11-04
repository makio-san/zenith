//! Pixel-art Zenitsu: dozes with zZz when the machine is idle, wakes under load, and flinches with ⚡ on a strike.

use egui::{Align2, Color32, Pos2, Rect, Vec2};

use crate::theme;

const Y: Color32 = Color32::from_rgb(255, 210, 63); // fur
const O: Color32 = Color32::from_rgb(255, 122, 61); // ear tips, nose
const K: Color32 = Color32::from_rgb(13, 10, 7); // eyes
const W: Color32 = Color32::from_rgb(255, 244, 176); // forehead lightning mark

/// 28×24 body. `y` fur, `o` orange, `k` ink, `w` white, `.` empty.
const BODY: [&str; 24] = [
    "............................",
    "............................",
    "............................",
    "............................",
    ".....oy.......yo............",
    ".....yy.......yy............",
    ".....yyyyyyyyyy.............",
    "....yyyyyyyyyyyy............",
    "....yyyyywyyyyyy............",
    "....yyyyyywyyyyy............",
    "....yyyyywyyyyyy............",
    "....yyEEEyyEEEyy............",
    "....yyyyyyyyyyyy............",
    "....yyyyyooyyyyyyyyyyyyyy...",
    "....yyyyyyyyyyyyyyyyyyyyy...",
    "....yyyyyyyyyyyyyyyyyyyyy...",
    "....yyyyyyyyyyyyyyyyyyyyy...",
    "......yy......yy......yy....",
    "......yy......yy......yy....",
    "......yy......yy......yy....",
    "............................",
    "............................",
    "............................",
    "............................",
];

pub struct Cat;

impl Cat {
    /// `t` seconds, `load` 0..1, `strike` true for the flinch frames.
    pub fn paint(painter: &egui::Painter, rect: Rect, t: f64, load: f32, strike: bool) {
        let px = (rect.width() / 28.0).min(rect.height() / 24.0).floor().max(1.0);
        let origin = Pos2::new(rect.left() + (rect.width() - px * 28.0) / 2.0, rect.top() + (rect.height() - px * 24.0) / 2.0);
        let awake = strike || load > 0.45;
        let blink = !strike && awake && (t * 0.7).fract() < 0.06;
        let put = |x: i32, y: i32, c: Color32| {
            painter.rect_filled(Rect::from_min_size(Pos2::new(origin.x + x as f32 * px, origin.y + y as f32 * px), Vec2::splat(px)), 0.0, c);
        };
        for (y, row) in BODY.iter().enumerate() {
            for (x, ch) in row.bytes().enumerate() {
                let c = match ch {
                    b'y' => Y,
                    b'o' => O,
                    b'w' => if strike { Color32::WHITE } else { W },
                    b'E' => {
                        // eyes: closed line when asleep/blinking, open when awake, wide when struck
                        if !awake || blink { if y == 11 { K } else { Y } }
                        else if strike { K }
                        else if y == 11 { K } else { Y }
                    }
                    _ => continue,
                };
                put(x as i32, y as i32, c);
            }
        }
        if awake && !blink {
            // open eyes: two-pixel-tall pupils
            for (x0, x1) in [(6, 8), (11, 13)] {
                for x in x0..=x1 {
                    put(x, 10, K);
                    if strike { put(x, 12, K); }
                }
            }
        }
        // tail: four-frame wag, discrete steps like the reference's pixel animation
        let frame = ((t * 2.5) as usize) % 4;
        let dx = [0, 1, 0, -1][frame];
        for y in 9..=13 { put(24 + dx, y, Y); put(25 + dx, y, Y); }
        for y in 7..=8 { put(25 + dx, y, Y); put(26 + dx, y, Y); }
        put(26 + dx, 6, O);

        // zZz when dozing; ⚡ when struck
        let font = theme::mono(px * 4.5);
        if !awake {
            let phase = ((t * 1.2) as usize) % 3;
            let zz = ["z", "zZ", "zZz"][phase];
            painter.text(Pos2::new(origin.x + 18.0 * px, origin.y + 4.0 * px), Align2::LEFT_CENTER, zz, font, theme::FG2);
        } else if strike {
            painter.text(Pos2::new(origin.x + 18.0 * px, origin.y + 3.0 * px), Align2::LEFT_CENTER, "⚡", egui::FontId::proportional(px * 5.0), theme::ACC);
        }
    }
}
