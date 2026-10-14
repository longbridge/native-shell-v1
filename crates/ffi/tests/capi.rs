//! The C ABI exercised exactly as a platform would: through the `extern "C"`
//! functions, with callbacks written as C-compatible Rust functions.

use std::collections::BTreeMap;
use std::ffi::{c_void, CStr};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use native_shell_ffi::*;
use native_shell_core::protocol::{decode, EventType, Mutation, Prop, PropValue};
use native_shell_services::bundle;

/// What the callbacks saw; the `context` pointer of every instance in these tests.
#[derive(Default)]
struct Recorder {
    errors: Mutex<Vec<String>>,
    logs: Mutex<Vec<String>>,
    ready: AtomicUsize,
    async_calls: Mutex<Vec<(u64, String, String, String)>>,
    /// Answer `call_async` from inside the callback (re-entrant respond).
    respond_inline: bool,
    /// Which `measure_text_batch` the callbacks carry.
    batch: Batch,
    single_measures: AtomicUsize,
    batch_calls: AtomicUsize,
    batched_texts: AtomicUsize,
    /// Set by the test once `nsh_instance_destroy` has returned.
    destroyed: AtomicBool,
    /// Callbacks that arrived after that: must stay 0.
    late: AtomicUsize,
    /// Every log callback sleeps this long (a slow host).
    slow_log: Option<Duration>,
    /// `call_async` destroys its own instance.
    destroy_in_async: bool,
}

/// Every callback starts here: one arriving after destroy returned is a contract breach.
fn seen(ctx: *mut c_void) -> &'static Recorder {
    let r: &'static Recorder = unsafe { &*(ctx as *const Recorder) };
    if r.destroyed.load(Ordering::SeqCst) {
        r.late.fetch_add(1, Ordering::SeqCst);
    }
    r
}

#[derive(Default, Clone, Copy, PartialEq)]
enum Batch {
    #[default]
    Null,
    Measure,
    Panic,
}

fn rec<'a>(ctx: *mut c_void) -> &'a Recorder {
    unsafe { &*(ctx as *const Recorder) }
}

fn s(ptr: *const u8, len: usize) -> String {
    if len == 0 {
        return String::new();
    }
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len) }).into_owned()
}

unsafe extern "C-unwind" fn measure_text(ctx: *mut c_void, json: *const u8, len: usize, _w: f32, _m: i32) -> NSHTextMetrics {
    seen(ctx);
    rec(ctx).single_measures.fetch_add(1, Ordering::SeqCst);
    metrics_for(json, len)
}

unsafe extern "C-unwind" fn measure_text_batch(ctx: *mut c_void, requests: *const NSHMeasureRequest, count: usize, out: *mut NSHTextMetrics) {
    seen(ctx);
    let r = rec(ctx);
    r.batch_calls.fetch_add(1, Ordering::SeqCst);
    r.batched_texts.fetch_add(count, Ordering::SeqCst);
    if r.batch == Batch::Panic {
        panic!("batch measurer exploded");
    }
    let requests = std::slice::from_raw_parts(requests, count);
    let out = std::slice::from_raw_parts_mut(out, count);
    for (req, slot) in requests.iter().zip(out) {
        *slot = metrics_for(req.attributed_json, req.len);
    }
}

/// Deterministic: 8 px per byte of the attributed JSON's text, one line.
unsafe fn metrics_for(json: *const u8, len: usize) -> NSHTextMetrics {
    let text = s(json, len);
    let chars = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v["runs"].as_array().map(|r| r.iter().map(|r| r["text"].as_str().unwrap_or("").len()).sum()))
        .unwrap_or(0usize);
    NSHTextMetrics { width: 8.0 * chars as f32, height: 20.0, baseline: 15.0 }
}

unsafe extern "C-unwind" fn batch_ready(ctx: *mut c_void, _: NSHInstance) {
    seen(ctx);
    rec(ctx).ready.fetch_add(1, Ordering::SeqCst);
}

unsafe extern "C-unwind" fn call_sync(
    _ctx: *mut c_void,
    _: NSHInstance,
    module: *const u8,
    module_len: usize,
    method: *const u8,
    method_len: usize,
    args: *const u8,
    args_len: usize,
    out: *mut NSHBuffer,
) -> i32 {
    seen(_ctx);
    let (module, method, args) = (s(module, module_len), s(method, method_len), s(args, args_len));
    assert_eq!(module, "demo/host");
    let (rc, text) = match method.as_str() {
        "greet" => {
            let a: Vec<String> = serde_json::from_str(&args).unwrap();
            (0, serde_json::to_string(&format!("hello {}", a[0])).unwrap())
        }
        "fail" => (1, "no such user".to_string()),
        "boom" => panic!("host callback exploded"),
        // `__listeners` and anything else: accept silently.
        _ => (0, "null".to_string()),
    };
    *out = nsh_string_new(text.as_ptr(), text.len());
    rc
}

unsafe extern "C-unwind" fn call_async(
    ctx: *mut c_void,
    instance: NSHInstance,
    request_id: u64,
    module: *const u8,
    module_len: usize,
    method: *const u8,
    method_len: usize,
    args: *const u8,
    args_len: usize,
) {
    seen(ctx);
    let r = rec(ctx);
    let call = (request_id, s(module, module_len), s(method, method_len), s(args, args_len));
    if r.destroy_in_async {
        nsh_instance_destroy(instance);
        r.async_calls.lock().unwrap().push(call);
        return;
    }
    if r.respond_inline {
        let answer = format!("\"inline {}\"", call.3);
        nsh_instance_host_respond(instance, request_id, 1, answer.as_ptr(), answer.len());
    }
    r.async_calls.lock().unwrap().push(call);
}

unsafe extern "C-unwind" fn log(ctx: *mut c_void, _: NSHInstance, level: i32, msg: *const u8, len: usize) {
    seen(ctx);
    if let Some(d) = rec(ctx).slow_log {
        std::thread::sleep(d);
    }
    rec(ctx).logs.lock().unwrap().push(format!("{level}: {}", s(msg, len)));
}

unsafe extern "C-unwind" fn error(ctx: *mut c_void, _: NSHInstance, msg: *const u8, len: usize) {
    seen(ctx);
    rec(ctx).errors.lock().unwrap().push(s(msg, len));
}

fn callbacks(r: &Recorder) -> NSHHostCallbacks {
    NSHHostCallbacks {
        context: r as *const Recorder as *mut c_void,
        measure_text: Some(measure_text),
        measure_text_batch: match r.batch {
            Batch::Null => None,
            Batch::Measure | Batch::Panic => Some(measure_text_batch),
        },
        measure_component: None,
        batch_ready: Some(batch_ready),
        call_sync: Some(call_sync),
        call_async: Some(call_async),
        log: Some(log),
        error: Some(error),
    }
}

fn last_error() -> String {
    unsafe { CStr::from_ptr(nsh_last_error()) }.to_string_lossy().into_owned()
}

const MAIN_JS: &str = r#"
import { View, div, v_flex } from "native-shell";
import { greet, fail, boom, fetch_name, on } from "demo/host";

export default class App extends View {
  init(_props, cx) {
    this.count = 0;
    this.sync = greet("ada");
    try { fail(); } catch (e) { this.failed = String(e.message ?? e); }
    this.answer = "pending";
    this.tick = 0;
    cx.spawn(async (cx) => {
      this.answer = await fetch_name(7);
      cx.notify();
    });
    on("tick", (payload, cx) => { this.tick = payload.n; cx.notify(); });
  }
  render(_cx) {
    return v_flex().size_full()
      .child(div().test_id("btn").bg("primary").on_click((_e, cx) => { this.count += 1; cx.notify(); })
        .child(`count ${this.count}`))
      .child(div().test_id("boom").bg("primary").on_click((_e, _cx) => { boom(); }).child("boom"))
      .child(div().child(`sync ${this.sync}`))
      .child(div().child(`failed ${this.failed}`))
      .child(div().child(`answer ${this.answer}`))
      .child(div().child(`tick ${this.tick}`));
  }
}
"#;

const MANIFEST: &str = r#"{"id":"org.example.ffi","name":"FFI test"}"#;

fn registry() -> serde_json::Value {
    serde_json::json!({
        "modules": [{
            "name": "demo/host",
            "methods": [
                {"name": "greet", "kind": "sync"},
                {"name": "fail", "kind": "sync"},
                {"name": "boom", "kind": "sync"},
                {"name": "fetch_name", "kind": "async"}
            ],
            "events": ["tick"],
            "requires_permission": false
        }]
    })
}

fn config(dir: &std::path::Path, threading: u32, extra: serde_json::Value) -> String {
    let mut c = serde_json::json!({
        "app_id": "org.example.ffi",
        "data_dir": dir,
        "threading": threading,
        "registry": registry(),
        "environment": {"width": 390, "height": 844, "platform": "test"}
    });
    if let (Some(c), Some(extra)) = (c.as_object_mut(), extra.as_object()) {
        c.extend(extra.clone());
    }
    c.to_string()
}

fn create(r: &Recorder, cfg: &str) -> NSHInstance {
    let cb = callbacks(r);
    let id = unsafe { nsh_instance_create(&cb, cfg.as_ptr(), cfg.len()) };
    assert_ne!(id, 0, "create failed: {}", last_error());
    id
}

fn load_sources(id: NSHInstance, main_js: &str) -> i32 {
    let json = serde_json::json!({
        "manifest": serde_json::from_str::<serde_json::Value>(MANIFEST).unwrap(),
        "files": {"main.js": main_js}
    })
    .to_string();
    unsafe { nsh_instance_load_sources(id, json.as_ptr(), json.len()) }
}

/// Everything the renderer would have applied so far.
#[derive(Default)]
struct Screen {
    mutations: Vec<Mutation>,
    seq: u32,
}

impl Screen {
    fn texts(&self) -> Vec<String> {
        self.mutations
            .iter()
            .filter_map(|m| match m {
                Mutation::SetProp { prop: Prop::AttributedText, value: PropValue::Json(j), .. } => Some(j.clone()),
                _ => None,
            })
            .collect()
    }

    fn has_text(&self, needle: &str) -> bool {
        self.texts().iter().any(|t| t.contains(needle))
    }

    fn node_with_test_id(&self, id: &str) -> Option<u32> {
        self.mutations.iter().find_map(|m| match m {
            Mutation::SetProp { node, prop: Prop::TestId, value: PropValue::Str(s) } if s == id => Some(*node),
            _ => None,
        })
    }

    /// Drives the instance (ticks in caller mode) and applies batches until `pred` holds.
    fn wait(&mut self, id: NSHInstance, caller: bool, what: &str, pred: impl Fn(&Screen) -> bool) {
        let start = Instant::now();
        let t0 = Instant::now();
        while !pred(self) {
            assert!(start.elapsed() < Duration::from_secs(10), "timed out waiting for {what}; texts: {:?}", self.texts());
            if caller {
                nsh_instance_tick(id, t0.elapsed().as_secs_f64() * 1000.0);
            }
            let b = nsh_instance_take_batch(id);
            if b.handle == 0 {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            let bytes = unsafe { std::slice::from_raw_parts(b.ptr, b.len) }.to_vec();
            nsh_buffer_free(b);
            let batch = decode(&bytes).expect("batch decodes");
            if let Some(Mutation::BeginBatch { seq, .. }) = batch.first() {
                self.seq = *seq;
            }
            self.mutations.extend(batch);
        }
    }

    fn click(&self, id: NSHInstance, test_id: &str) {
        let node = self.node_with_test_id(test_id).unwrap_or_else(|| panic!("no node with test id {test_id}"));
        let payload = r#"{"x":1,"y":1,"count":1}"#;
        unsafe { nsh_instance_dispatch_event(id, node, EventType::Click as u32, payload.as_ptr(), payload.len(), self.seq) };
    }
}

fn metrics(id: NSHInstance) -> serde_json::Value {
    let b = nsh_instance_metrics(id);
    assert_ne!(b.handle, 0, "metrics failed: {}", last_error());
    let json = unsafe { std::slice::from_raw_parts(b.ptr, b.len) }.to_vec();
    nsh_buffer_free(b);
    serde_json::from_slice(&json).expect("metrics are JSON")
}

fn respond(id: NSHInstance, request_id: u64, ok: bool, json: &str) {
    unsafe { nsh_instance_host_respond(id, request_id, ok as i32, json.as_ptr(), json.len()) };
}

fn emit(id: NSHInstance, module: &str, event: &str, payload: &str) {
    unsafe {
        nsh_instance_host_emit(id, module.as_ptr(), module.len(), event.as_ptr(), event.len(), payload.as_ptr(), payload.len())
    };
}

fn wait_async_call(r: &Recorder) -> (u64, String, String, String) {
    let start = Instant::now();
    loop {
        if let Some(c) = r.async_calls.lock().unwrap().first().cloned() {
            return c;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "no call_async arrived");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn full_round_trip(threading: u32) {
    let caller = threading == NSH_THREAD_CALLER;
    let dir = tempfile::tempdir().unwrap();
    let r = Recorder::default();
    let id = create(&r, &config(dir.path(), threading, serde_json::json!({})));
    assert_eq!(load_sources(id, MAIN_JS), 0, "{}", last_error());
    let env = r#"{"width": 320, "height": 640, "appearance": "dark", "platform": "test"}"#;
    unsafe { nsh_instance_set_environment(id, env.as_ptr(), env.len()) };

    let mut screen = Screen::default();
    screen.wait(id, caller, "first render", |s| s.has_text("count 0"));
    assert!(screen.has_text("sync hello ada"), "{:?}", screen.texts());
    assert!(screen.has_text("failed") && screen.has_text("no such user"), "{:?}", screen.texts());
    assert!(r.ready.load(Ordering::SeqCst) > 0, "batch_ready was never called");

    screen.click(id, "btn");
    screen.wait(id, caller, "re-render after click", |s| s.has_text("count 1"));

    // Metrics: from this thread, and from another one without touching the instance.
    let m = metrics(id);
    assert!(m["renders"].as_u64().unwrap_or(0) >= 2, "{m}");
    assert!(m["batches"].as_u64().unwrap_or(0) >= 2, "{m}");
    assert!(m["native_nodes"].as_u64().unwrap_or(0) > 0, "{m}");
    let other = std::thread::spawn(move || metrics(id)).join().unwrap();
    assert_eq!(other["renders"], m["renders"], "{other}");
    assert_eq!(other["native_nodes"], m["native_nodes"], "{other}");

    // Async host call: answered from outside the callback.
    if caller {
        // The spawned task runs on a tick.
        nsh_instance_tick(id, 0.0);
    }
    let (request_id, module, method, args) = wait_async_call(&r);
    assert_eq!((module.as_str(), method.as_str(), args.as_str()), ("demo/host", "fetch_name", "[7]"));
    respond(id, request_id, true, r#""Grace""#);
    screen.wait(id, caller, "async answer", |s| s.has_text("answer Grace"));

    emit(id, "demo/host", "tick", r#"{"n": 42}"#);
    screen.wait(id, caller, "emitted event", |s| s.has_text("tick 42"));

    nsh_instance_set_active(id, 0);
    nsh_instance_set_active(id, 1);
    nsh_instance_trim_memory(id);
    assert!(r.errors.lock().unwrap().is_empty(), "unexpected errors: {:?}", r.errors.lock().unwrap());
    nsh_instance_destroy(id);
    assert_eq!(load_sources(id, MAIN_JS), 1);
    assert!(last_error().contains("no instance"), "{}", last_error());
}

#[test]
fn dedicated_thread_round_trip() {
    full_round_trip(NSH_THREAD_DEDICATED);
}

#[test]
fn caller_thread_round_trip() {
    full_round_trip(NSH_THREAD_CALLER);
}

#[test]
fn caller_mode_respond_from_inside_call_async() {
    let dir = tempfile::tempdir().unwrap();
    let r = Recorder { respond_inline: true, ..Default::default() };
    let id = create(&r, &config(dir.path(), NSH_THREAD_CALLER, serde_json::json!({})));
    assert_eq!(load_sources(id, MAIN_JS), 0, "{}", last_error());
    let mut screen = Screen::default();
    screen.wait(id, true, "inline async answer", |s| s.has_text("answer inline [7]"));
    nsh_instance_destroy(id);
}

#[test]
fn caller_mode_rejects_other_threads() {
    let dir = tempfile::tempdir().unwrap();
    let r = Recorder::default();
    let id = create(&r, &config(dir.path(), NSH_THREAD_CALLER, serde_json::json!({})));
    let err = std::thread::spawn(move || {
        nsh_instance_destroy(id);
        last_error()
    })
    .join()
    .unwrap();
    assert!(err.contains("NSH_THREAD_CALLER"), "{err}");
    // Still alive on its own thread.
    assert_eq!(load_sources(id, MAIN_JS), 0, "{}", last_error());
    nsh_instance_destroy(id);
}

#[test]
fn panic_in_host_callback_destroys_only_that_instance() {
    for threading in [NSH_THREAD_CALLER, NSH_THREAD_DEDICATED] {
        let caller = threading == NSH_THREAD_CALLER;
        let dir = tempfile::tempdir().unwrap();
        let (r1, r2) = (Recorder::default(), Recorder::default());
        let a = create(&r1, &config(dir.path(), threading, serde_json::json!({})));
        let b = create(&r2, &config(dir.path(), threading, serde_json::json!({})));
        assert_eq!(load_sources(a, MAIN_JS), 0);
        assert_eq!(load_sources(b, MAIN_JS), 0);
        let (mut sa, mut sb) = (Screen::default(), Screen::default());
        sa.wait(a, caller, "a renders", |s| s.has_text("count 0"));
        sb.wait(b, caller, "b renders", |s| s.has_text("count 0"));

        sa.click(a, "boom");
        let start = Instant::now();
        while r1.errors.lock().unwrap().is_empty() {
            assert!(start.elapsed() < Duration::from_secs(10), "panic was not reported");
            if caller {
                nsh_instance_tick(a, 0.0);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(r1.errors.lock().unwrap()[0].contains("panicked"), "{:?}", r1.errors.lock().unwrap());
        // The next call finds the instance gone.
        let start = Instant::now();
        while load_sources(a, MAIN_JS) == 0 {
            assert!(start.elapsed() < Duration::from_secs(10), "poisoned instance kept running");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(nsh_instance_take_batch(a).handle, 0);

        // The other instance is unaffected.
        sb.click(b, "btn");
        sb.wait(b, caller, "b still works", |s| s.has_text("count 1"));
        assert!(r2.errors.lock().unwrap().is_empty());
        nsh_instance_destroy(b);
    }
}

#[test]
fn invalid_ids_and_garbage_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let r = Recorder::default();
    let cb = callbacks(&r);

    for bad in ["{nope", "[]", r#"{"data_dir": 5}"#, r#"{"threading": 7, "data_dir": "/tmp"}"#, r#"{"data_dirr": "/tmp"}"#] {
        assert_eq!(unsafe { nsh_instance_create(&cb, bad.as_ptr(), bad.len()) }, 0, "{bad} accepted");
        assert!(!last_error().is_empty());
    }
    assert_eq!(unsafe { nsh_instance_create(std::ptr::null(), b"{}".as_ptr(), 2) }, 0);
    assert_eq!(unsafe { nsh_instance_create(&cb, std::ptr::null(), 3) }, 0);

    for id in [0, 424242, u64::MAX] {
        assert_eq!(load_sources(id, MAIN_JS), 1);
        assert!(last_error().contains("no instance"), "{}", last_error());
        assert_eq!(unsafe { nsh_instance_load_bundle(id, b"x".as_ptr(), 1) }, 1);
        unsafe { nsh_instance_dispatch_event(id, 1, 1, std::ptr::null(), 0, 0) };
        assert_eq!(nsh_instance_tick(id, 0.0), -1);
        assert_eq!(nsh_instance_take_batch(id).handle, 0);
        assert_eq!(nsh_instance_metrics(id).handle, 0);
        nsh_instance_set_active(id, 1);
        nsh_instance_trim_memory(id);
        nsh_instance_destroy(id);
    }

    let id = create(&r, &config(dir.path(), NSH_THREAD_CALLER, serde_json::json!({})));
    for bad in ["", "{", r#"{"manifest": {"id": "x"}, "files": {}}"#, r#"{"manifest": 1}"#] {
        assert_eq!(unsafe { nsh_instance_load_sources(id, bad.as_ptr(), bad.len()) }, 1, "{bad} accepted");
    }
    let bad_env = "{width:";
    unsafe { nsh_instance_set_environment(id, bad_env.as_ptr(), bad_env.len()) };
    assert!(last_error().contains("environment"), "{}", last_error());
    unsafe { nsh_instance_dispatch_event(id, 1, 9999, b"{}".as_ptr(), 2, 0) };
    assert!(last_error().contains("event type"), "{}", last_error());
    let invalid_utf8 = [0xffu8, 0xfe];
    unsafe { nsh_instance_dispatch_event(id, 1, 1, invalid_utf8.as_ptr(), 2, 0) };
    assert!(last_error().contains("UTF-8"), "{}", last_error());
    nsh_instance_destroy(id);
}

#[test]
fn buffers_free_by_handle() {
    let b = unsafe { nsh_string_new(b"hello".as_ptr(), 5) };
    assert_eq!(b.len, 5);
    assert_eq!(unsafe { std::slice::from_raw_parts(b.ptr, b.len) }, b"hello");
    nsh_buffer_free(b);
    nsh_buffer_free(b);
    nsh_buffer_free(NSHBuffer { ptr: std::ptr::null(), len: 0, handle: 0 });
    nsh_buffer_free(NSHBuffer { ptr: std::ptr::null(), len: 0, handle: u64::MAX });
    let empty = unsafe { nsh_string_new(std::ptr::null(), 0) };
    assert_eq!(empty.handle, 0);
}

#[test]
fn version_and_hash() {
    assert_eq!(nsh_protocol_version(), native_shell_core::protocol::PROTOCOL_VERSION);
    let hash = unsafe { CStr::from_ptr(nsh_abi_hash()) }.to_str().unwrap().to_string();
    assert_eq!(hash, native_shell_core::protocol::abi_hash());
    assert_eq!(hash.len(), 16);
}

#[test]
fn signed_bundles_are_verified_before_loading() {
    let (secret_b64, public_b64) = bundle::generate_keypair();
    let secret = bundle::secret_key_from_base64(&secret_b64).unwrap();
    let manifest = native_shell_core::manifest::Manifest::parse(MANIFEST).unwrap();
    let mut files = BTreeMap::new();
    files.insert("main.js".to_string(), MAIN_JS.as_bytes().to_vec());
    let signed = bundle::build(&files, &manifest, &secret, "release-1", &Default::default()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let trust = serde_json::json!({"trusted_keys": [{"id": "release-1", "public_key": public_b64}]});

    // Tampered: one flipped byte anywhere is rejected before QuickJS sees it.
    let r = Recorder::default();
    let id = create(&r, &config(dir.path(), NSH_THREAD_CALLER, trust.clone()));
    let mut tampered = signed.clone();
    let mid = tampered.len() / 2;
    tampered[mid] ^= 0x20;
    assert_eq!(unsafe { nsh_instance_load_bundle(id, tampered.as_ptr(), tampered.len()) }, 1);
    assert!(last_error().contains("bundle rejected"), "{}", last_error());
    assert_eq!(unsafe { nsh_instance_load_bundle(id, b"garbage".as_ptr(), 7) }, 1);

    // Untrusted signer.
    let other = Recorder::default();
    let (_, other_pub) = bundle::generate_keypair();
    let untrusting = create(
        &other,
        &config(dir.path(), NSH_THREAD_CALLER, serde_json::json!({"trusted_keys": [{"id": "release-1", "public_key": other_pub}]})),
    );
    assert_eq!(unsafe { nsh_instance_load_bundle(untrusting, signed.as_ptr(), signed.len()) }, 1);
    assert!(last_error().contains("signature"), "{}", last_error());
    nsh_instance_destroy(untrusting);

    // Revoked.
    let sha = {
        use sha2::Digest;
        sha2::Sha256::digest(&signed).iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    let revoking = create(
        &other,
        &config(
            dir.path(),
            NSH_THREAD_CALLER,
            serde_json::json!({"trusted_keys": [{"id": "release-1", "public_key": public_b64}], "revoked": [sha]}),
        ),
    );
    assert_eq!(unsafe { nsh_instance_load_bundle(revoking, signed.as_ptr(), signed.len()) }, 1);
    assert!(last_error().contains("revoked"), "{}", last_error());
    nsh_instance_destroy(revoking);

    // Genuine.
    assert_eq!(unsafe { nsh_instance_load_bundle(id, signed.as_ptr(), signed.len()) }, 0, "{}", last_error());
    let mut screen = Screen::default();
    screen.wait(id, true, "bundle renders", |s| s.has_text("count 0"));
    nsh_instance_destroy(id);

    // A bad key in the config is a create error.
    let bad = config(dir.path(), 0, serde_json::json!({"trusted_keys": [{"id": "k", "public_key": "not base64!"}]}));
    let cb = callbacks(&r);
    assert_eq!(unsafe { nsh_instance_create(&cb, bad.as_ptr(), bad.len()) }, 0);
    assert!(last_error().contains("trusted key"), "{}", last_error());
}

fn background_of(screen: &Screen, test_id: &str) -> Option<u32> {
    let node = screen.node_with_test_id(test_id)?;
    screen.mutations.iter().rev().find_map(|m| match m {
        Mutation::SetProp { node: n, prop: Prop::BackgroundColor, value: PropValue::Color(c) } if *n == node => Some(*c),
        _ => None,
    })
}

#[test]
fn theme_override_changes_rendered_colors() {
    let dir = tempfile::tempdir().unwrap();
    let r = Recorder::default();
    let theme = serde_json::json!({"theme": {"light": {"primary": "#ff0000"}, "dark": {"primary": "#00ff00"}}});
    let id = create(&r, &config(dir.path(), NSH_THREAD_CALLER, theme));
    assert_eq!(load_sources(id, MAIN_JS), 0, "{}", last_error());
    let mut screen = Screen::default();
    screen.wait(id, true, "first render", |s| s.has_text("count 0"));
    assert_eq!(background_of(&screen, "btn"), Some(0xFFFF_0000), "light override applies");

    let dark = r#"{"width": 390, "height": 844, "appearance": "dark", "platform": "test"}"#;
    unsafe { nsh_instance_set_environment(id, dark.as_ptr(), dark.len()) };
    screen.wait(id, true, "dark override", |s| background_of(s, "btn") == Some(0xFF00_FF00));
    nsh_instance_destroy(id);
}

#[test]
fn bad_themes_and_reserved_modules_fail_create() {
    let dir = tempfile::tempdir().unwrap();
    let r = Recorder::default();
    let cb = callbacks(&r);
    let cases = [
        (serde_json::json!({"theme": {"light": {"primry": "#ff0000"}}}), "primry"),
        (serde_json::json!({"theme": {"dark": {"primary": "red-ish"}}}), "red-ish"),
        (serde_json::json!({"theme": {"sepia": {}}}), "sepia"),
        (serde_json::json!({"registry": {"modules": [{"name": "nsh/clipboard"}]}}), "reserved"),
    ];
    for (extra, needle) in cases {
        let cfg = config(dir.path(), NSH_THREAD_CALLER, extra);
        assert_eq!(unsafe { nsh_instance_create(&cb, cfg.as_ptr(), cfg.len()) }, 0, "{needle}");
        assert!(last_error().contains(needle), "{needle}: {}", last_error());
    }
}

#[test]
fn text_is_measured_in_batches_when_the_host_can() {
    let dir = tempfile::tempdir().unwrap();
    // With a batch callback: the tree hands it every new text at once.
    let r = Recorder { batch: Batch::Measure, ..Default::default() };
    let id = create(&r, &config(dir.path(), NSH_THREAD_CALLER, serde_json::json!({})));
    assert_eq!(load_sources(id, MAIN_JS), 0, "{}", last_error());
    let mut screen = Screen::default();
    screen.wait(id, true, "batch-measured render", |s| s.has_text("count 0"));
    assert!(r.batch_calls.load(Ordering::SeqCst) >= 1, "measure_text_batch was never called");
    assert!(r.batched_texts.load(Ordering::SeqCst) >= 6, "the batch should carry every text of the first frame");
    let m = metrics(id);
    assert!(m["measure_text_calls"].as_u64().is_some(), "{m}");
    nsh_instance_destroy(id);

    // NULL batch callback: the same frame through measure_text, one text at a time.
    let r = Recorder::default();
    let id = create(&r, &config(dir.path(), NSH_THREAD_CALLER, serde_json::json!({})));
    assert_eq!(load_sources(id, MAIN_JS), 0, "{}", last_error());
    let mut screen = Screen::default();
    screen.wait(id, true, "per-item render", |s| s.has_text("count 0"));
    assert_eq!(r.batch_calls.load(Ordering::SeqCst), 0);
    assert!(r.single_measures.load(Ordering::SeqCst) >= 6);
    nsh_instance_destroy(id);

    // A batch callback that panics poisons only its instance, like any callback.
    let r = Recorder { batch: Batch::Panic, ..Default::default() };
    let id = create(&r, &config(dir.path(), NSH_THREAD_CALLER, serde_json::json!({})));
    load_sources(id, MAIN_JS);
    nsh_instance_tick(id, 0.0);
    assert!(r.errors.lock().unwrap().iter().any(|e| e.contains("measure_text_batch")), "{:?}", r.errors.lock().unwrap());
    assert_eq!(load_sources(id, MAIN_JS), 1);
}

const BUSY_JS: &str = r#"
import { View, div } from "native-shell";
export default class Busy extends View {
  init(_props, cx) {
    this.n = 0;
    cx.timer.every(1, (cx) => {
      for (let i = 0; i < 20; i++) console.log("tick " + this.n + " " + i);
      this.n += 1;
      cx.notify();
    });
  }
  render(_cx) {
    return div().child(`n ${this.n}`).child(div().child(`m ${this.n * 2}`));
  }
}
"#;

#[test]
fn no_callback_after_destroy_returns() {
    for threading in [NSH_THREAD_DEDICATED, NSH_THREAD_CALLER] {
        let caller = threading == NSH_THREAD_CALLER;
        let dir = tempfile::tempdir().unwrap();
        // A slow, chatty host: the JS thread is almost always inside `log` when destroy arrives.
        let r = Recorder { slow_log: Some(Duration::from_millis(1)), ..Default::default() };
        let id = create(&r, &config(dir.path(), threading, serde_json::json!({})));
        assert_eq!(load_sources(id, BUSY_JS), 0, "{}", last_error());
        let mut screen = Screen::default();
        screen.wait(id, caller, "the busy program running", |s| s.texts().len() >= 6);
        nsh_instance_destroy(id);
        r.destroyed.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(r.late.load(Ordering::SeqCst), 0, "callbacks ran after destroy returned (threading {threading})");
        assert!(!r.logs.lock().unwrap().is_empty());
    }
}

#[test]
fn destroy_from_inside_a_callback() {
    for threading in [NSH_THREAD_CALLER, NSH_THREAD_DEDICATED] {
        let caller = threading == NSH_THREAD_CALLER;
        let dir = tempfile::tempdir().unwrap();
        let r = Recorder { destroy_in_async: true, ..Default::default() };
        let id = create(&r, &config(dir.path(), threading, serde_json::json!({})));
        load_sources(id, MAIN_JS);
        // `fetch_name` is called from init's task; its callback destroys the instance.
        let start = Instant::now();
        while r.async_calls.lock().unwrap().is_empty() {
            assert!(start.elapsed() < Duration::from_secs(10), "call_async never came");
            if caller {
                nsh_instance_tick(id, 0.0);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        r.destroyed.store(true, Ordering::SeqCst);
        // The instance is gone for the API, and silent for the host.
        let start = Instant::now();
        while load_sources(id, MAIN_JS) == 0 {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(last_error().contains("no instance"), "{}", last_error());
        if caller {
            assert_eq!(nsh_instance_tick(id, 1.0), -1);
        }
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(r.late.load(Ordering::SeqCst), 0, "callbacks after a destroy from inside a callback (threading {threading})");
    }
}
