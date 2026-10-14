#!/usr/bin/env bash
# Builds libnative_shell.so (C ABI + JNI) for arm64-v8a, armeabi-v7a and
# x86_64 into platforms/android/native-shell/src/main/jniLibs/<abi>/, and fails unless
# every LOAD segment is 16 KB aligned (Android 15+ 16 KB page devices).
#
# The alignment flag lives in .cargo/config.toml (target sections), so plain
# `cargo build --target aarch64-linux-android` gets it too.
#
# Usage: scripts/build-android.sh            (release, the default)
#        PROFILE=debug scripts/build-android.sh
# Env:   ANDROID_NDK_HOME (default ~/Library/Android/sdk/ndk/27.2.12479018)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PROFILE="${PROFILE:-release}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target/ffi}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$HOME/Library/Android/sdk/ndk/27.2.12479018}"
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
OUT="$ROOT/platforms/android/native-shell/src/main/jniLibs"
ABIS=(arm64-v8a armeabi-v7a x86_64)
API_LEVEL=24

if [[ ! -d "$ANDROID_NDK_HOME" ]]; then
  echo "error: Android NDK not found at $ANDROID_NDK_HOME (set ANDROID_NDK_HOME)" >&2
  exit 1
fi
command -v cargo-ndk >/dev/null || cargo install cargo-ndk --locked

installed="$(rustup target list --installed)"
for t in aarch64-linux-android armv7-linux-androideabi x86_64-linux-android; do
  grep -qx "$t" <<<"$installed" || rustup target add "$t"
done

READELF="$(ls -d "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-readelf 2>/dev/null | head -n1)"
if [[ -z "$READELF" ]]; then
  echo "error: llvm-readelf not found in $ANDROID_NDK_HOME" >&2
  exit 1
fi

cargo_profile_flag=()
if [[ "$PROFILE" == "release" ]]; then
  cargo_profile_flag=(--release)
fi

target_args=()
for abi in "${ABIS[@]}"; do
  target_args+=(-t "$abi")
done

profile_dir="debug"
[[ "$PROFILE" == "release" ]] && profile_dir="release"

echo "==> cargo ndk (${ABIS[*]}, $PROFILE, API $API_LEVEL)"
# The crate is native_shell_ffi; Android loads it as libnative_shell.so, so
# the soname says so too.
cargo ndk "${target_args[@]}" --platform "$API_LEVEL" \
  rustc -p native-shell-ffi --lib --features jni ${cargo_profile_flag[@]+"${cargo_profile_flag[@]}"} --crate-type cdylib \
  -- -C link-arg=-Wl,-soname,libnative_shell.so

triple() {
  case "$1" in
    arm64-v8a) echo aarch64-linux-android ;;
    armeabi-v7a) echo armv7-linux-androideabi ;;
    x86_64) echo x86_64-linux-android ;;
  esac
}
for abi in "${ABIS[@]}"; do
  mkdir -p "$OUT/$abi"
  # Only this build's library ships: a stale one would be packaged into the AAR too.
  rm -f "$OUT/$abi"/*.so
  cp "$CARGO_TARGET_DIR/$(triple "$abi")/$profile_dir/libnative_shell_ffi.so" "$OUT/$abi/libnative_shell.so"
done

human() { awk -v b="$1" 'BEGIN { printf "%.2f MiB", b / 1048576 }'; }
fail=0
echo
echo "libnative_shell.so ($PROFILE) -> ${OUT#"$ROOT/"}"
for abi in "${ABIS[@]}"; do
  so="$OUT/$abi/libnative_shell.so"
  if [[ ! -f "$so" ]]; then
    echo "error: $so was not produced" >&2
    fail=1
    continue
  fi
  # Every PT_LOAD alignment (last column of `readelf -l`) must be >= 0x4000.
  bad=""
  for align in $("$READELF" -lW "$so" | awk '$1 == "LOAD" { print $NF }'); do
    (( $((16#${align#0x})) >= 16384 )) || bad="$bad$align "
  done
  if [[ -n "$bad" ]]; then
    echo "error: $abi: LOAD segments aligned to ${bad}(need 0x4000)" >&2
    fail=1
    status="NOT 16 KB aligned"
  else
    status="16 KB aligned"
  fi
  printf "  %-12s %12s   %s\n" "$abi" "$(human "$(wc -c <"$so" | tr -d " ")")" "$status"
done
exit "$fail"
