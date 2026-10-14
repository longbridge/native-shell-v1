#!/usr/bin/env bash
# Builds platforms/ios/Frameworks/NativeShellFFI.xcframework: the native-shell static
# library for devices (arm64) and the simulator (arm64 + x86_64, lipo'd into
# one slice), with native_shell.h exposed as the Clang module `NativeShellFFI`.
#
# Usage: scripts/build-ios.sh            (release, the default)
#        PROFILE=debug scripts/build-ios.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PROFILE="${PROFILE:-release}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target/ffi}"
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-14.0}"
OUT="$ROOT/platforms/ios/Frameworks/NativeShellFFI.xcframework"
WORK="$CARGO_TARGET_DIR/xcframework"
TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios)

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
  # Only the staticlib: the cdylib/rlib crate types are not needed on iOS.
  cargo rustc -p native-shell-ffi --lib ${cargo_profile_flag[@]+"${cargo_profile_flag[@]}"} --target "$t" --crate-type staticlib
done

# The crate is native_shell_ffi; the framework ships it as libnative_shell.a.
lib() { echo "$CARGO_TARGET_DIR/$1/$profile_dir/libnative_shell_ffi.a"; }

rm -rf "$WORK"
mkdir -p "$WORK/headers" "$WORK/ios" "$WORK/ios-simulator"
cp crates/ffi/include/native_shell.h "$WORK/headers/"
cat >"$WORK/headers/module.modulemap" <<'EOF'
module NativeShellFFI {
    header "native_shell.h"
    export *
}
EOF

cp "$(lib aarch64-apple-ios)" "$WORK/ios/libnative_shell.a"
lipo -create "$(lib aarch64-apple-ios-sim)" "$(lib x86_64-apple-ios)" -output "$WORK/ios-simulator/libnative_shell.a"

rm -rf "$OUT"
mkdir -p "$(dirname "$OUT")"
xcodebuild -create-xcframework \
  -library "$WORK/ios/libnative_shell.a" -headers "$WORK/headers" \
  -library "$WORK/ios-simulator/libnative_shell.a" -headers "$WORK/headers" \
  -output "$OUT" >/dev/null

# What an app actually pays: the device slice linked into a dylib that keeps
# only the nsh_* entry points, dead-stripped and symbol-stripped.
echo "_nsh_*" >"$WORK/exports.txt"
xcrun clang -dynamiclib -target "arm64-apple-ios$IPHONEOS_DEPLOYMENT_TARGET" \
  -isysroot "$(xcrun --sdk iphoneos --show-sdk-path)" \
  -Wl,-force_load,"$WORK/ios/libnative_shell.a" -Wl,-dead_strip \
  -Wl,-exported_symbols_list,"$WORK/exports.txt" -o "$WORK/linked.dylib"
xcrun strip -x "$WORK/linked.dylib"

human() { awk -v b="$1" 'BEGIN { printf "%.1f MiB", b / 1048576 }'; }
echo
echo "NativeShellFFI.xcframework ($PROFILE) -> ${OUT#"$ROOT/"}"
for t in "${TARGETS[@]}"; do
  bytes=$(stat -f%z "$(lib "$t")")
  # __TEXT of the archive members: the code a final app links at most.
  text=$(size "$(lib "$t")" 2>/dev/null | awk 'NR > 1 && $1 ~ /^[0-9]+$/ { s += $1 } END { print s + 0 }')
  printf "  %-24s archive %10s   __TEXT %10s\n" "$t" "$(human "$bytes")" "$(human "$text")"
done
for slice in "$OUT"/*/; do
  printf "  slice %-34s %10s\n" "$(basename "$slice")" "$(human "$(stat -f%z "$slice/libnative_shell.a")")"
done
printf "  linked into an app (arm64, dead-stripped, stripped): %s\n" "$(human "$(stat -f%z "$WORK/linked.dylib")")"
