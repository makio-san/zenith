//! Pixel-art Zenitsu: a bright yellow tabby with light shading and a light outline.
//!
//! She is always awake and sitting up. Her left eye (the one on the viewer's right) is a normal green eye;
//! her other eye is half-closed and clouded by a cataract. Her tail has no tip: it was lost when she was a
//! kitten, so the stem ends bluntly.
//!
//! The sprite is built in code from shapes, with every head pixel placed together with its mirror image, so the
//! face can never end up lopsided. A shading pass lightens the top edges of the fur and darkens the bottom
//! edges, the outline is generated from the silhouette, and the whole tail (with its wag) stays inside the grid.
//! The tail wags faster under load, and she flinches with ⚡ on a strike.

use egui::{Align2, Color32, Pos2, Rect, Vec2};

use crate::theme;

pub const W: usize = 30;
pub const H: usize = 24;

const FUR: Color32 = Color32::from_rgb(255, 200, 48); // bright yellow
const HILITE: Color32 = Color32::from_rgb(255, 228, 112); // sunlit top edges
const SHADE: Color32 = Color32::from_rgb(235, 162, 28); // underside
const STRIPE: Color32 = Color32::from_rgb(208, 124, 18); // tabby stripes
const CREAM: Color32 = Color32::from_rgb(255, 243, 200); // muzzle, chest, paws
const PINK: Color32 = Color32::from_rgb(255, 140, 130); // nose, inner ears
const OUTLINE: Color32 = Color32::from_rgb(255, 244, 214); // light outline
const DARK: Color32 = Color32::from_rgb(42, 22, 8);
const IRIS: Color32 = Color32::from_rgb(88, 132, 28); // dark olive green
const WHITE: Color32 = Color32::from_rgb(232, 232, 222);
const CATARACT: Color32 = Color32::from_rgb(160, 180, 200); // milky blue-white cloud over the eye
const CATARACT_CORE: Color32 = Color32::from_rgb(92, 110, 130);

pub type Grid = [[Option<Color32>; W]; H];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Eyes {
    Blink,
    Open,
    Wide,
}

fn put(g: &mut Grid, x: i32, y: i32, c: Color32) {
    if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
        g[y as usize][x as usize] = Some(c);
    }
}

/// Set a pixel and its mirror image across the head's centre line (between columns 9 and 10).
fn sym(g: &mut Grid, x: i32, y: i32, c: Color32) {
    put(g, x, y, c);
    put(g, 19 - x, y, c);
}

fn span(g: &mut Grid, x0: i32, x1: i32, y: i32, c: Color32) {
    for x in x0..=x1 {
        put(g, x, y, c);
    }
}

fn span_sym(g: &mut Grid, x0: i32, x1: i32, y: i32, c: Color32) {
    for x in x0..=x1 {
        sym(g, x, y, c);
    }
}

/// Light the top edge of the plain fur and darken its underside (vertical lighting keeps the mirror symmetry).
fn shade(g: &mut Grid) {
    let src = *g;
    for y in 0..H {
        for x in 0..W {
            if src[y][x] != Some(FUR) {
                continue;
            }
            let above_empty = y == 0 || src[y - 1][x].is_none();
            let below_empty = y + 1 >= H || src[y + 1][x].is_none();
            if above_empty {
                g[y][x] = Some(HILITE);
            } else if below_empty {
                g[y][x] = Some(SHADE);
            }
        }
    }
}

/// Surround the silhouette with a one-pixel light outline.
fn outline(g: &mut Grid) {
    let src = *g;
    for y in 0..H {
        for x in 0..W {
            if src[y][x].is_some() {
                continue;
            }
            let touches = (x > 0 && src[y][x - 1].is_some())
                || (x + 1 < W && src[y][x + 1].is_some())
                || (y > 0 && src[y - 1][x].is_some())
                || (y + 1 < H && src[y + 1][x].is_some());
            if touches {
                g[y][x] = Some(OUTLINE);
            }
        }
    }
}

/// Build one frame. `tail_dx` (-1, 0 or 1) sways the upper half of the tail.
pub fn build(tail_dx: i32, eyes: Eyes) -> Grid {
    let mut g: Grid = [[None; W]; H];

    // ears
    span_sym(&mut g, 4, 5, 1, FUR);
    sym(&mut g, 4, 2, FUR);
    sym(&mut g, 5, 2, PINK);
    sym(&mut g, 6, 2, FUR);
    span_sym(&mut g, 3, 7, 3, FUR);

    // head
    for y in 4..=11 {
        span(&mut g, 3, 16, y, FUR);
    }
    span(&mut g, 4, 15, 12, FUR);
    sym(&mut g, 3, 9, STRIPE); // cheek stripes

    // muzzle
    span(&mut g, 8, 11, 10, CREAM);
    sym(&mut g, 8, 11, CREAM);
    sym(&mut g, 9, 11, PINK);
    span(&mut g, 8, 11, 12, CREAM);

    // body
    span(&mut g, 4, 15, 13, FUR);
    span(&mut g, 3, 16, 14, FUR);
    for y in 15..=20 {
        span(&mut g, 2, 17, y, FUR);
    }
    for y in 15..=19 {
        span(&mut g, 7, 12, y, CREAM); // chest
    }
    for y in [17, 19] {
        sym(&mut g, 3, y, STRIPE);
        sym(&mut g, 4, y, STRIPE);
    }
    span_sym(&mut g, 4, 6, 21, CREAM); // front paws
    span_sym(&mut g, 4, 6, 22, CREAM);

    // tail: a short stump. The tip is missing (lost when she was a kitten), so it rises only a little from the
    // floor and ends bluntly.
    span(&mut g, 17, 24, 21, FUR);
    span(&mut g, 17, 24, 22, FUR);
    put(&mut g, 20, 22, STRIPE);
    for y in 17..=20 {
        let s = if y <= 18 { tail_dx } else { 0 };
        let c = if y % 4 == 2 { STRIPE } else { FUR };
        span(&mut g, 22 + s, 24 + s, y, c);
    }

    shade(&mut g);

    // no eyebrows: she has none, so the fur above the eyes stays plain
    // Eyes. Her left eye (right side of the sprite, mirror of columns 5..=7) is normal; the other is squinted
    // with a cataract.
    let healthy = |g: &mut Grid, white: bool| {
        for y in 8..=9 {
            span(g, 12, 14, y, if white { WHITE } else { IRIS });
            put(g, 13, y, DARK);
        }
        if !white {
            put(g, 12, 8, WHITE); // catch-light
        }
    };
    match eyes {
        Eyes::Blink => span_sym(&mut g, 5, 7, 9, DARK),
        Eyes::Open => {
            healthy(&mut g, false);
            span(&mut g, 5, 7, 8, SHADE); // heavy lid, in fur colour so it does not read as a brow
            span(&mut g, 5, 7, 9, CATARACT);
            put(&mut g, 6, 9, CATARACT_CORE);
        }
        Eyes::Wide => {
            healthy(&mut g, true);
            for y in 8..=9 {
                span(&mut g, 5, 7, y, CATARACT);
            }
            put(&mut g, 6, 8, CATARACT_CORE);
            put(&mut g, 6, 9, CATARACT_CORE);
        }
    }

    outline(&mut g);

    // whiskers, drawn in the outline colour
    for y in [10, 12] {
        sym(&mut g, 0, y, OUTLINE);
        sym(&mut g, 1, y, OUTLINE);
    }
    g
}

pub struct Cat;

impl Cat {
    /// `t` in seconds, `load` 0..1, `strike` true for the flinch frames.
    pub fn paint(painter: &egui::Painter, rect: Rect, t: f64, load: f32, strike: bool) {
        let px = (rect.width() / W as f32).min(rect.height() / H as f32).floor().max(1.0);
        let origin = Pos2::new(
            (rect.left() + (rect.width() - px * W as f32) / 2.0).round(),
            (rect.top() + (rect.height() - px * H as f32) / 2.0).round(),
        );
        let blink = !strike && (t * 0.45).fract() < 0.05;
        let eyes = if strike {
            Eyes::Wide
        } else if blink {
            Eyes::Blink
        } else {
            Eyes::Open
        };
        let speed = 1.5 + 4.0 * load.clamp(0.0, 1.0) as f64;
        let dx = [0, 1, 0, -1][((t * speed) as usize) % 4];

        // soft warm glow behind her, brighter under load
        let centre = Pos2::new(origin.x + 11.0 * px, origin.y + 12.0 * px);
        let a = if strike { 0.18 } else { 0.08 + 0.06 * load.clamp(0.0, 1.0) };
        for k in 0..5 {
            painter.circle_filled(centre, px * (17.0 - 2.5 * k as f32), theme::ACC.gamma_multiply(a * (0.4 + 0.2 * k as f32)));
        }

        let grid = build(dx, eyes);
        for (y, row) in grid.iter().enumerate() {
            for (x, cell) in row.iter().enumerate() {
                if let Some(c) = cell {
                    let min = Pos2::new(origin.x + x as f32 * px, origin.y + y as f32 * px);
                    painter.rect_filled(Rect::from_min_size(min, Vec2::splat(px)), 0.0, *c);
                }
            }
        }

        if strike {
            let spot = Pos2::new(origin.x + 17.0 * px, origin.y + 3.0 * px);
            painter.text(spot, Align2::LEFT_CENTER, "⚡", egui::FontId::proportional(px * 5.0), theme::ACC);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Eyes; 3] = [Eyes::Blink, Eyes::Open, Eyes::Wide];

    #[test]
    fn face_and_body_are_mirror_symmetric_apart_from_the_eyes() {
        for dx in -1..=1 {
            for eyes in ALL {
                let g = build(dx, eyes);
                // rows 0..=19: below that the tail base (and its outline) starts on the right.
                // Rows 8 and 9 hold the eyes, which are deliberately different.
                for y in (0..=19).filter(|y| !(8..=9).contains(y)) {
                    for x in 0..=19 {
                        assert_eq!(g[y][x], g[y][19 - x], "asymmetric at ({x},{y}) eyes {eyes:?} dx {dx}");
                    }
                }
                for y in 8..=9 {
                    for x in (0..=19).filter(|x| !(5..=7).contains(x) && !(12..=14).contains(x)) {
                        assert_eq!(g[y][x], g[y][19 - x], "asymmetric outside the eyes at ({x},{y})");
                    }
                }
            }
        }
    }

    #[test]
    fn nothing_is_clipped_and_every_edge_has_an_outline() {
        for dx in -1..=1 {
            let g = build(dx, Eyes::Open);
            for y in 0..H {
                let fill_max = (0..W).rev().find(|&x| matches!(g[y][x], Some(c) if c != OUTLINE));
                if let Some(x) = fill_max {
                    assert!(x + 1 < W, "row {y} touches the right edge");
                    assert_eq!(g[y][x + 1], Some(OUTLINE), "row {y} has no outline right of the fur (dx {dx})");
                }
            }
            for x in 0..W {
                assert!(g[0][x].map_or(true, |c| c == OUTLINE), "top row must only hold outline");
                assert!(g[H - 1][x].map_or(true, |c| c == OUTLINE), "bottom row must only hold outline");
            }
        }
    }

    #[test]
    fn the_tail_is_a_short_stump_with_no_tip() {
        let g = build(0, Eyes::Open);
        let tail = (0..H)
            .flat_map(|y| (19..W).map(move |x| (x, y)))
            .filter(|&(x, y)| matches!(g[y][x], Some(c) if c != OUTLINE))
            .count();
        assert_eq!(tail, 12 + 12, "tail should be a floor base of 12 plus a 4-row stump of 3 columns");
        assert!((0..16).all(|y| (20..W).all(|x| g[y][x].is_none())), "nothing rises above the stump");
        let cream = (0..H).flat_map(|y| (19..W).map(move |x| (x, y))).filter(|&(x, y)| g[y][x] == Some(CREAM)).count();
        assert_eq!(cream, 0, "no cream tip on the tail");
    }

    #[test]
    fn she_has_no_eyebrows() {
        for eyes in ALL {
            let g = build(0, eyes);
            for x in (4..=8).chain(11..=15) {
                for y in 5..=7 {
                    assert!(
                        !matches!(g[y][x], Some(c) if c != FUR && c != HILITE && c != OUTLINE),
                        "unexpected mark above the eyes at ({x},{y})"
                    );
                }
                assert_ne!(g[7][x], Some(STRIPE), "no dark brow line directly over the eyes at x={x}");
            }
        }
    }

    #[test]
    fn one_eye_is_healthy_and_the_other_is_squinted_with_a_cataract() {
        let g = build(0, Eyes::Open);
        assert!((12..=14).any(|x| g[8][x] == Some(IRIS)), "healthy eye shows its iris");
        assert!((5..=7).all(|x| g[8][x] == Some(SHADE)), "squinted eye has a lid over its top row");
        assert!((5..=7).any(|x| g[9][x] == Some(CATARACT)), "cataract visible");
        assert!(!(12..=14).any(|x| g[9][x] == Some(CATARACT)), "the healthy eye has no cataract");
    }
}
