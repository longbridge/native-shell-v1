//! The generated declarations against real programs: every example type-checks
//! with `tsc` under `checkJs` (examples are the framework's showcase, so they
//! are idiomatic, checked code), including `examples/gpui-todolist`, a
//! gpui-shell program kept unchanged; so do a fresh `new` program and a
//! program using the wider gpui-base surface.
//!
//! Needs TypeScript: `tsc` on the PATH, or its path in `TSC`. Without one the
//! test is skipped, unless `NATIVE_SHELL_REQUIRE_TSC` is set (CI sets it).

use std::path::{Path, PathBuf};
use std::process::Command;

/// gpui-shell's own jsconfig for the todolist: strict, without null checks.
const STRICT: &str = r#"{
  "compilerOptions": {
    "target": "ES2022", "module": "ES2022", "moduleResolution": "bundler", "lib": ["ES2022"],
    "checkJs": true, "strict": true, "strictNullChecks": false, "noEmit": true
  },
  "include": ["**/*.js", "*.d.ts"]
}"#;

/// Uses the declarations beyond what the examples reach.
const API_PROGRAM: &str = r#"// @ts-check
import { View, div, v_flex, h_flex, shape, Toggle, Popover, Accordion, AccordionItem, AccordionHeader, AccordionTrigger,
  AccordionPanel, pagination_items, PathBuilder, Background, v_virtual_list, VirtualListScrollHandle, OtpState,
  OtpInput, NumberInput, InputState, Table, TableRow, TableCell, TextView, Select, SliderState, Slider, Checkbox,
  Progress, Button } from "gpui-base";
/** @import { AsyncContext, Context, KeyEvent } from "gpui-kit" */

export default class Api extends View {
  /** @param {unknown} _props @param {AsyncContext} cx */
  init(_props, cx) {
    /** @type {boolean} */
    this.on = false;
    /** @type {boolean} */
    this.open = false;
    this.scroll = VirtualListScrollHandle.new();
    this.code = OtpState.new(6);
    this.amount = InputState.new({ value: "1" });
    this.amount.set_step(1);
    this.slider = SliderState.new({ min: 0, max: 10 });
    cx.bind_keys([{ keystroke: "cmd-s", action: "save", context: "Editor" }]);
    this.code.on("complete", (_e, cx) => cx.notify());
  }

  /** @param {Context} cx */
  render(cx) {
    const path = PathBuilder.stroke(2).move_to(0, 0).line_to("100%", "100%").build();
    return v_flex()
      .key_context("Editor")
      .on_action("save", (event, cx) => { console.log(event.action); cx.notify(); })
      .on_key_down((/** @type {KeyEvent} */ e) => console.log(e.keystroke))
      .child(div().cursor_pointer().tooltip("Details").hover((s) => s.bg(cx.theme().colors.selection))
        .on_hover((hovered) => console.log(hovered)).on_click((e) => console.log(e.modifiers.shift, e.click_count))
        .text_color(cx.theme().colors.success).text_size(cx.theme().typography.body.size).child("Hover me"))
      .child(h_flex().child(Button.new("save").primary().size("large").child("Save"))
        .child(Button.new("delete").variant("destructive").size("small").child("Delete"))
        .child(Button.new("more").link().size(44).child("More")))
      .child(div().bg(cx.theme().colors.grouped_background))
      .child(Toggle.new("bold").pressed(this.on).on_change((on, cx) => { this.on = on; cx.notify(); }).child("B"))
      .child(Popover.new("menu").trigger(div().child("Open")).content(div().child("Hi")).open(this.open)
        .on_open_change((open, cx) => { this.open = open; cx.notify(); }))
      .child(Accordion.new("faq").child(AccordionItem.new().open(true)
        .header(AccordionHeader.new(AccordionTrigger.new("t").on_change(() => {}).child("Q")))
        .panel(AccordionPanel.new().keep_mounted(true).child("A"))))
      .children(pagination_items(3, 20).map((item) => (item.ellipsis ? div().child("…") : div().child(String(item.page)))))
      .child(window.paint_path(path, Background.solid(cx.theme().colors.primary)))
      .child(v_virtual_list("rows", 100, 28, (i) => String(i), (range) =>
        Array.from({ length: range.end - range.start }, (_, i) => div().child(String(range.start + i))))
        .track_scroll(this.scroll).on_item_click((key) => console.log(key)).alignment("bottom"))
      .child(OtpInput.new(this.code))
      .child(NumberInput.new(this.amount).on_step((action) => console.log(action)))
      .child(Table.new("t").row_count(1).child(TableRow.new("r", 1).child(TableCell.new("c", 1).child("x"))))
      .child(TextView.markdown("md", "**hi**").selectable())
      .child(Select.new("s").open(false).on_open_change(() => {}))
      .child(Slider.new(this.slider))
      .child(shape("M0 0h24v24H0zM6 6h12v12H6z", { view_box: [0, 0, 24, 24], fill_rule: "evenodd" }).fill(cx.theme().colors.primary))
      .child(h_flex().child(cx.read_from_clipboard() ?? ""))
      .child(Checkbox.new("c").headless().checked(this.on).child(div().w(16).h(16)))
      .child(Progress.new().headless().value(40).child(div().h(4)))
      .when(false, (el) => { cx.notify(cx.entity()); return el; });
  }
}
"#;

fn tsc() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("TSC") {
        return Some(PathBuf::from(p));
    }
    let found = Command::new("tsc").arg("--version").output().is_ok_and(|o| o.status.success());
    found.then(|| PathBuf::from("tsc"))
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn nshell(args: &[&str]) {
    let o = Command::new(env!("CARGO_BIN_EXE_nshell")).args(args).output().unwrap();
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
}

fn check(tsc: &Path, dir: &Path) -> Result<(), String> {
    let o = Command::new(tsc).arg("-p").arg(dir.join("jsconfig.json")).output().map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(())
    } else {
        Err(format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
    }
}

#[test]
fn examples_and_new_programs_type_check() {
    let Some(tsc) = tsc() else {
        assert!(std::env::var_os("NATIVE_SHELL_REQUIRE_TSC").is_none(), "tsc not found and NATIVE_SHELL_REQUIRE_TSC is set");
        eprintln!("skipping: tsc not found (set TSC or put TypeScript on the PATH)");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut failures = Vec::new();
    let mut checked = 0;

    for entry in std::fs::read_dir(&examples).unwrap() {
        let src = entry.unwrap().path();
        if !src.join("native-shell.json").is_file() {
            continue;
        }
        let name = src.file_name().unwrap().to_string_lossy().into_owned();
        let dir = tmp.path().join(&name);
        copy_dir(&src, &dir);
        let mut args = vec!["types".to_string(), dir.display().to_string()];
        if dir.join("host.json").is_file() {
            args.extend(["--registry".into(), dir.join("host.json").display().to_string()]);
        }
        nshell(&args.iter().map(String::as_str).collect::<Vec<_>>());
        if !dir.join("jsconfig.json").is_file() {
            // gpui-shell's annotated program is checked as strictly as gpui-shell checks it.
            let config = if name.starts_with("gpui-") { STRICT.to_string() } else { default_config() };
            std::fs::write(dir.join("jsconfig.json"), config).unwrap();
        }
        if let Err(e) = check(&tsc, &dir) {
            failures.push(format!("examples/{name}:\n{e}"));
        }
        checked += 1;
    }
    assert!(checked >= 2, "no examples found under {}", examples.display());

    let fresh = tmp.path().join("fresh");
    nshell(&["new", fresh.to_str().unwrap()]);
    if let Err(e) = check(&tsc, &fresh) {
        failures.push(format!("a new program:\n{e}"));
    }
    // The wider gpui-base surface, under gpui-shell's strict settings.
    let api = tmp.path().join("api");
    nshell(&["new", api.to_str().unwrap()]);
    std::fs::write(api.join("jsconfig.json"), STRICT).unwrap();
    std::fs::write(api.join("main.js"), API_PROGRAM).unwrap();
    if let Err(e) = check(&tsc, &api) {
        failures.push(format!("the API program:\n{e}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The jsconfig `nshell new` writes.
fn default_config() -> String {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p");
    nshell(&["new", dir.to_str().unwrap()]);
    std::fs::read_to_string(dir.join("jsconfig.json")).unwrap()
}
