//! Frame-phase timings of a click-triggered update at 1k, 3k and 10k nodes.
//!
//! `cargo run --release -p native-shell --example bench`
//!
//! Two programs per size: `full` re-renders one view holding every node (the
//! cost of a whole-screen render), `entity` keeps the rows in a child entity
//! that does not re-render (the cost of a small update next to a big view).

use std::time::Instant;

use native_shell::testing::Harness;
use native_shell::Metrics;

const ROW: &str = r#"
const row = (i) => h_flex().gap_2().px_2().py_1().items_center()
  .child(div().w(20).h(20).rounded_md().bg("primary"))
  .child(div().flex_1().text_sm().child(`row ${i}`))
  .child(div().text_xs().text_color("muted_foreground").child("detail"))
  .child(Button.new(`b${i}`).px_2().on_click(() => {}).child("go"));
"#;

fn program(rows: usize, split: bool) -> String {
    let rows_view = if split {
        format!(
            "class Rows extends View {{ render() {{ const r = []; for (let i = 0; i < {rows}; i++) r.push(row(i)); return v_flex().children(r); }} }}"
        )
    } else {
        String::new()
    };
    let body = if split {
        "this.rows".to_string()
    } else {
        format!("(() => {{ const r = []; for (let i = 0; i < {rows}; i++) r.push(row(i)); return v_flex().children(r); }})()")
    };
    format!(
        r#"
import {{ View, div }} from "gpui-kit";
import {{ h_flex, v_flex, Button }} from "gpui-base";
{ROW}
{rows_view}
export default class B extends View {{
  init(_p, cx) {{ this.n = 0; {init} }}
  render(cx) {{
    return v_flex().size_full()
      .child(div().test_id("counter").child(`n=${{this.n}}`).on_click((_e, cx) => {{ this.n += 1; cx.notify(); }}))
      .child({body});
  }}
}}
"#,
        init = if split { "this.rows = cx.new(Rows);" } else { "" },
    )
}

fn delta(a: &Metrics, b: &Metrics) -> [f64; 4] {
    let ms = |x: u64, y: u64| (y - x) as f64 / 1e6;
    [
        ms(a.render_ns, b.render_ns),
        ms(a.convert_ns, b.convert_ns),
        ms(a.commit_ns, b.commit_ns),
        ms(a.encode_ns, b.encode_ns),
    ]
}

fn main() {
    println!(
        "{:>6} {:>7} {:>9} {:>8} {:>9} {:>8} {:>8} {:>9}",
        "rows", "mode", "nodes", "load ms", "render", "convert", "commit", "total ms"
    );
    for rows in [170usize, 500, 1700] {
        for split in [false, true] {
            let src = program(rows, split);
            let t = Instant::now();
            let mut h = Harness::new(&[("main.js", &src)]);
            let load = t.elapsed().as_secs_f64() * 1000.0;
            assert!(h.load_error().is_none(), "{:?}", h.load_error());
            // Warm up, then measure the average of several clicks.
            h.tap("counter");
            let n = 10;
            let before = h.instance().metrics();
            let t = Instant::now();
            for _ in 0..n {
                h.tap("counter");
            }
            let total = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
            let after = h.instance().metrics();
            let d = delta(&before, &after).map(|x| x / n as f64);
            println!(
                "{:>6} {:>7} {:>9} {:>8.1} {:>9.2} {:>8.2} {:>8.2} {:>9.2}",
                rows,
                if split { "entity" } else { "full" },
                h.native().nodes.len(),
                load,
                d[0],
                d[1],
                d[2],
                total
            );
        }
    }
}
