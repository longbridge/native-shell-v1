//! Deterministic stand-ins for the platform, for this crate's tests and the engine's
//! headless harness.

use std::collections::{HashMap, HashSet};

use native_shell_core::host::{Measurer, TextMetrics, WidthMode};
use native_shell_core::protocol::{Frame, Mutation, Prop, PropValue, ViewType, ROOT_NODE};

/// A measurer with simple, documented metrics, so layout tests can compute expected
/// frames by hand.
///
/// Text (`AttributedText` JSON):
/// * a character is `size × 0.5` wide, or `size × 1.0` for CJK, emoji and full-width
///   characters (`size` of its run);
/// * a line is as tall as the tallest run's `line_height`, or `size × 1.2` when unset;
/// * lines break at `\n`, and greedily at spaces when the next word does not fit the
///   width; CJK characters may break anywhere; a word wider than the line overflows;
/// * `lines` caps the line count; `WidthMode::Undefined` never wraps;
/// * width is the widest line (`Exactly(w)` reports `w`), baseline is `0.8 ×` the first
///   line's height.
///
/// Components: `None` unless the props JSON has `"intrinsic_height"`, then
/// `(available width, intrinsic_height)` (0 wide when the width is undefined).
///
/// Built-in controls: `nsh/select` is `8 × chars + 32` wide (its `label`, else its
/// `placeholder`: 16 pt text plus chevron and padding) and 36 tall; `nsh/date-picker` is
/// `8 × chars + 24` wide (its `value`, else `"YYYY-MM-DD"`) and 36 tall.
#[derive(Debug, Clone, Copy, Default)]
pub struct FakeMeasurer;

fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x2600..=0x27BF | 0x1F300..=0x1FAFF | 0x20000..=0x3FFFD)
}

/// A unit the line breaker places whole: a word, a space, a CJK character or a newline.
struct Piece {
    width: f32,
    height: f32,
    space: bool,
    newline: bool,
}

impl Measurer for FakeMeasurer {
    fn measure_text(&self, attributed_json: &str, width: WidthMode) -> TextMetrics {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(attributed_json) else {
            return TextMetrics::default();
        };
        let f =
            |r: &serde_json::Value, k: &str| r.get(k).and_then(|x| x.as_f64()).map(|x| x as f32);
        let mut pieces: Vec<Piece> = Vec::new();
        let mut word: Option<Piece> = None;
        for run in v
            .get("runs")
            .and_then(|r| r.as_array())
            .into_iter()
            .flatten()
        {
            let size = f(run, "size").unwrap_or(16.0);
            let height = f(run, "line_height").unwrap_or(size * 1.2);
            for c in run
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .chars()
            {
                let breaks = c == ' ' || c == '\n' || is_wide(c);
                if breaks {
                    pieces.extend(word.take());
                    let w = if c == '\n' {
                        0.0
                    } else if is_wide(c) {
                        size
                    } else {
                        size * 0.5
                    };
                    pieces.push(Piece {
                        width: w,
                        height,
                        space: c == ' ',
                        newline: c == '\n',
                    });
                } else {
                    let p = word.get_or_insert(Piece {
                        width: 0.0,
                        height,
                        space: false,
                        newline: false,
                    });
                    p.width += size * 0.5;
                    p.height = p.height.max(height);
                }
            }
        }
        pieces.extend(word.take());

        let max = match width {
            WidthMode::Undefined => f32::INFINITY,
            WidthMode::AtMost(w) | WidthMode::Exactly(w) => w,
        };
        // (width, height) of each line.
        let mut lines: Vec<(f32, f32)> = vec![(0.0, 0.0)];
        let mut pending_space = 0.0;
        for p in pieces {
            let line = lines.last_mut().expect("at least one line");
            if p.newline {
                line.1 = line.1.max(p.height);
                lines.push((0.0, 0.0));
                pending_space = 0.0;
                continue;
            }
            line.1 = line.1.max(p.height);
            if p.space {
                pending_space += p.width;
                continue;
            }
            if line.0 > 0.0 && line.0 + pending_space + p.width > max + 0.001 {
                lines.push((p.width, p.height));
            } else {
                line.0 += pending_space + p.width;
            }
            pending_space = 0.0;
        }
        if let Some(limit) = v.get("lines").and_then(|l| l.as_u64()).filter(|l| *l > 0) {
            lines.truncate(limit as usize);
        }
        let default_height = || {
            let size = v
                .pointer("/runs/0/size")
                .and_then(|s| s.as_f64())
                .unwrap_or(16.0) as f32;
            v.pointer("/runs/0/line_height")
                .and_then(|s| s.as_f64())
                .map(|h| h as f32)
                .unwrap_or(size * 1.2)
        };
        for l in lines.iter_mut() {
            if l.1 == 0.0 {
                l.1 = default_height();
            }
        }
        let widest = lines.iter().map(|l| l.0).fold(0.0, f32::max);
        TextMetrics {
            width: if let WidthMode::Exactly(w) = width {
                w
            } else {
                widest
            },
            height: lines.iter().map(|l| l.1).sum(),
            baseline: lines[0].1 * 0.8,
        }
    }

    fn measure_component(
        &self,
        name: &str,
        props_json: &str,
        width: WidthMode,
    ) -> Option<(f32, f32)> {
        let v: serde_json::Value = serde_json::from_str(props_json).ok()?;
        let chars = |t: Option<&str>| t.map_or(0, |t| t.chars().count()) as f32;
        match name {
            "nsh/select" => {
                let shown = v["label"].as_str().or(v["placeholder"].as_str());
                return Some((8.0 * chars(shown) + 32.0, 36.0));
            }
            "nsh/date-picker" => {
                return Some((
                    8.0 * chars(v["value"].as_str().or(Some("YYYY-MM-DD"))) + 24.0,
                    36.0,
                ));
            }
            _ => {}
        }
        let h = v.get("intrinsic_height")?.as_f64()? as f32;
        let w = match width {
            WidthMode::Undefined => 0.0,
            WidthMode::AtMost(w) | WidthMode::Exactly(w) => w,
        };
        Some((w, h))
    }
}

#[derive(Debug, Clone)]
pub struct ReplayNode {
    pub view_type: ViewType,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub props: HashMap<Prop, PropValue>,
    pub frame: Option<Frame>,
    pub mask: u32,
}

/// Applies batches to an in-memory native tree the way a renderer would, and rejects any
/// batch a renderer could not apply: an op on an unknown or deleted node, an index out of
/// range, a node inserted while attached, a node left detached at the end of a batch.
#[derive(Debug, Default)]
pub struct Replayer {
    pub nodes: HashMap<u32, ReplayNode>,
    pub root_children: Vec<u32>,
    root_props: HashMap<Prop, PropValue>,
    deleted: HashSet<u32>,
}

impl Replayer {
    pub fn new() -> Replayer {
        Replayer::default()
    }

    pub fn children(&self, parent: u32) -> &[u32] {
        if parent == ROOT_NODE {
            return &self.root_children;
        }
        self.nodes
            .get(&parent)
            .map(|n| n.children.as_slice())
            .unwrap_or(&[])
    }

    /// Properties of the platform surface (`ROOT_NODE`), such as `ThemeTokens`.
    pub fn root_props(&self) -> &HashMap<Prop, PropValue> {
        &self.root_props
    }

    pub fn prop(&self, node: u32, prop: Prop) -> Option<&PropValue> {
        self.nodes.get(&node)?.props.get(&prop)
    }

    fn live(&self, node: u32, op: &Mutation) -> Result<(), String> {
        if self.deleted.contains(&node) {
            return Err(format!("{op:?}: node {node} was deleted"));
        }
        if !self.nodes.contains_key(&node) {
            return Err(format!("{op:?}: unknown node {node}"));
        }
        Ok(())
    }

    fn list_mut(&mut self, parent: u32, op: &Mutation) -> Result<&mut Vec<u32>, String> {
        if parent == ROOT_NODE {
            return Ok(&mut self.root_children);
        }
        if self.deleted.contains(&parent) {
            return Err(format!("{op:?}: parent {parent} was deleted"));
        }
        self.nodes
            .get_mut(&parent)
            .map(|n| &mut n.children)
            .ok_or_else(|| format!("{op:?}: unknown parent {parent}"))
    }

    fn destroy(&mut self, node: u32) {
        if let Some(n) = self.nodes.remove(&node) {
            self.deleted.insert(node);
            for c in n.children {
                self.destroy(c);
            }
        }
    }

    pub fn apply(&mut self, batch: &[Mutation]) -> Result<(), String> {
        for op in batch {
            match op {
                Mutation::BeginBatch { .. } | Mutation::EndBatch => {}
                Mutation::Create { node, view_type } => {
                    if *node == ROOT_NODE
                        || self.nodes.contains_key(node)
                        || self.deleted.contains(node)
                    {
                        return Err(format!("{op:?}: node id already used"));
                    }
                    let n = ReplayNode {
                        view_type: *view_type,
                        parent: None,
                        children: Vec::new(),
                        props: HashMap::new(),
                        frame: None,
                        mask: 0,
                    };
                    self.nodes.insert(*node, n);
                }
                Mutation::Insert {
                    node,
                    parent,
                    index,
                } => {
                    self.live(*node, op)?;
                    if self.nodes[node].parent.is_some() {
                        return Err(format!("{op:?}: node already attached"));
                    }
                    if node == parent {
                        return Err(format!("{op:?}: node inserted into itself"));
                    }
                    let list = self.list_mut(*parent, op)?;
                    if *index as usize > list.len() {
                        return Err(format!("{op:?}: index out of range (len {})", list.len()));
                    }
                    list.insert(*index as usize, *node);
                    self.nodes.get_mut(node).expect("live").parent = Some(*parent);
                }
                Mutation::Remove {
                    node,
                    parent,
                    index,
                } => {
                    self.live(*node, op)?;
                    let list = self.list_mut(*parent, op)?;
                    if list.get(*index as usize) != Some(node) {
                        return Err(format!("{op:?}: node is not at that index"));
                    }
                    list.remove(*index as usize);
                    self.nodes.get_mut(node).expect("live").parent = None;
                }
                Mutation::Move {
                    node,
                    parent,
                    from,
                    to,
                } => {
                    self.live(*node, op)?;
                    let list = self.list_mut(*parent, op)?;
                    if list.get(*from as usize) != Some(node) || *to as usize >= list.len() {
                        return Err(format!("{op:?}: bad move in list {list:?}"));
                    }
                    let n = list.remove(*from as usize);
                    list.insert(*to as usize, n);
                }
                Mutation::Delete { node } => {
                    self.live(*node, op)?;
                    if let Some(p) = self.nodes[node].parent {
                        let list = self.list_mut(p, op)?;
                        list.retain(|c| c != node);
                    }
                    self.destroy(*node);
                }
                Mutation::SetProp { node, prop, value } => {
                    // The platform surface takes instance-wide props; every other op on it is
                    // rejected below as an op on an unknown node.
                    let props = if *node == ROOT_NODE {
                        &mut self.root_props
                    } else {
                        self.live(*node, op)?;
                        &mut self.nodes.get_mut(node).expect("live").props
                    };
                    if *value == PropValue::Null {
                        props.remove(prop);
                    } else {
                        props.insert(*prop, value.clone());
                    }
                }
                Mutation::SetFrame { node, frame } => {
                    self.live(*node, op)?;
                    if ![frame.x, frame.y, frame.width, frame.height]
                        .iter()
                        .all(|v| v.is_finite())
                    {
                        return Err(format!("{op:?}: non-finite frame"));
                    }
                    self.nodes.get_mut(node).expect("live").frame = Some(*frame);
                }
                Mutation::SetEventMask { node, mask } => {
                    self.live(*node, op)?;
                    self.nodes.get_mut(node).expect("live").mask = *mask;
                }
                Mutation::Command { node, .. } => {
                    if *node != ROOT_NODE {
                        self.live(*node, op)?;
                    }
                }
            }
        }
        for (id, n) in &self.nodes {
            if n.parent.is_none() {
                return Err(format!("node {id} is detached at the end of the batch"));
            }
            if n.frame.is_none() && n.view_type != ViewType::Dialog {
                return Err(format!("node {id} ({:?}) has no frame", n.view_type));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_takes_props_and_nothing_else() {
        let mut r = Replayer::new();
        let tokens = PropValue::Json("{\"primary\":4278190080}".into());
        r.apply(&[Mutation::SetProp {
            node: ROOT_NODE,
            prop: Prop::ThemeTokens,
            value: tokens.clone(),
        }])
        .unwrap();
        assert_eq!(r.root_props().get(&Prop::ThemeTokens), Some(&tokens));
        r.apply(&[Mutation::SetProp {
            node: ROOT_NODE,
            prop: Prop::ThemeTokens,
            value: PropValue::Null,
        }])
        .unwrap();
        assert!(r.root_props().is_empty());

        let frame = Frame {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        };
        for bad in [
            Mutation::Create {
                node: ROOT_NODE,
                view_type: ViewType::View,
            },
            Mutation::Delete { node: ROOT_NODE },
            Mutation::SetFrame {
                node: ROOT_NODE,
                frame,
            },
            Mutation::SetEventMask {
                node: ROOT_NODE,
                mask: 1,
            },
            Mutation::Insert {
                node: ROOT_NODE,
                parent: ROOT_NODE,
                index: 0,
            },
        ] {
            assert!(
                Replayer::new().apply(std::slice::from_ref(&bad)).is_err(),
                "{bad:?} accepted"
            );
        }
        // Other nodes stay strict.
        assert!(Replayer::new()
            .apply(&[Mutation::SetProp {
                node: 9,
                prop: Prop::Opacity,
                value: PropValue::F32(0.5)
            }])
            .is_err());
    }
}
