//! Stretched shapes: a path that fills its laid-out frame.
//!
//! Renderers fit a shape's view box into its frame uniformly (an icon keeps
//! its proportions). A chart's line has to span the plot whatever width the
//! layout gives it, which only layout knows, so for a shape described with
//! `stretch` the tree rewrites the path into frame coordinates, scaling x and
//! y apart, and sends it with a view box equal to the frame. Strokes keep
//! their width in points, as they would not if the renderer scaled the path.

use native_shell_core::protocol::{Prop, PropValue};

/// `path` in view-box units, rewritten into a `width` × `height` frame.
/// Arcs scale their radii per axis, which is exact for unrotated arcs (the
/// only kind a stretched chart draws). `None` when the path does not parse
/// or the view box is empty.
pub(crate) fn stretch_path(path: &str, view_box: [f32; 4], width: f32, height: f32) -> Option<String> {
    let [vx, vy, vw, vh] = view_box;
    if !(vw > 0.0 && vh > 0.0) {
        return None;
    }
    let (sx, sy) = (width / vw, height / vh);
    let mut out = String::with_capacity(path.len() + path.len() / 4);
    let mut tokens = Tokens { s: path.as_bytes(), i: 0, held: None };
    let mut cmd = None;
    while let Some(tok) = tokens.next()? {
        let c = match tok {
            Token::Cmd(c) => {
                out.push(c as char);
                if c == b'Z' || c == b'z' {
                    cmd = None;
                    continue;
                }
                c
            }
            // Repeated arguments after a command reuse it (an implicit `L` after `M`).
            Token::Num(n) => {
                let c = cmd?;
                tokens.back(n);
                out.push(' ');
                c
            }
        };
        let relative = c.is_ascii_lowercase();
        // Absolute coordinates move with the view box origin; relative ones only scale.
        let x = |v: f64| if relative { v * sx as f64 } else { (v - vx as f64) * sx as f64 };
        let y = |v: f64| if relative { v * sy as f64 } else { (v - vy as f64) * sy as f64 };
        let args: Vec<f64> = match c.to_ascii_uppercase() {
            b'M' | b'L' | b'T' => {
                let [a, b] = tokens.nums()?;
                vec![x(a), y(b)]
            }
            b'H' => vec![x(tokens.nums::<1>()?[0])],
            b'V' => vec![y(tokens.nums::<1>()?[0])],
            b'C' => {
                let [a, b, c2, d, e, f] = tokens.nums()?;
                vec![x(a), y(b), x(c2), y(d), x(e), y(f)]
            }
            b'S' | b'Q' => {
                let [a, b, c2, d] = tokens.nums()?;
                vec![x(a), y(b), x(c2), y(d)]
            }
            b'A' => {
                // Flags are single digits and may run into the next number (`a1 1 0 012 2`).
                let [rx, ry, rot] = tokens.nums()?;
                let (large, sweep) = (tokens.flag()?, tokens.flag()?);
                let [ex, ey] = tokens.nums()?;
                vec![rx * sx as f64, ry * sy as f64, rot, large, sweep, x(ex), y(ey)]
            }
            _ => return None,
        };
        for (i, v) in args.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            write_num(&mut out, *v);
        }
        cmd = Some(match c {
            b'M' => b'L',
            b'm' => b'l',
            c => c,
        });
    }
    Some(out)
}

/// The props a stretched shape gets from layout: its path in frame units and a
/// view box equal to the frame.
pub(crate) fn stretched_props(src: &[(Prop, PropValue)], width: f32, height: f32) -> Option<(PropValue, PropValue)> {
    let path = src.iter().find_map(|(p, v)| match (p, v) {
        (Prop::PathData, PropValue::Str(s)) => Some(s.as_str()),
        _ => None,
    })?;
    let view_box = src
        .iter()
        .find_map(|(p, v)| match (p, v) {
            (Prop::ViewBox, PropValue::F32Array(a)) if a.len() == 4 => Some([a[0], a[1], a[2], a[3]]),
            _ => None,
        })
        .unwrap_or([0.0, 0.0, 24.0, 24.0]);
    let path = stretch_path(path, view_box, width, height)?;
    Some((
        PropValue::Str(path),
        PropValue::F32Array(vec![0.0, 0.0, width.max(f32::MIN_POSITIVE), height.max(f32::MIN_POSITIVE)]),
    ))
}

/// Two decimals, without trailing zeros: a hundredth of a point is finer than any screen.
fn write_num(out: &mut String, v: f64) {
    let r = (v * 100.0).round() / 100.0;
    let r = if r == 0.0 { 0.0 } else { r };
    out.push_str(&r.to_string());
}

enum Token {
    Cmd(u8),
    Num(f64),
}

struct Tokens<'a> {
    s: &'a [u8],
    i: usize,
    held: Option<f64>,
}

impl Tokens<'_> {
    /// The next command letter or number; `Ok(None)` at the end, `None` (outer) on bad input.
    fn next(&mut self) -> Option<Option<Token>> {
        self.skip();
        let Some(&c) = self.s.get(self.i) else {
            return Some(None);
        };
        if c.is_ascii_alphabetic() {
            self.i += 1;
            return b"MmLlHhVvCcSsQqTtAaZz".contains(&c).then_some(Some(Token::Cmd(c)));
        }
        self.number().map(|n| Some(Token::Num(n)))
    }

    /// Puts back a number `next` read: it is the first argument of the repeated command.
    fn back(&mut self, n: f64) {
        self.held = Some(n);
    }

    fn nums<const N: usize>(&mut self) -> Option<[f64; N]> {
        let mut out = [0.0; N];
        for v in &mut out {
            *v = match self.held.take() {
                Some(n) => n,
                None => {
                    self.skip();
                    self.number()?
                }
            };
        }
        Some(out)
    }

    fn flag(&mut self) -> Option<f64> {
        if let Some(n) = self.held.take() {
            return (n == 0.0 || n == 1.0).then_some(n);
        }
        self.skip();
        let c = *self.s.get(self.i)?;
        self.i += 1;
        match c {
            b'0' => Some(0.0),
            b'1' => Some(1.0),
            _ => None,
        }
    }

    fn skip(&mut self) {
        while self.s.get(self.i).is_some_and(|c| matches!(c, b' ' | b',' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn number(&mut self) -> Option<f64> {
        let start = self.i;
        let s = self.s;
        let mut j = self.i;
        if matches!(s.get(j), Some(b'-' | b'+')) {
            j += 1;
        }
        let mut dot = false;
        while let Some(&c) = s.get(j) {
            if c.is_ascii_digit() {
                j += 1;
            } else if c == b'.' && !dot {
                dot = true;
                j += 1;
            } else {
                break;
            }
        }
        if matches!(s.get(j), Some(b'e' | b'E')) {
            let mut k = j + 1;
            if matches!(s.get(k), Some(b'-' | b'+')) {
                k += 1;
            }
            if s.get(k).is_some_and(u8::is_ascii_digit) {
                while s.get(k).is_some_and(u8::is_ascii_digit) {
                    k += 1;
                }
                j = k;
            }
        }
        let text = std::str::from_utf8(&s[start..j]).ok()?;
        let v: f64 = text.parse().ok()?;
        self.i = j;
        v.is_finite().then_some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_paths_scale_each_axis_apart() {
        // A 2 × 100 view box drawn 300 × 150.
        let p = stretch_path("M0.5 100L1.5 0", [0.0, 0.0, 2.0, 100.0], 300.0, 150.0).unwrap();
        assert_eq!(p, "M75 150L225 0");
        let p = stretch_path("M0 0H2V100Z", [0.0, 0.0, 2.0, 100.0], 10.0, 20.0).unwrap();
        assert_eq!(p, "M0 0H10V20Z");
        let p = stretch_path("M0 0C1 2 3 4 5 6", [0.0, 0.0, 10.0, 10.0], 20.0, 5.0).unwrap();
        assert_eq!(p, "M0 0C2 1 6 2 10 3");
    }

    #[test]
    fn view_box_origin_moves_and_relative_commands_only_scale() {
        let p = stretch_path("M-1 -1l2 2", [-1.0, -1.0, 2.0, 2.0], 4.0, 8.0).unwrap();
        assert_eq!(p, "M0 0l4 8");
    }

    #[test]
    fn implicit_repeats_compact_numbers_and_arcs() {
        // After M, extra pairs are line-tos; `-` separates numbers.
        let p = stretch_path("M0,0 1,1-1-1", [0.0, 0.0, 1.0, 1.0], 10.0, 20.0).unwrap();
        assert_eq!(p, "M0 0 10 20 -10 -20");
        let p = stretch_path("M0 0A1 1 0 0 1 2 0", [0.0, 0.0, 2.0, 2.0], 4.0, 2.0).unwrap();
        assert_eq!(p, "M0 0A2 1 0 0 1 4 0");
        assert_eq!(stretch_path("M1e1 0", [0.0, 0.0, 10.0, 1.0], 1.0, 1.0).unwrap(), "M1 0");
        // Compact arc flags, and a relative arc repeated without its letter.
        let p = stretch_path("M0 0a1 1 0 012 0 1 1 0 0 0 2 0", [0.0, 0.0, 1.0, 1.0], 2.0, 3.0).unwrap();
        assert_eq!(p, "M0 0a2 3 0 0 1 4 0 2 3 0 0 0 4 0");
    }

    #[test]
    fn bad_input_is_refused() {
        assert_eq!(stretch_path("M0 0X1", [0.0, 0.0, 1.0, 1.0], 1.0, 1.0), None);
        assert_eq!(stretch_path("M0", [0.0, 0.0, 1.0, 1.0], 1.0, 1.0), None);
        assert_eq!(stretch_path("M0 0", [0.0, 0.0, 0.0, 1.0], 1.0, 1.0), None);
    }
}
