//! JNI entry points of `dev.nativeshell.NativeBridge` and the [`Host`] backed
//! by the Kotlin `HostCallbacks` object.
//!
//! Same semantics as the C ABI: every function maps onto [`crate::instance`].
//! Strings cross through the jni crate's conversions, which translate Java's
//! modified UTF-8 (surrogate pairs, embedded NUL) correctly; batches cross as
//! direct, little-endian `ByteBuffer`s over the Rust-owned bytes, zero copy.
//!
//! Callbacks run on the instance's JS thread, which is attached to the JVM
//! permanently on first use (attaching per call costs microseconds each).
//! Because that thread never returns to Java, every callback runs inside a
//! local reference frame so its local references are freed on return. A Kotlin
//! callback that throws has its exception cleared and turned into an error
//! (a rejected call for `callSync`, a reported `error` otherwise), and a Rust
//! panic poisons the instance just like the C host.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use jni::objects::{GlobalRef, JByteArray, JFloatArray, JMethodID, JObject, JString, JValue};
use jni::signature::{Primitive, ReturnType};
use jni::sys::{jboolean, jdouble, jint, jlong, jobject, jstring, JNI_TRUE};
use jni::{JNIEnv, JavaVM};
use native_shell_core::host::{Host, LogLevel, Measurer, TextMetrics, WidthMode};

use crate::error::{self, panic_message};
use crate::gate::Gate;
use crate::{buffer, instance};

struct Methods {
    measure_text: JMethodID,
    /// `None` for a `HostCallbacks` from before batching (an older library).
    measure_text_batch: Option<JMethodID>,
    measure_component: JMethodID,
    batch_ready: JMethodID,
    call_sync: JMethodID,
    call_async: JMethodID,
    log: JMethodID,
    error: JMethodID,
}

pub struct JniHost {
    vm: JavaVM,
    callbacks: GlobalRef,
    methods: Methods,
    poisoned: Arc<AtomicBool>,
    gate: Arc<Gate>,
}

const FRAME: i32 = 16;

impl JniHost {
    fn new(
        env: &mut JNIEnv,
        callbacks: &JObject,
        poisoned: Arc<AtomicBool>,
        gate: Arc<Gate>,
    ) -> jni::errors::Result<JniHost> {
        let class = env.get_object_class(callbacks)?;
        let methods = Methods {
            measure_text: env.get_method_id(&class, "measureText", "(Ljava/lang/String;FI)[F")?,
            measure_text_batch: match env.get_method_id(&class, "measureTextBatch", "([Ljava/lang/String;[F[I)[F") {
                Ok(id) => Some(id),
                Err(_) => {
                    // NoSuchMethodError: an older callbacks object; measure one text at a time.
                    let _ = env.exception_clear();
                    None
                }
            },
            measure_component: env.get_method_id(
                &class,
                "measureComponent",
                "(Ljava/lang/String;Ljava/lang/String;FI)[F",
            )?,
            batch_ready: env.get_method_id(&class, "batchReady", "()V")?,
            call_sync: env.get_method_id(
                &class,
                "callSync",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            )?,
            call_async: env.get_method_id(
                &class,
                "callAsync",
                "(JLjava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            )?,
            log: env.get_method_id(&class, "log", "(ILjava/lang/String;)V")?,
            error: env.get_method_id(&class, "error", "(Ljava/lang/String;)V")?,
        };
        Ok(JniHost { vm: env.get_java_vm()?, callbacks: env.new_global_ref(callbacks)?, methods, poisoned, gate })
    }

    /// Runs `f` on an attached env inside a local frame. A pending Java
    /// exception becomes `Err(message)`; a panic poisons the instance.
    fn call<R>(&self, what: &str, f: impl FnOnce(&mut JNIEnv) -> jni::errors::Result<R>) -> Result<R, String> {
        self.call_framed(what, FRAME, f)
    }

    /// [`call`](Self::call) with room for `capacity` local references.
    fn call_framed<R>(
        &self,
        what: &str,
        capacity: i32,
        f: impl FnOnce(&mut JNIEnv) -> jni::errors::Result<R>,
    ) -> Result<R, String> {
        let Some(_pass) = self.gate.enter() else { return Err(format!("{what}: the instance was destroyed")) };
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<R, String> {
            let mut env = self.vm.attach_current_thread_permanently().map_err(|e| format!("{what}: {e}"))?;
            env.with_local_frame(capacity, |env| -> Result<R, jni::errors::Error> { f(env) })
                .map_err(|e| take_exception(&mut env).unwrap_or_else(|| format!("{what}: {e}")))
        }));
        match result {
            Ok(r) => r,
            Err(payload) => {
                self.poisoned.store(true, Ordering::SeqCst);
                let message = format!("host callback `{what}` panicked: {}", panic_message(payload.as_ref()));
                self.report(&message);
                Err(message)
            }
        }
    }

    /// `HostCallbacks.error`; failures of the error callback itself are dropped.
    fn report(&self, message: &str) {
        let Some(_pass) = self.gate.enter() else { return };
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let Ok(mut env) = self.vm.attach_current_thread_permanently() else { return };
            let _ = env.with_local_frame(FRAME, |env| -> jni::errors::Result<()> {
                let m = env.new_string(message)?;
                unsafe {
                    env.call_method_unchecked(
                        &self.callbacks,
                        self.methods.error,
                        ReturnType::Primitive(Primitive::Void),
                        &[JValue::Object(&m).as_jni()],
                    )?;
                }
                Ok(())
            });
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
        }));
    }

    fn void_call(&self, what: &str, f: impl FnOnce(&mut JNIEnv) -> jni::errors::Result<()>) {
        if let Err(e) = self.call(what, f) {
            if !self.poisoned.load(Ordering::SeqCst) {
                self.report(&e);
            }
        }
    }
}

/// Clears a pending Java exception and describes it (`Throwable.toString()`).
fn take_exception(env: &mut JNIEnv) -> Option<String> {
    if !env.exception_check().unwrap_or(false) {
        return None;
    }
    let throwable = env.exception_occurred().ok();
    let _ = env.exception_clear();
    let throwable = throwable?;
    let text = env.call_method(&throwable, "toString", "()Ljava/lang/String;", &[]).ok()?.l().ok()?;
    let _ = env.exception_clear();
    let text = JString::from(text);
    let s: String = env.get_string(&text).ok()?.into();
    Some(s)
}

impl Measurer for JniHost {
    fn measure_text(&self, attributed_json: &str, width: WidthMode) -> TextMetrics {
        let (w, mode) = width.to_raw();
        let r = self.call("measureText", |env| {
            let json = env.new_string(attributed_json)?;
            let arr = unsafe {
                env.call_method_unchecked(
                    &self.callbacks,
                    self.methods.measure_text,
                    ReturnType::Array,
                    &[JValue::Object(&json).as_jni(), JValue::Float(w).as_jni(), JValue::Int(mode).as_jni()],
                )?
            }
            .l()?;
            let mut out = [0.0f32; 3];
            if !arr.is_null() {
                let arr = JFloatArray::from(arr);
                let n = env.get_array_length(&arr)?.clamp(0, 3) as usize;
                env.get_float_array_region(&arr, 0, &mut out[..n])?;
            }
            Ok(TextMetrics { width: out[0], height: out[1], baseline: out[2] })
        });
        r.unwrap_or_else(|e| {
            self.report(&e);
            TextMetrics::default()
        })
    }

    /// One `measureTextBatch` call (3 floats per text). Falls back to one
    /// `measureText` per text when the object has no batch method, the call
    /// throws, or it answers the wrong number of floats.
    fn measure_text_batch(&self, requests: &[(&str, WidthMode)]) -> Vec<TextMetrics> {
        let per_item = || requests.iter().map(|(json, width)| self.measure_text(json, *width)).collect();
        let Some(method) = self.methods.measure_text_batch else { return per_item() };
        let n = requests.len();
        let (widths, modes): (Vec<f32>, Vec<i32>) = requests.iter().map(|(_, w)| w.to_raw()).unzip();
        let r = self.call_framed("measureTextBatch", 8, |env| {
            let texts = env.new_object_array(n as i32, "java/lang/String", JObject::null())?;
            for (i, (json, _)) in requests.iter().enumerate() {
                let s = env.new_string(json)?;
                env.set_object_array_element(&texts, i as i32, &s)?;
                env.delete_local_ref(s)?;
            }
            let w = env.new_float_array(n as i32)?;
            env.set_float_array_region(&w, 0, &widths)?;
            let m = env.new_int_array(n as i32)?;
            env.set_int_array_region(&m, 0, &modes)?;
            let arr = unsafe {
                env.call_method_unchecked(
                    &self.callbacks,
                    method,
                    ReturnType::Array,
                    &[JValue::Object(&texts).as_jni(), JValue::Object(&w).as_jni(), JValue::Object(&m).as_jni()],
                )?
            }
            .l()?;
            if arr.is_null() {
                return Ok(None);
            }
            let arr = JFloatArray::from(arr);
            if env.get_array_length(&arr)? as usize != 3 * n {
                return Ok(None);
            }
            let mut out = vec![0.0f32; 3 * n];
            env.get_float_array_region(&arr, 0, &mut out)?;
            Ok(Some(out))
        });
        match r {
            Ok(Some(out)) => out.as_chunks::<3>().0.iter().map(|[w, h, b]| TextMetrics { width: *w, height: *h, baseline: *b }).collect(),
            Ok(None) => per_item(),
            Err(e) => {
                if !self.poisoned.load(Ordering::SeqCst) {
                    self.log(LogLevel::Warn, &format!("measureTextBatch failed, measuring one text at a time: {e}"));
                }
                per_item()
            }
        }
    }

    fn measure_component(&self, name: &str, props_json: &str, width: WidthMode) -> Option<(f32, f32)> {
        let (w, mode) = width.to_raw();
        let r = self.call("measureComponent", |env| {
            let n = env.new_string(name)?;
            let p = env.new_string(props_json)?;
            let arr = unsafe {
                env.call_method_unchecked(
                    &self.callbacks,
                    self.methods.measure_component,
                    ReturnType::Array,
                    &[
                        JValue::Object(&n).as_jni(),
                        JValue::Object(&p).as_jni(),
                        JValue::Float(w).as_jni(),
                        JValue::Int(mode).as_jni(),
                    ],
                )?
            }
            .l()?;
            if arr.is_null() {
                return Ok(None);
            }
            let arr = JFloatArray::from(arr);
            if env.get_array_length(&arr)? < 2 {
                return Ok(None);
            }
            let mut out = [0.0f32; 2];
            env.get_float_array_region(&arr, 0, &mut out)?;
            Ok(Some((out[0], out[1])))
        });
        r.unwrap_or_else(|e| {
            self.report(&e);
            None
        })
    }
}

impl Host for JniHost {
    fn batch_ready(&self) {
        self.void_call("batchReady", |env| {
            unsafe {
                env.call_method_unchecked(
                    &self.callbacks,
                    self.methods.batch_ready,
                    ReturnType::Primitive(Primitive::Void),
                    &[],
                )?;
            }
            Ok(())
        });
    }

    fn call_sync(&self, module: &str, method: &str, args_json: &str) -> Result<String, String> {
        self.call("callSync", |env| {
            let m = env.new_string(module)?;
            let me = env.new_string(method)?;
            let a = env.new_string(args_json)?;
            let r = unsafe {
                env.call_method_unchecked(
                    &self.callbacks,
                    self.methods.call_sync,
                    ReturnType::Object,
                    &[JValue::Object(&m).as_jni(), JValue::Object(&me).as_jni(), JValue::Object(&a).as_jni()],
                )?
            }
            .l()?;
            if r.is_null() {
                return Ok("null".to_string());
            }
            let r = JString::from(r);
            let s: String = env.get_string(&r)?.into();
            Ok(s)
        })
    }

    fn call_async(&self, request_id: u64, module: &str, method: &str, args_json: &str) {
        self.void_call("callAsync", |env| {
            let m = env.new_string(module)?;
            let me = env.new_string(method)?;
            let a = env.new_string(args_json)?;
            unsafe {
                env.call_method_unchecked(
                    &self.callbacks,
                    self.methods.call_async,
                    ReturnType::Primitive(Primitive::Void),
                    &[
                        JValue::Long(request_id as jlong).as_jni(),
                        JValue::Object(&m).as_jni(),
                        JValue::Object(&me).as_jni(),
                        JValue::Object(&a).as_jni(),
                    ],
                )?;
            }
            Ok(())
        });
    }

    fn log(&self, level: LogLevel, message: &str) {
        let level: jint = match level {
            LogLevel::Debug => 0,
            LogLevel::Info => 1,
            LogLevel::Warn => 2,
            LogLevel::Error => 3,
        };
        self.void_call("log", |env| {
            let m = env.new_string(message)?;
            unsafe {
                env.call_method_unchecked(
                    &self.callbacks,
                    self.methods.log,
                    ReturnType::Primitive(Primitive::Void),
                    &[JValue::Int(level).as_jni(), JValue::Object(&m).as_jni()],
                )?;
            }
            Ok(())
        });
    }

    fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
        self.report(message);
    }
}

/// A `String` parameter of a native method. The JVM guarantees the type, so
/// the checked conversion's class lookup is skipped.
fn string(env: &mut JNIEnv, s: &JString) -> Result<String, String> {
    if s.is_null() {
        return Ok(String::new());
    }
    unsafe { env.get_string_unchecked(s) }.map(Into::into).map_err(|e| e.to_string())
}

/// Records a failure for `lastError()`.
fn fail<R>(fallback: R, f: impl FnOnce() -> Result<R, String>) -> R {
    error::clear();
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            error::set(e);
            fallback
        }
        Err(p) => {
            error::set(format!("panic: {}", panic_message(p.as_ref())));
            fallback
        }
    }
}

fn id(handle: jlong) -> u64 {
    handle as u64
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_create<'l>(
    mut env: JNIEnv<'l>,
    _this: JObject<'l>,
    callbacks: JObject<'l>,
    config_json: JString<'l>,
) -> jlong {
    fail(0, || {
        if callbacks.is_null() {
            return Err("callbacks is null".into());
        }
        let config = string(&mut env, &config_json)?;
        let env = &mut env;
        let id = instance::create(
            config.as_bytes(),
            Box::new(move |_id, poisoned, gate| {
                let host = JniHost::new(env, &callbacks, poisoned, gate).map_err(|e| {
                    let detail = take_exception(env).unwrap_or_else(|| e.to_string());
                    format!("HostCallbacks: {detail}")
                })?;
                Ok(Arc::new(host) as Arc<dyn Host>)
            }),
        )?;
        Ok(id as jlong)
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_loadSources<'l>(
    mut env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
    json: JString<'l>,
) -> jint {
    fail(1, || {
        let json = string(&mut env, &json)?;
        instance::load_sources(id(handle), json.as_bytes()).map(|_| 0)
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_loadBundle<'l>(
    env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
    bytes: JByteArray<'l>,
) -> jint {
    fail(1, || {
        let bytes = env.convert_byte_array(&bytes).map_err(|e| e.to_string())?;
        instance::load_bundle(id(handle), &bytes).map(|_| 0)
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_setEnvironment<'l>(
    mut env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
    json: JString<'l>,
) {
    fail((), || {
        let json = string(&mut env, &json)?;
        instance::set_environment(id(handle), json.as_bytes())
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_dispatchEvent<'l>(
    mut env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
    node: jint,
    event_type: jint,
    payload_json: JString<'l>,
    seq: jint,
) {
    fail((), || {
        let payload = string(&mut env, &payload_json)?;
        instance::dispatch_event(id(handle), node as u32, event_type as u32, &payload, seq as u32)
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_hostRespond<'l>(
    mut env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
    request_id: jlong,
    ok: jboolean,
    json: JString<'l>,
) {
    fail((), || {
        let json = string(&mut env, &json)?;
        instance::host_respond(id(handle), request_id as u64, ok == JNI_TRUE, &json)
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_hostEmit<'l>(
    mut env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
    module: JString<'l>,
    event: JString<'l>,
    payload_json: JString<'l>,
) {
    fail((), || {
        let module = string(&mut env, &module)?;
        let event = string(&mut env, &event)?;
        let payload = string(&mut env, &payload_json)?;
        instance::host_emit(id(handle), &module, &event, &payload)
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_tick<'l>(
    _env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
    now_ms: jdouble,
) -> jlong {
    fail(-1, || instance::tick(id(handle), now_ms))
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_takeBatch<'l>(
    _env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
) -> jlong {
    fail(0, || Ok(instance::take_batch(id(handle))?.map(|b| buffer::insert(b).2 as jlong).unwrap_or(0)))
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_batchBuffer<'l>(
    mut env: JNIEnv<'l>,
    _this: JObject<'l>,
    batch: jlong,
) -> jobject {
    fail(std::ptr::null_mut(), || {
        let (ptr, len) = buffer::get(batch as u64).ok_or_else(|| format!("no batch {batch}"))?;
        // SAFETY: the bytes stay alive (and unmoved) until `freeBatch`; the
        // Kotlin side must not touch the buffer after freeing it.
        let buf = unsafe { env.new_direct_byte_buffer(ptr as *mut u8, len) }.map_err(|e| e.to_string())?;
        let order = env
            .get_static_field("java/nio/ByteOrder", "LITTLE_ENDIAN", "Ljava/nio/ByteOrder;")
            .and_then(|v| v.l())
            .map_err(|e| e.to_string())?;
        env.call_method(&buf, "order", "(Ljava/nio/ByteOrder;)Ljava/nio/ByteBuffer;", &[JValue::Object(&order)])
            .map_err(|e| e.to_string())?;
        Ok(buf.into_raw())
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_freeBatch<'l>(
    _env: JNIEnv<'l>,
    _this: JObject<'l>,
    batch: jlong,
) {
    buffer::free(batch as u64);
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_setActive<'l>(
    _env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
    active: jboolean,
) {
    fail((), || instance::set_active(id(handle), active == JNI_TRUE))
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_trimMemory<'l>(
    _env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
) {
    fail((), || instance::trim_memory(id(handle)))
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_metrics<'l>(
    mut env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
) -> jstring {
    // Empty on failure (never null: Kotlin declares it `String`); `lastError()` says why.
    let json = fail(String::new(), || instance::metrics(id(handle)));
    new_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_destroy<'l>(
    _env: JNIEnv<'l>,
    _this: JObject<'l>,
    handle: jlong,
) {
    fail((), || instance::destroy(id(handle)))
}

fn new_jstring(env: &mut JNIEnv, s: &str) -> jstring {
    env.new_string(s).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_lastError<'l>(
    mut env: JNIEnv<'l>,
    _this: JObject<'l>,
) -> jstring {
    new_jstring(&mut env, &error::get())
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_abiHash<'l>(mut env: JNIEnv<'l>, _this: JObject<'l>) -> jstring {
    new_jstring(&mut env, &native_shell_core::protocol::abi_hash())
}

#[no_mangle]
pub extern "system" fn Java_dev_nativeshell_NativeBridge_protocolVersion<'l>(
    _env: JNIEnv<'l>,
    _this: JObject<'l>,
) -> jint {
    native_shell_core::protocol::PROTOCOL_VERSION as jint
}
