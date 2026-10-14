//! Module resolution and loading, confined to the program.
//!
//! A program reaches exactly three kinds of module:
//!
//! * its own files, by relative specifier (`./ui.js`, `../lib/x`; `.js` is
//!   optional), never escaping the program root;
//! * the synthetic modules: `native-shell` (also `gpui-kit`, `gpui`,
//!   `gpui-base`) and `fs/promises`;
//! * one module per host-registered module (`demo/quote`), refused at link time
//!   when it requires permission the capabilities do not grant.
//!
//! The prelude's own modules are named `nsh:…` and can only be imported by the
//! prelude and by the generated host-module sources, never by the program.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use native_shell_core::host::HostRegistry;
use native_shell_core::manifest::Capabilities;
use native_shell_services::bundle::VerifiedBundle;
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::module::Declared;
use rquickjs::{Ctx, Error as JsError, Module};

/// Keep in sync with `MODULES` in `build.rs`.
const PRELUDE: &[(&str, &str)] = &[
    ("nsh:core", include_str!("../js/core.js")),
    ("nsh:elements", include_str!("../js/elements.js")),
    ("nsh:components", include_str!("../js/components.js")),
    ("nsh:base", include_str!("../js/base.js")),
    ("nsh:host", include_str!("../js/host.js")),
    ("nsh:index", include_str!("../js/index.js")),
    ("nsh:fs", include_str!("../js/fs.js")),
    ("nsh:component/feedback", include_str!("../js/gpui_component/feedback.js")),
    ("nsh:component/forms", include_str!("../js/gpui_component/forms.js")),
    ("nsh:component/data", include_str!("../js/gpui_component/data.js")),
    ("nsh:component/charts", include_str!("../js/gpui_component/charts.js")),
    ("nsh:component/chat", include_str!("../js/gpui_component/chat.js")),
    ("nsh:component", include_str!("../js/component.js")),
    ("nsh:shell", include_str!("../js/shell.js")),
    ("nsh:boot", include_str!("../js/boot.js")),
];

pub(crate) const BOOT: &str = "nsh:boot";

/// The prelude compiled by `build.rs`: `u32` count, then per module a `u32`-length
/// name and a `u32`-length bytecode. Empty when the target cannot use host bytecode.
static PRELUDE_BYTECODE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/prelude.bin"));

/// The build-time bytecode of a prelude module, if any.
fn prelude_bytecode(name: &str) -> Option<&'static [u8]> {
    let mut rest = PRELUDE_BYTECODE;
    let mut take = |n: usize| -> Option<&'static [u8]> {
        let (head, tail) = (rest.get(..n)?, rest.get(n..)?);
        rest = tail;
        Some(head)
    };
    let word = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
    let count = word(take(4)?);
    for _ in 0..count {
        let len = word(take(4)?);
        let module = take(len)?;
        let len = word(take(4)?);
        let bytes = take(len)?;
        if module == name.as_bytes() {
            return Some(bytes);
        }
    }
    None
}

/// Host-module names the engine answers itself through the host (`nsh/clipboard`);
/// neither scripts nor the host registry may use them.
pub(crate) const RESERVED_PREFIX: &str = "nsh/";

const PUBLIC_ALIASES: &[&str] = &["native-shell", "gpui-kit", "gpui", "gpui-base"];

/// What the loader serves: the program's files and the host registry.
#[derive(Default)]
pub(crate) struct Program {
    pub sources: BTreeMap<String, String>,
    /// The signed bundle the program came from; its bytecode loads only through it.
    pub bundle: Option<Rc<VerifiedBundle>>,
    pub registry: HostRegistry,
    pub capabilities: Capabilities,
    /// The last import refused for lack of a capability, to classify the load error.
    pub refusal: Option<String>,
    /// Prelude modules loaded from embedded bytecode, and from source.
    pub prelude_bytecode: u32,
    pub prelude_source: u32,
}

impl Program {
    /// Whether the program has a module at `path` (source or bytecode).
    pub fn has(&self, path: &str) -> bool {
        self.sources.contains_key(path) || self.bundle.as_ref().is_some_and(|b| b.bytecode.contains_key(path))
    }
}

pub(crate) type ProgramRef = Rc<RefCell<Program>>;

/// `a/b.js` + `../c` → `c`; `None` when the path climbs out of the program.
fn join(base: &str, spec: &str) -> Option<String> {
    let mut parts: Vec<&str> = match base.rsplit_once('/') {
        Some((dir, _)) => dir.split('/').filter(|p| !p.is_empty()).collect(),
        None => Vec::new(),
    };
    for seg in spec.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// A program path as the loader keys it: no `./` prefix, `/` separators.
pub(crate) fn normalize(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    if path.starts_with('/') {
        return None;
    }
    join("", &path)
}

fn is_internal(name: &str) -> bool {
    name.starts_with("nsh:")
}

fn is_identifier(s: &str) -> bool {
    const RESERVED: &[&str] = &[
        "on",
        "default",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "let",
        "new",
        "null",
        "return",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
    ];
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        && !RESERVED.contains(&s)
}

/// The source of a host module: its methods as named exports, plus `on` and a default export.
fn host_module_source(name: &str, methods: &[String]) -> String {
    let quoted = serde_json::to_string(name).unwrap_or_default();
    let mut s = format!(
        "import {{ host_module }} from \"nsh:host\";\nconst m = host_module({quoted});\nexport default m;\nexport const on = m.on;\n"
    );
    for method in methods.iter().filter(|m| is_identifier(m)) {
        s.push_str(&format!("export const {method} = m.{method};\n"));
    }
    s
}

pub(crate) struct ShellResolver(pub ProgramRef);

impl Resolver for ShellResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        let mut program = self.0.borrow_mut();
        let trusted_base = base.is_empty() || is_internal(base) || program.registry.module(base).is_some();
        if PUBLIC_ALIASES.contains(&name) {
            return Ok("nsh:index".into());
        }
        if name == "fs/promises" {
            return Ok("nsh:fs".into());
        }
        if name == "gpui-component" {
            return Ok("nsh:component".into());
        }
        if name == "gpui-shell" {
            return Ok("nsh:shell".into());
        }
        if name.starts_with(RESERVED_PREFIX) {
            return Err(JsError::new_resolving_message(base, name, "reserved module name"));
        }
        if is_internal(name) {
            if trusted_base && PRELUDE.iter().any(|(n, _)| *n == name) {
                return Ok(name.into());
            }
            return Err(JsError::new_resolving_message(base, name, "internal module"));
        }
        if let Some(m) = program.registry.module(name) {
            if m.requires_permission && !program.capabilities.allows_host_module(name) {
                let message = format!(
                    "host module `{name}` is not granted; add it to capabilities.host_modules in native-shell.json"
                );
                program.refusal = Some(message.clone());
                return Err(JsError::new_resolving_message(base, name, message));
            }
            return Ok(name.into());
        }
        let relative = name.starts_with("./") || name.starts_with("../");
        // Rust imports the entry by its path; nothing else may use a bare path.
        let path = if relative {
            join(base, name)
        } else if base.is_empty() {
            normalize(name)
        } else {
            let mut known: Vec<&str> =
                vec!["native-shell", "gpui-kit", "gpui-base", "gpui-component", "gpui-shell", "fs/promises"];
            known.extend(program.registry.modules.iter().map(|m| m.name.as_str()));
            return Err(JsError::new_resolving_message(
                base,
                name,
                format!(
                    "unknown module; program files are imported by relative path (\"./{name}.js\"), and the available modules are {}",
                    known.join(", ")
                ),
            ));
        };
        let Some(path) = path else {
            return Err(JsError::new_resolving_message(base, name, "the path leaves the program directory"));
        };
        for candidate in [path.clone(), format!("{path}.js")] {
            if program.has(&candidate) {
                return Ok(candidate);
            }
        }
        Err(JsError::new_resolving_message(base, name, format!("no file `{path}` in the program")))
    }
}

pub(crate) struct ShellLoader(pub ProgramRef);

impl Loader for ShellLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        if let Some((_, source)) = PRELUDE.iter().find(|(n, _)| *n == name) {
            if let Some(bytes) = prelude_bytecode(name) {
                // SAFETY: produced by this crate's own build script from the same
                // sources, with this QuickJS, for this endianness and pointer width.
                if let Ok(module) = unsafe { Module::load(ctx.clone(), bytes) } {
                    self.0.borrow_mut().prelude_bytecode += 1;
                    return Ok(module);
                }
                let _ = ctx.catch();
            }
            self.0.borrow_mut().prelude_source += 1;
            return Module::declare(ctx.clone(), name, *source);
        }
        let program = self.0.borrow();
        if let Some(m) = program.registry.module(name) {
            let methods: Vec<String> = m.methods.iter().map(|m| m.name.clone()).collect();
            let source = host_module_source(name, &methods);
            drop(program);
            return Module::declare(ctx.clone(), name, source);
        }
        if let Some(bundle) = program.bundle.clone() {
            // Declaring resolves imports, which re-enters the resolver: no borrow held.
            drop(program);
            if let Some(module) = bundle.load_module(ctx, name)? {
                return Ok(module);
            }
            let program = self.0.borrow();
            return match program.sources.get(name).cloned() {
                Some(source) => {
                    drop(program);
                    Module::declare(ctx.clone(), name, source)
                }
                None => Err(JsError::new_loading(name)),
            };
        }
        if let Some(source) = program.sources.get(name) {
            let source = source.clone();
            drop(program);
            return Module::declare(ctx.clone(), name, source);
        }
        Err(JsError::new_loading(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_the_program() {
        assert_eq!(join("main.js", "./ui.js").as_deref(), Some("ui.js"));
        assert_eq!(join("lib/a.js", "../b").as_deref(), Some("b"));
        assert_eq!(join("lib/a.js", "./x/../y.js").as_deref(), Some("lib/y.js"));
        assert_eq!(join("main.js", "../escape.js"), None);
        assert_eq!(normalize("./main.js").as_deref(), Some("main.js"));
        assert_eq!(normalize("/etc/passwd"), None);
    }

    #[test]
    fn host_module_sources_export_valid_names_only() {
        let s = host_module_source("demo/quote", &["quote".into(), "get-x".into(), "on".into(), "default".into()]);
        assert!(s.contains("export const quote = m.quote;"));
        assert!(!s.contains("get-x"));
        assert_eq!(s.matches("export const on").count(), 1);
    }
}
