//! The event bus (ADR-004): `tokio::sync::broadcast`, one receiver per subscriber.
//!
//! Receivers park when idle, so an idle bus costs nothing. Nothing in this module polls or
//! uses timers. A subscriber that falls behind is told so explicitly with [`Recv::Resync`] and
//! must reload its state from the database — lag is never silently ignored.

use tokio::sync::broadcast::{self, error::RecvError};

use crate::events::Event;

/// Default capacity. Human-driven events are a few per second at most; Phase 2's bursty
/// sources are buffered before they reach the bus (ADR-005), and V-5 verifies the margin.
pub const DEFAULT_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        Self {
            tx: broadcast::channel(capacity).0,
        }
    }

    /// Publish to every current subscriber. Returns how many received it. Publishing with no
    /// subscribers is not an error — it is simply 0.
    pub fn publish(&self, event: Event) -> usize {
        self.tx.send(event).unwrap_or(0)
    }

    pub fn subscribe(&self) -> Subscriber {
        Subscriber {
            rx: self.tx.subscribe(),
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

/// What a subscriber gets from [`Subscriber::recv`].
///
/// `Event` is much larger than `Resync` because events like `TaskCreated` carry a whole task. It
/// is returned by value once per receive and consumed immediately, so boxing every event to
/// shrink the rare small variants would only add an allocation per event.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Recv {
    Event(Event),
    /// This subscriber missed `missed` events. Reload from the database, then carry on.
    Resync {
        missed: u64,
    },
    /// Every sender is gone; no more events will arrive.
    Closed,
}

pub struct Subscriber {
    rx: broadcast::Receiver<Event>,
}

impl Subscriber {
    pub async fn recv(&mut self) -> Recv {
        match self.rx.recv().await {
            Ok(event) => Recv::Event(event),
            Err(RecvError::Lagged(missed)) => {
                tracing::warn!(
                    missed,
                    "event subscriber fell behind; it must resync from the database"
                );
                Recv::Resync { missed }
            }
            Err(RecvError::Closed) => Recv::Closed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::one_of_each;

    fn settings(at: i64) -> Event {
        Event::SettingChanged {
            at,
            key: "k".into(),
            value: serde_json::json!(at),
        }
    }

    #[tokio::test]
    async fn subscribers_receive_events_in_publish_order() {
        let bus = EventBus::default();
        let mut sub = bus.subscribe();
        for e in one_of_each() {
            bus.publish(e);
        }
        for expected in one_of_each() {
            assert_eq!(sub.recv().await, Recv::Event(expected));
        }
    }

    #[tokio::test]
    async fn every_subscriber_gets_every_event() {
        let bus = EventBus::default();
        let (mut a, mut b) = (bus.subscribe(), bus.subscribe());
        assert_eq!(
            bus.publish(settings(1)),
            2,
            "publish reports how many received it"
        );
        assert_eq!(a.recv().await, Recv::Event(settings(1)));
        assert_eq!(b.recv().await, Recv::Event(settings(1)));
    }

    #[tokio::test]
    async fn a_slow_subscriber_is_told_how_many_it_missed_and_then_catches_up() {
        let bus = EventBus::new(2);
        let mut slow = bus.subscribe();
        for i in 1..=5 {
            bus.publish(settings(i));
        }
        // Capacity 2 keeps the last two (4, 5); the first three were dropped for this subscriber.
        assert_eq!(slow.recv().await, Recv::Resync { missed: 3 });
        assert_eq!(slow.recv().await, Recv::Event(settings(4)));
        assert_eq!(slow.recv().await, Recv::Event(settings(5)));
    }

    #[tokio::test]
    async fn lag_on_one_subscriber_does_not_affect_another() {
        let bus = EventBus::new(2);
        let (mut slow, mut keeping_up) = (bus.subscribe(), bus.subscribe());
        bus.publish(settings(1));
        bus.publish(settings(2));
        assert_eq!(keeping_up.recv().await, Recv::Event(settings(1)));
        assert_eq!(keeping_up.recv().await, Recv::Event(settings(2)));
        bus.publish(settings(3));
        bus.publish(settings(4));
        // The subscriber that kept up sees everything, with no resync.
        assert_eq!(keeping_up.recv().await, Recv::Event(settings(3)));
        assert_eq!(keeping_up.recv().await, Recv::Event(settings(4)));
        // The one that never read missed events 1 and 2 only.
        assert_eq!(slow.recv().await, Recv::Resync { missed: 2 });
        assert_eq!(slow.recv().await, Recv::Event(settings(3)));
        assert_eq!(slow.recv().await, Recv::Event(settings(4)));
    }

    #[test]
    fn publishing_with_no_subscribers_is_fine() {
        assert_eq!(EventBus::default().publish(settings(1)), 0);
    }

    #[tokio::test]
    async fn closing_the_bus_drains_pending_events_then_reports_closed() {
        let bus = EventBus::default();
        let mut sub = bus.subscribe();
        bus.publish(settings(1));
        drop(bus);
        assert_eq!(sub.recv().await, Recv::Event(settings(1)));
        assert_eq!(sub.recv().await, Recv::Closed);
    }

    #[test]
    fn the_bus_modules_use_no_timers_or_sleeps() {
        for file in ["bus.rs", "events.rs"] {
            let src = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("src")
                    .join(file),
            )
            .unwrap();
            let production = src.split("#[cfg(test)]").next().unwrap();
            for banned in ["tokio::time", "thread::sleep", "interval("] {
                assert!(
                    !production.contains(banned),
                    "{file} must not use {banned} (Principle 1)"
                );
            }
        }
    }
}
