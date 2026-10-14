//! `Shape`: SVG path data drawn with GSK paths, scaled from its view box to
//! the bounds like SVG's default `xMidYMid meet`. Strokes use round caps and
//! joins everywhere: icon sets draw dots as zero-length segments.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{graphene, gsk};

use native_shell_core::protocol::{Prop, PropValue};

use crate::component::{Component, NodeCx};
use crate::mount::{color, float};
use crate::node_view::{rgba, NodeView};

struct State {
    path: RefCell<Option<gsk::Path>>,
    fill: Cell<Option<u32>>,
    stroke: Cell<Option<u32>>,
    stroke_width: Cell<f32>,
    view_box: Cell<Option<graphene::Rect>>,
    rule: Cell<gsk::FillRule>,
}

pub struct Shape {
    state: Rc<State>,
}

impl Shape {
    pub fn new(cx: &NodeCx) -> Shape {
        let state = Rc::new(State {
            path: RefCell::new(None),
            fill: Cell::new(None),
            stroke: Cell::new(None),
            stroke_width: Cell::new(1.0),
            view_box: Cell::new(None),
            rule: Cell::new(gsk::FillRule::Winding),
        });
        let s = state.clone();
        cx.view.set_draw(Some(Box::new(move |view, snapshot| {
            draw(&s, view, snapshot)
        })));
        Shape { state }
    }
}

/// Parses SVG path data (M L H V C S Q T A Z, absolute and relative, implicit
/// repeats, compact numbers like `1.5.5`). `None` when it is malformed or empty.
pub fn parse_path(d: &str) -> Option<gsk::Path> {
    gsk::Path::parse(d).ok().filter(|p| !p.is_empty())
}

/// The transform from view-box units to a `w`×`h` box: uniform scale,
/// centered (`xMidYMid meet`). Returns `(dx, dy, scale)`.
pub fn fit(view_box: graphene::Rect, w: f32, h: f32) -> (f32, f32, f32) {
    let (bw, bh) = (view_box.width(), view_box.height());
    if bw <= 0.0 || bh <= 0.0 || w <= 0.0 || h <= 0.0 {
        return (-view_box.x(), -view_box.y(), 1.0);
    }
    let s = (w / bw).min(h / bh);
    (
        (w - bw * s) / 2.0 - view_box.x() * s,
        (h - bh * s) / 2.0 - view_box.y() * s,
        s,
    )
}

impl Component for Shape {
    fn set_prop(&mut self, cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        let s = &self.state;
        match prop {
            Prop::PathData => {
                s.path.replace(match value {
                    PropValue::Str(d) => parse_path(d),
                    _ => None,
                });
            }
            Prop::FillColor => s.fill.set(color(value)),
            Prop::StrokeColor => s.stroke.set(color(value)),
            Prop::StrokeWidth => s.stroke_width.set(float(value).map_or(1.0, |w| w.max(0.0))),
            Prop::FillRule => s.rule.set(match value {
                PropValue::I32(1) => gsk::FillRule::EvenOdd,
                _ => gsk::FillRule::Winding,
            }),
            Prop::ViewBox => s.view_box.set(match value {
                PropValue::F32Array(a)
                    if a.len() == 4
                        && a[2] > 0.0
                        && a[3] > 0.0
                        && a.iter().all(|v| v.is_finite()) =>
                {
                    Some(graphene::Rect::new(a[0], a[1], a[2], a[3]))
                }
                _ => None,
            }),
            _ => return false,
        }
        cx.view.queue_draw();
        true
    }
}

fn draw(s: &State, view: &NodeView, snapshot: &gtk::Snapshot) {
    let path = s.path.borrow();
    let Some(path) = path.as_ref() else { return };
    let (fill, stroke) = (
        s.fill.get(),
        s.stroke.get().filter(|_| s.stroke_width.get() > 0.0),
    );
    if fill.is_none() && stroke.is_none() {
        return;
    }
    let Some(view_box) = s.view_box.get().or_else(|| path.bounds()) else {
        return;
    };
    let (w, h) = view.paint_size();
    let (dx, dy, scale) = fit(view_box, w, h);
    snapshot.save();
    snapshot.translate(&graphene::Point::new(dx, dy));
    snapshot.scale(scale, scale);
    if let Some(c) = fill {
        snapshot.append_fill(path, s.rule.get(), &rgba(c));
    }
    if let Some(c) = stroke {
        // In view-box units: the scale above applies to the width too.
        let st = gsk::Stroke::new(s.stroke_width.get());
        st.set_line_cap(gsk::LineCap::Round);
        st.set_line_join(gsk::LineJoin::Round);
        snapshot.append_stroke(path, &st, &rgba(c));
    }
    snapshot.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_boxes_fit_centered() {
        let vb = graphene::Rect::new(0.0, 0.0, 24.0, 24.0);
        assert_eq!(fit(vb, 48.0, 48.0), (0.0, 0.0, 2.0));
        // Wider box: centered horizontally.
        assert_eq!(fit(vb, 48.0, 24.0), (12.0, 0.0, 1.0));
        // A view box offset from the origin moves back.
        assert_eq!(
            fit(graphene::Rect::new(10.0, 10.0, 10.0, 10.0), 10.0, 10.0),
            (-10.0, -10.0, 1.0)
        );
    }
}
