//! Taffy layout, measurement and frames.
//!
//! Each surface is one taffy root; each list row is another, laid out at the
//! list's width after the list itself, so a row's height never depends on rows
//! that are not rendered. Only roots touched by the commit are laid out again;
//! taffy's own cache keeps that cheap when one leaf changed.
//!
//! Frames are rounded on the device pixel grid by their absolute edges (as
//! taffy does for whole points), so adjacent views never open a hairline gap.

use std::collections::HashMap;
use std::rc::Rc;

use native_shell_core::desc::ElementKind;
use native_shell_core::host::{Measurer, TextMetrics, WidthMode};
use native_shell_core::protocol::{Mutation, Prop, PropValue, ViewType};
use taffy::prelude::*;

use crate::{Batch, Frame, Kind, Measure, SurfaceTag, Tree};

const ZERO: Frame = Frame {
    x: 0.0,
    y: 0.0,
    width: 0.0,
    height: 0.0,
};

impl Tree {
    fn scale(&self) -> f32 {
        if self.env.scale > 0.0 {
            self.env.scale
        } else {
            1.0
        }
    }

    pub(crate) fn layout(&mut self, b: &mut Batch, measurer: &dyn Measurer) {
        self.premeasure(b, measurer);
        for s in self.surfaces.clone() {
            if !self.layout_all && !b.dirty_roots.contains_key(&s) {
                continue;
            }
            let Some(Kind::Surface(tag)) = self.nodes.get(&s).map(|n| n.kind.clone()) else {
                continue;
            };
            let (w, h) = (self.env.width, self.env.height);
            let height = match tag {
                SurfaceTag::Alert => continue,
                SurfaceTag::Root | SurfaceTag::Page => AvailableSpace::Definite(h),
                SurfaceTag::Sheet | SurfaceTag::Dialog => AvailableSpace::MaxContent,
            };
            self.compute(
                s,
                Size {
                    width: AvailableSpace::Definite(w),
                    height,
                },
                measurer,
            );
            self.walk(b, s, measurer);
        }

        // Rows that changed while their surface did not: lay them out alone, outer rows first.
        let mut rows: Vec<(u32, u32)> = b
            .dirty_roots
            .keys()
            .filter_map(|id| {
                self.nodes
                    .get(id)
                    .filter(|n| n.kind == Kind::ListRow)
                    .map(|n| (n.depth, *id))
            })
            .collect();
        rows.sort_unstable();
        let mut lists = Vec::new();
        for (_, row) in rows {
            if !b.dirty_roots.contains_key(&row) {
                continue;
            }
            let Some(list) = self.nodes.get(&row).map(|n| n.parent) else {
                continue;
            };
            let width = self.list_of(list).map(|l| l.width).unwrap_or(0.0);
            self.layout_row(b, row, width, measurer);
            if !lists.contains(&list) {
                lists.push(list);
            }
        }
        for list in lists {
            self.update_offsets(b, list);
        }
    }

    /// Measures every text new to the cache in one `measure_text_batch` call, unconstrained.
    /// Most texts fit their line, and the layout then answers their constrained probes from
    /// this result; only texts that wrap are measured again at their width.
    fn premeasure(&mut self, b: &mut Batch, measurer: &dyn Measurer) {
        let mut texts = std::mem::take(&mut b.texts);
        if self.layout_all {
            texts.extend(
                self.nodes
                    .values()
                    .filter(|n| n.kind == Kind::Text)
                    .filter_map(|n| match self.taffy.get_node_context(n.taffy) {
                        Some(Measure::Text(json)) => Some(json.clone()),
                        _ => None,
                    }),
            );
        }
        let mut seen = std::collections::HashSet::new();
        texts.retain(|j| {
            !self.text_cache.contains_key(&max_content_key(j)) && seen.insert(j.clone())
        });
        if texts.is_empty() {
            return;
        }
        let requests: Vec<(&str, WidthMode)> =
            texts.iter().map(|j| (&**j, WidthMode::Undefined)).collect();
        let results = measurer.measure_text_batch(&requests);
        self.counters.batch_calls += 1;
        // A short answer from a misbehaving measurer only loses the prefetch.
        for (json, m) in texts.iter().zip(results) {
            self.counters.batched_items += 1;
            self.text_cache.insert(max_content_key(json), m);
        }
    }

    fn compute(&mut self, id: u32, avail: Size<AvailableSpace>, measurer: &dyn Measurer) {
        let Some(tnode) = self.nodes.get(&id).map(|n| n.taffy) else {
            return;
        };
        let scale = self.scale();
        let Tree {
            taffy,
            text_cache,
            counters,
            component_cache,
            ..
        } = self;
        let mut cache = TextCache {
            map: text_cache,
            counters,
            components: component_cache,
            scale,
        };
        let _ = taffy.compute_layout_with_measure(tnode, avail, |known, avail, _, ctx, _| {
            measure(known, avail, ctx, measurer, &mut cache)
        });
    }

    /// Pixel-rounds every node of a laid-out root, emits changed frames and layout props,
    /// then lays out the rows of the lists it contains.
    fn walk(&mut self, b: &mut Batch, root: u32, measurer: &dyn Measurer) {
        let scale = self.scale();
        let round = |v: f32| (v * scale).round() / scale;
        let mut lists = Vec::new();
        // (node, parent's unrounded origin, nearest native ancestor's rounded frame)
        let mut stack = vec![(root, 0.0f32, 0.0f32, ZERO)];
        while let Some((id, px, py, pabs)) = stack.pop() {
            let Some(n) = self.nodes.get_mut(&id) else {
                continue;
            };
            let Ok(l) = self.taffy.layout(n.taffy) else {
                continue;
            };
            let (x, y) = if id == root {
                (0.0, 0.0)
            } else {
                (px + l.location.x, py + l.location.y)
            };
            let (left, top) = (round(x), round(y));
            let mut abs = Frame {
                x: left,
                y: top,
                width: round(x + l.size.width) - left,
                height: round(y + l.size.height) - top,
            };
            if n.kind == Kind::Text {
                // Exactly the width it was measured at (see `text_width`); rounding its edges
                // could shave a pixel and make the renderer wrap it again. Height rounds like
                // any frame: it cannot change the lines.
                abs.width = text_width(l.size.width, scale);
            }
            let content = [
                l.content_size.width.max(l.size.width),
                l.content_size.height.max(l.size.height),
            ];
            let padding = [
                l.padding.top,
                l.padding.right,
                l.padding.bottom,
                l.padding.left,
            ];
            n.abs = abs;
            let mut restretch = None;
            let below = if let Some(t) = n.native {
                let rel = Frame {
                    x: round(abs.x - pabs.x),
                    y: round(abs.y - pabs.y),
                    width: abs.width,
                    height: abs.height,
                };
                if n.frame != Some(rel) {
                    let resized = n.frame.is_none_or(|f| f.width != rel.width || f.height != rel.height);
                    n.frame = Some(rel);
                    b.frames.push(Mutation::SetFrame {
                        node: id,
                        frame: rel,
                    });
                    if resized && matches!(n.src_kind, Some(ElementKind::Shape { stretch: true })) {
                        restretch = Some((rel.width, rel.height));
                    }
                }
                match t {
                    // A text field's padding insets its editable text natively.
                    ViewType::TextInput => {
                        let value = if padding.iter().all(|p| *p == 0.0) {
                            PropValue::Null
                        } else {
                            PropValue::F32Array(padding.to_vec())
                        };
                        self.set_layout_prop(b, id, Prop::ContentInsets, value);
                    }
                    ViewType::Scroll => {
                        let content = content.map(round).to_vec();
                        self.set_layout_prop(
                            b,
                            id,
                            Prop::ContentSize,
                            PropValue::F32Array(content),
                        );
                    }
                    ViewType::List => lists.push(id),
                    _ => {}
                }
                abs
            } else {
                pabs
            };
            if let Some((w, h)) = restretch {
                self.stretch_to(b, id, w, h);
            }
            let Some(n) = self.nodes.get(&id) else {
                continue;
            };
            if !matches!(n.kind, Kind::Text | Kind::List) {
                for c in n.children.iter().rev() {
                    stack.push((*c, x, y, below));
                }
            }
        }
        for list in lists {
            let width = self.nodes.get(&list).map(|n| n.abs.width).unwrap_or(0.0);
            if let Some(Some(ElementKind::List(spec))) = self.nodes.get(&list).map(|n| &n.src_kind)
            {
                if let Some(state) = self.lists.get_mut(&spec.id) {
                    state.width = width;
                }
            }
            let rows = self
                .nodes
                .get(&list)
                .map(|n| n.children.clone())
                .unwrap_or_default();
            for row in rows {
                self.layout_row(b, row, width, measurer);
            }
            self.update_offsets(b, list);
        }
    }

    fn layout_row(&mut self, b: &mut Batch, row: u32, width: f32, measurer: &dyn Measurer) {
        b.dirty_roots.remove(&row);
        let Some(n) = self.nodes.get(&row) else {
            return;
        };
        let (tnode, list) = (n.taffy, n.parent);
        let index = match n.src_kind {
            Some(ElementKind::ListRow(i)) => i as usize,
            _ => return,
        };
        if let Ok(style) = self.taffy.style(tnode) {
            if style.size.width != length(width) {
                let mut style = style.clone();
                style.size.width = length(width);
                let _ = self.taffy.set_style(tnode, style);
            }
        }
        self.compute(
            row,
            Size {
                width: AvailableSpace::Definite(width),
                height: AvailableSpace::MaxContent,
            },
            measurer,
        );
        self.walk(b, row, measurer);
        let height = self.nodes.get(&row).map(|n| n.abs.height).unwrap_or(0.0);
        if let Some(Some(ElementKind::List(spec))) = self.nodes.get(&list).map(|n| &n.src_kind) {
            if let Some(slot) = self
                .lists
                .get_mut(&spec.id)
                .and_then(|l| l.heights.get_mut(index))
            {
                *slot = Some(height);
            }
        }
    }

    /// `RowOffsets`: measured heights where known; otherwise the average of the measured
    /// rows, or the estimate before any was measured. Uniform lists use the first measured
    /// row's height for every row.
    fn update_offsets(&mut self, b: &mut Batch, list: u32) {
        let Some(Some(ElementKind::List(spec))) = self.nodes.get(&list).map(|n| &n.src_kind) else {
            return;
        };
        let Some(state) = self.lists.get_mut(&spec.id) else {
            return;
        };
        let (sum, measured) = state
            .heights
            .iter()
            .flatten()
            .fold((0.0f64, 0u32), |(s, c), h| (s + *h as f64, c + 1));
        let fallback = if measured > 0 {
            sum / measured as f64
        } else {
            state.estimated as f64
        };
        let uniform = state
            .heights
            .iter()
            .flatten()
            .next()
            .map_or(state.estimated as f64, |h| *h as f64);
        let mut offsets = Vec::with_capacity(state.count as usize + 1);
        // Summed in f64: f32 drifts by tens of points over tens of thousands of rows.
        let mut y = 0.0f64;
        offsets.push(0.0);
        for i in 0..state.count as usize {
            y += if state.uniform {
                uniform
            } else {
                state
                    .heights
                    .get(i)
                    .copied()
                    .flatten()
                    .map_or(fallback, f64::from)
            };
            offsets.push(y as f32);
        }
        state.offsets = offsets.clone();
        self.set_layout_prop(b, list, Prop::RowOffsets, PropValue::F32Array(offsets));
    }
}

struct TextCache<'a> {
    map: &'a mut HashMap<(Rc<str>, u8, u32), TextMetrics>,
    counters: &'a mut crate::Counters,
    components: &'a mut crate::ComponentCache,
    scale: f32,
}

/// A component's intrinsic size, cached per (name, props, width constraint).
fn component_size(
    name: &Rc<str>,
    props: &Rc<str>,
    mode: WidthMode,
    measurer: &dyn Measurer,
    cache: &mut TextCache,
) -> Option<(f32, f32)> {
    let (tag, width) = match mode {
        WidthMode::Undefined => (0u8, 0.0f32),
        WidthMode::AtMost(w) => (1, w),
        WidthMode::Exactly(w) => (2, w),
    };
    let key = (name.clone(), props.clone(), tag, width.to_bits());
    if let Some(size) = cache.components.get(&key) {
        return *size;
    }
    let size = measurer.measure_component(name, props, mode);
    cache.components.insert(key, size);
    size
}

/// Size of a Select or DatePicker the host measurer does not know.
const CONTROL_FALLBACK: (f32, f32) = (96.0, 36.0);

fn max_content_key(json: &Rc<str>) -> (Rc<str>, u8, u32) {
    (json.clone(), 0, 0f32.to_bits())
}

/// A text frame's width: its layout width snapped up to the device pixel grid. Text is
/// measured at exactly this width, so the renderer draws in the frame the same lines the
/// layout reserved (wrapping happens at pixel resolution).
pub(crate) fn text_width(w: f32, scale: f32) -> f32 {
    ((w.max(0.0) * scale) - 1e-3).ceil().max(0.0) / scale
}

/// Leaves measure through the platform: text by its attributed JSON, host components by
/// their props. Text is cached per exact width constraint.
fn measure(
    known: Size<Option<f32>>,
    avail: Size<AvailableSpace>,
    ctx: Option<&mut Measure>,
    measurer: &dyn Measurer,
    cache: &mut TextCache,
) -> Size<f32> {
    if let Size {
        width: Some(width),
        height: Some(height),
    } = known
    {
        return Size { width, height };
    }
    let or_known = |w: f32, h: f32| Size {
        width: known.width.unwrap_or(w),
        height: known.height.unwrap_or(h),
    };
    let Some(ctx) = ctx else {
        return or_known(0.0, 0.0);
    };
    // As GPUI's text element: wrap at the known width, else at a definite available width;
    // min- and max-content probes lay the text out unwrapped. Widths are passed exactly:
    // measuring at a rounded width could wrap differently from the frame it is drawn in.
    let is_text = matches!(ctx, Measure::Text(_));
    let scale = cache.scale;
    let snap = |w: f32| {
        if is_text {
            text_width(w, scale)
        } else {
            w.max(0.0)
        }
    };
    let mode = match (known.width, avail.width) {
        (Some(w), _) => WidthMode::Exactly(snap(w)),
        (None, AvailableSpace::Definite(w)) => WidthMode::AtMost(snap(w)),
        (None, _) => WidthMode::Undefined,
    };
    match ctx {
        Measure::Text(json) => {
            let (width, tag) = match mode {
                WidthMode::Undefined => (0.0, 0u8),
                WidthMode::AtMost(w) => (w, 1),
                WidthMode::Exactly(w) => (w, 2),
            };
            let key = (json.clone(), tag, width.to_bits());
            // A width at least the unconstrained one (both on the pixel grid the renderer
            // draws at) lays the text out on the same lines: no call needed.
            let unconstrained = match mode {
                WidthMode::Undefined => None,
                _ => cache
                    .map
                    .get(&max_content_key(json))
                    .copied()
                    .filter(|m| text_width(m.width, cache.scale) <= width),
            };
            let m = if let Some(m) = unconstrained {
                cache.counters.derived_hits += 1;
                TextMetrics {
                    width: if tag == 2 { width } else { m.width },
                    ..m
                }
            } else if let Some(m) = cache.map.get(&key) {
                cache.counters.hits += 1;
                *m
            } else {
                cache.counters.misses += 1;
                let m = measurer.measure_text(json, mode);
                cache.map.insert(key, m);
                m
            };
            // GPUI ceils the widest line to whole points, so laying the text out again at
            // the width it reported never wraps it onto another line.
            let w = match mode {
                WidthMode::Exactly(w) => w,
                _ => m.width.ceil(),
            };
            or_known(w, m.height)
        }
        Measure::Component { name, props } => {
            match component_size(name, props, mode, measurer, cache) {
                Some((w, h)) => or_known(w, h),
                None => or_known(0.0, 0.0),
            }
        }
        Measure::Widest { name, candidates } => {
            let widest = candidates
                .iter()
                .filter_map(|props| component_size(name, props, mode, measurer, cache))
                .reduce(|(w, h), (w2, h2)| (w.max(w2), h.max(h2)));
            match widest {
                Some((w, h)) => or_known(w, h),
                // A built-in control the host cannot measure: a usable minimum.
                None => or_known(CONTROL_FALLBACK.0, CONTROL_FALLBACK.1),
            }
        }
    }
}
