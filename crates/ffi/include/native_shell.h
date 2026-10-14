// native-shell C ABI. iOS links it from the xcframework; Android reaches the
// same functions through the JNI layer in the same .so.
//
// Threading: an instance owns a JS thread (NSH_THREAD_DEDICATED, default) or
// runs on the thread that calls it (NSH_THREAD_CALLER: every call below must
// then come from that one thread, and the host drives it with nsh_instance_tick).
//
// Callbacks in NSHHostCallbacks are invoked on the instance's JS thread; they
// must be thread-safe and must not call back into the same instance, except
// that host_respond / host_emit may be called from inside call_async, and
// nsh_instance_destroy may be called from inside any callback.
//
// Once nsh_instance_destroy returns, no callback for that instance runs again,
// on any thread: destroy refuses later callbacks, waits for those in flight on
// other threads and stops the JS thread. `context` may be released then.
//
// Strings are UTF-8 with an explicit length, never NUL-terminated on input.
// Every entry point catches Rust panics; a panic destroys only that instance
// and is reported through `error`.

#ifndef NATIVE_SHELL_H
#define NATIVE_SHELL_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define NSH_ABI_VERSION 1

typedef uint64_t NSHInstance;  // 0 is never a valid instance

typedef struct NSHTextMetrics {
  float width;
  float height;
  float baseline;
} NSHTextMetrics;

typedef struct NSHSize {
  float width;
  float height;
  int32_t has_value;  // 0: no intrinsic size
} NSHSize;

typedef struct NSHBuffer {
  const uint8_t *ptr;
  size_t len;
  uint64_t handle;  // pass to nsh_buffer_free; 0 when empty
} NSHBuffer;

typedef enum NSHLogLevel { NSH_LOG_DEBUG = 0, NSH_LOG_INFO = 1, NSH_LOG_WARN = 2, NSH_LOG_ERROR = 3 } NSHLogLevel;

typedef struct NSHMeasureRequest {
  const uint8_t *attributed_json;
  size_t len;
  float width;
  int32_t width_mode;
} NSHMeasureRequest;

// width_mode: 0 undefined (width is +inf), 1 at most `width`, 2 exactly `width`.
typedef struct NSHHostCallbacks {
  void *context;
  NSHTextMetrics (*measure_text)(void *context, const uint8_t *attributed_json, size_t len, float width,
                                int32_t width_mode);
  // Optional (may be NULL): measure `count` texts at once, writing out[i] for requests[i].
  // The core calls it with every text it is about to lay out, so the renderer can shape
  // them in parallel. Results must equal what measure_text would return for each request.
  void (*measure_text_batch)(void *context, const NSHMeasureRequest *requests, size_t count, NSHTextMetrics *out);
  NSHSize (*measure_component)(void *context, const uint8_t *name, size_t name_len, const uint8_t *props_json,
                              size_t props_len, float width, int32_t width_mode);
  // A batch is ready: schedule nsh_instance_take_batch on the main thread (next frame).
  void (*batch_ready)(void *context, NSHInstance instance);
  // Sync host-module call. Write the JSON result with nsh_string_new and return 0, or an error message and return 1.
  int32_t (*call_sync)(void *context, NSHInstance instance, const uint8_t *module, size_t module_len,
                       const uint8_t *method, size_t method_len, const uint8_t *args_json, size_t args_len,
                       NSHBuffer *out);
  // Async host-module call. Answer later with nsh_instance_host_respond(request_id, …).
  void (*call_async)(void *context, NSHInstance instance, uint64_t request_id, const uint8_t *module,
                     size_t module_len, const uint8_t *method, size_t method_len, const uint8_t *args_json,
                     size_t args_len);
  void (*log)(void *context, NSHInstance instance, int32_t level, const uint8_t *message, size_t len);
  // Unrecoverable instance error (also logged). The instance stays alive showing its last good frame.
  void (*error)(void *context, NSHInstance instance, const uint8_t *message, size_t len);
} NSHHostCallbacks;

#define NSH_THREAD_DEDICATED 0
#define NSH_THREAD_CALLER 1

// config_json:
// {
//   "app_id": "com.example.todo",          // optional when loading a bundle (taken from its manifest)
//   "data_dir": "/…/native-shell/com.example.todo",
//   "threading": 0 | 1,
//   "dev": false,                           // relaxes eval/freeze for debugging only
//   "limits": {"memory_mb": 64, "stack_kb": 1024, "render_ms": 100, "event_ms": 500, "module_ms": 5000},
//   "capabilities": {…},                    // overrides the manifest (host grants); default: manifest
//   "registry": {"modules": [HostModuleDescriptor…], "components": [HostComponentDescriptor…]},
//   "environment": Environment,
//   "trusted_keys": [{"id": "release-2026", "public_key": "<base64 ed25519>"}],
//   "revoked": ["<bundle sha256 hex>"…],
//   "wall_clock": 1791971460000            // optional, tests: Date.now()/new Date() start here (Unix ms)
// }
// Returns 0 on failure; nsh_last_error() tells why.
NSHInstance nsh_instance_create(const NSHHostCallbacks *callbacks, const uint8_t *config_json, size_t len);

// Load plain sources: {"manifest": {…}, "files": {"main.js": "…", "lib/a.js": "…"}}.
// Asynchronous in dedicated mode: errors arrive through `error`. Returns 0 if queued.
int32_t nsh_instance_load_sources(NSHInstance instance, const uint8_t *json, size_t len);
// Load a signed .nspkg bundle. The signature is verified before any byte reaches QuickJS.
int32_t nsh_instance_load_bundle(NSHInstance instance, const uint8_t *bytes, size_t len);

void nsh_instance_set_environment(NSHInstance instance, const uint8_t *environment_json, size_t len);
// `seq` is the BeginBatch seq the renderer had applied when the event happened.
void nsh_instance_dispatch_event(NSHInstance instance, uint32_t node, uint32_t event_type, const uint8_t *payload_json,
                                size_t len, uint32_t seq);
// ok: 1 resolve with result_json, 0 reject with an error message.
void nsh_instance_host_respond(NSHInstance instance, uint64_t request_id, int32_t ok, const uint8_t *json, size_t len);
// Push a host-module event to scripts: module "demo/quote", event "tick", payload JSON.
void nsh_instance_host_emit(NSHInstance instance, const uint8_t *module, size_t module_len, const uint8_t *event,
                           size_t event_len, const uint8_t *payload_json, size_t payload_len);
// Caller-thread mode only: run due timers, jobs and a frame. Returns ms until the next timer (-1: none).
int64_t nsh_instance_tick(NSHInstance instance, double now_ms);
// Next pending batch, or an empty buffer. Call on the main thread; apply the whole batch, then free it.
NSHBuffer nsh_instance_take_batch(NSHInstance instance);
// Pause timers (app in background) / resume.
void nsh_instance_set_active(NSHInstance instance, int32_t active);
// Memory warning: drop caches.
void nsh_instance_trim_memory(NSHInstance instance);
// Runtime counters as JSON (renders, phase timings in ns, measure calls/time, native nodes…).
// Free with nsh_buffer_free. Callable from any thread.
NSHBuffer nsh_instance_metrics(NSHInstance instance);
// After it returns no callback for this instance runs again (see the top of this file). Called
// from inside one of the instance's own callbacks, it does not wait for that callback.
void nsh_instance_destroy(NSHInstance instance);

void nsh_buffer_free(NSHBuffer buffer);
// For call_sync answers.
NSHBuffer nsh_string_new(const uint8_t *bytes, size_t len);

// Last error on this thread, valid until the next call. NUL-terminated.
const char *nsh_last_error(void);
// Protocol/engine ABI hash (hex), for diagnostics and bundle compatibility checks. NUL-terminated, static.
const char *nsh_abi_hash(void);
uint32_t nsh_protocol_version(void);

#ifdef __cplusplus
}
#endif

#endif  // NATIVE_SHELL_H
