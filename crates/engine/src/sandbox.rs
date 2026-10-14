//! The sandbox: language trimming and per-call time budgets.
//!
//! The memory and stack ceilings are plain runtime settings (see
//! `Instance::new`). Time is the subtle one: QuickJS calls an interrupt handler
//! periodically while interpreting, and returning `true` unwinds with an
//! exception no `catch` can swallow. The budget is per Rust → JS call, not
//! global: each entry point starts its own clock with the budget of its kind
//! (a render is cut off much sooner than an event handler, module evaluation
//! much later), so a long-lived program never accumulates time toward a limit.
//!
//! Dynamic code is withheld by script after the prelude is installed and
//! before the program runs (ported from gpui-shell's sandbox): `eval` is
//! deleted, every function constructor is replaced by one that throws, and the
//! shared builtin prototypes are frozen so one module cannot change `for…in`
//! or `Array.prototype` for everyone else. `dev` keeps all three for REPL-style
//! debugging; it never relaxes a capability.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rquickjs::Ctx;

use crate::instance::Limits;

/// The kind of Rust → JS call in progress, which decides its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Render,
    Event,
    Task,
    Module,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Render => "render",
            Phase::Event => "event handler",
            Phase::Task => "task",
            Phase::Module => "module evaluation",
        }
    }
}

#[derive(Clone, Copy)]
struct Running {
    phase: Phase,
    started: Instant,
    budget: Duration,
}

/// Shared between the instance (which starts and stops calls) and the interrupt handler.
#[derive(Clone)]
pub(crate) struct Budget {
    limits: Rc<Limits>,
    running: Rc<Cell<Option<Running>>>,
    tripped: Rc<Cell<Option<Phase>>>,
}

impl Budget {
    pub fn new(limits: Limits) -> Budget {
        Budget { limits: Rc::new(limits), running: Rc::new(Cell::new(None)), tripped: Rc::new(Cell::new(None)) }
    }

    pub fn start(&self, phase: Phase) {
        let budget = match phase {
            Phase::Render => self.limits.render_budget,
            Phase::Event | Phase::Task => self.limits.event_budget,
            Phase::Module => self.limits.module_budget,
        };
        self.tripped.set(None);
        self.running.set(Some(Running { phase, started: Instant::now(), budget }));
    }

    pub fn stop(&self) {
        self.running.set(None);
    }

    /// The message for an exception that was the interrupt, if the last call was cut off.
    pub fn take_trip(&self) -> Option<String> {
        self.tripped.take().map(|phase| {
            let budget = match phase {
                Phase::Render => self.limits.render_budget,
                Phase::Event | Phase::Task => self.limits.event_budget,
                Phase::Module => self.limits.module_budget,
            };
            format!("script exceeded the {} budget of {} ms and was interrupted", phase.name(), budget.as_millis())
        })
    }

    pub fn interrupt_handler(&self) -> Box<dyn FnMut() -> bool> {
        let running = self.running.clone();
        let tripped = self.tripped.clone();
        Box::new(move || match running.get() {
            Some(r) if r.started.elapsed() > r.budget => {
                tripped.set(Some(r.phase));
                true
            }
            _ => false,
        })
    }
}

/// Withholds every path from a string to executable code (see the module docs).
const WITHHOLD_DYNAMIC_CODE: &str = r#"
(() => {
  const hint = "native-shell withholds dynamic code; it is available in dev mode only";
  const deny = (label) => function () {
    throw new TypeError(`${label} is disabled: ${hint}`);
  };
  const replaceConstructor = (holder, value) => {
    Object.defineProperty(Object.getPrototypeOf(holder), "constructor", {
      value, writable: true, enumerable: false, configurable: true,
    });
  };
  const blocked = deny("the Function constructor");
  blocked.prototype = Function.prototype;
  replaceConstructor(function () {}, blocked);
  replaceConstructor(async function () {}, deny("the AsyncFunction constructor"));
  replaceConstructor(function* () {}, deny("the GeneratorFunction constructor"));
  replaceConstructor(async function* () {}, deny("the AsyncGeneratorFunction constructor"));
  globalThis.Function = blocked;
  delete globalThis.eval;
})();
"#;

/// Runs after the constructor swap so the replacement is what gets frozen.
const FREEZE_BUILTINS: &str = r#"
(() => {
  // Not Error.prototype: `this.name = "MyError"` in a subclass constructor
  // would then fail (an inherited read-only property blocks the assignment).
  for (const proto of [Object.prototype, Array.prototype, Function.prototype, String.prototype, Number.prototype]) {
    Object.freeze(proto);
  }
})();
"#;

/// Names authors reach for that do not exist here, with what to use instead.
const ABSENT: &str = r#"
(() => {
  Object.defineProperty(globalThis, "require", {
    value: function require() { throw new TypeError("`require` is not available: native-shell programs are ES modules; use `import`"); },
    enumerable: false,
  });
})();
"#;

/// Installs the policy. Must run after the prelude and before any program module.
pub(crate) fn install(ctx: &Ctx<'_>, dev: bool) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(ABSENT)?;
    if !dev {
        ctx.eval::<(), _>(WITHHOLD_DYNAMIC_CODE)?;
        ctx.eval::<(), _>(FREEZE_BUILTINS)?;
    }
    Ok(())
}
