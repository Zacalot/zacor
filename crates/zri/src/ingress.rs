//! Foreground ingress kernel: how background threads deliver events into the
//! single-threaded host runtime.
//!
//! The design is queue + coalesced wake + budgeted drain at the turn boundary,
//! with no async runtime on the foreground:
//!
//! - background threads `send` source events through a cloned [`IngressSender`]
//! - the first send after a drain posts exactly one wake through the
//!   caller-supplied waker (in the native app: a winit `EventLoopProxy` user
//!   event; this module itself stays platform-free)
//! - the foreground drains at a defined turn boundary with a budget, so event
//!   bursts cannot starve input or paint; if events remain after the budget,
//!   the caller re-requests a wake and yields back to the platform loop
//!
//! The wake flag protocol is lost-wakeup safe: `drain` clears the flag before
//! popping events, so any send that lands after the drain began posts a fresh
//! wake; any send that landed before is already in the queue.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use zacor_protocol::daemon_invoke::InvocationEvent;

use crate::host::BufferId;

/// Identifies one daemon command invocation across its event stream. Allocated
/// by the service plane, monotonic per plane.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InvocationId(pub u64);

/// A deferred event applied at the runtime turn boundary. Events are
/// deferred-by-default: nothing in this enum executes off the foreground.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum IngressEvent {
    /// Append text to a local buffer. The target is revalidated at apply time;
    /// a missing buffer means the event is skipped, never an error.
    BufferAppend { buffer: BufferId, text: String },
    /// One streamed event from a daemon command invocation, forwarded by the
    /// service plane's reader thread.
    Invocation {
        id: InvocationId,
        event: InvocationEvent,
    },
    /// The invocation's stream ended (EOF or transport failure after the
    /// terminal event). The runtime unbinds any buffer routing for it.
    InvocationClosed { id: InvocationId },
    /// The service plane hit a transport failure outside a normal stream end.
    PlaneError { message: String },
}

/// Wakes the platform loop so it will drain the ingress queue. Must be safe to
/// invoke from any thread.
pub type IngressWaker = Arc<dyn Fn() + Send + Sync>;

/// Outcome of one budgeted drain.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DrainOutcome {
    pub applied: usize,
    /// Events remain beyond the budget; the caller should request another
    /// wake and yield back to the platform loop.
    pub remaining: bool,
}

/// Cloneable producer half. One clone per background thread.
#[derive(Clone)]
pub struct IngressSender {
    sender: Sender<IngressEvent>,
    wake_posted: Arc<AtomicBool>,
    waker: IngressWaker,
}

impl IngressSender {
    /// Enqueue an event, posting a coalesced wake. Returns false when the
    /// ingress (foreground side) has been dropped.
    pub fn send(&self, event: IngressEvent) -> bool {
        if self.sender.send(event).is_err() {
            return false;
        }
        if self
            .wake_posted
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            (self.waker)();
        }
        true
    }
}

/// Foreground-owned consumer half.
pub struct ZrIngress {
    receiver: Receiver<IngressEvent>,
    sender: Sender<IngressEvent>,
    wake_posted: Arc<AtomicBool>,
    waker: IngressWaker,
    /// One-event lookahead used to answer `remaining` without exceeding the
    /// budget by more than a peek.
    peeked: Option<IngressEvent>,
}

impl ZrIngress {
    pub fn new(waker: IngressWaker) -> Self {
        let (sender, receiver) = channel();
        Self {
            receiver,
            sender,
            wake_posted: Arc::new(AtomicBool::new(false)),
            waker,
            peeked: None,
        }
    }

    /// Mint a sender for a background thread.
    pub fn sender(&self) -> IngressSender {
        IngressSender {
            sender: self.sender.clone(),
            wake_posted: self.wake_posted.clone(),
            waker: self.waker.clone(),
        }
    }

    /// Request a wake without sending an event (used by the caller after a
    /// budget-limited drain reported `remaining`). Coalesced like sends.
    pub fn request_wake(&self) {
        if self
            .wake_posted
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            (self.waker)();
        }
    }

    /// Drain up to `budget` events into `apply`.
    ///
    /// Clears the wake flag before draining (lost-wakeup-safe: senders that
    /// enqueue after this point post a fresh wake; senders that enqueued
    /// before are already in the queue).
    pub fn drain(&mut self, budget: usize, mut apply: impl FnMut(IngressEvent)) -> DrainOutcome {
        self.wake_posted.store(false, Ordering::Release);

        let mut applied = 0;
        while applied < budget {
            let Some(event) = self.next_event() else {
                return DrainOutcome {
                    applied,
                    remaining: false,
                };
            };
            apply(event);
            applied += 1;
        }

        self.peeked = self.receiver.try_recv().ok();
        DrainOutcome {
            applied,
            remaining: self.peeked.is_some(),
        }
    }

    fn next_event(&mut self) -> Option<IngressEvent> {
        self.peeked.take().or_else(|| self.receiver.try_recv().ok())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    use super::*;

    fn counting_ingress() -> (ZrIngress, Arc<AtomicUsize>) {
        let wakes = Arc::new(AtomicUsize::new(0));
        let ingress = ZrIngress::new(Arc::new({
            let wakes = wakes.clone();
            move || {
                wakes.fetch_add(1, Ordering::SeqCst);
            }
        }));
        (ingress, wakes)
    }

    fn append(text: &str) -> IngressEvent {
        IngressEvent::BufferAppend {
            buffer: BufferId(1),
            text: text.to_string(),
        }
    }

    #[test]
    fn rapid_sends_coalesce_into_one_wake() {
        let (ingress, wakes) = counting_ingress();
        let sender = ingress.sender();

        for _ in 0..100 {
            assert!(sender.send(append("x")));
        }

        assert_eq!(wakes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn drain_clears_flag_so_new_sends_rewake() {
        let (mut ingress, wakes) = counting_ingress();
        let sender = ingress.sender();

        sender.send(append("a"));
        let outcome = ingress.drain(16, |_| {});
        assert_eq!(outcome.applied, 1);
        assert!(!outcome.remaining);

        sender.send(append("b"));

        assert_eq!(wakes.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn drain_respects_budget_and_reports_remaining() {
        let (mut ingress, _) = counting_ingress();
        let sender = ingress.sender();
        for index in 0..5 {
            sender.send(append(&index.to_string()));
        }
        let seen = Mutex::new(Vec::new());
        let apply = |event: IngressEvent| {
            let IngressEvent::BufferAppend { text, .. } = event else {
                panic!("unexpected ingress event in test: {event:?}");
            };
            seen.lock().unwrap().push(text);
        };

        let first = ingress.drain(2, apply);
        let second = ingress.drain(2, apply);
        let third = ingress.drain(2, apply);

        assert_eq!(
            (
                first.applied,
                first.remaining,
                second.applied,
                second.remaining
            ),
            (2, true, 2, true)
        );
        assert_eq!((third.applied, third.remaining), (1, false));
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &["0", "1", "2", "3", "4"],
            "FIFO order across budgeted drains"
        );
    }

    #[test]
    fn request_wake_is_coalesced() {
        let (ingress, wakes) = counting_ingress();

        ingress.request_wake();
        ingress.request_wake();

        assert_eq!(wakes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn send_during_drain_posts_fresh_wake() {
        let (mut ingress, wakes) = counting_ingress();
        let sender = ingress.sender();
        sender.send(append("a"));
        assert_eq!(wakes.load(Ordering::SeqCst), 1);

        // The drain clears the flag before popping; this send lands mid-drain
        // (after the clear) and must post a fresh wake even though the
        // earlier wake was never "used up" by the platform loop.
        let mid_drain_sender = ingress.sender();
        ingress.drain(16, move |_| {
            mid_drain_sender.send(append("late"));
        });

        assert_eq!(wakes.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn sender_reports_dropped_ingress() {
        let (ingress, _) = counting_ingress();
        let sender = ingress.sender();
        drop(ingress);

        assert!(!sender.send(append("x")));
    }

    #[test]
    fn threaded_flood_delivers_everything_in_order_across_budgeted_drains() {
        let (mut ingress, wakes) = counting_ingress();
        let sender = ingress.sender();
        let total = 10_000usize;

        let producer = std::thread::spawn(move || {
            for index in 0..total {
                assert!(sender.send(append(&index.to_string())));
            }
        });

        let mut received = Vec::with_capacity(total);
        let mut drains = 0usize;
        while received.len() < total {
            ingress.drain(64, |event| {
                let IngressEvent::BufferAppend { text, .. } = event else {
                    panic!("unexpected ingress event in test: {event:?}");
                };
                received.push(text);
            });
            drains += 1;
            std::thread::yield_now();
            assert!(drains < 1_000_000, "flood did not converge");
        }
        producer.join().unwrap();

        assert!(drains >= total / 64, "budget was not enforced");
        assert!(wakes.load(Ordering::SeqCst) >= 1);
        for (index, text) in received.iter().enumerate() {
            assert_eq!(text, &index.to_string(), "FIFO order from one producer");
        }
    }
}
