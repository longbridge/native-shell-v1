//! Builders for descriptions and a harness that checks every batch against the replayer.

#![allow(dead_code)]

use std::collections::HashMap;
use std::rc::Rc;

use native_shell_core::desc::{
    CommitInput, Desc, ElementKind, EntityId, Surface, SurfaceKind, TextRun,
};
use native_shell_core::host::Environment;
use native_shell_core::protocol::{
    decode, encode, Frame, Mutation, Prop, PropValue, ViewType, ROOT_NODE,
};
use native_shell_core::style::{resolve, Arg, Resolved, StyleOp, TextStyle};
use native_shell_core::theme::{Appearance, Theme};
use native_shell_tree::testing::{FakeMeasurer, Replayer};
use native_shell_tree::Tree;

thread_local! {
    static STYLES: std::cell::RefCell<HashMap<String, Rc<Resolved>>> = Default::default();
}

/// `"flex_row p_4 w=100 bg=#ff0000"`: nullary names, or `name=arg` with a numeric or string argument.
/// Interned by spec, as the engine's style cache does, so equal chains share one `Rc`.
pub fn st(spec: &str) -> Rc<Resolved> {
    if let Some(r) = STYLES.with(|s| s.borrow().get(spec).cloned()) {
        return r;
    }
    let r = parse(spec);
    STYLES.with(|s| s.borrow_mut().insert(spec.to_string(), r.clone()));
    r
}

fn parse(spec: &str) -> Rc<Resolved> {
    let ops: Vec<StyleOp> = spec
        .split_whitespace()
        .map(|t| match t.split_once('=') {
            Some((name, arg)) => {
                let arg = arg
                    .parse::<f64>()
                    .map(Arg::Num)
                    .unwrap_or_else(|_| Arg::Str(arg.to_string()));
                StyleOp::new(name, vec![arg])
            }
            None => StyleOp::new(t, vec![]),
        })
        .collect();
    Rc::new(resolve(&ops, &Theme::default_for(Appearance::Light)).expect("style"))
}

pub fn view(spec: &str, children: Vec<Desc>) -> Desc {
    let mut d = Desc::new(ElementKind::View, st(spec));
    d.children = children;
    d
}

pub fn text(spec: &str, s: &str) -> Desc {
    Desc::new(
        ElementKind::Text(vec![TextRun {
            text: s.into(),
            style: TextStyle::default(),
            url: None,
        }]),
        st(spec),
    )
}

pub fn el(kind: ElementKind, spec: &str) -> Desc {
    Desc::new(kind, st(spec))
}

pub fn keyed(mut d: Desc, k: &str) -> Desc {
    d.key = Some(k.into());
    d
}

pub fn with_prop(mut d: Desc, p: Prop, v: PropValue) -> Desc {
    d.props.push((p, v));
    d
}

pub fn root(content: Desc) -> Vec<Surface> {
    vec![Surface {
        key: "root".into(),
        kind: SurfaceKind::Root,
        content,
    }]
}

pub struct H {
    pub tree: Tree,
    pub rep: Replayer,
    pub entities: HashMap<EntityId, Rc<Desc>>,
}

impl H {
    pub fn new() -> H {
        let mut tree = Tree::new();
        tree.set_environment(&Environment {
            width: 390.0,
            height: 844.0,
            scale: 3.0,
            ..Default::default()
        });
        H {
            tree,
            rep: Replayer::new(),
            entities: HashMap::new(),
        }
    }

    pub fn with_env(env: Environment) -> H {
        let mut h = H::new();
        h.tree.set_environment(&env);
        h
    }

    /// Commits, replays, and checks that the replayed native tree is the one the tree believes in.
    pub fn commit(&mut self, surfaces: &[Surface]) -> Vec<Mutation> {
        let input = CommitInput {
            surfaces,
            entities: &self.entities,
        };
        let batch = self.tree.commit(&input, &FakeMeasurer);
        if let Err(e) = self.rep.apply(&batch) {
            panic!(
                "replay failed: {e}\nbatch: {batch:#?}\ntree:\n{}",
                self.tree.dump()
            );
        }
        assert_eq!(decode(&encode(&batch)).expect("decodes"), batch);
        self.check_mirror(ROOT_NODE);
        self.check_text_frames();
        assert_eq!(
            self.rep.nodes.len(),
            self.tree.native_count(),
            "native count"
        );
        check_order(&batch);
        batch
    }

    /// Every visible text frame is as tall as its text measured at the frame's own width:
    /// the renderer, drawing at that width, gets the same lines the layout reserved.
    fn check_text_frames(&self) {
        use native_shell_core::host::{Measurer, WidthMode};
        for (id, n) in &self.rep.nodes {
            let (Some(f), Some(PropValue::Json(json))) =
                (n.frame, n.props.get(&Prop::AttributedText))
            else {
                continue;
            };
            if n.view_type != ViewType::Text || (f.width == 0.0 && f.height == 0.0) {
                continue;
            }
            let has_text = matches!(serde_json::from_str::<serde_json::Value>(json), Ok(v)
                if v["runs"].as_array().is_some_and(|r| r.iter().any(|r| r["text"].as_str().is_some_and(|t| !t.is_empty()))));
            assert!(
                !has_text || f.width > 0.0,
                "visible non-empty text {id} collapsed to {f:?}: {json}"
            );
            let m = FakeMeasurer.measure_text(json, WidthMode::Exactly(f.width));
            assert!(
                (m.height - f.height).abs() <= 1.0 / 3.0 + 1e-3,
                "text {id} frame {f:?} but measures {m:?} at its width: {json}"
            );
        }
    }

    fn check_mirror(&self, parent: u32) {
        let mine = self.tree.native_children(parent);
        assert_eq!(
            self.rep.children(parent),
            mine.as_slice(),
            "children of {parent}\n{}",
            self.tree.dump()
        );
        for c in mine {
            self.check_mirror(c);
        }
    }

    pub fn frame(&self, node: u32) -> Frame {
        self.rep.nodes[&node].frame.expect("frame")
    }

    pub fn prop(&self, node: u32, p: Prop) -> Option<&PropValue> {
        self.rep.prop(node, p)
    }

    pub fn of_type(&self, t: ViewType) -> Vec<u32> {
        let mut v: Vec<u32> = self
            .rep
            .nodes
            .iter()
            .filter(|(_, n)| n.view_type == t)
            .map(|(id, _)| *id)
            .collect();
        v.sort();
        v
    }
}

/// Deletes, then each Create followed by its own props, then structure, then updates, then frames.
fn check_order(batch: &[Mutation]) {
    fn phase(m: &Mutation) -> u8 {
        match m {
            Mutation::Delete { .. } => 0,
            Mutation::Create { .. } => 1,
            Mutation::Insert { .. } | Mutation::Move { .. } | Mutation::Remove { .. } => 2,
            Mutation::SetFrame { .. } => 4,
            _ => 3,
        }
    }
    let mut last = 0;
    let mut current_create = None;
    for m in batch {
        let mut p = phase(m);
        if let (Mutation::SetProp { node, .. } | Mutation::SetEventMask { node, .. }, 1) = (m, last)
        {
            if Some(*node) == current_create {
                p = 1;
            }
        }
        if let Mutation::Create { node, .. } = m {
            current_create = Some(*node);
        }
        assert!(p >= last, "op {m:?} out of order in {batch:#?}");
        last = p;
    }
    // Parent before child: nothing goes into a node that is itself inserted later in the batch.
    let inserted_at: HashMap<u32, usize> = batch
        .iter()
        .enumerate()
        .filter_map(|(i, m)| match m {
            Mutation::Insert { node, .. } => Some((*node, i)),
            _ => None,
        })
        .collect();
    for (i, m) in batch.iter().enumerate() {
        if let Mutation::Insert { parent, .. } | Mutation::Move { parent, .. } = m {
            if let Some(j) = inserted_at.get(parent) {
                assert!(*j < i, "{m:?} before its parent is inserted");
            }
        }
    }
}

pub fn count(batch: &[Mutation], f: impl Fn(&Mutation) -> bool) -> usize {
    batch.iter().filter(|m| f(m)).count()
}

pub fn frame_of(batch: &[Mutation], id: u32) -> Option<Frame> {
    batch.iter().rev().find_map(|m| match m {
        Mutation::SetFrame { node, frame } if *node == id => Some(*frame),
        _ => None,
    })
}
