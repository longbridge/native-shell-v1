//! A tiny engine stand-in: one runtime, one context, `Services` installed, and
//! a loop that pumps `poll` and the job queue until a script's promise settles.

#![allow(dead_code)]

use std::time::{Duration, Instant};

use native_shell_core::manifest::Capabilities;
use native_shell_services::{Limits, Services, ServicesConfig};
use rquickjs::{Context, Runtime};

pub struct Harness {
    pub rt: Runtime,
    pub ctx: Context,
    pub services: Services,
    pub dir: tempfile::TempDir,
}

impl Harness {
    pub fn new(capabilities: Capabilities) -> Harness {
        Harness::with(capabilities, Limits::default(), tempfile::tempdir().unwrap())
    }

    pub fn with(capabilities: Capabilities, limits: Limits, dir: tempfile::TempDir) -> Harness {
        let rt = Runtime::new().unwrap();
        let ctx = Context::full(&rt).unwrap();
        let mut config = ServicesConfig::new("com.example.test", Some(dir.path().to_path_buf()), capabilities);
        config.limits = limits;
        let services = Services::new(config);
        ctx.with(|ctx| services.install(&ctx)).unwrap();
        Harness { rt, ctx, services, dir }
    }

    /// Runs `body` as the body of an async function; returns the JSON of what
    /// it returns, or `Err` with the rejection as `"<name>: <message>"`.
    pub fn run(&self, body: &str) -> Result<serde_json::Value, String> {
        let script = format!(
            "globalThis.__done = undefined; (async () => {{ {body} }})().then(
                v => {{ globalThis.__done = ['ok', JSON.stringify(v === undefined ? null : v)]; }},
                e => {{ globalThis.__done = ['err', (e && e.name ? e.name + ': ' : '') + (e && e.message !== undefined ? e.message : String(e))]; }});"
        );
        self.ctx.with(|ctx| ctx.eval::<(), _>(script.as_str())).map_err(|e| format!("eval: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.ctx.with(|ctx| self.services.poll(&ctx));
            while self.rt.execute_pending_job().map_err(|_| "job failed".to_string())? {}
            let done: Option<Vec<String>> = self.ctx.with(|ctx| ctx.globals().get("__done")).unwrap();
            if let Some(done) = done {
                return if done[0] == "ok" {
                    Ok(serde_json::from_str(&done[1]).unwrap())
                } else {
                    Err(done[1].clone())
                };
            }
            assert!(Instant::now() < deadline, "script did not settle: {body}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.services.shutdown();
    }
}

pub fn network(hosts: &[&str]) -> Capabilities {
    let mut caps = Capabilities::default();
    caps.network.hosts = hosts.iter().map(|h| h.to_string()).collect();
    caps
}
