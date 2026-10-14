# native-shell for Android

Gradle project with three modules:

* `:native-shell` — the AAR: binary batch decoder, mounting (`ShellViewGroup`, one view per native node, pooled per type with a reset for every prop), every view type, text measurement shared with drawing, the controlled `EditText`, the `RecyclerView` list, page stack, bottom sheets, dialogs, the toast stack and the host registration API (`NativeShell`, `ShellInstance`, `ShellSurfaceView`, `ShellActivityDelegate`). Android Views only, no Compose.
* `:native-shell-compose` — `NativeShellSurface(instance, modifier)` for Compose hosts.
* `:example` — an app running the programs in `examples/` (bundled as assets: tasks, settings, chat, watchlist, components…) with the demo host modules `demo/quote`, `demo/user` and the `demo-stock-cell` component that `examples/watchlist/host.json` declares.

The native library comes from `scripts/build-android.sh` (into `native-shell/src/main/jniLibs/<abi>/`).

## Commands

```sh
./gradlew :native-shell:testDebugUnitTest          # JVM + Robolectric
./gradlew :native-shell:connectedDebugAndroidTest  # emulator / device
./gradlew :example:installDebug                    # demo app

# Screenshots into example/screenshots/
./gradlew :example:installDebug :example:installDebugAndroidTest
adb shell am instrument -w -e class dev.nativeshell.example.ScreenshotTest \
  dev.nativeshell.example.test/androidx.test.runner.AndroidJUnitRunner
adb pull /sdcard/Android/data/dev.nativeshell.example/files/screenshots/. example/screenshots/
```

`local.properties` (git-ignored) needs `sdk.dir`.

## Test fixtures

`BatchDecoderTest`'s expected bytes are the output of `protocol::encode` on the
same instructions as `protocol::tests::sample` in `crates/core/src/protocol.rs`,
printed by a throwaway binary depending on `native-shell-core`. Regenerate them
the same way whenever the sample changes.

## Debugging

`NativeShell.traceBatches = true` logs every applied mutation under the tag
`native-shell`; `NativeShell.testIdAsContentDescription = true` exposes test
ids to UI-testing tools that only see content descriptions.
