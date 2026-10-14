// Stand-in for the Rust core while Frameworks/NativeShellFFI.xcframework has
// not been built: same C ABI, every instance operation fails. Package.swift
// selects it automatically; it is never shipped.

#include "native_shell.h"

NSHInstance nsh_instance_create(const NSHHostCallbacks *callbacks, const uint8_t *config_json, size_t len) {
  (void)callbacks; (void)config_json; (void)len;
  return 0;
}
int32_t nsh_instance_load_sources(NSHInstance instance, const uint8_t *json, size_t len) {
  (void)instance; (void)json; (void)len;
  return 1;
}
int32_t nsh_instance_load_bundle(NSHInstance instance, const uint8_t *bytes, size_t len) {
  (void)instance; (void)bytes; (void)len;
  return 1;
}
void nsh_instance_set_environment(NSHInstance instance, const uint8_t *environment_json, size_t len) {
  (void)instance; (void)environment_json; (void)len;
}
void nsh_instance_dispatch_event(NSHInstance instance, uint32_t node, uint32_t event_type, const uint8_t *payload_json,
                                size_t len, uint32_t seq) {
  (void)instance; (void)node; (void)event_type; (void)payload_json; (void)len; (void)seq;
}
void nsh_instance_host_respond(NSHInstance instance, uint64_t request_id, int32_t ok, const uint8_t *json, size_t len) {
  (void)instance; (void)request_id; (void)ok; (void)json; (void)len;
}
void nsh_instance_host_emit(NSHInstance instance, const uint8_t *module, size_t module_len, const uint8_t *event,
                           size_t event_len, const uint8_t *payload_json, size_t payload_len) {
  (void)instance; (void)module; (void)module_len; (void)event; (void)event_len; (void)payload_json; (void)payload_len;
}
int64_t nsh_instance_tick(NSHInstance instance, double now_ms) {
  (void)instance; (void)now_ms;
  return -1;
}
NSHBuffer nsh_instance_take_batch(NSHInstance instance) {
  (void)instance;
  NSHBuffer b = {0, 0, 0};
  return b;
}
void nsh_instance_set_active(NSHInstance instance, int32_t active) { (void)instance; (void)active; }
void nsh_instance_trim_memory(NSHInstance instance) { (void)instance; }
void nsh_instance_destroy(NSHInstance instance) { (void)instance; }
void nsh_buffer_free(NSHBuffer buffer) { (void)buffer; }
NSHBuffer nsh_string_new(const uint8_t *bytes, size_t len) {
  (void)bytes; (void)len;
  NSHBuffer b = {0, 0, 0};
  return b;
}
const char *nsh_last_error(void) { return "the native-shell core is not built (run scripts/build-ios.sh)"; }
const char *nsh_abi_hash(void) { return "stub"; }
uint32_t nsh_protocol_version(void) { return 1; }
NSHBuffer nsh_instance_metrics(NSHInstance instance) {
  (void)instance;
  NSHBuffer b = {0, 0, 0};
  return b;
}
