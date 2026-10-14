// `fs/promises`, confined to the program's data directory. The implementation
// (and the `capabilities.fs` check) belongs to native-shell-services; this
// module only gives it its Node-compatible import name.

const fs = globalThis.__nsh_services?.fs;

function call(name, args) {
  if (!fs || typeof fs[name] !== "function") {
    return Promise.reject(new Error(`fs/promises.${name} is unavailable in this instance`));
  }
  return fs[name](...args);
}

export const readFile = (...a) => call("readFile", a);
export const writeFile = (...a) => call("writeFile", a);
export const readdir = (...a) => call("readdir", a);
export const exists = (...a) => call("exists", a);
export const unlink = (...a) => call("unlink", a);
export const mkdir = (...a) => call("mkdir", a);
export const stat = (...a) => call("stat", a);
export const rename = (...a) => call("rename", a);
export default { readFile, writeFile, readdir, exists, unlink, mkdir, stat, rename };
