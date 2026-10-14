// Host modules on the script side. Each module the host registered becomes an
// importable module (`import { quote, on } from "demo/quote"`) whose methods
// forward JSON to the host: sync methods return the answer, async ones a
// promise settled by `Host::respond`. `on(event, fn)` listens to `emit`.

import { N, scope, hostRequest, hostListeners } from "nsh:core";

const modules = new Map();

function listen(name, events, event, fn) {
  event = String(event);
  if (events.length > 0 && !events.includes(event)) {
    throw new TypeError(`host module \`${name}\` emits no ${JSON.stringify(event)}; it emits ${events.join(", ")}`);
  }
  if (typeof fn !== "function") throw new TypeError(`${name}: on(event, fn) expects a function`);
  let byEvent = hostListeners.get(name);
  if (!byEvent) hostListeners.set(name, (byEvent = new Map()));
  let list = byEvent.get(event);
  if (!list) byEvent.set(event, (list = []));
  const entry = { fn, owner: scope.owner };
  list.push(entry);
  N.host_listeners(name, event, list.length);
  let live = true;
  return Object.freeze({
    cancel() {
      if (!live) return;
      live = false;
      const i = list.indexOf(entry);
      if (i >= 0) list.splice(i, 1);
      N.host_listeners(name, event, list.length);
    },
  });
}

/** The script-side object of one host module; one per instance, shared by every importer. */
export function host_module(name) {
  let m = modules.get(name);
  if (m) return m;
  const spec = JSON.parse(N.host_module(name));
  const events = spec.events ?? [];
  m = {};
  for (const method of spec.methods ?? []) {
    const id = method.name;
    m[id] =
      method.kind === "sync"
        ? (...args) => {
            const json = N.host_call_sync(name, id, JSON.stringify(args));
            return json === "" ? undefined : JSON.parse(json);
          }
        : (...args) => hostRequest(N.host_call_async(name, id, JSON.stringify(args)));
  }
  m.on = (event, fn) => listen(name, events, event, fn);
  Object.freeze(m);
  modules.set(name, m);
  return m;
}
