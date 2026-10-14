//! Compiles the JS prelude to QuickJS bytecode at build time, so an instance
//! loads it instead of parsing ~3,000 lines at every boot.
//!
//! The bytecode is our own build artifact, embedded with `include_bytes!`, so
//! it needs no signature. QuickJS bytecode depends on the engine version (the
//! same crate builds both sides) and on endianness and pointer width, so when
//! the target differs from the build host in either, nothing is embedded and
//! the prelude is parsed from source at run time, as it also is if a module
//! fails to load from bytecode.

use std::path::PathBuf;

use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::module::{Declared, WriteOptions, WriteOptionsEndianness};
use rquickjs::{Context, Ctx, Module, Runtime};

/// Keep in sync with `PRELUDE` in `src/loader.rs`.
const MODULES: &[(&str, &str)] = &[
    ("nsh:core", "js/core.js"),
    ("nsh:elements", "js/elements.js"),
    ("nsh:components", "js/components.js"),
    ("nsh:base", "js/base.js"),
    ("nsh:host", "js/host.js"),
    ("nsh:index", "js/index.js"),
    ("nsh:fs", "js/fs.js"),
    ("nsh:component/feedback", "js/gpui_component/feedback.js"),
    ("nsh:component/forms", "js/gpui_component/forms.js"),
    ("nsh:component/data", "js/gpui_component/data.js"),
    ("nsh:component/charts", "js/gpui_component/charts.js"),
    ("nsh:component/chat", "js/gpui_component/chat.js"),
    ("nsh:component", "js/component.js"),
    ("nsh:shell", "js/shell.js"),
    ("nsh:boot", "js/boot.js"),
];

/// Imports stay specifiers in the bytecode; they are resolved when an instance loads it.
struct Stub;

impl Resolver for Stub {
    fn resolve<'js>(&mut self, _: &Ctx<'js>, _: &str, name: &str, _: Option<ImportAttributes<'js>>) -> rquickjs::Result<String> {
        Ok(name.to_string())
    }
}

impl Loader for Stub {
    fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str, _: Option<ImportAttributes<'js>>) -> rquickjs::Result<Module<'js, Declared>> {
        Module::declare(ctx.clone(), name, "")
    }
}

fn main() {
    println!("cargo:rerun-if-changed=js");
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("prelude.bin");
    let target_endian = std::env::var("CARGO_CFG_TARGET_ENDIAN").unwrap_or_default();
    let target_width = std::env::var("CARGO_CFG_TARGET_POINTER_WIDTH").unwrap_or_default();
    let host_endian = if cfg!(target_endian = "little") { "little" } else { "big" };
    let host_width = (usize::BITS).to_string();
    if target_endian != host_endian || target_width != host_width {
        std::fs::write(&out, []).expect("write prelude.bin");
        return;
    }

    let rt = Runtime::new().expect("QuickJS runtime");
    rt.set_loader(Stub, Stub);
    let ctx = Context::full(&rt).expect("QuickJS context");
    let mut blob = Vec::new();
    blob.extend((MODULES.len() as u32).to_le_bytes());
    ctx.with(|ctx| {
        for (name, path) in MODULES {
            println!("cargo:rerun-if-changed={path}");
            let source = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
            let options = WriteOptions {
                endianness: if host_endian == "little" { WriteOptionsEndianness::Little } else { WriteOptionsEndianness::Big },
                ..WriteOptions::default()
            };
            let bytes = Module::declare(ctx.clone(), *name, source)
                .and_then(|m| m.write(options))
                .unwrap_or_else(|e| panic!("compiling {path}: {e} {:?}", ctx.catch()));
            for part in [name.as_bytes(), &bytes] {
                blob.extend((part.len() as u32).to_le_bytes());
                blob.extend(part);
            }
        }
    });
    std::fs::write(&out, blob).expect("write prelude.bin");
}
