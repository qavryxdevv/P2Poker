//! PokerTH's line icons, from PokerTH's own SVG files.
//!
//! The QML client shows its icons as SVG images colourised by a `MultiEffect`.
//! egui draws no SVG, so the few paths the table needs are filled here: the
//! path is flattened into straight edges, filled with SVG's nonzero rule at
//! sixteen samples a pixel, at the pixel size the icon is shown, and the white
//! coverage is kept as a texture that the painter tints -- the colourisation.
//!
//! The files are PokerTH's (`src/gui/qt6-qml/resources`, commit `944e8b83`;
//! Material Design icons), unchanged under `assets/pokerth/icons`. The reader
//! knows the commands those files use: `M L H V C S Q T Z`, absolute and
//! relative. An arc (`A`) is taken as a straight line to its end, which none
//! of them has.

use eframe::egui::{self, pos2, Color32, ColorImage, Painter, Pos2, Rect, TextureHandle, TextureOptions};

/// An icon of the table window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    /// `doorExit.svg`: the top bar's *Leave Game*.
    Door,
    /// `gameChat.svg`: the chat toggle.
    Chat,
    /// `gameLog.svg`: the log toggle.
    Log,
    /// `trophy.svg`: the ranking.
    Trophy,
    /// `settings.svg`: the settings.
    Settings,
    /// `send.svg`: the chat's send button.
    Send,
}

impl Icon {
    pub const ALL: [Icon; 6] = [Icon::Door, Icon::Chat, Icon::Log, Icon::Trophy, Icon::Settings, Icon::Send];

    fn svg(self) -> &'static str {
        match self {
            Icon::Door => include_str!("../../../assets/pokerth/icons/doorExit.svg"),
            Icon::Chat => include_str!("../../../assets/pokerth/icons/gameChat.svg"),
            Icon::Log => include_str!("../../../assets/pokerth/icons/gameLog.svg"),
            Icon::Trophy => include_str!("../../../assets/pokerth/icons/trophy.svg"),
            Icon::Settings => include_str!("../../../assets/pokerth/icons/settings.svg"),
            Icon::Send => include_str!("../../../assets/pokerth/icons/send.svg"),
        }
    }
}

/// Draw `which` into `rect`, tinted `colour`.
pub fn icon(p: &Painter, rect: Rect, which: Icon, colour: Color32) {
    let ctx = p.ctx();
    let ppp = ctx.pixels_per_point();
    let px = (rect.width().min(rect.height()) * ppp).round().clamp(4.0, 512.0) as usize;
    // On the pixel grid, so a texel is a pixel.
    let side = px as f32 / ppp;
    let centre = rect.center();
    let min = pos2(((centre.x - side / 2.0) * ppp).round() / ppp, ((centre.y - side / 2.0) * ppp).round() / ppp);
    let target = Rect::from_min_size(min, egui::vec2(side, side));
    let texture = texture(ctx, which, px);
    p.image(texture.id(), target, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), colour);
}

fn texture(ctx: &egui::Context, which: Icon, px: usize) -> TextureHandle {
    let id = egui::Id::new(("pokerth-icon", which, px));
    if let Some(cached) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return cached;
    }
    let coverage = rasterise(which.svg(), px);
    let rgba: Vec<u8> = coverage.iter().flat_map(|&a| [255, 255, 255, a]).collect();
    let image = ColorImage::from_rgba_unmultiplied([px, px], &rgba);
    let handle = ctx.load_texture(format!("pokerth-icon-{which:?}-{px}"), image, TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, handle.clone()));
    handle
}

/// The icon's coverage at `px` by `px`, row by row, 0 to 255.
pub fn rasterise(svg: &str, px: usize) -> Vec<u8> {
    let (vx, vy, vw, vh) = view_box(svg).unwrap_or((0.0, 0.0, 24.0, 24.0));
    let scale = px as f32 / vw.max(vh);
    // `xMidYMid meet`: the view box centred in the square.
    let ox = (px as f32 - vw * scale) / 2.0;
    let oy = (px as f32 - vh * scale) / 2.0;
    let mut edges: Vec<(Pos2, Pos2)> = Vec::new();
    for d in path_data(svg) {
        for ring in flatten(d) {
            let at = |q: Pos2| pos2(ox + (q.x - vx) * scale, oy + (q.y - vy) * scale);
            for i in 0..ring.len() {
                let a = at(ring[i]);
                let b = at(ring[(i + 1) % ring.len()]);
                if a.y != b.y {
                    edges.push((a, b));
                }
            }
        }
    }
    fill_nonzero(&edges, px)
}

/// Sixteen samples a pixel, filled by the nonzero winding rule.
fn fill_nonzero(edges: &[(Pos2, Pos2)], px: usize) -> Vec<u8> {
    const SS: usize = 4;
    let mut hits = vec![0u8; px * px];
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for sy in 0..px * SS {
        let y = (sy as f32 + 0.5) / SS as f32;
        crossings.clear();
        for (a, b) in edges {
            let (up, lo, hi) = if a.y < b.y { (1, a, b) } else { (-1, b, a) };
            if lo.y <= y && y < hi.y {
                let t = (y - lo.y) / (hi.y - lo.y);
                crossings.push((lo.x + (hi.x - lo.x) * t, up));
            }
        }
        crossings.sort_by(|l, r| l.0.total_cmp(&r.0));
        let row = sy / SS;
        let mut winding = 0;
        let mut next = 0;
        for sx in 0..px * SS {
            let x = (sx as f32 + 0.5) / SS as f32;
            while next < crossings.len() && crossings[next].0 <= x {
                winding += crossings[next].1;
                next += 1;
            }
            if winding != 0 {
                hits[row * px + sx / SS] += 1;
            }
        }
    }
    let full = (SS * SS) as u32;
    hits.iter().map(|&h| ((h as u32 * 255 + full / 2) / full) as u8).collect()
}

fn attribute<'a>(tag_text: &'a str, name: &str) -> Vec<&'a str> {
    let needle = format!(" {name}=\"");
    let mut out = Vec::new();
    let mut rest = tag_text;
    while let Some(i) = rest.find(&needle) {
        let after = &rest[i + needle.len()..];
        match after.find('"') {
            Some(end) => {
                out.push(&after[..end]);
                rest = &after[end..];
            }
            None => break,
        }
    }
    out
}

fn view_box(svg: &str) -> Option<(f32, f32, f32, f32)> {
    let v = attribute(svg, "viewBox").into_iter().next()?;
    let n: Vec<f32> = v.split([' ', ',']).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
    (n.len() == 4 && n[2] > 0.0 && n[3] > 0.0).then(|| (n[0], n[1], n[2], n[3]))
}

fn path_data(svg: &str) -> Vec<&str> {
    // Only the `d` of `<path` elements.
    svg.split("<path").skip(1).flat_map(|chunk| attribute(chunk, "d").into_iter().take(1)).collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Command(char),
    Number(f32),
}

fn tokens(d: &str) -> Vec<Token> {
    let b = d.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            out.push(Token::Command(c));
            i += 1;
        } else if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() {
            let start = i;
            let mut dot = c == '.';
            let mut exp = false;
            i += 1;
            while i < b.len() {
                let ch = b[i] as char;
                if ch.is_ascii_digit() {
                    i += 1;
                } else if ch == '.' && !dot && !exp {
                    dot = true;
                    i += 1;
                } else if (ch == 'e' || ch == 'E') && !exp {
                    exp = true;
                    i += 1;
                    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            if let Ok(v) = d[start..i].parse::<f32>() {
                out.push(Token::Number(v));
            }
        } else {
            i += 1;
        }
    }
    out
}

/// The path as closed rings of points.
fn flatten(d: &str) -> Vec<Vec<Pos2>> {
    const STEPS: usize = 10;
    let toks = tokens(d);
    let mut rings: Vec<Vec<Pos2>> = Vec::new();
    let mut ring: Vec<Pos2> = Vec::new();
    let mut cur = pos2(0.0, 0.0);
    let mut start = cur;
    let mut cubic_ctrl: Option<Pos2> = None;
    let mut quad_ctrl: Option<Pos2> = None;
    let mut command = 'M';
    let mut i = 0;

    let finish = |ring: &mut Vec<Pos2>, rings: &mut Vec<Vec<Pos2>>| {
        if ring.len() >= 3 {
            rings.push(std::mem::take(ring));
        } else {
            ring.clear();
        }
    };

    while i < toks.len() {
        if let Token::Command(c) = toks[i] {
            command = c;
            i += 1;
            if c == 'Z' || c == 'z' {
                cur = start;
                finish(&mut ring, &mut rings);
                cubic_ctrl = None;
                quad_ctrl = None;
                continue;
            }
        }
        let count = match command.to_ascii_uppercase() {
            'M' | 'L' | 'T' => 2,
            'H' | 'V' => 1,
            'S' | 'Q' => 4,
            'C' => 6,
            'A' => 7,
            _ => {
                i += 1;
                continue;
            }
        };
        let mut n = [0.0f32; 7];
        let mut got = 0;
        while got < count && i < toks.len() {
            match toks[i] {
                Token::Number(v) => {
                    n[got] = v;
                    got += 1;
                    i += 1;
                }
                Token::Command(_) => break,
            }
        }
        if got < count {
            continue;
        }
        let rel = command.is_ascii_lowercase();
        let abs = |x: f32, y: f32, cur: Pos2| if rel { pos2(cur.x + x, cur.y + y) } else { pos2(x, y) };
        if ring.is_empty() && !command.eq_ignore_ascii_case(&'M') {
            ring.push(cur);
        }
        let mut next_cubic = None;
        let mut next_quad = None;
        match command.to_ascii_uppercase() {
            'M' => {
                finish(&mut ring, &mut rings);
                cur = abs(n[0], n[1], cur);
                start = cur;
                ring.push(cur);
                // Pairs after a move are lines.
                command = if rel { 'l' } else { 'L' };
            }
            'L' => {
                cur = abs(n[0], n[1], cur);
                ring.push(cur);
            }
            'H' => {
                cur = pos2(if rel { cur.x + n[0] } else { n[0] }, cur.y);
                ring.push(cur);
            }
            'V' => {
                cur = pos2(cur.x, if rel { cur.y + n[0] } else { n[0] });
                ring.push(cur);
            }
            'C' | 'S' => {
                let (c1, c2, end) = if command.eq_ignore_ascii_case(&'C') {
                    (abs(n[0], n[1], cur), abs(n[2], n[3], cur), abs(n[4], n[5], cur))
                } else {
                    let c1 = cubic_ctrl.map(|c| pos2(2.0 * cur.x - c.x, 2.0 * cur.y - c.y)).unwrap_or(cur);
                    (c1, abs(n[0], n[1], cur), abs(n[2], n[3], cur))
                };
                for k in 1..=STEPS {
                    let t = k as f32 / STEPS as f32;
                    let u = 1.0 - t;
                    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    ring.push(pos2(
                        w[0] * cur.x + w[1] * c1.x + w[2] * c2.x + w[3] * end.x,
                        w[0] * cur.y + w[1] * c1.y + w[2] * c2.y + w[3] * end.y,
                    ));
                }
                next_cubic = Some(c2);
                cur = end;
            }
            'Q' | 'T' => {
                let (c, end) = if command.eq_ignore_ascii_case(&'Q') {
                    (abs(n[0], n[1], cur), abs(n[2], n[3], cur))
                } else {
                    let c = quad_ctrl.map(|c| pos2(2.0 * cur.x - c.x, 2.0 * cur.y - c.y)).unwrap_or(cur);
                    (c, abs(n[0], n[1], cur))
                };
                for k in 1..=STEPS {
                    let t = k as f32 / STEPS as f32;
                    let u = 1.0 - t;
                    ring.push(pos2(
                        u * u * cur.x + 2.0 * u * t * c.x + t * t * end.x,
                        u * u * cur.y + 2.0 * u * t * c.y + t * t * end.y,
                    ));
                }
                next_quad = Some(c);
                cur = end;
            }
            _ => {
                // 'A': a straight line to its end (none of the table's icons has one).
                cur = abs(n[5], n[6], cur);
                ring.push(cur);
            }
        }
        cubic_ctrl = next_cubic;
        quad_ctrl = next_quad;
    }
    finish(&mut ring, &mut rings);
    rings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(cov: &[u8], px: usize, x: f32, y: f32) -> u8 {
        let (cx, cy) = ((x * px as f32 / 24.0) as usize, (y * px as f32 / 24.0) as usize);
        cov[cy * px + cx]
    }

    #[test]
    fn numbers_run_together_as_svg_writes_them() {
        let t = tokens("m9.25 22l-.4-3.2q-.325-.125-.612-.3");
        let nums: Vec<f32> = t.iter().filter_map(|t| if let Token::Number(v) = t { Some(*v) } else { None }).collect();
        assert_eq!(nums, vec![9.25, 22.0, -0.4, -3.2, -0.325, -0.125, -0.612, -0.3]);
    }

    #[test]
    fn every_icon_reads_and_covers_something() {
        for icon in Icon::ALL {
            let cov = rasterise(icon.svg(), 48);
            let filled = cov.iter().filter(|&&a| a > 128).count();
            assert!(filled > 48 * 48 / 20, "{icon:?} covers {filled} pixels");
            assert!(filled < 48 * 48 * 3 / 4, "{icon:?} covers {filled} pixels");
        }
    }

    #[test]
    fn the_gear_has_teeth_and_a_hole_and_a_hub() {
        let px = 96;
        let cov = rasterise(Icon::Settings.svg(), px);
        // The top tooth is solid at the top edge, the gap beside it is empty.
        assert!(at(&cov, px, 12.0, 2.6) > 200);
        assert!(at(&cov, px, 6.0, 3.0) < 30);
        // The hub is filled, the ring between the hub and the outline is not.
        assert!(at(&cov, px, 11.35, 12.0) > 200);
        assert!(at(&cov, px, 11.35, 16.3) < 30);
        assert!(at(&cov, px, 15.6, 12.0) < 30);
    }

    #[test]
    fn the_door_keeps_its_open_leaf() {
        let px = 96;
        let cov = rasterise(Icon::Door.svg(), px);
        // The floor line is filled, the inside of the open leaf is not.
        assert!(at(&cov, px, 12.0, 20.0) > 200);
        assert!(at(&cov, px, 15.5, 12.0) < 30);
    }
}
