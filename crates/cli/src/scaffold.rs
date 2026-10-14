//! `native-shell new <dir>`: a minimal program that passes `check`, with the
//! editor and coding-agent setup next to it.

use std::path::Path;

use anyhow::{bail, Context, Result};
use native_shell_core::host::HostRegistry;

const MAIN_JS: &str = r#"// @ts-check
import { View, div, v_flex, h_flex, Button } from "native-shell";

export default class Counter extends View {
  init(_props, _cx) {
    this.count = Number(localStorage.getItem("count") ?? "0");
  }

  render(_cx) {
    return v_flex()
      .size_full()
      .bg("background")
      .items_center()
      .justify_center()
      .gap_4()
      .child(div().text_2xl().font_bold().test_id("count").child(`Count: ${this.count}`))
      .child(
        h_flex()
          .gap_2()
          .child(Button.new("dec").label("-").on_click((_e, cx) => this.add(-1, cx)))
          .child(Button.new("inc").label("+").variant("primary").on_click((_e, cx) => this.add(1, cx)))
      );
  }

  add(delta, cx) {
    this.count += delta;
    localStorage.setItem("count", String(this.count));
    cx.notify();
  }
}
"#;

const JSCONFIG: &str = r#"{
  "compilerOptions": {
    "checkJs": true,
    "strict": false,
    "target": "es2023",
    "module": "es2022",
    "moduleResolution": "bundler",
    "lib": ["es2023"],
    "noEmit": true
  },
  "include": ["**/*.js", "*.d.ts"]
}
"#;

const AGENTS_MD: &str = r#"# Writing this native-shell program

This directory is a native-shell program: JavaScript run by QuickJS inside a
native iOS/Android app and rendered with native views (UIKit / Android View).
There is no DOM, no HTML, no CSS and no npm at run time.

## Files

- `native-shell.json`: the manifest (`id`, `name`, `version`, `entry`,
  `capabilities`). Everything is denied unless listed under `capabilities`
  (`network.hosts`, `fs`, `clipboard`, `open_url`, `host_modules`,
  `host_components`). Unknown fields are errors.
- `main.js`: the entry module. It `export default`s a `View` subclass.
- `native-shell.d.ts`: the complete API as TypeScript declarations. Read it
  for every name; regenerate it with `native-shell types .`.

## Programming model

- `class X extends View { init(props, cx) {…} render(cx) {…} }`. `init` runs
  once; `render` returns an element tree and runs again after `cx.notify()`.
- Never call `cx.notify()`, open overlays or create `InputState` inside
  `render`; do it in `init`, event handlers or tasks.
- Elements: `div()`, `h_flex()`, `v_flex()`, `text()`, `image(src)`,
  `list(id, count, key, row)`, components (`Button.new(id)`, `Input.new(state)`,
  `Switch`, `Checkbox`, `Slider`, `Tabs`, `Select`, `DatePicker`…).
- Style with chained methods named like Tailwind/gpui: `.p_4()`, `.gap_2()`,
  `.bg("primary")`, `.text_color("muted_foreground")`, `.rounded_md()`. Colors
  are theme tokens or hex strings. Unknown method names throw with a
  "did you mean" hint.
- Events: `.on_click((e, cx) => …)`; mutate state, then `cx.notify()`.
- Async work: `cx.spawn(async cx => { … cx.notify(); })`, `fetch` (needs
  `capabilities.network`), `localStorage`, `import … from "fs/promises"`.
- Host app services are imported as modules (`import { x } from "demo/quote"`)
  and must be listed in `capabilities.host_modules`.

## Workflow

- `native-shell check .` loads and renders once headlessly; it must exit 0.
  `native-shell check . --print-tree` shows the native view tree.
- `native-shell dev .` serves the program to a device over the LAN with hot reload.
- `native-shell bundle . --key <secret> --key-id <id> -o app.nspkg` signs a release.
"#;

pub fn new_program(dir: &Path, registry: &HostRegistry) -> Result<()> {
    if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
        bail!("{} exists and is not empty", dir.display());
    }
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "app".into());
    let id_part: String =
        name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect();
    let manifest = serde_json::json!({
        "id": format!("org.example.{id_part}"),
        "name": name,
        "version": "0.1.0",
        "entry": "main.js",
        "capabilities": { "storage": true }
    });
    let write = |file: &str, content: &str| -> Result<()> {
        let path = dir.join(file);
        std::fs::write(&path, content).with_context(|| format!("cannot write {}", path.display()))
    };
    write("native-shell.json", &(serde_json::to_string_pretty(&manifest)? + "\n"))?;
    write("main.js", MAIN_JS)?;
    write("jsconfig.json", JSCONFIG)?;
    write("native-shell.d.ts", &crate::types::generate(registry))?;
    write("AGENTS.md", AGENTS_MD)?;
    Ok(())
}
