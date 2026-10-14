#!/usr/bin/env bash
# Builds the example app and captures every example, light and dark, plus the
# pushed pages and overlays, into platforms/macos/screenshots/. Each capture
# opens the app's window briefly, then quits it.
#
# Safe next to an instance someone is using: scripted runs keep their data in a
# temporary directory, and each capture targets its own process's window and
# checks it shows the requested example (a mismatch fails the run, so nothing
# wrong is written).
set -euo pipefail
cd "$(dirname "$0")"
OUT="$(cd .. && pwd)/screenshots"
mkdir -p "$OUT"
xcodegen generate >/dev/null
xcodebuild -project NativeShellMacExample.xcodeproj -scheme NativeShellMacExample -configuration Debug \
  -derivedDataPath build build >/dev/null
APP=build/Build/Products/Debug/native-shell.app/Contents/MacOS/native-shell
run() { "$APP" -reset "$@" >/dev/null 2>&1; }

for ex in tasks chat watchlist settings components gpui-todolist; do
  for ap in light dark; do run -example "$ex" -appearance "$ap" -actions "wait:1.5;capture:$OUT/$ex-$ap.png;quit"; done
done
for ap in light dark; do
  run -example tasks -appearance "$ap" -actions "tap:task-2;wait:1.2;capture:$OUT/tasks-edit-$ap.png;quit"
  run -example settings -appearance "$ap" -actions "tap:account;wait:1;capture:$OUT/settings-account-$ap.png;quit"
  run -example components -appearance "$ap" -actions "tap:open-overlays;wait:1;tap:share;wait:1.2;capture:$OUT/components-sheet-$ap.png;quit"
  run -example components -appearance "$ap" -actions "tap:open-overlays;wait:1;tap:rename;wait:1.2;capture:$OUT/components-dialog-$ap.png;quit"
  run -example components -appearance "$ap" -actions "tap:open-overlays;wait:1;tap:delete;wait:1.2;capture:$OUT/components-alert-$ap.png;quit"
  run -example components -appearance "$ap" -actions "tap:open-overlays;wait:1;tap:offline;wait:0.5;capture:$OUT/components-toast-$ap.png;quit"
  for p in buttons fields selection progress pickers; do
    run -example components -appearance "$ap" -actions "tap:open-$p;wait:1;capture:$OUT/components-$p-$ap.png;quit"
  done
  run -example watchlist -appearance "$ap" -actions "wait:1.5;tap:add;wait:1.2;capture:$OUT/watchlist-add-$ap.png;quit"
done
echo "screenshots -> $OUT"
