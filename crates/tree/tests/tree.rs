mod common;

use std::rc::Rc;
use std::time::Instant;

use common::*;
use native_shell_core::desc::{ElementKind, ListSpec, Surface, SurfaceKind};
use native_shell_core::host::Environment;
use native_shell_core::protocol::{
    event_mask, EventType, Mutation, Prop, PropValue, ViewType, ROOT_NODE,
};

/// One line of 16 pt text: GPUI's default line height is φ × size.
const LINE: f32 = 16.0 * 1.618_034;

/// Equal up to the pixel grid (scale 3).
fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.34
}

fn created(batch: &[Mutation]) -> Vec<u32> {
    batch
        .iter()
        .filter_map(|m| match m {
            Mutation::Create { node, .. } => Some(*node),
            _ => None,
        })
        .collect()
}

#[test]
fn initial_mount_flattens_layout_only_views_and_offsets_frames() {
    let mut h = H::new();
    let red = keyed(view("bg=#ff0000 w=50 h=20", vec![]), "red");
    let content = view(
        "size_full",
        vec![view(
            "p_4",
            vec![view("p_2 mt_3", vec![red, text("", "Hello")])],
        )],
    );
    let batch = h.commit(&root(content));

    let stats = h.tree.stats();
    assert_eq!(
        (stats.nodes, stats.natives, stats.flattened),
        (6, 3, 3),
        "{}",
        h.tree.dump()
    );
    let surface = h.tree.surface_node("root").unwrap();
    assert_eq!(h.rep.children(ROOT_NODE), &[surface]);
    let kids = h.rep.children(surface).to_vec();
    assert_eq!(kids.len(), 2, "both natives attach to the surface");
    assert_eq!(h.rep.nodes[&kids[0]].view_type, ViewType::View);
    assert_eq!(h.rep.nodes[&kids[1]].view_type, ViewType::Text);

    // p_4 (16) + p_2 (8) across, p_4 (16) + mt_3 (12) + p_2 (8) down.
    let f = h.frame(kids[0]);
    assert_eq!((f.x, f.y, f.width, f.height), (24.0, 36.0, 50.0, 20.0));
    let t = h.frame(kids[1]);
    assert_eq!((t.x, t.y, t.width), (24.0, 56.0, 342.0));
    assert!(approx(t.height, LINE), "{t:?}");
    assert_eq!(h.tree.frame(kids[1]), Some(t));
    assert_eq!(frame_of(&batch, surface).unwrap().width, 390.0);
    let Some(PropValue::Json(json)) = h.prop(kids[1], Prop::AttributedText) else {
        panic!()
    };
    assert_eq!(
        json,
        "{\"runs\":[{\"text\":\"Hello\",\"size\":16,\"weight\":400,\"color\":4278848010,\"line_height\":25.888544}]}"
    );
    assert_eq!(
        h.prop(kids[0], Prop::BackgroundColor),
        Some(&PropValue::Color(0xFFFF0000))
    );
    // Creates come before inserts; the creates' props right after each create.
    assert!(matches!(batch[0], Mutation::Create { .. }));
}

fn column(keys: &[&str]) -> Vec<Surface> {
    let items = keys
        .iter()
        .map(|k| keyed(view("bg=#ff0000 h=10 border_1", vec![]), k))
        .collect();
    root(view("size_full", items))
}

#[test]
fn keyed_reorder_emits_moves_not_recreation() {
    let mut h = H::new();
    h.commit(&column(&["a", "b", "c", "d", "e"]));
    let surface = h.tree.surface_node("root").unwrap();
    let before = h.rep.children(surface).to_vec();

    let batch = h.commit(&column(&["e", "a", "b", "c", "d"]));
    assert_eq!(
        count(&batch, |m| matches!(
            m,
            Mutation::Create { .. } | Mutation::Delete { .. }
        )),
        0
    );
    assert_eq!(
        count(&batch, |m| matches!(m, Mutation::Move { .. })),
        1,
        "{batch:#?}"
    );
    assert_eq!(
        h.rep.children(surface),
        &[before[4], before[0], before[1], before[2], before[3]]
    );
    assert_eq!(count(&batch, |m| matches!(m, Mutation::SetFrame { .. })), 5);
    assert_eq!(h.frame(before[4]).y, 0.0);

    // Remove one, insert one, swap two: survivors keep their ids.
    let batch = h.commit(&column(&["a", "x", "c", "b", "e"]));
    assert_eq!(count(&batch, |m| matches!(m, Mutation::Delete { .. })), 1);
    assert_eq!(created(&batch).len(), 1);
    let now = h.rep.children(surface).to_vec();
    assert_eq!(
        (now[0], now[2], now[3], now[4]),
        (before[0], before[2], before[1], before[4])
    );
}

#[test]
fn removed_props_are_reset_with_null() {
    let mut h = H::new();
    let shadowed = view("border_1 bg=#ff0000 opacity=0.5 rounded_lg", vec![]);
    h.commit(&root(view("", vec![shadowed])));
    let node = h.of_type(ViewType::View).into_iter().max().unwrap();
    assert_eq!(h.prop(node, Prop::Opacity), Some(&PropValue::F32(0.5)));

    let batch = h.commit(&root(view("", vec![view("border_1 rounded_lg", vec![])])));
    let set: Vec<_> = batch
        .iter()
        .filter(|m| matches!(m, Mutation::SetProp { .. }))
        .cloned()
        .collect();
    assert_eq!(
        set,
        vec![
            Mutation::SetProp {
                node,
                prop: Prop::BackgroundColor,
                value: PropValue::Null
            },
            Mutation::SetProp {
                node,
                prop: Prop::Opacity,
                value: PropValue::Null
            },
        ]
    );
    assert_eq!(h.prop(node, Prop::BackgroundColor), None);
}

#[test]
fn flatten_status_flip_recreates_the_subtree() {
    let mut h = H::new();
    let tree = |bg: &str| {
        root(view(
            "",
            vec![view(
                &format!("p_2 {bg}"),
                vec![view("bg=#00ff00 h=10", vec![])],
            )],
        ))
    };
    h.commit(&tree(""));
    let surface = h.tree.surface_node("root").unwrap();
    let green = h.rep.children(surface)[0];
    assert_eq!(h.tree.stats().flattened, 2);
    assert_eq!(
        h.frame(green).x,
        8.0,
        "offset by the flattened parent's padding"
    );

    let batch = h.commit(&tree("bg=#0000ff"));
    assert_eq!(batch[0], Mutation::Delete { node: green });
    let new = created(&batch);
    assert_eq!(new.len(), 2, "the parent and its child, with fresh ids");
    assert!(new.iter().all(|id| *id > green));
    let blue = h.rep.children(surface)[0];
    assert_eq!(h.rep.children(blue), &[new[1]]);
    assert_eq!(h.frame(new[1]).x, 8.0, "now relative to the native parent");

    // And back: flattened again.
    let batch = h.commit(&tree(""));
    assert_eq!(batch[0], Mutation::Delete { node: blue });
    assert_eq!(h.tree.stats().flattened, 2);
}

#[test]
fn unchanged_entities_are_skipped() {
    let mut h = H::new();
    h.entities
        .insert(1, Rc::new(view("bg=#ff0000 p_2", vec![text("", "inside")])));
    let surfaces = |spec: &str| {
        root(view(
            spec,
            vec![el(ElementKind::Entity(1), ""), text("", "outside")],
        ))
    };
    h.commit(&surfaces(""));
    assert!(h.commit(&surfaces("")).is_empty(), "identical recommit");

    // Same Rc, but the inherited text color changed: the entity's text is rebuilt.
    let batch = h.commit(&surfaces("text_color=#ff0000"));
    let colored = batch
        .iter()
        .filter(|m| matches!(m, Mutation::SetProp { prop: Prop::AttributedText, value: PropValue::Json(j), .. } if j.contains("4294901760")))
        .count();
    assert_eq!(colored, 2, "{batch:#?}");
    assert!(h.commit(&surfaces("text_color=#ff0000")).is_empty());

    // A new description for the entity: one prop changes.
    h.entities
        .insert(1, Rc::new(view("bg=#00ff00 p_2", vec![text("", "inside")])));
    let batch = h.commit(&surfaces("text_color=#ff0000"));
    assert_eq!(batch.len(), 1, "{batch:#?}");
    assert!(matches!(
        batch[0],
        Mutation::SetProp {
            prop: Prop::BackgroundColor,
            value: PropValue::Color(0xFF00FF00),
            ..
        }
    ));

    // A missing entity renders nothing and takes no position: the unkeyed sibling text moves
    // to position 0, where the entity's view was, so it is recreated.
    h.entities.clear();
    let batch = h.commit(&surfaces("text_color=#ff0000"));
    assert_eq!(
        count(&batch, |m| matches!(m, Mutation::Delete { .. })),
        2,
        "{batch:#?}"
    );
    assert_eq!(h.of_type(ViewType::Text).len(), 1);
    // Also as a surface's content.
    let batch = h.commit(&root(el(ElementKind::Entity(42), "")));
    assert!(
        h.rep
            .children(h.tree.surface_node("root").unwrap())
            .is_empty(),
        "{batch:#?}"
    );
}

#[test]
fn text_rewraps_when_the_environment_rotates() {
    let long = "lorem ipsum dolor sit amet ".repeat(6);
    let mut h = H::new();
    let surfaces = root(view("size_full p_4", vec![text("", &long)]));
    h.commit(&surfaces);
    let t = h.of_type(ViewType::Text)[0];
    let portrait = h.frame(t);

    h.tree.set_environment(&Environment {
        width: 844.0,
        height: 390.0,
        scale: 3.0,
        ..Default::default()
    });
    let batch = h.commit(&surfaces);
    let landscape = h.frame(t);
    assert!(
        landscape.height < portrait.height,
        "{portrait:?} → {landscape:?}"
    );
    assert_eq!(landscape.width, 812.0);
    assert_eq!(
        count(&batch, |m| matches!(m, Mutation::SetProp { .. })),
        0,
        "only frames change: {batch:#?}"
    );
    // 162 chars at 8 px: 358 px fits 44 per line → 4 lines; 812 px fits 101 → 2 lines.
    assert!(
        approx(portrait.height, 4.0 * LINE) || approx(portrait.height, 5.0 * LINE),
        "{portrait:?}"
    );
    assert!(approx(landscape.height, 2.0 * LINE), "{landscape:?}");
    let s = h.tree.stats();
    assert!(s.measure_misses > 0);
}

#[test]
fn text_measurements_are_cached() {
    let mut h = H::new();
    let row = || view("flex_row", vec![text("", "same"), text("", "same")]);
    h.commit(&root(view("", vec![row(), row(), row()])));
    let s = h.tree.stats();
    // Six equal texts: measured once, in one batch; every probe answered from it.
    assert_eq!(
        (s.batch_calls, s.batched_items, s.measure_misses),
        (1, 1, 0),
        "{s:?}"
    );
    assert!(s.derived_hits + s.measure_hits >= 6, "{s:?}");
    h.tree.clear_measure_cache();
    let batch = h.commit(&root(view("", vec![row(), row(), row()])));
    assert!(batch.is_empty());
    assert_eq!(
        h.tree.stats().batch_calls,
        2,
        "measured again after clearing"
    );
}

fn input(value: &str) -> Vec<Surface> {
    let field = with_prop(
        el(ElementKind::TextInput, ""),
        Prop::TextValue,
        PropValue::Str(value.into()),
    );
    root(view("p_4", vec![field]))
}

#[test]
fn native_text_is_not_echoed_back() {
    let mut h = H::new();
    h.commit(&input("a"));
    let field = h.of_type(ViewType::TextInput)[0];
    assert_eq!(
        h.prop(field, Prop::TextValue),
        Some(&PropValue::Json("{\"text\":\"a\",\"count\":0}".into()))
    );
    assert!(
        approx(h.frame(field).height, 35.2),
        "default height {:?}",
        h.frame(field)
    );
    assert!(
        matches!(h.prop(field, Prop::TextStyle), Some(PropValue::Json(j)) if j.contains("\"size\":16"))
    );

    // The user typed "ab"; the script echoes it.
    h.tree.record_native_text(field, "ab", 1);
    assert_eq!(h.tree.native_text_count(field), 1);
    assert!(h.commit(&input("ab")).is_empty());

    // The script rewrites the value: sent with the latest count.
    let batch = h.commit(&input("AB"));
    assert_eq!(
        batch,
        vec![Mutation::SetProp {
            node: field,
            prop: Prop::TextValue,
            value: PropValue::Json("{\"text\":\"AB\",\"count\":1}".into())
        }]
    );

    // The user typed, the script ignored it (controlled): the native value is reverted.
    h.tree.record_native_text(field, "ABC", 2);
    let batch = h.commit(&input("AB"));
    assert_eq!(
        batch,
        vec![Mutation::SetProp {
            node: field,
            prop: Prop::TextValue,
            value: PropValue::Json("{\"text\":\"AB\",\"count\":2}".into())
        }]
    );
}

fn list(first: u32, last: u32, count: u32, uniform: bool) -> Vec<Surface> {
    let spec = ListSpec {
        id: "items".into(),
        count,
        estimated_row_height: 50.0,
        uniform,
    };
    let mut l = el(ElementKind::List(spec), "flex_1");
    l.children = (first..last)
        .map(|i| {
            let mut r = el(ElementKind::ListRow(i), "p_2");
            let label = if i % 7 == 3 {
                format!(
                    "Row {i} has a much longer label that wraps onto more lines than the others do"
                )
            } else {
                format!("Row {i}")
            };
            r.children = vec![text("", &label)];
            r
        })
        .collect();
    // The list grows to fill a flex column (in a block parent `flex_1` does not grow, as in GPUI).
    root(view("flex flex_col size_full", vec![l]))
}

fn offsets(h: &H, list: u32) -> Vec<f32> {
    match h.prop(list, Prop::RowOffsets) {
        Some(PropValue::F32Array(a)) => a.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn ten_thousand_row_list_renders_only_its_window() {
    let mut h = H::new();
    assert_eq!(h.tree.list_window("items"), (0, 20));
    assert!(h.tree.set_list_window("items", 0, 30));
    let (first, last) = h.tree.list_window("items");
    h.commit(&list(first, last, 10_000, false));
    let list_node = h.of_type(ViewType::List)[0];
    assert_eq!(h.of_type(ViewType::ListRow).len(), 30);
    assert_eq!(h.tree.native_count(), 2 + 30 * 2);
    assert_eq!(
        h.prop(list_node, Prop::ItemCount),
        Some(&PropValue::I32(10_000))
    );
    assert_eq!(
        h.prop(list_node, Prop::Prefetch),
        Some(&PropValue::F32(10.0))
    );
    assert_eq!(h.rep.nodes[&list_node].frame.unwrap().height, 844.0);

    let row = h.of_type(ViewType::ListRow)[0];
    assert_eq!(h.prop(row, Prop::RowIndex), Some(&PropValue::I32(0)));
    let rf = h.frame(row);
    assert_eq!((rf.x, rf.y, rf.width), (0.0, 0.0, 390.0));
    assert!(approx(rf.height, 16.0 + LINE), "{rf:?}");
    let label = h.rep.children(row)[0];
    assert_eq!(h.tree.list_id(label), Some("items"));
    assert_eq!(h.tree.list_id(list_node), Some("items"));
    assert_eq!(h.frame(label).x, 8.0);

    let check = |h: &H| {
        let o = offsets(h, list_node);
        assert_eq!(o.len(), 10_001);
        assert!(o.windows(2).all(|w| w[1] > w[0]));
        for r in h.of_type(ViewType::ListRow) {
            let Some(PropValue::I32(i)) = h.prop(r, Prop::RowIndex) else {
                panic!()
            };
            let height = h.frame(r).height;
            assert!(
                approx(o[*i as usize + 1] - o[*i as usize], height),
                "row {i}"
            );
        }
        o
    };
    let o = check(&h);
    let short = 16.0 + LINE;
    // Rows 3, 10, 17, 24 wrap onto more lines; unknown rows take the measured average.
    let measured: f32 = (0..30).map(|i| o[i + 1] - o[i]).sum();
    let rest = o[10_000] - o[30];
    assert!(
        (rest / (measured / 30.0 * 9_970.0) - 1.0).abs() < 1e-4,
        "{rest} vs {}",
        measured / 30.0 * 9_970.0
    );
    assert!(approx(o[1], short));

    // Scroll: the window moves, natives stay ~30 rows, offsets stay consistent.
    for start in [10, 100, 5_000, 9_970] {
        assert!(h.tree.set_list_window("items", start, start + 30));
        let (first, last) = h.tree.list_window("items");
        let batch = h.commit(&list(first, last, 10_000, false));
        assert_eq!(h.of_type(ViewType::ListRow).len(), 30);
        assert_eq!(h.tree.native_count(), 2 + 30 * 2);
        assert!(count(&batch, |m| matches!(m, Mutation::Delete { .. })) <= 30);
        let o = check(&h);
        assert!(approx(o[1], short), "measured heights are kept");
    }
    assert!(!h.tree.set_list_window("items", 9_970, 10_000));
    assert!(h.commit(&list(9_970, 10_000, 10_000, false)).is_empty());
    // Rows inside a list report absolute frames at their offset in the content.
    let row = h.of_type(ViewType::ListRow)[0];
    let Some(PropValue::I32(i)) = h.prop(row, Prop::RowIndex).cloned() else {
        panic!()
    };
    assert!(approx(
        h.tree.frame(row).unwrap().y,
        offsets(&h, list_node)[i as usize]
    ));
}

#[test]
fn uniform_lists_use_the_first_measured_height() {
    let mut h = H::new();
    h.commit(&list(0, 5, 100, true));
    let l = h.of_type(ViewType::List)[0];
    let o = offsets(&h, l);
    let first = o[1];
    assert!(o.windows(2).all(|w| approx(w[1] - w[0], first)));
    assert!(approx(o[100], first * 100.0));
}

#[test]
fn list_count_shrinking_clamps_the_window() {
    let mut h = H::new();
    h.tree.set_list_window("items", 0, 30);
    h.commit(&list(0, 30, 50, false));
    h.commit(&list(0, 10, 10, false));
    assert_eq!(h.tree.list_window("items"), (0, 10));
    assert_eq!(offsets(&h, h.of_type(ViewType::List)[0]).len(), 11);
}

fn page(n: u32, title: &str) -> Surface {
    Surface {
        key: format!("page:{n}"),
        kind: SurfaceKind::Page {
            title: title.into(),
            can_go_back: true,
            nav_bar_hidden: false,
        },
        content: view("size_full bg=#ffffff", vec![text("", title)]),
    }
}

#[test]
fn pages_push_and_pop() {
    let mut h = H::new();
    let base = root(view("", vec![text("", "home")]));
    h.commit(&base);
    let mut stack = base.clone();
    stack.push(page(1, "Detail"));
    let batch = h.commit(&stack);
    let p = h.tree.surface_node("page:1").unwrap();
    assert_eq!(h.rep.nodes[&p].view_type, ViewType::Page);
    assert!(batch.contains(&Mutation::Insert {
        node: p,
        parent: ROOT_NODE,
        index: 1
    }));
    assert_eq!(
        h.prop(p, Prop::Title),
        Some(&PropValue::Str("Detail".into()))
    );
    assert_eq!(h.prop(p, Prop::CanGoBack), Some(&PropValue::Bool(true)));
    assert_eq!(h.rep.nodes[&p].mask, event_mask::DISMISS);
    let f = h.frame(p);
    assert_eq!((f.width, f.height), (390.0, 844.0));
    assert_eq!(h.tree.surface_for_node(p), Some("page:1"));

    stack[1] = page(1, "Renamed");
    let batch = h.commit(&stack);
    assert_eq!(
        batch
            .iter()
            .filter(|m| matches!(m, Mutation::SetProp { node, .. } if *node == p))
            .count(),
        1
    );

    let batch = h.commit(&base);
    assert_eq!(batch, vec![Mutation::Delete { node: p }]);
    assert_eq!(h.rep.children(ROOT_NODE).len(), 1);
}

#[test]
fn sheets_dialogs_and_alerts_are_sized_from_content() {
    let mut h = H::new();
    let mut s = root(view("", vec![]));
    s.push(Surface {
        key: "sheet:1".into(),
        kind: SurfaceKind::Sheet {
            detents: vec!["medium".into()],
            dismissable: true,
        },
        content: view("p_4", vec![view("h=200 bg=#eeeeee", vec![])]),
    });
    s.push(Surface {
        key: "dialog:1".into(),
        kind: SurfaceKind::Dialog { dismissable: false },
        content: view("p_4", vec![text("", "Sure?")]),
    });
    s.push(Surface {
        key: "alert:1".into(),
        kind: SurfaceKind::Alert {
            spec_json: "{\"title\":\"Hi\",\"actions\":[]}".into(),
        },
        content: view("", vec![]),
    });
    h.commit(&s);
    let sheet = h.tree.surface_node("sheet:1").unwrap();
    let f = h.frame(sheet);
    assert_eq!((f.width, f.height), (390.0, 232.0));
    assert_eq!(
        h.prop(sheet, Prop::Detents),
        Some(&PropValue::Json("[\"medium\"]".into()))
    );
    let dialog = h.tree.surface_node("dialog:1").unwrap();
    let f = h.frame(dialog);
    assert_eq!(f.width, 342.0);
    assert!(approx(f.height, 32.0 + LINE), "{f:?}");
    let alert = h.tree.surface_node("alert:1").unwrap();
    assert_eq!(h.rep.nodes[&alert].view_type, ViewType::Dialog);
    assert!(h.rep.children(alert).is_empty());
    assert!(matches!(
        h.prop(alert, Prop::AlertSpec),
        Some(PropValue::Json(_))
    ));
    assert_eq!(
        h.rep.nodes[&alert].mask,
        event_mask::DISMISS | event_mask::ACTION
    );

    // Rotating resizes the sheet's width, not its height.
    h.tree.set_environment(&Environment {
        width: 844.0,
        height: 390.0,
        scale: 3.0,
        ..Default::default()
    });
    h.commit(&s);
    assert_eq!(h.frame(sheet).width, 844.0);
    assert_eq!(h.frame(dialog).width, 400.0);

    // A key whose kind changes is recreated.
    s[1].kind = SurfaceKind::Dialog { dismissable: true };
    let batch = h.commit(&s);
    assert!(batch.contains(&Mutation::Delete { node: sheet }));
}

#[test]
fn host_components_are_measured_and_get_their_props() {
    let mut h = H::new();
    let cell = |json: &str, spec: &str| {
        with_prop(
            el(ElementKind::HostComponent("demo-stock-cell".into()), spec),
            Prop::HostProps,
            PropValue::Json(json.into()),
        )
    };
    let s = |c| root(view("p_4", vec![c]));
    let batch = h.commit(&s(cell(
        "{\"symbol\":\"DEMO\",\"intrinsic_height\":60}",
        "",
    )));
    let node = h.of_type(ViewType::HostComponent)[0];
    let at = batch
        .iter()
        .position(|m| matches!(m, Mutation::Create { node: n, .. } if *n == node))
        .unwrap();
    assert_eq!(
        batch[at + 1],
        Mutation::SetProp {
            node,
            prop: Prop::HostComponent,
            value: PropValue::Str("demo-stock-cell".into())
        }
    );
    let f = h.frame(node);
    assert_eq!((f.x, f.y, f.width, f.height), (16.0, 16.0, 358.0, 60.0));

    let batch = h.commit(&s(cell(
        "{\"symbol\":\"DEMO\",\"intrinsic_height\":80}",
        "",
    )));
    assert!(batch.iter().any(|m| matches!(
        m,
        Mutation::SetProp {
            prop: Prop::HostProps,
            ..
        }
    )));
    assert!(batch.iter().all(|m| !matches!(
        m,
        Mutation::SetProp {
            prop: Prop::HostComponent,
            ..
        }
    )));
    assert_eq!(h.frame(node).height, 80.0);

    // Not measurable: the style sizes it.
    h.commit(&s(cell("{\"symbol\":\"DEMO2\"}", "h=40")));
    assert_eq!(h.frame(node).height, 40.0);
    // Another component name is another view.
    let batch = h.commit(&s(with_prop(
        el(ElementKind::HostComponent("demo-chart".into()), "h=40"),
        Prop::HostProps,
        PropValue::Json("{}".into()),
    )));
    assert_eq!(batch[0], Mutation::Delete { node });
}

#[test]
fn scroll_views_report_content_size() {
    let mut h = H::new();
    let s = root(view(
        "",
        vec![view(
            "overflow_y_scroll h=100",
            vec![
                view("h=300 bg=#ff0000", vec![]),
                view("h=50 bg=#00ff00", vec![]),
            ],
        )],
    ));
    h.commit(&s);
    let scroll = h.of_type(ViewType::Scroll)[0];
    assert_eq!(
        h.prop(scroll, Prop::ContentSize),
        Some(&PropValue::F32Array(vec![390.0, 350.0]))
    );
    assert_eq!(h.frame(scroll).height, 100.0);
    let horizontal = root(view(
        "",
        vec![view(
            "overflow_x_scroll flex_row",
            vec![view("w=900 h=10 bg=#ff0000", vec![])],
        )],
    ));
    h.commit(&horizontal);
    let scroll = h.of_type(ViewType::Scroll)[0];
    assert_eq!(
        h.prop(scroll, Prop::Horizontal),
        Some(&PropValue::Bool(true))
    );
    assert_eq!(
        h.prop(scroll, Prop::ContentSize),
        Some(&PropValue::F32Array(vec![900.0, 10.0]))
    );
}

#[test]
fn handlers_refs_and_test_ids() {
    let mut h = H::new();
    let mut button = view("p_2", vec![text("", "Tap")]);
    button.handlers = vec![(EventType::Click, 7), (EventType::LongPress, 8)];
    button.ref_id = Some("btn".into());
    let button = with_prop(button, Prop::TestId, PropValue::Str("save".into()));
    h.commit(&root(view("", vec![button.clone()])));
    let node = h.tree.node_for_ref("btn").unwrap();
    assert_eq!(h.tree.find_by_test_id("save"), Some(node));
    assert_eq!(h.tree.handler(node, EventType::Click), Some(7));
    assert_eq!(h.tree.handler(node, EventType::Pan), None);
    assert_eq!(
        h.rep.nodes[&node].mask,
        event_mask::CLICK | event_mask::LONG_PRESS
    );

    let mut fewer = button.clone();
    fewer.handlers = vec![(EventType::Click, 9)];
    let batch = h.commit(&root(view("", vec![fewer])));
    assert_eq!(
        batch,
        vec![Mutation::SetEventMask {
            node,
            mask: event_mask::CLICK
        }]
    );
    assert_eq!(h.tree.handler(node, EventType::Click), Some(9));
    assert!(h.tree.dump().contains("test_id=\"save\""));
}

#[test]
fn controls_get_default_sizes() {
    let mut h = H::new();
    let s = root(view(
        "items_start",
        vec![
            el(ElementKind::Switch, ""),
            el(ElementKind::Checkbox, ""),
            el(ElementKind::Spinner, ""),
            el(ElementKind::Slider, "w=200"),
            el(ElementKind::Progress, "w_full"),
            el(ElementKind::Segmented, ""),
            el(ElementKind::Select, ""),
            el(ElementKind::Switch, "w=60 h=40"),
            with_prop(
                el(ElementKind::TextInput, "w_full"),
                Prop::Multiline,
                PropValue::Bool(true),
            ),
        ],
    ));
    h.commit(&s);
    let sizes = |t| {
        h.of_type(t)
            .iter()
            .map(|n| (h.frame(*n).width, h.frame(*n).height))
            .collect::<Vec<_>>()
    };
    assert_eq!(sizes(ViewType::Switch), vec![(51.0, 31.0), (60.0, 40.0)]);
    assert_eq!(sizes(ViewType::Checkbox), vec![(24.0, 24.0)]);
    assert_eq!(sizes(ViewType::Spinner), vec![(24.0, 24.0)]);
    assert_eq!(sizes(ViewType::Slider), vec![(200.0, 32.0)]);
    assert_eq!(sizes(ViewType::Progress), vec![(390.0, 4.0)]);
    assert_eq!(sizes(ViewType::Select)[0].1, 36.0);
    assert_eq!(sizes(ViewType::Segmented)[0].1, 32.0);
    let input = sizes(ViewType::TextInput)[0];
    assert!(approx(input.1, 16.0 * 1.2 * 3.0 + 16.0), "{input:?}");
}

#[test]
fn deep_and_wide_trees_round_trip_through_random_edits() {
    // Pseudo-random keyed edits over a nested tree; every batch must replay cleanly.
    let mut h = H::new();
    let mut seed = 0x9E37_79B9u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    for round in 0..60 {
        let mut groups = Vec::new();
        for g in 0..(next() % 6) {
            let mut items = Vec::new();
            let mut keys: Vec<u32> = (0..8).filter(|_| next() % 3 != 0).collect();
            if next() % 2 == 0 {
                keys.reverse();
            }
            for k in keys {
                let spec = match next() % 4 {
                    0 => "p_1",
                    1 => "p_1 bg=#ff0000",
                    2 => "border_1 h=4",
                    _ => "hidden",
                };
                let inner = if next() % 2 == 0 {
                    vec![text("", &format!("t{k}"))]
                } else {
                    vec![view("", vec![view("bg=#00ff00 h=2", vec![])])]
                };
                items.push(keyed(view(spec, inner), &format!("k{k}")));
            }
            let spec = if next() % 3 == 0 {
                "flex_row bg=#0000ff"
            } else {
                "flex_row"
            };
            groups.push(keyed(view(spec, items), &format!("g{g}")));
        }
        if round % 10 == 9 {
            groups.reverse();
        }
        h.commit(&root(view("", groups)));
    }
}

/// Builds a column of `rows` rows of three nodes each: a native row with a text and a swatch.
fn bench_tree(rows: usize, changed: Option<usize>) -> Vec<Surface> {
    let items = (0..rows)
        .map(|i| {
            let bg = if changed == Some(i) {
                "flex flex_row p_1 bg=#00ff00"
            } else {
                "flex flex_row p_1 bg=#ff0000"
            };
            keyed(
                view(
                    bg,
                    vec![
                        text("", &format!("Item {i}")),
                        view("w=10 h=10 bg=#0000ff", vec![]),
                    ],
                ),
                &i.to_string(),
            )
        })
        .collect();
    root(view("overflow_y_scroll size_full", items))
}

#[test]
fn bench_mount_and_single_prop_update() {
    use native_shell_core::desc::CommitInput;
    use native_shell_tree::{testing::FakeMeasurer, Tree};
    let entities = Default::default();
    let median = |mut v: Vec<std::time::Duration>| {
        v.sort();
        v[v.len() / 2]
    };
    for nodes in [1_000usize, 3_000, 10_000] {
        let rows = nodes / 3;
        let initial = bench_tree(rows, None);
        let changed = bench_tree(rows, Some(rows / 2));
        let mut tree = Tree::new();
        let t = Instant::now();
        let mount = tree.commit(
            &CommitInput {
                surfaces: &initial,
                entities: &entities,
            },
            &FakeMeasurer,
        );
        let mount_time = t.elapsed();
        let (mut updates, mut noops) = (Vec::new(), Vec::new());
        for i in 0..21 {
            let next = if i % 2 == 0 { &changed } else { &initial };
            let t = Instant::now();
            let update = tree.commit(
                &CommitInput {
                    surfaces: next,
                    entities: &entities,
                },
                &FakeMeasurer,
            );
            updates.push(t.elapsed());
            assert_eq!(update.len(), 1);
            let t = Instant::now();
            let noop = tree.commit(
                &CommitInput {
                    surfaces: next,
                    entities: &entities,
                },
                &FakeMeasurer,
            );
            noops.push(t.elapsed());
            assert!(noop.is_empty());
        }
        println!(
            "bench {nodes:>6} nodes ({} natives, {} ops): mount {:>8.2?}  single-prop update {:>8.2?}  no-op {:>8.2?} (medians of 21)",
            tree.native_count(),
            mount.len(),
            mount_time,
            median(updates),
            median(noops),
        );
    }
}

#[test]
fn style_derived_props() {
    let mut h = H::new();
    let mut r = (*st("shadow_md overflow_hidden rotate=45")).clone();
    r.visual.gradient = Some((90.0, vec![(0xFF000000, 0.0), (0xFFFFFFFF, 1.0)]));
    r.active_opacity = Some(0.6);
    r.transitions = st("transition=opacity").transitions.clone();
    let mut d = view("", vec![]);
    d.style = Rc::new(r);
    h.commit(&root(view(
        "",
        vec![d, view("invisible", vec![]), view("hidden", vec![])],
    )));
    let views = h.of_type(ViewType::View);
    let node = views[1];
    for p in [
        Prop::Shadow,
        Prop::Gradient,
        Prop::Transform,
        Prop::Animation,
    ] {
        assert!(matches!(h.prop(node, p), Some(PropValue::Json(_))), "{p:?}");
    }
    assert_eq!(
        h.prop(node, Prop::ClipsToBounds),
        Some(&PropValue::Bool(true))
    );
    assert_eq!(
        h.prop(node, Prop::ActiveOpacity),
        Some(&PropValue::F32(0.6))
    );
    let Some(PropValue::Json(anim)) = h.prop(node, Prop::Animation) else {
        panic!()
    };
    assert!(anim.starts_with("{\"opacity\":{"), "{anim}");
    assert_eq!(h.prop(views[2], Prop::Hidden), Some(&PropValue::Bool(true)));
    assert_eq!(h.prop(views[3], Prop::Hidden), Some(&PropValue::Bool(true)));
    assert_eq!(h.frame(views[3]).height, 0.0);
}

#[test]
fn json_props_follow_the_schema() {
    let mut h = H::new();
    let mut runs = text("line_height=1.5 truncate font_bold", "a");
    if let ElementKind::Text(r) = &mut runs.kind {
        r.push(native_shell_core::desc::TextRun {
            text: "b".into(),
            style: native_shell_core::style::TextStyle {
                size: Some(20.0),
                underline: Some(true),
                italic: Some(false),
                ..Default::default()
            },
            url: Some("https://example.org".into()),
        });
    }
    let nowrap = text("whitespace_nowrap line_height=24px", "c");
    let field = keyed(
        with_prop(
            el(ElementKind::TextInput, "p_2 px_3"),
            Prop::TextValue,
            PropValue::Str("".into()),
        ),
        "field",
    );
    let menu = "{\"items\":[{\"label\":\"Name\",\"value\":\"name\"},{\"label\":\"Date\",\"value\":\"date\"}]}";
    let select = with_prop(
        with_prop(
            el(ElementKind::Select, ""),
            Prop::Menu,
            PropValue::Json(menu.into()),
        ),
        Prop::Value,
        PropValue::Str("date".into()),
    );
    let mut s = root(view("", vec![runs, nowrap, field, select]));
    s.push(Surface {
        key: "sheet".into(),
        kind: SurfaceKind::Sheet {
            detents: vec!["medium".into(), "{\"height\":320}".into()],
            dismissable: true,
        },
        content: view("h=10", vec![]),
    });
    h.commit(&s);
    let texts = h.of_type(ViewType::Text);
    let Some(PropValue::Json(a)) = h.prop(texts[0], Prop::AttributedText) else {
        panic!()
    };
    assert_eq!(
        a,
        "{\"runs\":[{\"text\":\"a\",\"size\":16,\"weight\":700,\"color\":4278848010,\"line_height\":24},\
         {\"text\":\"b\",\"size\":20,\"weight\":700,\"color\":4278848010,\"underline\":true,\"line_height\":30,\
         \"url\":\"https://example.org\"}],\"lines\":1,\"truncate\":true}"
    );
    let Some(PropValue::Json(c)) = h.prop(texts[1], Prop::AttributedText) else {
        panic!()
    };
    assert_eq!(c, "{\"runs\":[{\"text\":\"c\",\"size\":16,\"weight\":400,\"color\":4278848010,\"line_height\":24}],\"lines\":1}");
    assert!(!a.contains("null") && !c.contains("null"));
    assert_eq!(
        h.frame(texts[1]).height,
        24.0,
        "the measurer honors line_height"
    );

    let input = h.of_type(ViewType::TextInput)[0];
    assert_eq!(
        h.prop(input, Prop::ContentInsets),
        Some(&PropValue::F32Array(vec![8.0, 12.0, 8.0, 12.0]))
    );
    let select = h.of_type(ViewType::Select)[0];
    assert_eq!(
        h.prop(select, Prop::SelectedIndex),
        Some(&PropValue::I32(1))
    );
    assert_eq!(h.prop(select, Prop::Value), None);
    let sheet = h.tree.surface_node("sheet").unwrap();
    assert_eq!(
        h.prop(sheet, Prop::Detents),
        Some(&PropValue::Json("[\"medium\",{\"height\":320}]".into()))
    );

    // Padding removed: the insets are reset.
    let field = keyed(
        with_prop(
            el(ElementKind::TextInput, ""),
            Prop::TextValue,
            PropValue::Str("".into()),
        ),
        "field",
    );
    let batch = h.commit(&root(view("", vec![field])));
    assert!(
        batch.contains(&Mutation::SetProp {
            node: input,
            prop: Prop::ContentInsets,
            value: PropValue::Null
        }),
        "{batch:#?}"
    );
}

#[test]
fn font_scale_remeasures_without_scaling_the_json() {
    let mut h = H::new();
    let field = with_prop(
        el(ElementKind::TextInput, "text_xl line_height=1.5"),
        Prop::TextValue,
        PropValue::Str("".into()),
    );
    let s = root(view(
        "",
        vec![text("letter_spacing=0.4 line_height=1.5", "Hello"), field],
    ));
    h.commit(&s);
    let t = h.of_type(ViewType::Text)[0];
    let input = h.of_type(ViewType::TextInput)[0];
    let json = |h: &H, n, p| match h.prop(n, p) {
        Some(PropValue::Json(j)) => j.clone(),
        other => panic!("{other:?}"),
    };
    let text_before = json(&h, t, Prop::AttributedText);
    let style_before = json(&h, input, Prop::TextStyle);
    assert_eq!(text_before, "{\"runs\":[{\"text\":\"Hello\",\"size\":16,\"weight\":400,\"color\":4278848010,\"line_height\":24,\"letter_spacing\":0.4}]}");
    assert_eq!(
        style_before,
        "{\"size\":20,\"weight\":400,\"color\":4278848010,\"line_height\":30}"
    );
    let input_height = h.frame(input).height;
    let batches = h.tree.stats().batch_calls;

    h.tree.set_environment(&Environment {
        font_scale: 1.5,
        scale: 3.0,
        ..Default::default()
    });
    let batch = h.commit(&s);
    assert!(
        batch.iter().all(|m| !matches!(m, Mutation::SetProp { .. })),
        "{batch:#?}"
    );
    assert!(h.tree.stats().batch_calls > batches, "measured again");
    assert_eq!(json(&h, t, Prop::AttributedText), text_before);
    assert_eq!(json(&h, input, Prop::TextStyle), style_before);
    assert_eq!(
        h.frame(input).height,
        input_height,
        "default height is in unscaled points"
    );
}

fn ent(id: u32) -> native_shell_core::desc::Desc {
    el(ElementKind::Entity(id), "")
}

fn texts_in(batch: &[Mutation]) -> Vec<String> {
    batch
        .iter()
        .filter_map(|m| match m {
            Mutation::SetProp {
                prop: Prop::AttributedText,
                value: PropValue::Json(j),
                ..
            } => Some(j.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn dirty_entities_under_clean_entities_are_updated() {
    let mut h = H::new();
    let s = root(view("", vec![ent(1)]));
    h.entities
        .insert(1, Rc::new(view("p_2", vec![text("", "parent"), ent(2)])));
    h.entities
        .insert(2, Rc::new(view("", vec![text("", "child v1"), ent(3)])));
    h.entities
        .insert(3, Rc::new(view("", vec![text("", "grandchild v1")])));
    h.commit(&s);
    assert!(h.commit(&s).is_empty(), "no-op recommit");

    // Parent clean, child dirty.
    h.entities.insert(
        2,
        Rc::new(view(
            "",
            vec![text("", &"child v2 wraps ".repeat(20)), ent(3)],
        )),
    );
    let batch = h.commit(&s);
    let t = texts_in(&batch);
    assert_eq!(t.len(), 1, "{batch:#?}");
    assert!(t[0].contains("child v2"));
    assert!(
        batch.iter().any(|m| matches!(m, Mutation::SetFrame { .. })),
        "relaid out: {batch:#?}"
    );
    assert!(h.commit(&s).is_empty());

    // Grandchild dirty under two clean levels.
    h.entities
        .insert(3, Rc::new(view("", vec![text("", "grandchild v2")])));
    let batch = h.commit(&s);
    assert_eq!(texts_in(&batch).len(), 1, "{batch:#?}");
    assert!(texts_in(&batch)[0].contains("grandchild v2"));
    assert!(h.commit(&s).is_empty());
}

#[test]
fn nested_entities_added_removed_and_recreated_by_their_own_render() {
    let mut h = H::new();
    let s = root(view("", vec![ent(1)]));
    h.entities.insert(1, Rc::new(view("p_2", vec![ent(2)])));
    h.entities.insert(2, Rc::new(view("", vec![ent(3)])));
    h.entities.insert(3, Rc::new(text("", "three")));
    h.commit(&s);

    // Entity 2 re-renders with a new child entity 4; entity 1 does not.
    h.entities.insert(4, Rc::new(text("", "four v1")));
    h.entities
        .insert(2, Rc::new(view("", vec![ent(3), ent(4)])));
    let batch = h.commit(&s);
    assert_eq!(created(&batch).len(), 1, "{batch:#?}");
    assert_eq!(h.of_type(ViewType::Text).len(), 2);

    // Only the new entity re-renders: entity 2 must know about it.
    h.entities.insert(4, Rc::new(text("", "four v2")));
    let batch = h.commit(&s);
    assert!(
        texts_in(&batch).iter().any(|t| t.contains("four v2")),
        "{batch:#?}"
    );

    // A nested entity changes kind (text → view): recreated in place under clean parents.
    let old_three = h.of_type(ViewType::Text)[0];
    h.entities
        .insert(3, Rc::new(view("bg=#ff0000 h=10", vec![])));
    let batch = h.commit(&s);
    assert!(
        batch.contains(&Mutation::Delete { node: old_three }),
        "{batch:#?}"
    );
    assert_eq!(created(&batch).len(), 1);
    // And it is still tracked: changing it again updates it.
    h.entities
        .insert(3, Rc::new(view("bg=#00ff00 h=10", vec![])));
    let batch = h.commit(&s);
    assert_eq!(batch.len(), 1, "{batch:#?}");

    // Entity 2 drops its children.
    h.entities.insert(2, Rc::new(view("", vec![])));
    let batch = h.commit(&s);
    assert_eq!(
        count(&batch, |m| matches!(m, Mutation::Delete { .. })),
        2,
        "{batch:#?}"
    );
    assert!(h.of_type(ViewType::Text).is_empty());
    assert!(h.commit(&s).is_empty());
}

#[test]
fn native_text_inside_a_clean_entity_is_reverted() {
    let mut h = H::new();
    let s = root(view("", vec![ent(1)]));
    let field = with_prop(
        el(ElementKind::TextInput, ""),
        Prop::TextValue,
        PropValue::Str("ab".into()),
    );
    h.entities.insert(1, Rc::new(view("", vec![ent(2)])));
    h.entities.insert(2, Rc::new(view("p_2", vec![field])));
    h.commit(&s);
    let input = h.of_type(ViewType::TextInput)[0];
    h.tree.record_native_text(input, "abc", 1);
    let batch = h.commit(&s);
    assert_eq!(
        batch,
        vec![Mutation::SetProp {
            node: input,
            prop: Prop::TextValue,
            value: PropValue::Json("{\"text\":\"ab\",\"count\":1}".into())
        }]
    );
    assert!(h.commit(&s).is_empty());
}

#[test]
fn block_and_flex_scroll_content_does_not_shrink() {
    let mut h = H::new();
    let kids = || {
        vec![
            view("h=300 bg=#ff0000", vec![]),
            view("h=50 bg=#00ff00", vec![]),
        ]
    };
    // Block: children stack at full width and overflow.
    h.commit(&root(view(
        "",
        vec![view("overflow_y_scroll h=100", kids())],
    )));
    let scroll = h.of_type(ViewType::Scroll)[0];
    let first = h.rep.children(scroll)[0];
    assert_eq!(h.frame(first).width, 390.0, "block children stretch");
    assert_eq!(
        h.prop(scroll, Prop::ContentSize),
        Some(&PropValue::F32Array(vec![390.0, 350.0]))
    );
    // Flex column: without the rule both children would shrink to fit 100 pt.
    h.commit(&root(view(
        "",
        vec![view("flex flex_col overflow_y_scroll h=100", kids())],
    )));
    let scroll = h.of_type(ViewType::Scroll)[0];
    let heights: Vec<f32> = h
        .rep
        .children(scroll)
        .iter()
        .map(|c| h.frame(*c).height)
        .collect();
    assert_eq!(heights, vec![300.0, 50.0]);
    // Flex row scrolling horizontally.
    let wide = || {
        vec![
            view("w=300 h=10 bg=#ff0000", vec![]),
            view("w=300 h=10 bg=#00ff00", vec![]),
        ]
    };
    h.commit(&root(view(
        "",
        vec![view("flex flex_row overflow_x_scroll", wide())],
    )));
    let scroll = h.of_type(ViewType::Scroll)[0];
    let widths: Vec<f32> = h
        .rep
        .children(scroll)
        .iter()
        .map(|c| h.frame(*c).width)
        .collect();
    assert_eq!(widths, vec![300.0, 300.0]);
    assert_eq!(
        h.prop(scroll, Prop::ContentSize),
        Some(&PropValue::F32Array(vec![600.0, 10.0]))
    );
}

#[test]
fn single_axis_clip_and_first_shadow_layer() {
    let mut h = H::new();
    h.commit(&root(view(
        "",
        vec![
            view("overflow_x_hidden h=10", vec![]),
            view("shadow_lg h=10", vec![]),
        ],
    )));
    let views = h.of_type(ViewType::View);
    assert_eq!(
        h.prop(views[1], Prop::ClipsToBounds),
        Some(&PropValue::Bool(true))
    );
    let style = st("shadow_lg h=10");
    let first = style
        .visual
        .shadows
        .iter()
        .find(|s| !s.inset)
        .expect("a shadow layer");
    let Some(PropValue::Json(json)) = h.prop(views[2], Prop::Shadow) else {
        panic!()
    };
    assert_eq!(*json, serde_json::to_string(first).unwrap());
    assert!(!json.contains("inset"));
}

/// Wraps as soon as a line is wider than the width, with no tolerance at all.
struct StrictMeasurer;

impl native_shell_core::host::Measurer for StrictMeasurer {
    fn measure_text(
        &self,
        json: &str,
        width: native_shell_core::host::WidthMode,
    ) -> native_shell_core::host::TextMetrics {
        use native_shell_core::host::WidthMode;
        // FakeMeasurer lets a line overflow by 0.001; take it back.
        let width = match width {
            WidthMode::AtMost(w) => WidthMode::AtMost(w - 0.001),
            WidthMode::Exactly(w) => WidthMode::Exactly(w - 0.001),
            WidthMode::Undefined => WidthMode::Undefined,
        };
        let mut m = native_shell_tree::testing::FakeMeasurer.measure_text(json, width);
        if let WidthMode::Exactly(w) = width {
            m.width = w + 0.001;
        }
        m
    }
}

#[test]
fn text_heights_match_their_final_width_at_pixel_boundaries() {
    use native_shell_core::desc::CommitInput;
    use native_shell_core::host::{Measurer, WidthMode};
    use native_shell_tree::testing::Replayer;
    use native_shell_tree::Tree;

    // "1 left" at 10.4133 pt is about 31.24 pt wide: not on any quarter-point grid.
    let label = || text("text_size=10.4133", "1 left");
    let entities = Default::default();
    let json = attributed(&root(view("", vec![label()])));
    let natural = StrictMeasurer.measure_text(&json, WidthMode::Undefined);
    let line = natural.height;
    assert!((natural.width - 31.24).abs() < 0.01, "{natural:?}");

    // The text fits a box as wide as it is; a box one device pixel (1/3 pt) narrower
    // makes it wrap, and the frames agree either way. Narrower by less than a pixel still
    // fits: text wraps at pixel resolution, at the width the renderer will draw it in.
    for (box_width, lines) in [
        (natural.width, 1.0),
        (natural.width - 0.04, 1.0),
        (natural.width - 0.34, 2.0),
    ] {
        let mut tree = Tree::new();
        let mut rep = Replayer::new();
        let s = root(view(
            "",
            vec![view(&format!("w={box_width} bg=#eeeeee"), vec![label()])],
        ));
        let batch = tree.commit(
            &CommitInput {
                surfaces: &s,
                entities: &entities,
            },
            &StrictMeasurer,
        );
        rep.apply(&batch).unwrap();
        let boxed = rep
            .nodes
            .iter()
            .find(|(_, n)| {
                n.view_type == ViewType::View && n.props.contains_key(&Prop::BackgroundColor)
            })
            .map(|(id, _)| *id)
            .unwrap();
        let t = rep.children(boxed)[0];
        let (bf, tf) = (
            rep.nodes[&boxed].frame.unwrap(),
            rep.nodes[&t].frame.unwrap(),
        );
        assert!(
            approx(tf.height, lines * line),
            "{box_width}: {tf:?}, line {line}"
        );
        assert!(
            tf.y + tf.height <= bf.height + 1e-3,
            "child {tf:?} overflows its parent {bf:?}"
        );
        let again = StrictMeasurer.measure_text(&json, WidthMode::Exactly(tf.width));
        assert!(
            approx(again.height, tf.height),
            "measured at the final width: {again:?} vs {tf:?}"
        );
    }

    // A title and a count in a justify_between row: neither collapses, neither wraps,
    // and the row is as tall as its tallest child.
    let mut tree = Tree::new();
    let mut rep = Replayer::new();
    let s = root(view(
        "flex flex_row justify_between items_center w_full bg=#eeeeee",
        vec![text("text_xl font_semibold", "Inbox"), label()],
    ));
    let batch = tree.commit(
        &CommitInput {
            surfaces: &s,
            entities: &entities,
        },
        &StrictMeasurer,
    );
    rep.apply(&batch).unwrap();
    let row = rep
        .nodes
        .iter()
        .find(|(_, n)| n.props.contains_key(&Prop::BackgroundColor))
        .map(|(id, _)| *id)
        .unwrap();
    let kids = rep.children(row).to_vec();
    let (title, count) = (
        rep.nodes[&kids[0]].frame.unwrap(),
        rep.nodes[&kids[1]].frame.unwrap(),
    );
    let rf = rep.nodes[&row].frame.unwrap();
    assert_eq!(
        title.width, 50.0,
        "unwrapped width, ceiled to whole points: {title:?}"
    );
    assert_eq!(count.width, 32.0, "{count:?}");
    assert!(approx(count.height, line), "{count:?}");
    assert_eq!(count.x + count.width, 390.0);
    for f in [title, count] {
        assert!(
            f.y >= 0.0 && f.y + f.height <= rf.height + 1e-3,
            "{f:?} in {rf:?}"
        );
    }
}

/// The attributed JSON of the single text in a one-surface tree.
fn attributed(s: &[Surface]) -> String {
    let mut h = H::new();
    h.commit(s);
    let t = h.of_type(ViewType::Text)[0];
    match h.prop(t, Prop::AttributedText) {
        Some(PropValue::Json(j)) => j.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn fake_measurer_min_content_is_the_widest_word() {
    use native_shell_core::host::{Measurer, WidthMode};
    let json = attributed(&root(view("", vec![text("", "a bbbb cc")])));
    let m = native_shell_tree::testing::FakeMeasurer.measure_text(&json, WidthMode::AtMost(0.0));
    assert_eq!(m.width, 32.0);
    assert!(approx(m.height, 3.0 * LINE));
}

/// The gpui-todolist screen: text in block `div`s inside `h_flex` rows, around a list
/// that changes while the texts do not.
fn todo_screen(items: &[&str]) -> Vec<Surface> {
    let h_flex = "flex flex_row items_center";
    let label = |spec: &str, s: &str| view("", vec![text(spec, s)]);
    let header = view(
        &format!("{h_flex} justify_between"),
        vec![
            view(
                &format!("{h_flex} gap_3"),
                vec![
                    view("w=18 h=18 bg=#000000", vec![]),
                    label("text_size=16 line_height=1 font_semibold", "Todo"),
                ],
            ),
            label(
                "text_size=12 line_height=1",
                &if items.is_empty() {
                    "Nothing yet".to_string()
                } else {
                    format!("{} of {} remaining", items.len(), items.len())
                },
            ),
        ],
    );
    let rows: Vec<_> = items
        .iter()
        .map(|i| {
            keyed(
                view(
                    &format!("{h_flex} justify_between p_3"),
                    vec![
                        label("text_size=12", i),
                        view("w=28 h=28 bg=#eeeeee", vec![]),
                    ],
                ),
                i,
            )
        })
        .collect();
    let list = view(
        "flex flex_col flex_1 min_h_0 border_1",
        vec![view("flex flex_col flex_1", rows)],
    );
    let footer = view(
        &format!("{h_flex} justify_between"),
        vec![
            label("text_size=12 line_height=1", "Saved"),
            label(
                "text_size=12 line_height=1",
                &format!("{} completed", items.len()),
            ),
        ],
    );
    root(view(
        "flex flex_col size_full p_6 gap_4",
        vec![header, list, footer],
    ))
}

#[test]
fn unchanged_texts_keep_their_size_when_a_sibling_list_relays_out() {
    let mut h = H::new();
    h.commit(&todo_screen(&[]));
    let todo = h.tree.surface_node("root").map(|_| ()).and_then(|_| {
        h.of_type(ViewType::Text).into_iter().find(|t| matches!(h.prop(*t, Prop::AttributedText), Some(PropValue::Json(j)) if j.contains("\"Todo\"")))
    });
    let saved = h.of_type(ViewType::Text).into_iter().find(|t| matches!(h.prop(*t, Prop::AttributedText), Some(PropValue::Json(j)) if j.contains("\"Saved\"")));
    let (todo, saved) = (todo.unwrap(), saved.unwrap());
    let sizes = |h: &H| {
        (
            (h.frame(todo).width, h.frame(todo).height),
            (h.frame(saved).width, h.frame(saved).height),
        )
    };
    let initial = sizes(&h);
    assert_eq!(initial, ((32.0, 16.0), (30.0, 12.0)));
    let steps: [&[&str]; 5] = [
        &["Buy milk"],
        &["Buy milk", "Write tests"],
        &["Write tests"],
        &[],
        &["a", "b", "c", "d"],
    ];
    for items in steps {
        h.commit(&todo_screen(items));
        assert_eq!(sizes(&h), initial, "after {items:?}:\n{}", h.tree.dump());
    }
}

/// Counts calls and forwards to the FakeMeasurer.
#[derive(Default)]
struct Counting {
    singles: std::cell::Cell<u32>,
    batches: std::cell::Cell<u32>,
    batched: std::cell::Cell<u32>,
}

impl native_shell_core::host::Measurer for Counting {
    fn measure_text(
        &self,
        json: &str,
        width: native_shell_core::host::WidthMode,
    ) -> native_shell_core::host::TextMetrics {
        self.singles.set(self.singles.get() + 1);
        native_shell_tree::testing::FakeMeasurer.measure_text(json, width)
    }

    fn measure_text_batch(
        &self,
        requests: &[(&str, native_shell_core::host::WidthMode)],
    ) -> Vec<native_shell_core::host::TextMetrics> {
        self.batches.set(self.batches.get() + 1);
        self.batched.set(self.batched.get() + requests.len() as u32);
        requests
            .iter()
            .map(|(j, w)| native_shell_tree::testing::FakeMeasurer.measure_text(j, *w))
            .collect()
    }
}

#[test]
fn changed_single_line_texts_cost_one_batch_call() {
    use native_shell_core::desc::CommitInput;
    use native_shell_tree::Tree;
    let entities = Default::default();
    let screen = |v: u32| {
        let rows = (0..500)
            .map(|i| {
                keyed(
                    view(
                        "flex flex_row items_center",
                        vec![text("", &format!("Item {i} v{v}")), text("", "x")],
                    ),
                    &i.to_string(),
                )
            })
            .collect();
        root(view("flex flex_col size_full", rows))
    };
    let mut tree = Tree::new();
    let m = Counting::default();
    let mut rep = native_shell_tree::testing::Replayer::new();
    for v in 0..3 {
        let (s0, b0) = (m.singles.get(), m.batches.get());
        rep.apply(&tree.commit(
            &CommitInput {
                surfaces: &screen(v),
                entities: &entities,
            },
            &m,
        ))
        .unwrap();
        assert_eq!(m.batches.get() - b0, 1, "one batch per commit");
        assert_eq!(
            m.singles.get() - s0,
            0,
            "no single calls for texts that fit"
        );
    }
    // 500 texts changed per commit, plus the shared "x" once.
    assert_eq!(m.batched.get(), 501 + 500 + 500);
    // A no-op commit measures nothing.
    let (s0, b0) = (m.singles.get(), m.batches.get());
    assert!(tree
        .commit(
            &CommitInput {
                surfaces: &screen(2),
                entities: &entities
            },
            &m
        )
        .is_empty());
    assert_eq!((m.singles.get(), m.batches.get()), (s0, b0));
    let s = tree.stats();
    assert_eq!(
        (s.batch_calls, s.batched_items, s.measure_misses),
        (3, 1501, 0)
    );
    assert!(s.derived_hits > 0);
}

#[test]
fn wrapping_texts_are_still_measured_at_their_width() {
    use native_shell_core::desc::CommitInput;
    use native_shell_tree::Tree;
    let entities = Default::default();
    let long = "lorem ipsum dolor sit amet ".repeat(6);
    let s = root(view("p_4", vec![text("", &long), text("", "short")]));
    let m = Counting::default();
    let mut tree = Tree::new();
    let batch = tree.commit(
        &CommitInput {
            surfaces: &s,
            entities: &entities,
        },
        &m,
    );
    assert_eq!((m.batches.get(), m.batched.get()), (1, 2));
    assert!(m.singles.get() >= 1, "the long text is measured at 358 pt");
    let mut rep = native_shell_tree::testing::Replayer::new();
    rep.apply(&batch).unwrap();
    let heights: Vec<f32> = rep
        .nodes
        .values()
        .filter(|n| n.view_type == ViewType::Text)
        .map(|n| n.frame.unwrap().height)
        .collect();
    assert!(
        heights.iter().any(|h| approx(*h, 4.0 * LINE)) && heights.iter().any(|h| approx(*h, LINE)),
        "{heights:?}"
    );
}

/// Measures without the batch shortcut: every probe is a single call.
struct Unbatched;

impl native_shell_core::host::Measurer for Unbatched {
    fn measure_text(
        &self,
        json: &str,
        width: native_shell_core::host::WidthMode,
    ) -> native_shell_core::host::TextMetrics {
        native_shell_tree::testing::FakeMeasurer.measure_text(json, width)
    }

    fn measure_text_batch(
        &self,
        requests: &[(&str, native_shell_core::host::WidthMode)],
    ) -> Vec<native_shell_core::host::TextMetrics> {
        // Returning nothing leaves the cache empty, so the layout falls back to single calls.
        let _ = requests;
        Vec::new()
    }
}

#[test]
fn batched_layout_equals_unbatched_layout() {
    use native_shell_core::desc::CommitInput;
    use native_shell_tree::testing::Replayer;
    use native_shell_tree::Tree;
    let entities = Default::default();
    let mut seed = 0x2545_F491u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let words = [
        "a",
        "lorem",
        "ipsum",
        "中文",
        "dolor",
        "sit",
        "amet,",
        "😀",
        "consectetur",
        "x",
    ];
    for _ in 0..40 {
        let mut children = Vec::new();
        for _ in 0..12 {
            let n = 1 + next() % 12;
            let t: Vec<&str> = (0..n)
                .map(|_| words[(next() % words.len() as u32) as usize])
                .collect();
            let spec = [
                "",
                "text_xl",
                "text_size=11.3",
                "truncate",
                "line_height=1.2 text_sm",
            ][(next() % 5) as usize];
            let width = 20 + next() % 300;
            let wrapper = ["", "flex flex_row", "flex flex_col items_start"][(next() % 3) as usize];
            children.push(view(
                &format!("w={width}.37 {wrapper}"),
                vec![text(spec, &t.join(" ")), text("", "tail")],
            ));
        }
        let s = root(view("flex flex_col p_2 gap_1", children));
        let frames = |m: &dyn native_shell_core::host::Measurer| {
            let mut tree = Tree::new();
            let mut rep = Replayer::new();
            rep.apply(&tree.commit(
                &CommitInput {
                    surfaces: &s,
                    entities: &entities,
                },
                m,
            ))
            .unwrap();
            let mut f: Vec<(u32, [u32; 4])> = rep
                .nodes
                .iter()
                .map(|(id, n)| {
                    let f = n.frame.unwrap();
                    (*id, [f.x, f.y, f.width, f.height].map(f32::to_bits))
                })
                .collect();
            f.sort();
            f
        };
        assert_eq!(
            frames(&native_shell_tree::testing::FakeMeasurer),
            frames(&Unbatched)
        );
    }
}

/// Records component measurements; answers them like the FakeMeasurer, or not at all.
#[derive(Default)]
struct ComponentLog {
    calls: std::cell::RefCell<Vec<(String, String)>>,
    unknown: bool,
}

impl native_shell_core::host::Measurer for ComponentLog {
    fn measure_text(
        &self,
        json: &str,
        width: native_shell_core::host::WidthMode,
    ) -> native_shell_core::host::TextMetrics {
        native_shell_tree::testing::FakeMeasurer.measure_text(json, width)
    }

    fn measure_component(
        &self,
        name: &str,
        props: &str,
        width: native_shell_core::host::WidthMode,
    ) -> Option<(f32, f32)> {
        self.calls
            .borrow_mut()
            .push((name.to_string(), props.to_string()));
        if self.unknown {
            return None;
        }
        native_shell_tree::testing::FakeMeasurer.measure_component(name, props, width)
    }
}

fn controls_row(select_value: Option<&str>, picker_spec: &str, extra: &str) -> Vec<Surface> {
    let menu = "{\"placeholder\":\"Choose…\",\"items\":[{\"label\":\"Name\",\"value\":\"name\"},{\"label\":\"Weekly\",\"value\":\"weekly\"}]}";
    let mut select = with_prop(
        el(ElementKind::Select, ""),
        Prop::Menu,
        PropValue::Json(menu.into()),
    );
    if let Some(v) = select_value {
        select = with_prop(select, Prop::Value, PropValue::Str(v.into()));
    }
    let picker = with_prop(
        with_prop(
            el(ElementKind::DatePicker, picker_spec),
            Prop::Date,
            PropValue::Str("09:30".into()),
        ),
        Prop::DateMode,
        PropValue::I32(1),
    );
    root(view(
        "flex flex_row items_center gap_2",
        vec![
            keyed(select, "s"),
            keyed(picker, "p"),
            keyed(text("", extra), "t"),
        ],
    ))
}

fn commit_with(
    tree: &mut native_shell_tree::Tree,
    rep: &mut native_shell_tree::testing::Replayer,
    s: &[Surface],
    m: &dyn native_shell_core::host::Measurer,
) -> Vec<Mutation> {
    let entities = Default::default();
    let batch = tree.commit(
        &native_shell_core::desc::CommitInput {
            surfaces: s,
            entities: &entities,
        },
        m,
    );
    rep.apply(&batch).unwrap();
    batch
}

#[test]
fn select_and_date_picker_are_measured_by_the_host() {
    let (mut tree, mut rep, m) = (
        native_shell_tree::Tree::new(),
        native_shell_tree::testing::Replayer::new(),
        ComponentLog::default(),
    );
    commit_with(
        &mut tree,
        &mut rep,
        &controls_row(Some("weekly"), "", "a"),
        &m,
    );
    let of = |rep: &native_shell_tree::testing::Replayer, t: ViewType| {
        let id = rep
            .nodes
            .iter()
            .find(|(_, n)| n.view_type == t)
            .map(|(id, _)| *id)
            .unwrap();
        let f = rep.nodes[&id].frame.unwrap();
        (f.width, f.height)
    };
    // "Weekly": 6 × 8 + 32; "09:30": 5 × 8 + 24. Neither collapses in the row.
    assert_eq!(of(&rep, ViewType::Select), (80.0, 36.0));
    assert_eq!(of(&rep, ViewType::DatePicker), (64.0, 36.0));
    let calls = m.calls.borrow().clone();
    assert!(
        calls.contains(&(
            "nsh/select".into(),
            "{\"label\":\"Weekly\",\"placeholder\":\"Choose…\"}".into()
        )),
        "{calls:?}"
    );
    assert!(
        calls.contains(&(
            "nsh/date-picker".into(),
            "{\"mode\":\"time\",\"value\":\"09:30\"}".into()
        )),
        "{calls:?}"
    );

    // A sibling changes: the controls' measurements come from the cache.
    let before = m.calls.borrow().len();
    commit_with(
        &mut tree,
        &mut rep,
        &controls_row(Some("weekly"), "", "b"),
        &m,
    );
    assert_eq!(m.calls.borrow().len(), before);

    // Nothing selected: sized to the placeholder.
    commit_with(&mut tree, &mut rep, &controls_row(None, "", "b"), &m);
    assert_eq!(of(&rep, ViewType::Select), (8.0 * 7.0 + 32.0, 36.0));

    // A style width: not measured, default height.
    let before = m.calls.borrow().len();
    commit_with(&mut tree, &mut rep, &controls_row(None, "w=150", "b"), &m);
    assert_eq!(of(&rep, ViewType::DatePicker), (150.0, 36.0));
    assert!(m.calls.borrow()[before..]
        .iter()
        .all(|(n, _)| n != "nsh/date-picker"));

    // Font scale change: measured again.
    let before = m.calls.borrow().len();
    tree.set_environment(&Environment {
        font_scale: 1.3,
        ..Default::default()
    });
    commit_with(&mut tree, &mut rep, &controls_row(None, "w=150", "b"), &m);
    assert!(m.calls.borrow()[before..]
        .iter()
        .any(|(n, _)| n == "nsh/select"));
}

#[test]
fn unmeasurable_controls_fall_back_to_a_minimum_size() {
    let (mut tree, mut rep) = (
        native_shell_tree::Tree::new(),
        native_shell_tree::testing::Replayer::new(),
    );
    let m = ComponentLog {
        unknown: true,
        ..Default::default()
    };
    commit_with(
        &mut tree,
        &mut rep,
        &controls_row(Some("weekly"), "", "a"),
        &m,
    );
    for t in [ViewType::Select, ViewType::DatePicker] {
        let f = rep
            .nodes
            .values()
            .find(|n| n.view_type == t)
            .unwrap()
            .frame
            .unwrap();
        assert_eq!((f.width, f.height), (96.0, 36.0), "{t:?}");
    }
}

#[test]
fn rem_lengths_follow_the_clamped_font_scale() {
    let mut h = H::new();
    let s = root(view(
        "",
        vec![
            keyed(
                view("p_4 size_10 rounded_md border_1 bg=#ff0000", vec![]),
                "rem",
            ),
            keyed(view("w=40 h=40 p=8px bg=#00ff00", vec![]), "px"),
            text("text_sm", "Label"),
        ],
    ));
    h.commit(&s);
    let surface = h.tree.surface_node("root").unwrap();
    let kids = h.rep.children(surface).to_vec();
    let (rem_box, px_box, label) = (kids[0], kids[1], kids[2]);
    let label_json = h.prop(label, Prop::AttributedText).cloned();
    for (font_scale, layout_scale) in [(1.0, 1.0), (1.3, 1.3), (2.0, 1.6), (0.85, 1.0), (1.0, 1.0)]
    {
        h.tree.set_environment(&Environment {
            font_scale,
            ..Default::default()
        });
        let batch = h.commit(&s);
        assert_eq!(h.tree.rem_size(), 16.0 * layout_scale);
        // size_10 is 2.5 rem; p_4 is 1 rem (padding sits inside the border box).
        let f = h.frame(rem_box);
        assert!(
            approx(f.width, 40.0 * layout_scale) && approx(f.height, 40.0 * layout_scale),
            "{font_scale}: {f:?}"
        );
        let Some(PropValue::F32Array(radii)) = h.prop(rem_box, Prop::CornerRadii) else {
            panic!()
        };
        assert!(approx(radii[0], 6.0 * layout_scale), "{radii:?}");
        assert_eq!(
            h.prop(rem_box, Prop::BorderWidths),
            Some(&PropValue::F32Array(vec![1.0; 4])),
            "px border unchanged"
        );
        let p = h.frame(px_box);
        assert_eq!((p.width, p.height), (40.0, 40.0), "px lengths unchanged");
        assert!(
            approx(p.y, 40.0 * layout_scale),
            "stacked below the rem box: {p:?}"
        );
        // Text keeps its unscaled size: renderers apply the font scale, so it is never resent.
        assert_eq!(h.prop(label, Prop::AttributedText).cloned(), label_json);
        assert!(
            batch.iter().all(|m| !matches!(
                m,
                Mutation::SetProp {
                    prop: Prop::AttributedText,
                    ..
                }
            )),
            "{batch:#?}"
        );
    }
}

#[test]
fn hover_cursor_and_tooltip_keep_a_native_view() {
    use native_shell_core::style::{Arg, StyleOp};
    let mut h = H::new();
    let mut hover = StyleOp::new("hover", vec![]);
    hover.nested = Some((
        "hover".into(),
        vec![
            StyleOp::new("opacity", vec![Arg::Num(0.7)]),
            StyleOp::new("bg", vec![Arg::Str("#eeeeee".into())]),
        ],
    ));
    let mut styled = view("", vec![]);
    let theme =
        native_shell_core::theme::Theme::default_for(native_shell_core::theme::Appearance::Light);
    styled.style = Rc::new(
        native_shell_core::style::resolve(&[hover, StyleOp::new("cursor_pointer", vec![])], &theme)
            .unwrap(),
    );
    let tooltip = with_prop(
        view("", vec![]),
        Prop::Tooltip,
        PropValue::Str("Copy link".into()),
    );
    let mut on_hover = view("", vec![]);
    on_hover.handlers = vec![(EventType::Hover, 3)];
    h.commit(&root(view(
        "",
        vec![
            keyed(styled, "a"),
            keyed(tooltip, "b"),
            keyed(on_hover, "c"),
            keyed(view("p_2", vec![]), "flat"),
        ],
    )));
    let surface = h.tree.surface_node("root").unwrap();
    let kids = h.rep.children(surface).to_vec();
    assert_eq!(
        kids.len(),
        3,
        "the plain view is flattened, the others are native:\n{}",
        h.tree.dump()
    );
    assert_eq!(
        h.prop(kids[0], Prop::HoverOpacity),
        Some(&PropValue::F32(0.7))
    );
    assert_eq!(
        h.prop(kids[0], Prop::HoverBackground),
        Some(&PropValue::Color(0xFFEE_EEEE))
    );
    assert_eq!(
        h.prop(kids[0], Prop::Cursor),
        Some(&PropValue::I32(
            native_shell_core::protocol::CursorStyle::PointingHand as i32
        ))
    );
    assert_eq!(
        h.prop(kids[1], Prop::Tooltip),
        Some(&PropValue::Str("Copy link".into()))
    );
    assert_eq!(h.rep.nodes[&kids[2]].mask, event_mask::HOVER);
    assert_eq!(h.tree.handler(kids[2], EventType::Hover), Some(3));

    // Hover styling removed: the props are reset.
    let batch = h.commit(&root(view(
        "",
        vec![keyed(view("bg=#ffffff", vec![]), "a")],
    )));
    assert!(
        batch.iter().any(|m| matches!(
            m,
            Mutation::SetProp {
                prop: Prop::Cursor,
                value: PropValue::Null,
                ..
            }
        )),
        "{batch:#?}"
    );
}

/// A bottom-aligned (chat) list rendering `window` of `count` rows.
fn chat(window: (u32, u32), count: u32) -> Vec<Surface> {
    let spec = ListSpec {
        id: "chat".into(),
        count,
        estimated_row_height: 44.0,
        uniform: false,
    };
    let mut l = with_prop(
        el(ElementKind::List(spec), "flex_1"),
        Prop::ListAlignment,
        PropValue::I32(1),
    );
    l.children = (window.0..window.1)
        .map(|i| {
            let mut r = el(ElementKind::ListRow(i), "p_2");
            r.children = vec![text("", &format!("Message {i}"))];
            r
        })
        .collect();
    root(view("flex flex_col size_full", vec![l]))
}

fn rendered_rows(h: &H) -> Vec<i32> {
    let mut rows: Vec<i32> = h
        .of_type(ViewType::ListRow)
        .iter()
        .filter_map(|r| match h.prop(*r, Prop::RowIndex) {
            Some(PropValue::I32(i)) => Some(*i),
            _ => None,
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn bottom_aligned_lists_open_at_their_tail_and_follow_appends() {
    let mut h = H::new();
    // Before the first commit the render already gets the tail.
    let w = h.tree.list_window_for("chat", 100, true);
    assert_eq!(w, (80, 100));
    h.commit(&chat(w, 100));
    assert_eq!(h.tree.list_window("chat"), (80, 100));
    assert_eq!(rendered_rows(&h), (80..100).collect::<Vec<_>>());
    let list = h.of_type(ViewType::List)[0];
    assert_eq!(h.prop(list, Prop::ListAlignment), Some(&PropValue::I32(1)));

    // Two messages arrive while the window reaches the end: the new rows render at once.
    let w = h.tree.list_window_for("chat", 102, true);
    assert_eq!(w, (82, 102));
    h.commit(&chat(w, 102));
    assert_eq!(rendered_rows(&h), (82..102).collect::<Vec<_>>());

    // A caller that rendered with the old window still ends up following the end.
    h.commit(&chat((82, 102), 103));
    assert_eq!(h.tree.list_window("chat"), (83, 103));

    // The user scrolled up: new messages do not move the window.
    assert!(h.tree.set_list_window("chat", 10, 40));
    let w = h.tree.list_window_for("chat", 110, true);
    assert_eq!(w, (10, 40));
    h.commit(&chat(w, 110));
    assert_eq!(h.tree.list_window("chat"), (10, 40));

    // Top-aligned lists keep opening at the head.
    assert_eq!(h.tree.list_window_for("other", 100, false), (0, 20));
}

#[test]
fn an_empty_bottom_aligned_list_grows_from_its_end() {
    let mut h = H::new();
    let w = h.tree.list_window_for("chat", 0, true);
    assert_eq!(w, (0, 0));
    h.commit(&chat(w, 0));
    let w = h.tree.list_window_for("chat", 3, true);
    assert_eq!(w, (0, 3));
    h.commit(&chat(w, 3));
    assert_eq!(rendered_rows(&h), vec![0, 1, 2]);
    // The renderer reported a range before the list was ever committed: it is kept.
    let mut fresh = H::new();
    fresh.tree.set_list_window("chat", 5, 15);
    assert_eq!(fresh.tree.list_window_for("chat", 50, true), (5, 15));
}
