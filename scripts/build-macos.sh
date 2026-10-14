#!/usr/bin/env bash
# Builds platforms/macos/Frameworks/NativeShellFFI.xcframework: the native-shell
# static library for macOS (arm64 + x86_64, lipo'd into one universal slice),
# with native_shell.h exposed as the Clang module `NativeShellFFI` (the same
# module the iOS package imports).
#
# Usage: scripts/build-macos.sh            (release, the default)
#        PROFILE=debug scripts/build-macos.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PROFILE="${PROFILE:-release}"
# Separate from the iOS / Android builds so they never invalidate each other.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target/macos}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
OUT="$ROOT/platforms/macos/Frameworks/NativeShellFFI.xcframework"
WORK="$CARGO_TARGET_DIR/xcframework"
TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)

cargo_profile_flag=()
profile_dir="debug"
if [[ "$PROFILE" == "release" ]]; then
  cargo_profile_flag=(--release)
  profile_dir="release"
fi

installed="$(rustup target list --installed)"
for t in "${TARGETS[@]}"; do
  grep -qx "$t" <<<"$installed" || rustup target add "$t"
done

for t in "${TARGETS[@]}"; do
  echo "==> cargo build ($t, $PROFILE)"
  # Only the staticlib: apps link it into their own binary.
  cargo rustc -p native-shell-ffi --lib ${cargo_profile_flag[@]+"${cargo_profile_flag[@]}"} --target "$t" --crate-type staticlib
done

# The crate is native_shell_ffi; the framework ships it as libnative_shell.a.
lib() { echo "$CARGO_TARGET_DIR/$1/$profile_dir/libnative_shell_ffi.a"; }

rm -rf "$WORK"
mkdir -p "$WORK/headers" "$WORK/macos"
cp crates/ffi/include/native_shell.h "$WORK/headers/"
cat >"$WORK/headers/module.modulemap" <<'EOF'
module NativeShellFFI {
    header "native_shell.h"
    export *
}
EOF

lipo -create "$(lib aarch64-apple-darwin)" "$(lib x86_64-apple-darwin)" -output "$WORK/macos/libnative_shell.a"

rm -rf "$OUT"
mkdir -p "$(dirname "$OUT")"
xcodebuild -create-xcframework \
  -library "$WORK/macos/libnative_shell.a" -headers "$WORK/headers" \
  -output "$OUT" >/dev/null

human() { awk -v b="$1" 'BEGIN { printf "%.1f MiB", b / 1048576 }'; }
echo
echo "NativeShellFFI.xcframework ($PROFILE) -> ${OUT#"$ROOT/"}"
for t in "${TARGETS[@]}"; do
  printf "  %-24s archive %10s\n" "$t" "$(human "$(stat -f%z "$(lib "$t")")")"
done
for slice in "$OUT"/*/; do
  printf "  slice %-34s %10s\n" "$(basename "$slice")" "$(human "$(stat -f%z "$slice/libnative_shell.a")")"
done
