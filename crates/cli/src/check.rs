//! `nshell check`: load a program headlessly, render it once, report.
//!
//! Runs the real engine with the deterministic test measurer, so what fails
//! here fails on a device: syntax and import errors, capability violations,
//! unknown style methods (with the runtime's "did you mean"), and exceptions
//! in `init` or the first `render`. Host modules from `--registry` answer
//! sync calls with `null` and leave async calls pending, which is what a
//! screen looks like while the app's services load.

use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use native_shell::testing::Harness;
use native_shell_core::host::HostRegistry;

use crate::program::Program;

pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub tree: String,
    pub native_views: usize,
}

pub fn check(dir: &Path, registry: &HostRegistry) -> Result<(Program, Report)> {
    let program = Program::read(dir)?;
    let sources = program.sources();
    let files: Vec<(&str, &str)> = sources.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut builder = Harness::build(&files).manifest(&program.manifest_json).registry(registry.clone());
    for m in &registry.modules {
        builder = builder.sync_module(&m.name, |_, _| Ok("null".into())).async_module(&m.name, |_, _| None);
    }
    let h = builder.start();

    let mut errors = Vec::new();
    if let Some(e) = h.load_error() {
        errors.push(e.to_string());
    }
    // The engine also reports the load error through `Host::error`, prefixed.
    for e in h.host().errors() {
        if !errors.iter().any(|known| e.contains(known.as_str())) {
            errors.push(e);
        }
    }
    if h.load_error().is_none() && h.find("nsh-error-banner").is_some() {
        errors.push(format!("render failed after the first frame: {}", h.text_of("nsh-error-banner")));
    }
    let warnings = h
        .error_logs()
        .into_iter()
        .filter(|w| !errors.iter().any(|e| e.contains(w.as_str()) || w.contains(e.as_str())))
        .collect();
    let (tree, native_views) = match h.load_error() {
        Some(_) => (String::new(), 0),
        None => (h.dump(), h.instance().tree().native_count()),
    };
    Ok((program, Report { errors, warnings, tree, native_views }))
}

pub fn run(dir: &Path, registry: &HostRegistry, print_tree: bool) -> Result<ExitCode> {
    let (program, report) = check(dir, registry)?;
    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    if print_tree && !report.tree.is_empty() {
        println!("{}", report.tree.trim_end());
    }
    if !report.errors.is_empty() {
        for e in &report.errors {
            eprintln!("error: {e}");
        }
        return Ok(ExitCode::from(1));
    }
    println!(
        "OK {} {}: rendered {} native views",
        program.manifest.id, program.manifest.version, report.native_views
    );
    Ok(ExitCode::SUCCESS)
}
