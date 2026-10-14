# AGENTS.md

Shared guidance for coding agents working in this repository. Keep project
rules here; `CLAUDE.md` imports this file for Claude Code.

## Project overview

native-shell runs JavaScript programs inside iOS and Android apps and renders
them with native views (UIKit, Android `View`). It is the mobile sibling of
gpui-shell: the script API, the style vocabulary and their behavior follow
gpui-shell and GPUI. See `README.md`, `docs/architecture.md`,
`docs/protocol-json.md` and `docs/gpui-shell-compat.md`.

| Path | What |
| --- | --- |
| `crates/core` | Contracts: protocol, styles (generated from GPUI), theme, descriptions, host interfaces, manifest |
| `crates/tree` | Keyed diff, flattening, taffy layout, measurement, lists |
| `crates/services` | Storage, fs, fetch, WebSocket, crypto, signed bundles |
| `crates/engine` | The `native-shell` runtime: QuickJS, JS prelude, scheduler, events |
| `crates/ffi`, `crates/cli` | C ABI + JNI; the `nshell` command |
| `platforms/ios`, `platforms/android` | UIKit and Android View renderers, example apps |
| `platforms/linux` | GTK 4 renderer (own Cargo workspace), example app |
| `examples/` | Example programs |
| `tools/style-gen` | Regenerates `crates/core/src/style_table.rs` from GPUI |

## We build a UI framework: design and interaction come first

Every screen this repository ships—examples, example apps, error pages,
toasts, default component styling—is a demonstration of the framework. A
rough screen tells users the framework produces rough screens. Treat design
and interaction quality as a release requirement, not polish.

### Design direction: iOS

The visual and interaction language is **iOS**, following Apple's
[Human Interface Guidelines](https://developer.apple.com/design/human-interface-guidelines/)
(iOS sections: layout, typography, color, navigation, lists and tables,
sheets, alerts, menus, buttons, text fields, toggles, pickers, gestures,
motion, accessibility). GPUI Kit's components are designed for desktop
interaction (pointer hover, dense toolbars, menus bars, resizable panels,
keyboard-first focus rings); do **not** copy their look or interaction model.

From GPUI Kit we keep the *way of working* only — its
[Design Guides](https://gpui-kit.com/docs/design-guides.md) process: start from
the user's task, tokens before values, every state visible, composition before
invention, and its accessibility and design review checklists. Where the GPUI
Kit guide prescribes a desktop pattern, the iOS pattern wins.

The same iOS-oriented design applies to everything the framework draws itself
on both platforms (default component styling, toasts, error pages, example
programs). Where a renderer uses a platform control the OS provides (text
input and keyboard, date/time pickers, switches, menus, alerts, share
sheets), it stays the native platform control on each OS.

### iOS design conventions

- **Layout**: content respects safe areas and readable width; standard
  margins of 16 pt (20 pt on large phones); 8 pt spacing grid.
- **Navigation**: a navigation bar with a centered title on detail screens and
  a left-aligned large title (34 pt bold) on top-level screens that collapses
  on scroll; back is a chevron with the previous title; edge-swipe back.
  Tab bars for top-level sections (≤ 5 items).
- **Lists**: inset grouped lists for settings and forms (section headers in
  secondary text, footers for explanations), plain lists for content; rows at
  least 44 pt high with 16 pt insets, hairline separators inset to the text,
  chevrons for drill-down, trailing values in secondary color; swipe actions
  with a visible alternative (long press menu or edit mode).
- **Typography**: the iOS text styles — Large Title 34, Title 1 28, Title 2
  22, Title 3 20, Headline 17 semibold, Body 17, Callout 16, Subheadline 15,
  Footnote 13, Caption 1 12, Caption 2 11 — all scaling with Dynamic Type.
- **Color**: a single tint (accent) for interactive elements; label /
  secondary label / tertiary label hierarchy for text; grouped backgrounds;
  system red for destructive actions; semantic colors that adapt to dark mode.
- **Controls**: filled buttons (12 pt radius, 50 pt high) only for the one
  primary action; tinted or plain text buttons otherwise; switches, steppers,
  segmented controls and pickers as on iOS. Touch targets ≥ 44 × 44 pt.
- **Sheets and alerts**: sheets with a grabber and medium/large detents for
  secondary tasks; action sheets / menus for choices; alerts only for
  decisions, naming the object and verb (`Delete "Groceries"?` / `Delete`),
  cancel on the leading side, destructive styled red.
- **Feedback**: pressed states (highlight/opacity) on every tappable element,
  haptics for confirmations and selection changes, toasts sparingly with an
  undo action for destructive operations.
- **Text input**: correct keyboard type, return key and content type; the
  field stays visible above the keyboard; tap outside or scroll dismisses.
- **Motion**: system-like springs for presentation and state changes;
  nothing animates for decoration; Reduce Motion respected.
- **Text scales and dark mode**: every screen survives the largest
  accessibility text size and is designed (not inverted) for dark mode.

### Example programs

`examples/` is the first thing people read and run. Each example must:

1. **Solve a real, recognizable task** (a to-do list, a settings screen, a
   chat thread, a portfolio, a reader…), with realistic content — never
   "Button 1", "lorem ipsum" or a component dump without a purpose. A catalog
   (gallery) is allowed only when every section shows a component in a
   believable context with its states.
2. **Look finished and iOS-native**: iOS conventions above, coherent spacing scale, alignment spines, typography
   hierarchy, theme tokens only (no raw hex), both appearances, safe areas.
3. **Cover every state**: empty, loading, error, offline / permission denied
   where relevant, long content, large text.
4. **Interact properly**: pressed and disabled states, keyboard return-key
   behavior and focus order in forms, dismissal of every overlay, undo or
   confirmation for destructive actions.
5. **Be idiomatic gpui-shell code**: the same API a desktop program uses,
   mobile-only APIs only where the platform needs them, small files, clear
   names, no dead code.
6. **Be verified on both platforms**: run it in the iOS and Android example
   apps, capture light and dark screenshots into the platform `screenshots/`
   folders, and review them against the design review checklist. A harness
   test in `crates/engine/tests` covers its main flow.

## Engineering rules

- **gpui-shell is the reference.** Names, signatures, defaults and error
  messages follow gpui-shell; mobile additions are additive and never change
  the meaning of a gpui-shell name. The style table is generated
  (`tools/style-gen`); never hand-edit `style_table.rs`.
- **Contracts**: `crates/core/src/protocol.rs`, `docs/protocol-json.md`,
  `crates/ffi/include/native_shell.h` and the JNI surface change together, with
  both renderers and tests updated in the same change. Protocol keys carry no
  unit suffixes: all lengths are points.
- **Open source**: no employer, customer or product-specific names, internal
  class names or their abbreviations anywhere in the tree (code, tests,
  examples, docs, CI). Use neutral examples (`demo/quote`, example.org).
- **Language**: code comments and repository docs in English. Comments explain
  why, not what; no dead code; no `unwrap` on data from scripts or platforms.
- **Every behavior change ships with tests** at the layer that owns it (Rust
  unit/integration, harness, XCTest, JVM/instrumented). Pure visual tweaks may
  skip tests but not screenshots.

## Working in parallel

Several people and agents work on `main` at the same time, on different
machines (platform work happens where the platform runs).

- Commit small and often, straight to `main`; stage only the paths you
  changed (`git add <paths>`, never `git add -A`).
- `git pull --rebase` before every push; never force-push or rewrite pushed
  history. Resolve conflicts in files you don't own by asking their owner.
- Each platform lives in its own `platforms/<name>` directory and owns it.
  Shared contracts (`crates/core`, the C ABI, `docs/protocol-json.md`) change
  in a dedicated commit that says so in its subject, with every existing
  renderer updated or a note on what it still needs.
- A commit leaves the tests of the crates and platforms it touches green.

## Commands

```bash
cargo test --workspace --features native-shell-ffi/jni    # Rust (all crates)
cargo clippy --workspace --all-targets --features native-shell-ffi/jni
cargo run -p native-shell-cli -- check examples/<name> [--registry examples/<name>/host.json]
cargo run --release -p native-shell --example bench       # core performance
scripts/build-ios.sh && scripts/build-android.sh          # native libraries
cd platforms/ios && xcodebuild test -scheme NativeShell -destination 'platform=iOS Simulator,name=iPhone 17 Pro'
cd platforms/android && ./gradlew :native-shell:testDebugUnitTest :native-shell:connectedDebugAndroidTest
cd platforms/linux && cargo test && cargo clippy --all-targets -- -D warnings   # headless (Broadway)
```
