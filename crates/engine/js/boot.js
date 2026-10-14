// Runs once per instance, before the program: loads the prelude, installs the
// globals every program gets (console, timers, queueMicrotask), and hands the
// engine object to Rust. The sandbox freezes the builtins right after.

import { N, engine, scope, startTimer, cancelTimer, report, describe, windowObject } from "nsh:core";
import "nsh:elements";
import "nsh:components";
import "nsh:host";
import "nsh:base";
import "nsh:index";

function format(args) {
  return args
    .map((a) => {
      if (typeof a === "string") return a;
      if (a instanceof Error) return describe(a);
      try {
        const s = JSON.stringify(a);
        return s === undefined ? String(a) : s;
      } catch {
        return String(a);
      }
    })
    .join(" ");
}

const consoleObject = {};
for (const [method, level] of [
  ["log", "info"],
  ["info", "info"],
  ["debug", "debug"],
  ["warn", "warn"],
  ["error", "error"],
  ["trace", "debug"],
]) {
  consoleObject[method] = (...args) => N.log(level, format(args));
}
consoleObject.assert = (cond, ...args) => {
  if (!cond) N.log("error", "Assertion failed: " + format(args));
};

const g = globalThis;
g.console = Object.freeze(consoleObject);

// A pinned wall clock (`InstanceConfig.wall_clock`, for tests and screenshots):
// `Date.now()` and `new Date()` start at the host's instant and run on with
// the engine's time, so they agree with timers. Everything else about `Date`
// (arguments, `parse`, `UTC`, the prototype, `instanceof`) is the builtin's.
const wallStart = N.wall_clock();
if (wallStart !== null && wallStart !== undefined) {
  const BuiltinDate = Date;
  const wallNow = () => Math.floor(wallStart + N.now());
  function PinnedDate(...args) {
    if (new.target === undefined) return new BuiltinDate(wallNow()).toString();
    return Reflect.construct(BuiltinDate, args.length === 0 ? [wallNow()] : args, new.target);
  }
  Object.defineProperty(PinnedDate, "length", { value: 7 });
  Object.defineProperty(PinnedDate, "name", { value: "Date" });
  PinnedDate.prototype = BuiltinDate.prototype;
  PinnedDate.now = wallNow;
  PinnedDate.parse = BuiltinDate.parse;
  PinnedDate.UTC = BuiltinDate.UTC;
  Object.defineProperty(BuiltinDate.prototype, "constructor", { value: PinnedDate, writable: true, configurable: true });
  g.Date = PinnedDate;
}

function timeoutFn(fn, args) {
  if (typeof fn !== "function") throw new TypeError("setTimeout/setInterval expect a function (strings are not evaluated)");
  return () => fn(...args);
}

g.setTimeout = (fn, ms, ...args) => startTimer(ms, false, scope.owner, timeoutFn(fn, args));
g.setInterval = (fn, ms, ...args) => startTimer(Math.max(1, Number(ms) || 0), true, scope.owner, timeoutFn(fn, args));
g.clearTimeout = (id) => cancelTimer(id);
g.clearInterval = (id) => cancelTimer(id);
// gpui-shell's `window`: overlays, toasts, storage, measurements.
Object.defineProperty(g, "window", { value: Object.freeze(windowObject), enumerable: false, writable: false });

g.queueMicrotask = (fn) => {
  if (typeof fn !== "function") throw new TypeError("queueMicrotask expects a function");
  Promise.resolve().then(fn).then(undefined, report);
};

N.register(engine);
