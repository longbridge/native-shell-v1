# native-shell

Run small JavaScript programs inside iOS, Android, macOS and Linux apps, rendered
with **native views** (UIKit, Android `View`, AppKit, GTK 4), with the programming model of
gpui-shell:

```js
import { View, v_flex, div, Button } from "native-shell";

export default class Counter extends View {
  init() { this.count = 0; }
  render(cx) {
    return v_flex().size_full().items_center().justify_center().gap_4()
      .child(div().text_3xl().font_bold().child(`${this.count}`))
      .child(Button.new("inc").label("Increment").on_click((_e, cx) => {
        this.count += 1;
        cx.notify();
      }));
  }
}
```

* **QuickJS** (interpreter only, so it is allowed on iOS), one VM per program,
  memory and time limits, frozen builtins, no `eval`.
* **Rust core**: keyed diff, view flattening, [taffy](https://github.com/DioxusLabs/taffy)
  layout (flexbox, grid, block), a compact binary mutation stream.
* **Native rendering**: UIKit on iOS, Android `View` on Android; native text
  input (IME-safe), native scrolling and recycled lists, native navigation,
  sheets, alerts and gestures.
* **Host extension**: the embedding app registers JS-importable modules
  (sync, async, event push) and native components scripts can lay out.
* **Capabilities and signed bundles**: programs declare what they may use;
  bundles are Ed25519-signed with key rotation and revocation.
* **Tooling**: `nshell new | check | types | bundle | keygen | verify | dev`.

## Layout

| Path | What |
| --- | --- |
| `crates/core` | Contracts: protocol, styles, theme, descriptions, host interfaces, manifest |
| `crates/tree` | Diff, flattening, layout, lists |
| `crates/services` | Storage, fs, fetch, WebSocket, crypto, signed bundles |
| `crates/engine` | The `native-shell` runtime crate (QuickJS, prelude, scheduler, events) |
| `crates/ffi` | C ABI and JNI |
| `crates/cli` | The `nshell` command |
| `platforms/ios/` | Swift Package / CocoaPods: UIKit renderer |
| `platforms/android/` | Gradle: AAR with the View renderer, Compose wrapper, example app |
| `platforms/linux/` | Cargo: GTK 4 / libadwaita renderer, example app ([README](platforms/linux/README.md)) |
| `platforms/macos/` | Swift Package: AppKit renderer, example app |
| `examples/` | Example programs |
| `docs/` | Architecture, protocol, plans |

## Documentation

* [Architecture](docs/architecture.md)
* [Protocol JSON values](docs/protocol-json.md) and `crates/core/src/protocol.rs`
* [Implementation plan](docs/agents-plan.md) (script API, host integration)
* [macOS platform](docs/platform-macos.md)

## License

Apache-2.0
