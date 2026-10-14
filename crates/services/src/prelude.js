// The script-facing half of native-shell-services. Evaluated once per context
// as a function of the native object `n` and the global object `g`; `n` stays
// private to this closure. Returns the `deliver` function `Services::poll`
// calls with completed background work.
(function (n, g) {
  "use strict";

  const pending = new Map(); // fetch id -> [resolve, reject]
  const sockets = new Map(); // socket id -> WebSocket
  let socketEvent; // (ws, type, data), set by WebSocket's static block
  let paramsReset; // (params, search)
  let paramsOwn; // (params, onchange)

  const define = (name, value) =>
    Object.defineProperty(g, name, { value, writable: true, configurable: true, enumerable: false });

  function domError(name, message) {
    const e = new Error(message);
    e.name = name;
    return e;
  }

  function report(error) {
    try {
      g.console && g.console.error && g.console.error(error);
    } catch (_) {}
  }

  function toBytes(x, what) {
    if (x instanceof Uint8Array) return x;
    if (x instanceof ArrayBuffer) return new Uint8Array(x);
    if (ArrayBuffer.isView(x)) return new Uint8Array(x.buffer, x.byteOffset, x.byteLength);
    throw new TypeError(`${what}: expected a string, ArrayBuffer or typed array`);
  }

  const copyBuffer = (u8) => u8.buffer.slice(u8.byteOffset, u8.byteOffset + u8.byteLength);

  // --- encoding -------------------------------------------------------------

  class TextEncoder {
    get encoding() {
      return "utf-8";
    }
    encode(input = "") {
      return n.utf8_encode(String(input));
    }
    encodeInto(input, dest) {
      const bytes = n.utf8_encode(String(input));
      const written = Math.min(bytes.length, dest.length);
      dest.set(bytes.subarray(0, written));
      return { read: n.utf8_decode(bytes.subarray(0, written), false, true).length, written };
    }
  }

  class TextDecoder {
    #fatal;
    #ignoreBOM;
    constructor(label = "utf-8", options = {}) {
      const name = String(label).trim().toLowerCase();
      if (name !== "utf-8" && name !== "utf8" && name !== "unicode-1-1-utf-8") {
        throw new RangeError(`TextDecoder: unsupported encoding "${label}" (only utf-8)`);
      }
      this.#fatal = !!(options && options.fatal);
      this.#ignoreBOM = !!(options && options.ignoreBOM);
    }
    get encoding() {
      return "utf-8";
    }
    get fatal() {
      return this.#fatal;
    }
    get ignoreBOM() {
      return this.#ignoreBOM;
    }
    decode(input) {
      if (input === undefined) return "";
      const s = n.utf8_decode(toBytes(input, "TextDecoder.decode"), this.#fatal, this.#ignoreBOM);
      if (s === undefined) throw new TypeError("TextDecoder.decode: invalid utf-8");
      return s;
    }
  }

  define("TextEncoder", TextEncoder);
  define("TextDecoder", TextDecoder);
  define("btoa", (s) => {
    const r = n.btoa(String(s));
    if (r === undefined) throw domError("InvalidCharacterError", "btoa: the string has characters outside Latin-1");
    return r;
  });
  define("atob", (s) => {
    const r = n.atob(String(s));
    if (r === undefined) throw domError("InvalidCharacterError", "atob: the string is not valid base64");
    return r;
  });

  // --- URL ------------------------------------------------------------------

  class URLSearchParams {
    #list = [];
    #owner = null;
    constructor(init = "") {
      if (init instanceof URLSearchParams) {
        this.#list = init.#list.map((p) => [p[0], p[1]]);
      } else if (init !== null && typeof init === "object") {
        if (typeof init[Symbol.iterator] === "function") {
          for (const pair of init) {
            const p = [...pair];
            if (p.length !== 2) throw new TypeError("URLSearchParams: each pair needs exactly two items");
            this.#list.push([String(p[0]), String(p[1])]);
          }
        } else {
          for (const k of Object.keys(init)) this.#list.push([k, String(init[k])]);
        }
      } else {
        const s = String(init);
        this.#list = n.form_parse(s.startsWith("?") ? s.slice(1) : s);
      }
    }
    #changed() {
      if (this.#owner) this.#owner(this.toString());
    }
    get size() {
      return this.#list.length;
    }
    append(k, v) {
      this.#list.push([String(k), String(v)]);
      this.#changed();
    }
    delete(k, v) {
      k = String(k);
      this.#list = this.#list.filter((p) => p[0] !== k || (v !== undefined && p[1] !== String(v)));
      this.#changed();
    }
    get(k) {
      const p = this.#list.find((p) => p[0] === String(k));
      return p ? p[1] : null;
    }
    getAll(k) {
      return this.#list.filter((p) => p[0] === String(k)).map((p) => p[1]);
    }
    has(k, v) {
      return this.#list.some((p) => p[0] === String(k) && (v === undefined || p[1] === String(v)));
    }
    set(k, v) {
      k = String(k);
      const i = this.#list.findIndex((p) => p[0] === k);
      if (i < 0) this.#list.push([k, String(v)]);
      else {
        this.#list[i][1] = String(v);
        this.#list = this.#list.filter((p, j) => j <= i || p[0] !== k);
      }
      this.#changed();
    }
    sort() {
      this.#list.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
      this.#changed();
    }
    forEach(fn, thisArg) {
      for (const [k, v] of this.#list) fn.call(thisArg, v, k, this);
    }
    *entries() {
      for (const [k, v] of this.#list) yield [k, v];
    }
    *keys() {
      for (const [k] of this.#list) yield k;
    }
    *values() {
      for (const [, v] of this.#list) yield v;
    }
    [Symbol.iterator]() {
      return this.entries();
    }
    toString() {
      return n.form_serialize(this.#list);
    }
    static {
      paramsReset = (p, search) => {
        p.#list = n.form_parse(search.startsWith("?") ? search.slice(1) : search);
      };
      paramsOwn = (p, fn) => {
        p.#owner = fn;
      };
    }
  }

  const baseHref = (base) => (base === undefined ? undefined : base instanceof URL ? base.href : String(base));

  class URL {
    #c;
    #params = null;
    constructor(url, base) {
      const c = n.url_parse(url instanceof URL ? url.href : String(url), baseHref(base));
      if (c === undefined) throw new TypeError(`Invalid URL: ${url}`);
      this.#c = c;
    }
    static canParse(url, base) {
      return n.url_parse(String(url), baseHref(base)) !== undefined;
    }
    static parse(url, base) {
      try {
        return new URL(url, base);
      } catch (_) {
        return null;
      }
    }
    #set(prop, value) {
      this.#c = n.url_set(this.#c.href, prop, String(value));
      if (this.#params) paramsReset(this.#params, this.#c.search);
    }
    get href() {
      return this.#c.href;
    }
    set href(v) {
      const c = n.url_parse(String(v), undefined);
      if (c === undefined) throw new TypeError(`Invalid URL: ${v}`);
      this.#c = c;
      if (this.#params) paramsReset(this.#params, c.search);
    }
    get origin() {
      return this.#c.origin;
    }
    get protocol() {
      return this.#c.protocol;
    }
    set protocol(v) {
      this.#set("protocol", v);
    }
    get username() {
      return this.#c.username;
    }
    set username(v) {
      this.#set("username", v);
    }
    get password() {
      return this.#c.password;
    }
    set password(v) {
      this.#set("password", v);
    }
    get host() {
      return this.#c.host;
    }
    set host(v) {
      this.#set("host", v);
    }
    get hostname() {
      return this.#c.hostname;
    }
    set hostname(v) {
      this.#set("hostname", v);
    }
    get port() {
      return this.#c.port;
    }
    set port(v) {
      this.#set("port", v);
    }
    get pathname() {
      return this.#c.pathname;
    }
    set pathname(v) {
      this.#set("pathname", v);
    }
    get search() {
      return this.#c.search;
    }
    set search(v) {
      this.#set("search", v);
    }
    get hash() {
      return this.#c.hash;
    }
    set hash(v) {
      this.#set("hash", v);
    }
    get searchParams() {
      if (!this.#params) {
        this.#params = new URLSearchParams(this.#c.search);
        paramsOwn(this.#params, (s) => {
          this.#c = n.url_set(this.#c.href, "search", s);
        });
      }
      return this.#params;
    }
    toString() {
      return this.#c.href;
    }
    toJSON() {
      return this.#c.href;
    }
  }

  define("URL", URL);
  define("URLSearchParams", URLSearchParams);

  // --- fetch ----------------------------------------------------------------

  class Headers {
    #list = [];
    constructor(init) {
      if (init == null) return;
      if (init instanceof Headers) init = init.entries();
      if (typeof init[Symbol.iterator] === "function") {
        for (const pair of init) {
          const p = [...pair];
          if (p.length !== 2) throw new TypeError("Headers: each pair needs exactly two items");
          this.append(p[0], p[1]);
        }
      } else {
        for (const k of Object.keys(init)) this.append(k, init[k]);
      }
    }
    append(k, v) {
      this.#list.push([String(k).toLowerCase(), String(v)]);
    }
    set(k, v) {
      this.delete(k);
      this.append(k, v);
    }
    delete(k) {
      k = String(k).toLowerCase();
      this.#list = this.#list.filter((p) => p[0] !== k);
    }
    get(k) {
      k = String(k).toLowerCase();
      const values = this.#list.filter((p) => p[0] === k).map((p) => p[1]);
      return values.length ? values.join(", ") : null;
    }
    has(k) {
      k = String(k).toLowerCase();
      return this.#list.some((p) => p[0] === k);
    }
    *entries() {
      const names = [...new Set(this.#list.map((p) => p[0]))].sort();
      for (const k of names) yield [k, this.get(k)];
    }
    *keys() {
      for (const [k] of this.entries()) yield k;
    }
    *values() {
      for (const [, v] of this.entries()) yield v;
    }
    forEach(fn, thisArg) {
      for (const [k, v] of this.entries()) fn.call(thisArg, v, k, this);
    }
    [Symbol.iterator]() {
      return this.entries();
    }
  }

  class Response {
    #body;
    constructor(body = null, init = {}) {
      init = init || {};
      this.#body =
        body == null ? new Uint8Array(0) : typeof body === "string" ? n.utf8_encode(body) : toBytes(body, "Response");
      this.status = init.status === undefined ? 200 : init.status | 0;
      this.statusText = init.statusText === undefined ? "" : String(init.statusText);
      this.headers = new Headers(init.headers);
      this.ok = this.status >= 200 && this.status < 300;
      this.url = init.url === undefined ? "" : String(init.url);
      this.redirected = !!init.redirected;
      this.type = "basic";
    }
    async arrayBuffer() {
      return copyBuffer(this.#body);
    }
    async bytes() {
      return new Uint8Array(copyBuffer(this.#body));
    }
    async text() {
      return n.utf8_decode(this.#body, false, false);
    }
    async json() {
      return JSON.parse(await this.text());
    }
    clone() {
      return new Response(this.#body, this);
    }
  }

  async function fetch(input, init) {
    init = init || {};
    const url =
      input instanceof URL ? input.href : input !== null && typeof input === "object" && "url" in input ? input.url : String(input);
    const method = init.method === undefined ? "GET" : String(init.method).toUpperCase();
    const headers = [...new Headers(init.headers)];
    const hasType = headers.some((p) => p[0] === "content-type");
    let body;
    if (init.body != null) {
      if (typeof init.body === "string") {
        body = n.utf8_encode(init.body);
        if (!hasType) headers.push(["content-type", "text/plain;charset=UTF-8"]);
      } else if (init.body instanceof URLSearchParams) {
        body = n.utf8_encode(init.body.toString());
        if (!hasType) headers.push(["content-type", "application/x-www-form-urlencoded;charset=UTF-8"]);
      } else {
        body = toBytes(init.body, "fetch(url, options).body");
      }
    }
    const id = n.fetch(url, method, headers, body);
    const r = await new Promise((resolve, reject) => pending.set(id, [resolve, reject]));
    return new Response(r.body, r);
  }

  define("Headers", Headers);
  define("Response", Response);
  define("fetch", fetch);

  // --- WebSocket ------------------------------------------------------------

  class WebSocket {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSING = 2;
    static CLOSED = 3;
    #id;
    #url;
    #state = 0;
    #protocol = "";
    #listeners = new Map();
    onopen = null;
    onmessage = null;
    onerror = null;
    onclose = null;
    constructor(url, protocols) {
      url = url instanceof URL ? url.href : String(url);
      const list = protocols === undefined ? [] : typeof protocols === "string" ? [protocols] : [...protocols].map(String);
      this.#id = n.ws_open(url, list);
      this.#url = url;
      sockets.set(this.#id, this);
    }
    get url() {
      return this.#url;
    }
    get readyState() {
      return this.#state;
    }
    get protocol() {
      return this.#protocol;
    }
    get extensions() {
      return "";
    }
    get bufferedAmount() {
      return 0;
    }
    get binaryType() {
      return "arraybuffer";
    }
    set binaryType(v) {
      if (v !== "arraybuffer") throw new SyntaxError('WebSocket.binaryType: only "arraybuffer" is supported');
    }
    send(data) {
      if (this.#state === 0) throw domError("InvalidStateError", "WebSocket.send: the socket is still connecting");
      if (this.#state !== 1) return;
      n.ws_send(this.#id, typeof data === "string" ? data : toBytes(data, "WebSocket.send"));
    }
    close(code, reason) {
      if (code !== undefined && code !== 1000 && (code < 3000 || code > 4999)) {
        throw domError("InvalidAccessError", `WebSocket.close: code ${code} must be 1000 or in 3000-4999`);
      }
      reason = reason === undefined ? "" : String(reason);
      if (n.utf8_encode(reason).length > 123) throw new SyntaxError("WebSocket.close: reason over 123 bytes");
      if (this.#state >= 2) return;
      this.#state = 2;
      n.ws_close(this.#id, code === undefined ? 1000 : code, reason);
    }
    addEventListener(type, fn) {
      if (typeof fn !== "function") return;
      const list = this.#listeners.get(type) || [];
      if (!list.includes(fn)) list.push(fn);
      this.#listeners.set(type, list);
    }
    removeEventListener(type, fn) {
      const list = this.#listeners.get(type);
      if (list) this.#listeners.set(type, list.filter((f) => f !== fn));
    }
    #emit(type, event) {
      event.type = type;
      event.target = this;
      event.currentTarget = this;
      const handler = this["on" + type];
      const fns = [...(this.#listeners.get(type) || [])];
      if (typeof handler === "function") fns.unshift(handler);
      for (const fn of fns) {
        try {
          fn.call(this, event);
        } catch (e) {
          report(e);
        }
      }
    }
    static {
      socketEvent = (ws, type, data) => {
        if (type === "open") {
          if (ws.#state !== 0) return;
          ws.#state = 1;
          ws.#protocol = data;
          ws.#emit("open", {});
        } else if (type === "message") {
          if (ws.#state === 1) ws.#emit("message", { data, origin: new URL(ws.#url).origin });
        } else if (type === "error") {
          ws.#emit("error", { message: data });
        } else if (type === "close") {
          ws.#state = 3;
          ws.#emit("close", { code: data.code, reason: data.reason, wasClean: data.clean });
        }
      };
    }
  }

  define("WebSocket", WebSocket);

  // --- storage --------------------------------------------------------------

  const TOKEN = {};
  class Storage {
    #k;
    constructor(token, k) {
      if (token !== TOKEN) throw new TypeError("Illegal constructor");
      this.#k = k;
    }
    get length() {
      return n.st_len(this.#k);
    }
    key(i) {
      return n.st_key(this.#k, i >>> 0) ?? null;
    }
    getItem(k) {
      return n.st_get(this.#k, String(k)) ?? null;
    }
    setItem(k, v) {
      const err = n.st_set(this.#k, String(k), String(v));
      if (err !== undefined) throw domError("QuotaExceededError", err);
    }
    removeItem(k) {
      n.st_remove(this.#k, String(k));
    }
    clear() {
      n.st_clear(this.#k);
    }
    flush() {
      n.st_flush(this.#k);
    }
  }
  const local = new Storage(TOKEN, 0);
  const session = new Storage(TOKEN, 1);
  define("Storage", Storage);
  Object.defineProperty(g, "localStorage", {
    get() {
      n.st_check("localStorage");
      return local;
    },
    configurable: true,
    enumerable: false,
  });
  Object.defineProperty(g, "sessionStorage", {
    get() {
      n.st_check("sessionStorage");
      return session;
    },
    configurable: true,
    enumerable: false,
  });

  // --- crypto ---------------------------------------------------------------

  const integerArrays = [Int8Array, Uint8Array, Uint8ClampedArray, Int16Array, Uint16Array, Int32Array, Uint32Array];
  if (typeof BigInt64Array !== "undefined") integerArrays.push(BigInt64Array, BigUint64Array);

  define(
    "crypto",
    Object.freeze({
      randomUUID: () => n.uuid(),
      getRandomValues(a) {
        if (!integerArrays.some((T) => a instanceof T)) {
          throw domError("TypeMismatchError", "crypto.getRandomValues: expected an integer typed array");
        }
        if (a.byteLength > 65536) {
          throw domError("QuotaExceededError", "crypto.getRandomValues: at most 65536 bytes per call");
        }
        new Uint8Array(a.buffer, a.byteOffset, a.byteLength).set(n.random(a.byteLength));
        return a;
      },
      subtle: Object.freeze({
        async digest(algorithm, data) {
          const name = typeof algorithm === "string" ? algorithm : algorithm && algorithm.name;
          return n.digest(String(name).toUpperCase(), toBytes(data, "crypto.subtle.digest"));
        },
      }),
    })
  );

  // --- fs (re-exported by the engine as "fs/promises") ----------------------

  const fsCall = (f) => {
    try {
      return f();
    } catch (e) {
      const m = /^(E[A-Z]+):/.exec(e && e.message);
      if (m) e.code = m[1];
      throw e;
    }
  };
  const encodingOf = (o) => (o !== null && typeof o === "object" ? o.encoding : o);
  const fs = Object.freeze({
    readFile: async (path, encoding) => {
      const e = encodingOf(encoding);
      return fsCall(() => n.fs_read(String(path), e === "utf8" || e === "utf-8"));
    },
    writeFile: async (path, data) =>
      fsCall(() => n.fs_write(String(path), typeof data === "string" ? n.utf8_encode(data) : toBytes(data, "writeFile"))),
    readdir: async (path) => fsCall(() => n.fs_readdir(path === undefined ? "." : String(path))),
    exists: async (path) => fsCall(() => n.fs_exists(String(path))),
    unlink: async (path) => fsCall(() => n.fs_unlink(String(path))),
    mkdir: async (path, options) => fsCall(() => n.fs_mkdir(String(path), !!(options && options.recursive))),
    rmdir: async (path) => fsCall(() => n.fs_rmdir(String(path))),
    stat: async (path) => fsCall(() => n.fs_stat(String(path))),
    rename: async (from, to) => fsCall(() => n.fs_rename(String(from), String(to))),
  });
  Object.defineProperty(g, "__nsh_services", { value: Object.freeze({ fs }), enumerable: false });

  // --- delivery -------------------------------------------------------------

  return function deliver(kind, id, a, b) {
    if (kind === "fetch") {
      const p = pending.get(id);
      if (!p) return;
      pending.delete(id);
      if (a) p[0](b);
      else p[1](new TypeError(b));
    } else if (kind === "ws") {
      const ws = sockets.get(id);
      if (!ws) return;
      if (a === "close") sockets.delete(id);
      socketEvent(ws, a, b);
    }
  };
});
