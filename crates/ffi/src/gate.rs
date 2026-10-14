//! The "no callback after destroy" guarantee.
//!
//! Every host callback passes through its instance's [`Gate`]: it enters (and
//! is refused once the gate is closed) and leaves when it returns. `destroy`
//! closes the gate and waits until no callback is in flight on another
//! thread, so once it returns no callback for that instance runs again, from
//! the JS thread or from a background thread (a services completion waking the
//! instance).
//!
//! `destroy` may be called from inside a callback of the same instance (a
//! host closing a screen from `call_async`, say). That callback is in flight
//! on this very thread and cannot finish until `destroy` returns, so the wait
//! leaves out the callbacks this thread is inside of.

use std::cell::RefCell;
use std::sync::{Condvar, Mutex, MutexGuard};

#[derive(Default)]
pub struct Gate {
    state: Mutex<State>,
    idle: Condvar,
}

#[derive(Default)]
struct State {
    closed: bool,
    in_flight: usize,
}

thread_local! {
    /// Gates (by address) this thread is inside a callback of, innermost last.
    static INSIDE: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// One callback in flight; leaving the scope lets `close` proceed.
pub struct Pass<'a>(&'a Gate);

impl Gate {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn id(&self) -> usize {
        self as *const Gate as usize
    }

    /// Lets one callback through, or refuses it once the instance is destroyed.
    pub fn enter(&self) -> Option<Pass<'_>> {
        let mut s = self.lock();
        if s.closed {
            return None;
        }
        s.in_flight += 1;
        INSIDE.with(|i| i.borrow_mut().push(self.id()));
        Some(Pass(self))
    }

    /// Whether this thread is running one of this instance's callbacks.
    pub fn is_inside(&self) -> bool {
        INSIDE.with(|i| i.borrow().contains(&self.id()))
    }

    /// Refuses every later callback and waits for those in flight on other threads.
    pub fn close(&self) {
        let own = INSIDE.with(|i| i.borrow().iter().filter(|g| **g == self.id()).count());
        let mut s = self.lock();
        s.closed = true;
        while s.in_flight > own {
            s = self.idle.wait(s).unwrap_or_else(|p| p.into_inner());
        }
    }
}

impl Drop for Pass<'_> {
    fn drop(&mut self) {
        INSIDE.with(|i| {
            let mut i = i.borrow_mut();
            if let Some(pos) = i.iter().rposition(|g| *g == self.0.id()) {
                i.remove(pos);
            }
        });
        let mut s = self.0.lock();
        s.in_flight -= 1;
        drop(s);
        self.0.idle.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn close_waits_for_other_threads_and_refuses_later_callbacks() {
        let gate = Arc::new(Gate::default());
        let g = gate.clone();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let t = std::thread::spawn(move || {
            let _pass = g.enter().unwrap();
            entered_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
        });
        entered_rx.recv().unwrap();
        let started = std::time::Instant::now();
        gate.close();
        assert!(started.elapsed() >= Duration::from_millis(50), "close returned while a callback was in flight");
        assert!(gate.enter().is_none());
        t.join().unwrap();
    }

    #[test]
    fn close_from_inside_a_callback_does_not_wait_for_itself() {
        let gate = Gate::default();
        let pass = gate.enter().unwrap();
        assert!(gate.is_inside());
        gate.close();
        assert!(gate.enter().is_none());
        drop(pass);
        assert!(!gate.is_inside());
    }
}
