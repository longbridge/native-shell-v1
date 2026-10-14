// The engine half of the prelude: call scopes, `cx`, views and entities, the
// handler registry, timers, and the surface stack (pages, sheets, dialogs,
// alerts). Rust drives everything through the `engine` object at the bottom;
// scripts never see it.
//
// Every Rust → JS entry point (`render`, `dispatch`, `fire_timer`, …) opens a
// scope with a fresh generation and restores the previous one on the way out.
// A `cx` handed to a callback carries the generation it was made for and
// refuses to work once that call has returned; an async `cx` (init, spawn,
// timers) carries none and works in any call except a render.

export const N = globalThis.__nsh_native;
delete globalThis.__nsh_native;

// ---------------------------------------------------------------------------
// Scopes
// ---------------------------------------------------------------------------

let generation = 0;

/** The call in progress. `sink` collects handler ids registered while rendering. */
export const scope = { phase: null, gen: 0, owner: null, sink: null, cx: null, popovers: null };

export function enter(phase, owner, sink) {
  const prev = { ...scope };
  scope.phase = phase;
  scope.gen = ++generation;
  scope.owner = owner;
  scope.sink = sink;
  scope.cx = new Cx(owner, scope.gen);
  scope.popovers = phase === "render" ? [] : null;
  return prev;
}

export function leave(prev) {
  Object.assign(scope, prev);
}

export function refuseInRender(what, hint) {
  if (scope.phase === "render") {
    throw new Error(`${what} is not allowed during the \`render\` phase; ${hint}`);
  }
}

/** gpui-shell's refusal for retained state created while rendering. */
export function refuseStateInRender(what) {
  if (scope.phase === "render") {
    throw new Error(`${what} cannot run during render; ${STATE_HINT}`);
  }
}

const STATE_HINT = "create state in init() or in an event handler and keep it on the view";
const EFFECT_HINT = "do it from an event handler, a timer or a task";

export function describe(error) {
  if (error instanceof Error) {
    return error.stack ? `${error.name}: ${error.message}\n${error.stack}` : `${error.name}: ${error.message}`;
  }
  try {
    return typeof error === "string" ? error : JSON.stringify(error);
  } catch {
    return String(error);
  }
}

export function report(error) {
  N.log("error", describe(error));
}

/** Runs a callback that may return a promise; failures are logged, never thrown into Rust. */
function settleAsync(result) {
  if (result && typeof result.then === "function") {
    result.then(undefined, report);
  }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

// A render's handlers get consecutive ids: registering one is an array push,
// and an event finds its function by binary search over the live ranges (one
// per entity or overlay), not through a per-handler map entry.

let nextHandler = 1;

/** The handlers of one render: ids `base .. base + fns.length`, notifying `owner`. */
class Range {
  constructor(owner) {
    this.base = nextHandler;
    this.fns = [];
    this.owner = owner;
  }
}

/** Live ranges, sorted by `base` (ids only grow, so adopting one appends). */
const ranges = [];

function findRange(id) {
  let lo = 0;
  let hi = ranges.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const r = ranges[mid];
    if (id < r.base) hi = mid - 1;
    else if (id >= r.base + r.fns.length) lo = mid + 1;
    else return r;
  }
  return null;
}

function dropRange(r) {
  if (r === null) return;
  const i = ranges.indexOf(r);
  if (i >= 0) ranges.splice(i, 1);
}

function adoptRange(r) {
  let i = ranges.length;
  while (i > 0 && ranges[i - 1].base > r.base) i--;
  ranges.splice(i, 0, r);
}

/** Registers a render-bound callback and answers its id. */
export function handler(fn, what) {
  const sink = scope.sink;
  if (sink === null) {
    throw new Error(`${what} can only be attached while rendering (inside render(cx))`);
  }
  if (typeof fn !== "function") {
    throw new TypeError(`${what} expects a function`);
  }
  sink.fns.push(fn);
  return nextHandler++;
}

/** Something that owns handlers: an entity, or an overlay's content. */
class Sink {
  constructor(key) {
    this.key = key;
    this.range = null;
    this.pending = null;
  }
  begin(owner) {
    this.pending = new Range(owner);
    return this.pending;
  }
  settle(ok) {
    if (this.pending === null) return;
    if (ok) {
      dropRange(this.range);
      this.range = this.pending;
      adoptRange(this.range);
    }
    this.pending = null;
  }
  release() {
    dropRange(this.range);
    this.range = null;
    this.pending = null;
  }
}

const sinks = new Map();

const NO_MODIFIERS = Object.freeze({ shift: false, control: false, alt: false, platform: false, function: false });

// ---------------------------------------------------------------------------
// Views and entities
// ---------------------------------------------------------------------------

export class View {
  init(_props, _cx) {}
  render(_cx) {
    throw new Error(`${this.constructor.name} must implement render(cx)`);
  }
}

let nextEntity = 1;
const entities = new Map();
const dirty = new Set();
const released = [];
let root = null;

class EntityRecord extends Sink {
  constructor(id, cls) {
    super("e" + id);
    this.id = id;
    this.cls = cls;
    this.view = null;
    this.released = false;
  }
}

export function markDirty(rec) {
  const target = rec ?? root;
  if (target && !target.released) {
    dirty.add(target.id);
    N.request_frame();
  }
}

function isViewClass(cls) {
  return typeof cls === "function" && (cls === View || cls.prototype instanceof View);
}

function createEntity(cls, props) {
  if (!isViewClass(cls)) {
    throw new TypeError("cx.new(Class, props) expects a class that extends View");
  }
  const rec = new EntityRecord(nextEntity++, cls);
  const view = new cls();
  view.props = props;
  rec.view = view;
  entities.set(rec.id, rec);
  sinks.set(rec.key, rec);
  const prev = enter("event", rec, null);
  try {
    settleAsync(view.init(props, new Cx(rec, null)));
  } catch (e) {
    entities.delete(rec.id);
    sinks.delete(rec.key);
    rec.released = true;
    throw e;
  } finally {
    leave(prev);
  }
  markDirty(rec);
  return rec;
}

function releaseEntity(rec) {
  if (rec.released) return;
  rec.released = true;
  rec.release();
  entities.delete(rec.id);
  sinks.delete(rec.key);
  dirty.delete(rec.id);
  released.push(rec.id);
  if (typeof rec.view.release === "function") {
    try {
      rec.view.release();
    } catch (e) {
      report(e);
    }
  }
}

/** What `cx.new` returns: a handle to a nested view, placed with `.child(entity)`. */
export class Entity {
  #rec;
  constructor(rec) {
    this.__entity = rec.id;
    this.#rec = rec;
  }
  #live(what) {
    if (this.#rec.released) throw new Error(`entity.${what}: this entity has been released`);
    return this.#rec;
  }
  get view() {
    return this.#live("view").view;
  }
  set_props(props) {
    const rec = this.#live("set_props(props)");
    refuseInRender("entity.set_props(props)", EFFECT_HINT);
    rec.view.props = props;
    if (typeof rec.view.update === "function") rec.view.update(props);
    markDirty(rec);
  }
  notify() {
    markDirty(this.#live("notify()"));
  }
  /** `cx.notify(entity)`: like `notify()`, but a released entity is quietly skipped. */
  _notify_if_live() {
    markDirty(this.#rec);
  }
  /** The element this entity was placed in by its parent's latest render. */
  _mount(parent) {
    this.#rec.mount = parent;
  }
  /** Releases the nested view; answers whether it was still live. */
  release() {
    refuseInRender("entity.release()", EFFECT_HINT);
    if (this.#rec.released) return false;
    releaseEntity(this.#rec);
    return true;
  }
}

/** The record of a view instance, for `TaskOptions.owner`. */
function recordOf(view) {
  for (const rec of entities.values()) if (rec.view === view) return rec;
  return null;
}

// ---------------------------------------------------------------------------
// Theme and window snapshots
// ---------------------------------------------------------------------------

function deepFreeze(value) {
  if (value && typeof value === "object") {
    for (const v of Object.values(value)) deepFreeze(v);
    Object.freeze(value);
  }
  return value;
}

/** gpui-base's type scale. Mobile renderers set text in the system face. */
const TYPOGRAPHY = {
  sans: "system-ui",
  xs: { size: 12, line_height: 16, weight: 400 },
  sm: { size: 14, line_height: 20, weight: 400 },
  md: { size: 16, line_height: 24, weight: 400 },
  lg: { size: 18, line_height: 28, weight: 500 },
  xl: { size: 20, line_height: 28, weight: 600 },
  mono: "monospace",
  mono_md: { size: 13, line_height: 20, weight: 400 },
};

let themeCache = null;
let windowCache = null;

/**
 * `cx.theme()`, shaped as gpui-shell's: every color token at the top level and
 * under `colors`, plus `spacing`, `radius`, `typography`, `appearance`, `is_dark`.
 */
export function theme() {
  if (themeCache === null) {
    const t = JSON.parse(N.theme());
    themeCache = deepFreeze({ ...t.colors, ...t, typography: TYPOGRAPHY });
  }
  return themeCache;
}

export function windowInfo() {
  if (windowCache === null) windowCache = deepFreeze(JSON.parse(N.window()));
  return windowCache;
}

/**
 * `set_theme({appearance, tokens})`. `tokens.colors` takes gpui-base's color
 * roles (a flat map of roles is accepted too); spacing, radius and typography
 * have fixed values on mobile and are ignored.
 */
export function setTheme(spec = {}) {
  refuseInRender("set_theme(theme)", "change the theme from an event handler or a task");
  const tokens = spec.tokens ?? {};
  const colors = tokens.colors && typeof tokens.colors === "object" ? tokens.colors : tokens;
  const flat = {};
  for (const [k, v] of Object.entries(colors)) if (typeof v === "string") flat[k] = v;
  const out = { tokens: flat };
  if (spec.appearance !== undefined) out.appearance = spec.appearance;
  N.set_theme(JSON.stringify(out));
}

// ---------------------------------------------------------------------------
// Timers and tasks
// ---------------------------------------------------------------------------

const timers = new Map();

export function startTimer(ms, repeat, owner, run) {
  const delay = Math.max(0, Number(ms) || 0);
  const id = N.timer_start(delay, repeat);
  timers.set(id, { run, owner, repeat, done: false });
  return id;
}

export function cancelTimer(id) {
  const t = timers.get(id);
  if (t) {
    timers.delete(id);
    t.done = true;
    N.timer_cancel(id);
  }
}

/** A running task (`cx.spawn`, `cx.timer`). Cancelling one leaves its promise pending for ever. */
class Task {
  #state;
  constructor(state) {
    this.#state = state;
  }
  cancel() {
    this.#state.cancelled = true;
    if (this.#state.timer !== null) cancelTimer(this.#state.timer);
  }
  is_done() {
    return this.#state.done || this.#state.cancelled || (this.#state.owner?.released ?? false);
  }
}

function taskState(owner) {
  return { owner, cancelled: false, done: false, timer: null };
}

/** Whether work bound to `state` may still run: not cancelled, its view not released. */
function live(state) {
  return state === null || !(state.cancelled || (state.owner && state.owner.released));
}

function taskOwner(opts, fallback) {
  if (!opts || !("owner" in opts)) return fallback;
  if (opts.owner === null) return null;
  const rec = recordOf(opts.owner);
  if (!rec) throw new TypeError("TaskOptions.owner must be a live View, or null");
  return rec;
}

// ---------------------------------------------------------------------------
// Surfaces: pages, sheets, dialogs, alerts, toasts
// ---------------------------------------------------------------------------

let nextSurface = 1;
let nextToast = 1;
const pages = [];
const overlays = [];
const rootSurface = { key: "root", title: "", can_go_back: false, on_back: null };
let surfacesChanged = true;
const renderedSince = new Set();
const OVERLAY_HINT = "overlays may only be opened or closed while handling an event or a task";

function surfacesDirty() {
  surfacesChanged = true;
  N.request_frame();
}

class OverlayRecord extends Sink {
  constructor(kind, opener, fn, opts) {
    super(`${kind}:${nextSurface++}`);
    this.kind = kind;
    this.opener = opener;
    this.fn = fn;
    this.opts = opts;
    this.stale = true;
  }
}

function surfaceOf(owner) {
  for (const p of pages) if (p.rec === owner) return p;
  return rootSurface;
}

/**
 * Closing an overlay redraws the view that opened it, as closing one redraws
 * gpui-shell's window root: what the overlay changed is usually that view's state.
 */
function removeOverlay(o, redraw = true) {
  const i = overlays.indexOf(o);
  if (i >= 0) overlays.splice(i, 1);
  o.release();
  sinks.delete(o.key);
  surfacesDirty();
  if (redraw) markDirty(o.opener);
}

// ---------------------------------------------------------------------------
// Anchored surfaces (Popover, HoverCard, Popup)
// ---------------------------------------------------------------------------
//
// gpui-base anchors these to their trigger. A phone has no room to anchor
// them, so each is presented as a sheet. They are described in their view's
// render (not opened from an event), so after every render the view's sheets
// are reconciled with what it described: new ones open, vanished ones close.

/** Uncontrolled open states (`default_open`, `HoverCard`), by view and id. */
const popoverOpen = new Map();

export function popoverState(id, initial) {
  const key = `${scope.owner?.id ?? 0}:${id}`;
  if (!popoverOpen.has(key)) popoverOpen.set(key, !!initial);
  return {
    get: () => popoverOpen.get(key),
    set: (open) => popoverOpen.set(key, !!open),
  };
}

/** Called while rendering: this view shows `content` in an anchored surface `id`. */
export function describePopover(id, content, onDismiss) {
  if (scope.popovers === null) throw new Error("an anchored surface can only be described while rendering");
  scope.popovers.push({ id: String(id), content, onDismiss });
}

/**
 * Called while rendering by a surface whose chain is still being built: `thunk`
 * runs when the render returns and answers its description, or null if closed.
 */
export function deferPopover(thunk) {
  if (scope.popovers === null) throw new Error("an anchored surface can only be described while rendering");
  scope.popovers.push(thunk);
}

function reconcilePopovers(rec, entries) {
  const described = entries.map((e) => (typeof e === "function" ? e() : e)).filter((d) => d);
  const mine = overlays.filter((o) => o.popover !== undefined && o.opener === rec);
  for (const d of described) {
    const o = mine.find((o) => o.popover === d.id);
    if (o) {
      o.content = d.content;
      o.opts.on_dismiss = d.onDismiss;
      continue;
    }
    const created = new OverlayRecord("sheet", rec, () => created.content, { on_dismiss: d.onDismiss, detents: ["medium"] });
    created.popover = d.id;
    created.content = d.content;
    overlays.push(created);
    sinks.set(created.key, created);
    surfacesDirty();
  }
  for (const o of mine) if (!described.some((d) => d.id === o.popover)) removeOverlay(o, false);
}

// ---------------------------------------------------------------------------
// Key bindings (`cx.bind_keys`)
// ---------------------------------------------------------------------------

const keymap = [];

/** gpui's keystroke spelling of a renderer Key event: `cmd-shift-s`, `escape`. */
export function keystrokeOf(e) {
  const m = e?.modifiers ?? {};
  const parts = [];
  if (m.ctrl) parts.push("ctrl");
  if (m.alt) parts.push("alt");
  if (m.shift) parts.push("shift");
  if (m.cmd) parts.push("cmd");
  parts.push(String(e?.key ?? "").toLowerCase());
  return parts.join("-");
}

/** gpui's `KeyEvent` from a renderer Key payload. */
export function keyEvent(e) {
  const m = e?.modifiers ?? {};
  return {
    key: String(e?.key ?? "").toLowerCase(),
    keystroke: keystrokeOf(e),
    key_char: e?.key_char ?? undefined,
    modifiers: { shift: !!m.shift, control: !!m.ctrl, alt: !!m.alt, platform: !!m.cmd, function: false },
  };
}

/**
 * Delivers a hardware key event along the focus path. A keystroke bound to an
 * action (contextual bindings whose `key_context` is on the path first, then
 * global ones) goes to the nearest `on_action` handler for it on the path —
 * the target first, then its ancestors, across nested views — and is consumed
 * there. Otherwise it reaches the target's `on_key_down`; key-ups go to
 * `on_key_up`.
 */
export function routeKey(target, e, cx) {
  const path = [];
  for (let el = target; el; el = el._parent ?? el._owner?.mount ?? null) path.push(el);
  const own = target?._k_routes;
  if (e.phase === "up") {
    own?.up?.(e, cx);
    return;
  }
  const contexts = path.map((el) => el._k_routes?.context).filter((c) => c);
  const action = actionFor(e.keystroke, contexts);
  if (action !== null) {
    for (const el of path) {
      const fn = el._k_routes?.actions.get(action);
      if (fn) {
        fn({ action }, cx);
        return;
      }
    }
  }
  own?.down?.(e, cx);
}

/**
 * The action bound to a keystroke: bindings for the innermost context on the
 * path win, then outer contexts, then global bindings; within one context the
 * later binding wins.
 */
export function actionFor(keystroke, contexts) {
  for (const context of [...contexts, undefined]) {
    for (let i = keymap.length - 1; i >= 0; i--) {
      const b = keymap[i];
      if (b.keystroke === keystroke && b.context === context) return b.action;
    }
  }
  return null;
}

function openOverlay(kind, fn, opts, what) {
  refuseInRender(what, OVERLAY_HINT);
  if (typeof fn !== "function") throw new TypeError(`${what} expects a function returning an element`);
  // At most one sheet is ever open: a new one replaces it, as in gpui-shell.
  if (kind === "sheet") for (const o of overlays.filter((o) => o.kind === "sheet")) removeOverlay(o);
  const o = new OverlayRecord(kind, scope.owner ?? root, fn, opts ?? {});
  overlays.push(o);
  sinks.set(o.key, o);
  surfacesDirty();
  return overlays.filter((x) => x.kind === kind).length;
}

function closeTop(kind, what) {
  refuseInRender(what, OVERLAY_HINT);
  for (let i = overlays.length - 1; i >= 0; i--) {
    if (overlays[i].kind === kind) {
      removeOverlay(overlays[i]);
      return true;
    }
  }
  return false;
}

function inEvent(owner, fn) {
  const prev = enter("event", owner, null);
  try {
    settleAsync(fn(scope.cx));
  } finally {
    leave(prev);
  }
}

const navigator = Object.freeze({
  push(cls, props, opts = {}) {
    refuseInRender("navigator.push(Class, props)", EFFECT_HINT);
    const rec = createEntity(cls, props);
    pages.push({
      key: `page:${nextSurface++}`,
      rec,
      title: String(opts.title ?? ""),
      nav_bar_hidden: !!opts.nav_bar_hidden,
      can_go_back: true,
      on_back: null,
    });
    surfacesDirty();
    return (rec.handle = new Entity(rec));
  },
  pop() {
    refuseInRender("navigator.pop()", EFFECT_HINT);
    if (pages.length === 0) return false;
    removePage(pages[pages.length - 1]);
    return true;
  },
  get can_go_back() {
    return pages.length > 0;
  },
});

function removePage(page) {
  const i = pages.indexOf(page);
  const wasTop = i >= 0 && i === pages.length - 1;
  if (i >= 0) pages.splice(i, 1);
  releaseEntity(page.rec);
  surfacesDirty();
  // As in GPUI, the surface a pop reveals draws again (it may show what the page changed).
  if (wasTop) markDirty(pages.length > 0 ? pages[pages.length - 1].rec : root);
}

const ALERT_STYLES = new Set(["default", "cancel", "destructive"]);
const TOAST_LEVELS = new Set(["info", "success", "warning", "error"]);
const HAPTICS = new Set(["light", "medium", "heavy", "success", "warning", "error", "selection"]);

/**
 * Toasts still on screen: id → `{until, action}`. `until` is when it goes (ms
 * on the engine clock; Infinity until dismissed); `action` is `{owner, on_click,
 * timer}` for a toast with a button, released when the toast goes (expiry
 * timer, `remove_toast`, `clear_toasts`, a replacing push, or the tap itself).
 */
const toasts = new Map();

function dropToast(id) {
  const t = toasts.get(id);
  if (!t) return;
  toasts.delete(id);
  if (t.action?.timer != null) cancelTimer(t.action.timer);
}

function toastShowing(id) {
  const t = toasts.get(id);
  if (t === undefined) return false;
  if (t.until > N.now()) return true;
  dropToast(id);
  return false;
}

/** `action: {label, on_click(cx)}` as given to `push_toast`, or null. */
function toastAction(action, what) {
  if (action == null) return null;
  if (typeof action !== "object") throw new TypeError(`${what}: \`action\` must be {label, on_click}`);
  if (typeof action.on_click !== "function") throw new TypeError(`${what}: \`action.on_click\` must be a function`);
  return { label: String(action.label ?? ""), on_click: action.on_click };
}

/** gpui-shell's `ToastOptions` (`timeout`), with mobile's `duration` accepted too. Answers the id. */
function pushToast(options = {}, what = "window.push_toast(options)") {
  refuseInRender(what, OVERLAY_HINT);
  const level = options.level ?? "info";
  if (!TOAST_LEVELS.has(level)) throw new TypeError(`push_toast: unknown level ${JSON.stringify(level)}`);
  const action = toastAction(options.action, what);
  const id = options.id === undefined ? `toast-${nextToast++}` : String(options.id);
  const timeout = "timeout" in options ? options.timeout : options.duration;
  const duration = timeout === null ? 0 : (timeout ?? 3000);
  const payload = { id, title: String(options.title ?? ""), level, duration };
  if (options.description != null) payload.description = String(options.description);
  if (action) payload.action = { label: action.label };
  N.command("toast", null, JSON.stringify(payload));
  dropToast(id);
  const entry = { until: duration === 0 ? Infinity : N.now() + duration, action: null };
  if (action) {
    entry.action = { owner: scope.owner ?? root, on_click: action.on_click, timer: null };
    if (duration !== 0) {
      entry.action.timer = startTimer(duration, false, null, () => {
        if (toasts.get(id) === entry) toasts.delete(id);
      });
    }
  }
  toasts.set(id, entry);
  return id;
}

/** Retracts one toast; answers whether it was still showing. */
function removeToast(id) {
  refuseInRender("window.remove_toast(id)", OVERLAY_HINT);
  id = String(id);
  const showing = toastShowing(id);
  dropToast(id);
  if (showing) N.command("toast_dismiss", null, JSON.stringify({ id }));
  return showing;
}

/** Retracts every toast; answers how many were showing. */
function clearToasts() {
  refuseInRender("window.clear_toasts()", OVERLAY_HINT);
  const showing = [...toasts.keys()].filter(toastShowing).length;
  for (const id of [...toasts.keys()]) dropToast(id);
  if (showing > 0) N.command("toast_dismiss", null, "{}");
  return showing;
}

/** gpui-shell's `DialogOptions`; `dismissable` and `on_dismiss` are mobile additions. */
function dialogOptions(options = {}) {
  return { ...options, dismissable: options.dismissable ?? options.backdrop_dismissable ?? true };
}

/**
 * The `window` global, as gpui-shell's `Window`: overlays, toasts, storage and
 * the measurements that exist on a phone. Desktop window management
 * (minimize, move, fullscreen, rem size) does not exist here.
 */
export const windowObject = {
  open_dialog: (content, options) => openOverlay("dialog", content, dialogOptions(options), "window.open_dialog(content, options)"),
  close_dialog: () => closeTop("dialog", "window.close_dialog()"),
  close_all_dialogs() {
    let n = 0;
    while (closeTop("dialog", "window.close_all_dialogs()")) n++;
    return n;
  },
  has_active_dialog: () => overlays.some((o) => o.kind === "dialog"),
  open_sheet(content) {
    openOverlay("sheet", content, {}, "window.open_sheet(content)");
  },
  /** Mobile sheets rise from the bottom; the placement is accepted and ignored. */
  open_sheet_at(_placement, content) {
    openOverlay("sheet", content, {}, "window.open_sheet_at(placement, content)");
  },
  close_sheet: () => closeTop("sheet", "window.close_sheet()"),
  has_active_sheet: () => overlays.some((o) => o.kind === "sheet"),
  push_toast: (options) => pushToast(options),
  remove_toast: (id) => removeToast(id),
  clear_toasts: () => clearToasts(),
  get localStorage() {
    return globalThis.localStorage;
  },
  get sessionStorage() {
    return globalThis.sessionStorage;
  },
  // rem is what layout scales rem lengths by (font scale, clamped); a line of
  // default text grows with the full font scale, as renderers draw it.
  rem_size: () => N.rem_size(),
  line_height: () => 16 * 1.618 * windowInfo().font_scale,
  viewport_size: () => ({ width: windowInfo().width, height: windowInfo().height }),
  bounds: () => ({ x: 0, y: 0, width: windowInfo().width, height: windowInfo().height }),
  mouse_position: () => ({ x: 0, y: 0 }),
  appearance: () => windowInfo().appearance,
  is_window_active: () => true,
  is_fullscreen: () => true,
  is_maximized: () => true,
  refresh() {
    refuseInRender("window.refresh()", EFFECT_HINT);
    engine.invalidate_all();
    N.request_frame();
  },
};

// ---------------------------------------------------------------------------
// cx
// ---------------------------------------------------------------------------

let nextFocus = 1;

/** `cx.focus_handle()`: a focus target the script keeps and hands to `.track_focus(handle)`. */
export class FocusHandle {
  #released = false;
  constructor() {
    this._ref = "__focus:" + nextFocus++;
    this._focused = false;
  }
  focus() {
    if (!this.#released) N.command("focus", this._ref, "{}");
  }
  is_focused() {
    return this._focused;
  }
  release() {
    const was = !this.#released;
    this.#released = true;
    return was;
  }
}

/**
 * The script-side context. A `Context` (render, handlers) names one call by
 * generation; an `AsyncContext` (init, spawn, timers) names none and may be
 * kept across `await`. Members and messages follow gpui-shell's.
 */
export class Cx {
  #owner;
  #gen;
  #task;
  constructor(owner, gen, task = null) {
    this.#owner = owner;
    this.#gen = gen;
    this.#task = task;
  }

  #check(what, renderOk = false) {
    if (this.#gen !== null && this.#gen !== scope.gen) {
      throw new Error(
        `cx is no longer valid: it was captured during an earlier call and used later (${what}). ` +
          "Use cx.spawn or take cx from the callback arguments instead.",
      );
    }
    if (!renderOk && scope.phase === "render") {
      throw new Error(`${what} is not allowed during the \`render\` phase; request a re-render from an event handler instead`);
    }
  }

  /** Re-renders the current view, or the given entity. */
  notify(target) {
    this.#check("cx.notify()");
    if (!live(this.#task)) return;
    if (target instanceof Entity) target._notify_if_live();
    else markDirty(this.#owner);
  }

  /**
   * The current view as an Entity, as GPUI's `cx.entity()`: hand it to a page
   * or keep it for a later callback, which re-renders this view with
   * `cx.notify(entity)` (a no-op once the view is released).
   */
  entity() {
    this.#check("cx.entity()", true);
    const rec = this.#owner ?? root;
    rec.handle ??= new Entity(rec);
    return rec.handle;
  }

  phase() {
    return scope.phase ?? "task";
  }

  /** Touch events reach one view natively; there is nothing above to stop. */
  stop_propagation() {}

  propagate() {}

  /**
   * Installs key bindings for hardware keyboards and answers how many. Each is
   * `{keystroke, action, context?}`; a keystroke reaching an element with
   * `on_action(action, …)` (and a matching `key_context`) runs it.
   */
  bind_keys(bindings) {
    this.#check("cx.bind_keys(bindings)");
    if (!Array.isArray(bindings)) throw new TypeError("cx.bind_keys expects an array of {keystroke, action, context}");
    const parsed = bindings.map((b) => {
      if (typeof b?.keystroke !== "string" || typeof b?.action !== "string") {
        throw new TypeError("cx.bind_keys: every binding needs a `keystroke` and an `action` string");
      }
      if (/\s/.test(b.keystroke.trim())) throw new TypeError(`cx.bind_keys: chord sequences are not supported on mobile (\`${b.keystroke}\`)`);
      return { keystroke: b.keystroke.trim().toLowerCase(), action: b.action, context: b.context };
    });
    keymap.push(...parsed);
    return parsed.length;
  }

  new(cls, props) {
    refuseStateInRender("cx.new(Class, props)");
    this.#check("cx.new(Class, props)");
    const rec = createEntity(cls, props);
    return (rec.handle = new Entity(rec));
  }

  spawn(body, opts) {
    this.#check("cx.spawn(body)");
    if (typeof body !== "function") throw new TypeError("cx.spawn expects a function");
    const state = taskState(taskOwner(opts, this.#owner));
    const promise = Promise.resolve().then(() => (live(state) ? body(new Cx(state.owner, null, state)) : undefined));
    promise.then(
      () => (state.done = true),
      (e) => {
        state.done = true;
        if (live(state)) report(e);
      },
    );
    return new Task(state);
  }

  sleep(ms = 0) {
    this.#check("cx.sleep(ms)");
    const state = this.#task;
    return new Promise((resolve) =>
      startTimer(ms, false, null, () => {
        if (live(state)) resolve();
      }),
    );
  }

  get timer() {
    this.#check("cx.timer");
    const owner = this.#owner;
    const schedule = (what, ms, repeat, handler, opts) => {
      if (typeof handler !== "function") throw new TypeError(`cx.timer.${what}(ms, handler) expects a function`);
      const state = taskState(taskOwner(opts, owner));
      state.timer = startTimer(repeat ? Math.max(1, Number(ms) || 0) : ms, repeat, state.owner, () => {
        if (!repeat) state.done = true;
        return handler(new Cx(state.owner, null, state));
      });
      return new Task(state);
    };
    return Object.freeze({
      after: (ms, handler, opts) => schedule("after", ms, false, handler, opts),
      every: (ms, handler, opts) => schedule("every", ms, true, handler, opts),
    });
  }

  /** A read of shared state: works from any cx, a stale one included (gpui-shell's dialogs rely on it). */
  theme() {
    return theme();
  }

  set_theme(spec) {
    this.#check("cx.set_theme(theme)");
    setTheme(spec);
  }

  focus_handle() {
    refuseStateInRender("cx.focus_handle()");
    this.#check("cx.focus_handle()");
    return new FocusHandle();
  }

  /** Mobile: the screen, safe area and keyboard (gpui-shell has `window.viewport_size()`). */
  window() {
    return windowInfo();
  }

  focus(id) {
    this.#check("cx.focus(id)");
    N.command("focus", String(id), "{}");
  }

  blur() {
    this.#check("cx.blur()");
    N.command("blur", null, "{}");
  }

  scroll_to(id, opts = {}) {
    this.#check("cx.scroll_to(id, options)");
    N.command("scroll_to", String(id), JSON.stringify({ x: opts.x ?? 0, y: opts.y ?? 0, animated: opts.animated ?? true }));
  }

  scroll_to_item(list, index, position = "top") {
    this.#check("cx.scroll_to_item(list, index, position)");
    if (!["top", "center", "bottom"].includes(position)) {
      throw new TypeError(`cx.scroll_to_item: position must be "top", "center" or "bottom", got ${JSON.stringify(position)}`);
    }
    N.command("scroll_to_index", "__list:" + list, JSON.stringify({ index: Number(index) | 0, position, animated: true }));
  }

  open_url(url) {
    this.#check("cx.open_url(url)");
    N.command("open_url", null, JSON.stringify({ url: String(url) }));
  }

  write_to_clipboard(text) {
    this.#check("cx.write_to_clipboard(text)");
    N.command("clipboard", null, JSON.stringify({ text: String(text) }));
  }

  /** The clipboard's text, or `undefined` when it holds none. */
  read_from_clipboard() {
    this.#check("cx.read_from_clipboard()");
    const text = JSON.parse(N.clipboard_read());
    return typeof text === "string" ? text : undefined;
  }

  haptic(kind = "light") {
    this.#check("cx.haptic(kind)");
    if (!HAPTICS.has(kind)) throw new TypeError(`cx.haptic: unknown kind ${JSON.stringify(kind)}; use one of ${[...HAPTICS].join(", ")}`);
    N.command("haptic", null, JSON.stringify({ kind }));
  }

  // The overlays live on `window` in gpui-shell; these are the same calls.

  push_toast(options) {
    this.#check("cx.push_toast(options)", true);
    return pushToast(options, "cx.push_toast(options)");
  }

  open_sheet(content, opts = {}) {
    this.#check("cx.open_sheet(content, options)", true);
    return openOverlay("sheet", content, opts, "cx.open_sheet(content, options)");
  }

  close_sheet() {
    this.#check("cx.close_sheet()", true);
    return closeTop("sheet", "cx.close_sheet()");
  }

  open_dialog(content, opts) {
    this.#check("cx.open_dialog(content, options)", true);
    return openOverlay("dialog", content, dialogOptions(opts), "cx.open_dialog(content, options)");
  }

  close_dialog() {
    this.#check("cx.close_dialog()", true);
    return closeTop("dialog", "cx.close_dialog()");
  }

  alert(spec = {}) {
    this.#check("cx.alert(spec)", true);
    refuseInRender("cx.alert(spec)", OVERLAY_HINT);
    const actions = (spec.actions ?? [{ label: "OK" }]).map((a) => {
      const style = a.style ?? "default";
      if (!ALERT_STYLES.has(style)) throw new TypeError(`cx.alert: unknown action style ${JSON.stringify(style)}`);
      if (a.on_press !== undefined && typeof a.on_press !== "function") throw new TypeError("cx.alert: on_press must be a function");
      return { label: String(a.label ?? ""), style, on_press: a.on_press ?? null };
    });
    const o = new OverlayRecord("alert", this.#owner ?? root, null, {});
    o.actions = actions;
    const shown = { title: String(spec.title ?? ""), actions: actions.map(({ label, style }) => ({ label, style })) };
    if (spec.message != null) shown.message = String(spec.message);
    o.spec = JSON.stringify(shown);
    overlays.push(o);
    surfacesDirty();
    return o.key;
  }

  get navigator() {
    this.#check("cx.navigator");
    return navigator;
  }

  set_title(title) {
    this.#check("cx.set_title(title)");
    const s = surfaceOf(this.#owner);
    s.title = String(title);
    surfacesDirty();
  }

  set_can_go_back(value) {
    this.#check("cx.set_can_go_back(value)");
    const s = surfaceOf(this.#owner);
    if (s === rootSurface) return;
    s.can_go_back = !!value;
    surfacesDirty();
  }

  on_back(fn) {
    this.#check("cx.on_back(fn)");
    if (fn !== null && typeof fn !== "function") throw new TypeError("cx.on_back expects a function or null");
    surfaceOf(this.#owner).on_back = fn;
  }
}

// ---------------------------------------------------------------------------
// Host requests (async host-module methods and clipboard reads)
// ---------------------------------------------------------------------------

const requests = new Map();

export function hostRequest(id) {
  return new Promise((resolve, reject) => requests.set(id, { resolve, reject }));
}

function parseHostJson(json) {
  if (json === "") return undefined;
  try {
    return JSON.parse(json);
  } catch {
    return json;
  }
}

/** Listeners of host-module events: `module → event → [{fn, owner}]`. */
export const hostListeners = new Map();

// ---------------------------------------------------------------------------
// The engine API (Rust only)
// ---------------------------------------------------------------------------

export const engine = {
  mount(cls) {
    if (!isViewClass(cls)) {
      throw new TypeError("the entry module must `export default` a class that extends View");
    }
    root = createEntity(cls, undefined);
    return root.id;
  },

  /** Entities to render and entities released since the last frame. */
  take_work() {
    const work = { dirty: [...dirty].sort((a, b) => a - b), released: released.splice(0) };
    dirty.clear();
    return work;
  },

  render(id) {
    const rec = entities.get(id);
    if (!rec) return null;
    renderedSince.add(id);
    const prev = enter("render", rec, rec.begin(rec));
    try {
      const el = rec.view.render(scope.cx);
      reconcilePopovers(rec, scope.popovers);
      rec.root = el;
      if (el && typeof el === "object") el._owner = rec;
      return el;
    } finally {
      leave(prev);
    }
  },

  /** Adopts (or discards) the handlers a render registered, once Rust has converted it. */
  settle(key, ok) {
    sinks.get(key)?.settle(ok);
  },

  /** The surface list, with overlay contents re-evaluated where their opener re-rendered; null if unchanged. */
  surfaces() {
    const stale = overlays.some((o) => o.kind !== "alert" && (o.stale || renderedSince.has(o.opener?.id)));
    if (!surfacesChanged && !stale) {
      renderedSince.clear();
      return null;
    }
    const list = [];
    for (const p of pages) {
      list.push({
        key: p.key,
        kind: "page",
        entity: p.rec.id,
        title: p.title,
        can_go_back: p.can_go_back,
        nav_bar_hidden: p.nav_bar_hidden,
      });
    }
    for (const o of overlays) {
      if (o.kind === "alert") {
        list.push({ key: o.key, kind: "alert", spec: o.spec });
        continue;
      }
      const item = {
        key: o.key,
        kind: o.kind,
        opener: o.opener?.id ?? 0,
        detents: o.opts.detents ?? ["medium", "large"],
        dismissable: o.opts.dismissable ?? true,
        el: null,
        error: null,
      };
      if (o.stale || renderedSince.has(o.opener?.id)) {
        const prev = enter("render", o.opener, o.begin(o.opener));
        try {
          item.el = o.fn(scope.cx);
          o.stale = false;
        } catch (e) {
          item.error = describe(e);
          o.settle(false);
        } finally {
          leave(prev);
        }
      }
      list.push(item);
    }
    surfacesChanged = false;
    renderedSince.clear();
    return list;
  },

  dispatch(hid, payload, event) {
    const r = findRange(hid);
    if (r === null) return false;
    const fn = r.fns[hid - r.base];
    // gpui-shell's ClickEvent, plus the touch point.
    const arg =
      event === "click"
        ? { click_count: payload?.count ?? 1, modifiers: NO_MODIFIERS, x: payload?.x ?? 0, y: payload?.y ?? 0 }
        : event === "key"
          ? { ...keyEvent(payload), phase: payload?.phase ?? "down" }
          : payload;
    const prev = enter("event", r.owner, null);
    try {
      settleAsync(fn(arg, scope.cx));
    } finally {
      leave(prev);
    }
    return true;
  },

  fire_timer(id) {
    const t = timers.get(id);
    if (!t) return;
    if (t.owner && t.owner.released) {
      cancelTimer(id);
      return;
    }
    if (!t.repeat) {
      timers.delete(id);
      t.done = true;
    }
    const prev = enter("task", t.owner, null);
    try {
      settleAsync(t.run());
    } finally {
      leave(prev);
    }
  },

  /**
   * A hardware key on the root node (nothing focused): routed from the root
   * element of the top surface (the last pushed page, else the root view).
   */
  key_root(payload) {
    const top = pages.length > 0 ? pages[pages.length - 1].rec : root;
    const target = top?.root;
    if (!target || typeof target !== "object") return false;
    const prev = enter("event", top, null);
    try {
      routeKey(target, { ...keyEvent(payload), phase: payload?.phase ?? "down" }, scope.cx);
    } finally {
      leave(prev);
    }
    return true;
  },

  respond(id, ok, json) {
    const r = requests.get(id);
    if (!r) return;
    requests.delete(id);
    const value = parseHostJson(json);
    if (ok) r.resolve(value);
    else r.reject(new Error(typeof value === "string" ? value : JSON.stringify(value)));
  },

  emit(module, event, payload) {
    const list = hostListeners.get(module)?.get(event);
    if (!list) return 0;
    for (const l of [...list]) {
      if (l.owner && l.owner.released) continue;
      const prev = enter("task", l.owner, null);
      try {
        settleAsync(l.fn(payload, scope.cx));
      } catch (e) {
        report(e);
      } finally {
        leave(prev);
      }
    }
    return list.length;
  },

  /** Theme, environment or font scale changed: every view and overlay renders again. */
  invalidate_all() {
    themeCache = null;
    windowCache = null;
    for (const id of entities.keys()) dirty.add(id);
    for (const o of overlays) o.stale = true;
    surfacesChanged = true;
  },

  invalidate(id) {
    const rec = entities.get(id);
    if (rec) markDirty(rec);
  },

  /** System back on a page (`key`) or the root. */
  back(key) {
    const page = pages.find((p) => p.key === key);
    const s = page ?? rootSurface;
    if (s.on_back) {
      inEvent(page ? page.rec : root, s.on_back);
    } else if (page) {
      removePage(page);
    }
  },

  /** The user dismissed a surface natively (swipe, backdrop, back gesture). */
  dismissed(key) {
    const page = pages.find((p) => p.key === key);
    if (page) {
      removePage(page);
      return;
    }
    const o = overlays.find((o) => o.key === key);
    if (!o) return;
    removeOverlay(o);
    const cancel = o.kind === "alert" ? o.actions.find((a) => a.style === "cancel") : null;
    const callback = o.kind === "alert" ? cancel?.on_press : o.opts.on_dismiss;
    if (typeof callback === "function") inEvent(o.opener, callback);
  },

  /** A toast's action button was tapped; false when that toast is gone (expired, retracted, replaced). */
  toast_action(id) {
    const t = toastShowing(id) ? toasts.get(id) : undefined;
    if (!t || !t.action) return false;
    // The renderer dismissed the toast itself.
    dropToast(id);
    if (t.action.owner.released) return false;
    inEvent(t.action.owner, t.action.on_click);
    return true;
  },

  alert_action(key, index) {
    const o = overlays.find((o) => o.key === key && o.kind === "alert");
    if (!o) return;
    removeOverlay(o);
    const action = o.actions[index];
    if (action && action.on_press) inEvent(o.opener, action.on_press);
  },
};
