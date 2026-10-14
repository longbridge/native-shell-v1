//! The JNI layer against a real JVM: `NativeBridge` functions are called the
//! way the JVM would call them, with a Java `HostCallbacks` implementation
//! (tests/java). Skipped when no JDK is installed.
#![cfg(feature = "jni")]

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use jni::objects::{JByteBuffer, JObject, JString, JValue};
use jni::sys::{jlong, JNI_FALSE, JNI_TRUE};
use jni::{InitArgsBuilder, JNIEnv, JavaVM};
use native_shell_ffi::jni::*;
use native_shell_core::protocol::{decode, EventType, Mutation, Prop, PropValue};

/// One JVM per process (a JVM cannot be created twice), or `None` without a JDK.
fn vm() -> Option<&'static JavaVM> {
    static VM: OnceLock<Option<JavaVM>> = OnceLock::new();
    VM.get_or_init(|| {
        let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("jni-classes");
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/java/dev/nativeshell");
        let javac = Command::new("javac")
            .arg("-d")
            .arg(&out)
            .arg(src.join("HostCallbacks.java"))
            .arg(src.join("TestCallbacks.java"))
            .arg(src.join("LegacyCallbacks.java"))
            .status();
        if !matches!(javac, Ok(s) if s.success()) {
            eprintln!("skipping JNI tests: javac unavailable");
            return None;
        }
        let args = InitArgsBuilder::new()
            .option(format!("-Djava.class.path={}", out.display()))
            .option("-Xcheck:jni")
            .build()
            .ok()?;
        match JavaVM::new(args) {
            Ok(vm) => Some(vm),
            Err(e) => {
                eprintln!("skipping JNI tests: no JVM ({e})");
                None
            }
        }
    })
    .as_ref()
}

const MAIN_JS: &str = r#"
import { View, div, v_flex } from "native-shell";
import { greet, fail, fetch_name } from "demo/host";
export default class App extends View {
  init(_props, cx) {
    this.count = 0;
    this.sync = greet("ada");
    try { fail(); } catch (e) { this.failed = String(e.message ?? e); }
    this.answer = "pending";
    cx.spawn(async (cx) => { this.answer = await fetch_name(7); cx.notify(); });
  }
  render(_cx) {
    return v_flex()
      .child(div().test_id("btn").bg("primary").on_click((_e, cx) => { this.count += 1; cx.notify(); }).child(`count ${this.count}`))
      .child(div().child(`sync ${this.sync}`))
      .child(div().child(`failed ${this.failed}`))
      .child(div().child(`answer ${this.answer}`));
  }
}
"#;

fn config(dir: &std::path::Path, threading: u32) -> String {
    serde_json::json!({
        "app_id": "org.example.jni",
        "data_dir": dir,
        "threading": threading,
        "registry": {"modules": [{"name": "demo/host", "requires_permission": false,
            "methods": [{"name": "greet", "kind": "sync"}, {"name": "fail", "kind": "sync"}, {"name": "fetch_name", "kind": "async"}]}]}
    })
    .to_string()
}

fn this<'a>() -> JObject<'a> {
    JObject::null()
}

fn jstr<'a>(env: &mut JNIEnv<'a>, s: &str) -> JString<'a> {
    env.new_string(s).unwrap()
}

fn clone<'a>(env: &JNIEnv<'a>) -> JNIEnv<'a> {
    unsafe { env.unsafe_clone() }
}

fn last_error(env: &mut JNIEnv) -> String {
    let s = Java_dev_nativeshell_NativeBridge_lastError(clone(env), this());
    let s = unsafe { JString::from_raw(s) };
    env.get_string(&s).unwrap().into()
}

struct Screen {
    mutations: Vec<Mutation>,
    seq: u32,
}

impl Screen {
    fn has_text(&self, needle: &str) -> bool {
        self.mutations.iter().any(|m| {
            matches!(m, Mutation::SetProp { prop: Prop::AttributedText, value: PropValue::Json(j), .. } if j.contains(needle))
        })
    }

    fn node(&self, test_id: &str) -> u32 {
        self.mutations
            .iter()
            .find_map(|m| match m {
                Mutation::SetProp { node, prop: Prop::TestId, value: PropValue::Str(s) } if s == test_id => Some(*node),
                _ => None,
            })
            .unwrap()
    }

    fn wait(&mut self, env: &mut JNIEnv, handle: jlong, caller: bool, needle: &str) {
        let start = Instant::now();
        while !self.has_text(needle) {
            assert!(start.elapsed() < Duration::from_secs(10), "timed out waiting for {needle:?}");
            if caller {
                Java_dev_nativeshell_NativeBridge_tick(clone(env), this(), handle, start.elapsed().as_secs_f64() * 1000.0);
            }
            let batch = Java_dev_nativeshell_NativeBridge_takeBatch(clone(env), this(), handle);
            if batch == 0 {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            // A local frame per batch, as a renderer's loop would use.
            let bytes = env
                .with_local_frame(8, |env| -> jni::errors::Result<Vec<u8>> {
                    let raw = Java_dev_nativeshell_NativeBridge_batchBuffer(clone(env), this(), batch);
                    assert!(!raw.is_null(), "{}", last_error(env));
                    let buf = unsafe { JByteBuffer::from_raw(raw) };
                    // The buffer is little-endian and points at the Rust bytes (no copy).
                    let order = env.call_method(&buf, "order", "()Ljava/nio/ByteOrder;", &[])?.l()?;
                    let little =
                        env.get_static_field("java/nio/ByteOrder", "LITTLE_ENDIAN", "Ljava/nio/ByteOrder;")?.l()?;
                    assert!(env.is_same_object(&order, &little)?);
                    let ptr = env.get_direct_buffer_address(&buf)?;
                    let len = env.get_direct_buffer_capacity(&buf)?;
                    let first = env.call_method(&buf, "getInt", "(I)I", &[JValue::Int(0)])?.i()?;
                    assert_eq!(first as u32, u32::from_le_bytes(*b"NSB1"));
                    Ok(unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec())
                })
                .unwrap();
            Java_dev_nativeshell_NativeBridge_freeBatch(clone(env), this(), batch);
            Java_dev_nativeshell_NativeBridge_freeBatch(clone(env), this(), batch);
            let batch = decode(&bytes).unwrap();
            if let Some(Mutation::BeginBatch { seq, .. }) = batch.first() {
                self.seq = *seq;
            }
            self.mutations.extend(batch);
        }
    }
}

fn round_trip(threading: u32) {
    let Some(vm) = vm() else { return };
    let caller = threading == 1;
    let mut guard = vm.attach_current_thread().unwrap();
    let env: &mut JNIEnv = &mut guard;
    let dir = tempfile::tempdir().unwrap();

    let callbacks = env.new_object("dev/nativeshell/TestCallbacks", "()V", &[]).unwrap();
    let cfg = jstr(env, &config(dir.path(), threading));
    let handle = Java_dev_nativeshell_NativeBridge_create(clone(env), this(), clone_obj(env, &callbacks), cfg);
    assert_ne!(handle, 0, "{}", last_error(env));

    let sources = serde_json::json!({"manifest": {"id": "org.example.jni", "name": "JNI"}, "files": {"main.js": MAIN_JS}});
    let sources = jstr(env, &sources.to_string());
    assert_eq!(Java_dev_nativeshell_NativeBridge_loadSources(clone(env), this(), handle, sources), 0, "{}", last_error(env));

    let mut screen = Screen { mutations: Vec::new(), seq: 0 };
    screen.wait(env, handle, caller, "count 0");
    assert!(screen.has_text("sync hello \u{1F600} ada"), "non-BMP text must survive the JNI string conversion");
    assert!(screen.has_text("no such user"), "a Java exception rejects the sync call with its message");

    let payload = jstr(env, r#"{"x":1,"y":1,"count":1}"#);
    let node = screen.node("btn");
    Java_dev_nativeshell_NativeBridge_dispatchEvent(
        clone(env),
        this(),
        handle,
        node as i32,
        EventType::Click as i32,
        payload,
        screen.seq as i32,
    );
    screen.wait(env, handle, caller, "count 1");

    let m = Java_dev_nativeshell_NativeBridge_metrics(clone(env), this(), handle);
    let m: String = env.get_string(&unsafe { JString::from_raw(m) }).unwrap().into();
    let m: serde_json::Value = serde_json::from_str(&m).expect("metrics are JSON");
    assert!(m["renders"].as_u64().unwrap_or(0) >= 2, "{m}");

    let start = Instant::now();
    let request_id = loop {
        if caller {
            Java_dev_nativeshell_NativeBridge_tick(clone(env), this(), handle, 0.0);
        }
        let id = env.call_method(&callbacks, "firstAsyncId", "()J", &[]).unwrap().j().unwrap();
        if id >= 0 {
            break id;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "no callAsync");
        std::thread::sleep(Duration::from_millis(2));
    };
    let call = env.call_method(&callbacks, "firstAsyncCall", "()Ljava/lang/String;", &[]).unwrap().l().unwrap();
    let call: String = env.get_string(&JString::from(call)).unwrap().into();
    assert_eq!(call, "demo/host.fetch_name[7]");
    let answer = jstr(env, "\"Grace \u{1F680}\"");
    Java_dev_nativeshell_NativeBridge_hostRespond(clone(env), this(), handle, request_id, JNI_TRUE, answer);
    screen.wait(env, handle, caller, "answer Grace \u{1F680}");

    let ready = env.call_method(&callbacks, "readyCount", "()I", &[]).unwrap().i().unwrap();
    assert!(ready > 0);
    let errors = env.call_method(&callbacks, "errorCount", "()I", &[]).unwrap().i().unwrap();
    assert_eq!(errors, 0);

    Java_dev_nativeshell_NativeBridge_setActive(clone(env), this(), handle, JNI_FALSE);
    Java_dev_nativeshell_NativeBridge_trimMemory(clone(env), this(), handle);
    Java_dev_nativeshell_NativeBridge_destroy(clone(env), this(), handle);
    let again = jstr(env, &serde_json::json!({"manifest": {"id": "a.b", "name": "x"}, "files": {}}).to_string());
    assert_eq!(Java_dev_nativeshell_NativeBridge_loadSources(clone(env), this(), handle, again), 1);
    assert!(last_error(env).contains("no instance"));
}

fn clone_obj<'a>(env: &mut JNIEnv<'a>, obj: &JObject<'a>) -> JObject<'a> {
    env.new_local_ref(obj).unwrap()
}

#[test]
fn jni_round_trip_caller_thread() {
    round_trip(1);
}

#[test]
fn jni_round_trip_dedicated_thread() {
    round_trip(0);
}

#[test]
fn jni_errors_and_constants() {
    let Some(vm) = vm() else { return };
    let mut guard = vm.attach_current_thread().unwrap();
    let env: &mut JNIEnv = &mut guard;

    let callbacks = env.new_object("dev/nativeshell/TestCallbacks", "()V", &[]).unwrap();
    let cfg = jstr(env, "{not json");
    assert_eq!(Java_dev_nativeshell_NativeBridge_create(clone(env), this(), callbacks, cfg), 0);
    assert!(last_error(env).contains("config JSON"), "{}", last_error(env));

    // An object that is not a HostCallbacks fails creation instead of crashing later.
    let dir = tempfile::tempdir().unwrap();
    let not_callbacks = env.new_object("java/lang/Object", "()V", &[]).unwrap();
    let cfg = jstr(env, &config(dir.path(), 0));
    assert_eq!(Java_dev_nativeshell_NativeBridge_create(clone(env), this(), not_callbacks, cfg), 0);
    assert!(last_error(env).contains("HostCallbacks"), "{}", last_error(env));
    assert!(!env.exception_check().unwrap());

    assert_eq!(Java_dev_nativeshell_NativeBridge_takeBatch(clone(env), this(), 99999), 0);
    let m = Java_dev_nativeshell_NativeBridge_metrics(clone(env), this(), 99999);
    let m: String = env.get_string(&unsafe { JString::from_raw(m) }).unwrap().into();
    assert_eq!(m, "");
    assert!(last_error(env).contains("no instance"));
    assert!(Java_dev_nativeshell_NativeBridge_batchBuffer(clone(env), this(), 12345).is_null());
    Java_dev_nativeshell_NativeBridge_freeBatch(clone(env), this(), 0);

    let hash = Java_dev_nativeshell_NativeBridge_abiHash(clone(env), this());
    let hash: String = env.get_string(&unsafe { JString::from_raw(hash) }).unwrap().into();
    assert_eq!(hash, native_shell_core::protocol::abi_hash());
    assert_eq!(
        Java_dev_nativeshell_NativeBridge_protocolVersion(clone(env), this()) as u32,
        native_shell_core::protocol::PROTOCOL_VERSION
    );
}

const TEXTS_JS: &str = r#"
import { View, div, v_flex } from "native-shell";
export default class Texts extends View {
  render(_cx) {
    return v_flex().children(["one", "two", "three", "four", "five", "six"].map((t) => div().child(`text ${t}`)));
  }
}
"#;

/// Renders `TEXTS_JS` with `callbacks` in caller mode and returns once every text is on screen.
fn render_texts<'a>(env: &mut JNIEnv<'a>, callbacks: &JObject<'a>) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = jstr(env, &config(dir.path(), 1));
    let handle = Java_dev_nativeshell_NativeBridge_create(clone(env), this(), clone_obj(env, callbacks), cfg);
    assert_ne!(handle, 0, "{}", last_error(env));
    let sources = serde_json::json!({"manifest": {"id": "org.example.jni", "name": "JNI"}, "files": {"main.js": TEXTS_JS}});
    let sources = jstr(env, &sources.to_string());
    assert_eq!(Java_dev_nativeshell_NativeBridge_loadSources(clone(env), this(), handle, sources), 0, "{}", last_error(env));
    let mut screen = Screen { mutations: Vec::new(), seq: 0 };
    screen.wait(env, handle, true, "text six");
    assert!(screen.has_text("text one"));
    Java_dev_nativeshell_NativeBridge_destroy(clone(env), this(), handle);
}

fn counter(env: &mut JNIEnv, callbacks: &JObject, name: &str) -> i32 {
    let n = jstr(env, name);
    env.call_method(callbacks, "counter", "(Ljava/lang/String;)I", &[JValue::Object(&n)]).unwrap().i().unwrap()
}

#[test]
fn jni_measure_text_batch_and_fallbacks() {
    let Some(vm) = vm() else { return };
    let mut guard = vm.attach_current_thread().unwrap();
    let env: &mut JNIEnv = &mut guard;

    // Batched: one call carries the frame's texts.
    let callbacks = env.new_object("dev/nativeshell/TestCallbacks", "()V", &[]).unwrap();
    render_texts(env, &callbacks);
    assert!(counter(env, &callbacks, "calls") >= 1, "measureTextBatch was never called");
    assert!(counter(env, &callbacks, "texts") >= 6);

    // A batch method that throws: the exception is cleared and each text is measured alone.
    let throwing = env.new_object("dev/nativeshell/TestCallbacks", "()V", &[]).unwrap();
    env.set_field(&throwing, "batchMode", "I", JValue::Int(1)).unwrap();
    render_texts(env, &throwing);
    assert!(counter(env, &throwing, "calls") >= 1);
    assert!(counter(env, &throwing, "single") >= 6, "no per-text fallback after the exception");
    assert!(!env.exception_check().unwrap());

    // An object without the batch method (an older library): per-text measuring.
    let legacy = env.new_object("dev/nativeshell/LegacyCallbacks", "()V", &[]).unwrap();
    render_texts(env, &legacy);
    let singles = env.call_method(&legacy, "singles", "()I", &[]).unwrap().i().unwrap();
    assert!(singles >= 6, "legacy callbacks were not used per text ({singles})");
}

const BUSY_JS: &str = r#"
import { View, div } from "native-shell";
export default class Busy extends View {
  init(_props, cx) {
    this.n = 0;
    cx.timer.every(1, (cx) => {
      for (let i = 0; i < 5; i++) console.log("tick " + this.n + " " + i);
      this.n += 1;
      cx.notify();
    });
  }
  render(_cx) {
    return div().child(`n ${this.n}`);
  }
}
"#;

#[test]
fn jni_no_callback_after_destroy_returns() {
    let Some(vm) = vm() else { return };
    let mut guard = vm.attach_current_thread().unwrap();
    let env: &mut JNIEnv = &mut guard;
    for threading in [0, 1] {
        let dir = tempfile::tempdir().unwrap();
        let callbacks = env.new_object("dev/nativeshell/TestCallbacks", "()V", &[]).unwrap();
        env.set_field(&callbacks, "slowLog", "I", JValue::Int(1)).unwrap();
        let cfg = jstr(env, &config(dir.path(), threading));
        let handle = Java_dev_nativeshell_NativeBridge_create(clone(env), this(), clone_obj(env, &callbacks), cfg);
        assert_ne!(handle, 0, "{}", last_error(env));
        let sources = serde_json::json!({"manifest": {"id": "org.example.jni", "name": "JNI"}, "files": {"main.js": BUSY_JS}});
        let sources = jstr(env, &sources.to_string());
        assert_eq!(Java_dev_nativeshell_NativeBridge_loadSources(clone(env), this(), handle, sources), 0, "{}", last_error(env));
        let mut screen = Screen { mutations: Vec::new(), seq: 0 };
        screen.wait(env, handle, threading == 1, "n 1");
        Java_dev_nativeshell_NativeBridge_destroy(clone(env), this(), handle);
        env.set_field(&callbacks, "destroyed", "Z", JValue::Bool(JNI_TRUE)).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        let late = env.call_method(&callbacks, "lateCount", "()I", &[]).unwrap().i().unwrap();
        assert_eq!(late, 0, "callbacks after destroy returned (threading {threading})");
    }
}
