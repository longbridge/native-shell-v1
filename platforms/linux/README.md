# native-shell for Linux (GTK 4)

`native-shell-gtk` renders native-shell programs with GTK 4 and libadwaita.
It is its own Cargo workspace (it needs the GTK development libraries, GTK ≥
4.18 and libadwaita ≥ 1.7) and links the engine crate directly.

See [docs/platform-linux.md](../../docs/platform-linux.md) for the mapping of
the protocol to GTK, the environment, the host API and how testing works.

## Commands

```sh
cargo test                                   # unit + GTK integration tests (headless Broadway)
cargo clippy --all-targets -- -D warnings
cargo run --example launcher                 # the example app; -- --example tasks --reset
cargo run --example run -- ../../examples/chat
cargo run --example screenshot [-- tasks …]  # screenshots/example-<name>-{light,dark}.png
cargo run --example launcher -- --screenshot screenshots
NSH_SCREENSHOTS=1 cargo test                 # refresh the committed test screenshots
```
