//! The sandbox: dynamic code, frozen builtins, budgets, memory, module confinement.

use std::time::{Duration, Instant};

use native_shell::testing::Harness;
use native_shell::{Error, Limits};

fn probe(expr: &str) -> String {
    let src = format!(
        r#"
import {{ View, div }} from "native-shell";
let result;
try {{ result = String({expr}); }} catch (e) {{ result = "threw: " + e.message; }}
export default class P extends View {{ render() {{ return div().test_id("r").child(result); }} }}
"#
    );
    let h = Harness::new(&[("main.js", &src)]);
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    h.text_of("r")
}

#[test]
fn dynamic_code_is_withheld() {
    assert!(probe("typeof eval").contains("undefined"));
    for expr in [
        "new Function('return 1')()",
        "(() => {}).constructor('return 1')()",
        "(async function () {}).constructor('x')",
    ] {
        let r = probe(expr);
        assert!(r.starts_with("threw:") && r.contains("disabled"), "{expr}: {r}");
    }
    assert_eq!(probe("((f) => f instanceof Function && f.call(null, 2))((x) => x)"), "2");
    assert!(probe("require('fs')").contains("ES modules"));
}

#[test]
fn builtin_prototypes_are_frozen() {
    let r = probe("(Array.prototype.first = 1)");
    assert!(r.starts_with("threw:"), "{r}");
    assert_eq!(probe("({}).polluted === undefined"), "true");
}

#[test]
fn dev_mode_restores_eval_but_not_capabilities() {
    let h = Harness::build(&[(
        "main.js",
        r#"
import { View, div } from "native-shell";
const two = eval("1 + 1");
export default class P extends View { render() { return div().test_id("r").child(String(two)); } }
"#,
    )])
    .dev(true)
    .start();
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    assert_eq!(h.text_of("r"), "2");
}

fn fast_limits() -> Limits {
    Limits {
        render_budget: Duration::from_millis(50),
        event_budget: Duration::from_millis(100),
        module_budget: Duration::from_millis(200),
        ..Limits::default()
    }
}

#[test]
fn an_infinite_render_is_interrupted() {
    let start = Instant::now();
    let h = Harness::build(&[(
        "main.js",
        r#"
import { View, div } from "native-shell";
export default class P extends View { render() { try { while (true) {} } catch (e) {} return div(); } }
"#,
    )])
    .limits(fast_limits())
    .start();
    assert!(start.elapsed() < Duration::from_secs(5));
    let Some(Error::FirstRender(e)) = h.load_error() else { panic!("{:?}", h.load_error()) };
    assert!(e.message.contains("render budget of 50 ms"), "{e}");
}

#[test]
fn an_infinite_handler_is_interrupted_and_the_app_keeps_running() {
    let mut h = Harness::build(&[(
        "main.js",
        r#"
import { View, div, v_flex } from "native-shell";
export default class P extends View {
  init() { this.n = 0; }
  render() {
    return v_flex()
      .child(div().test_id("spin").child("spin").on_click(() => { for (;;) {} }))
      .child(div().test_id("n").child(`${this.n}`).on_click((_e, cx) => { this.n += 1; cx.notify(); }));
  }
}
"#,
    )])
    .limits(fast_limits())
    .start();
    let start = Instant::now();
    h.tap("spin");
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(h.error_logs().join("\n").contains("event handler budget of 100 ms"), "{:?}", h.error_logs());
    h.tap("n");
    assert_eq!(h.text_of("n"), "1");
}

#[test]
fn an_infinite_module_top_level_is_interrupted() {
    let h = Harness::build(&[("main.js", "while (true) {}\nexport default 1;")]).limits(fast_limits()).start();
    let Some(Error::Module(e)) = h.load_error() else { panic!("{:?}", h.load_error()) };
    assert!(e.message.contains("module evaluation budget"), "{e}");
}

#[test]
fn the_memory_limit_surfaces_as_an_error() {
    let h = Harness::build(&[(
        "main.js",
        r#"
import { View, div } from "native-shell";
let result = "ok";
try { const hog = []; for (;;) hog.push(new Array(100000).fill(1)); } catch (e) { result = "caught: " + e.message; }
export default class P extends View { render() { return div().test_id("r").child(result); } }
"#,
    )])
    .limits(Limits { memory_bytes: 16 << 20, ..Limits::default() })
    .start();
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    assert!(h.text_of("r").contains("out of memory"), "{}", h.text_of("r"));
}

#[test]
fn modules_are_confined_to_the_program() {
    let cases = [
        ("import x from '../outside.js';", "leaves the program"),
        ("import x from 'lodash';", "unknown module"),
        ("import { engine } from 'nsh:core';", "internal module"),
        ("import x from './missing.js';", "no file `missing.js`"),
        ("import x from '/etc/passwd';", "unknown module"),
    ];
    for (import, expect) in cases {
        let h = Harness::new(&[("main.js", &format!("{import}\nexport default x;"))]);
        let e = h.load_error().map(|e| e.to_string()).unwrap_or_default();
        assert!(e.contains(expect), "{import}: {e}");
    }
    let h = Harness::new(&[
        ("main.js", "import { View, div } from 'gpui-kit';\nimport { label } from './ui/label';\nexport default class P extends View { render() { return label('hi'); } }"),
        ("ui/label.js", "import { div } from '../lib.js';\nexport const label = (t) => div().test_id('l').child(t);"),
        ("lib.js", "export { div } from 'native-shell';"),
    ]);
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    assert_eq!(h.text_of("l"), "hi");
}

#[test]
fn fs_promises_is_importable_and_capability_gated() {
    let dir = std::env::temp_dir().join(format!("nsh-engine-fs-{}", std::process::id()));
    let mut h = Harness::build(&[(
        "main.js",
        r#"
import { View, div } from "native-shell";
import { writeFile, readFile } from "fs/promises";
export default class P extends View {
  init(_p, cx) {
    this.text = "…";
    cx.spawn(async (cx) => {
      try { await writeFile("note.txt", "hello"); this.text = await readFile("note.txt", "utf8"); }
      catch (e) { this.text = "error: " + e.message; }
      cx.notify();
    });
  }
  render() { return div().test_id("r").child(this.text); }
}
"#,
    )])
    .manifest(r#"{"id":"t.fs","name":"FS","capabilities":{"fs":true}}"#)
    .data_dir(&dir)
    .start();
    h.advance(10.0);
    assert_eq!(h.text_of("r"), "hello", "{:?}", h.logs());
    let _ = std::fs::remove_dir_all(&dir);
}
