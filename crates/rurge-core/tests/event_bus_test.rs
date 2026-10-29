use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use rurge_core::{CancellableEvent, EventBus, EventPriority};

#[derive(Debug, Default)]
struct OrderTestEvent {
    order: Vec<&'static str>,
}

#[derive(Debug, Default)]
struct CancellableTestEvent {
    log: Vec<&'static str>,
    cancelled: bool,
}

impl CancellableEvent for CancellableTestEvent {
    fn cancel(&mut self) {
        self.cancelled = true;
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

#[test]
fn test_priority_dispatch_order() {
    let bus = EventBus::new();

    // Register in reverse/scrambled order to prove priority ordering
    bus.subscribe::<OrderTestEvent, _>(EventPriority::Lowest, |ev| {
        ev.order.push("Lowest");
    });
    bus.subscribe::<OrderTestEvent, _>(EventPriority::Monitor, |ev| {
        ev.order.push("Monitor");
    });
    bus.subscribe::<OrderTestEvent, _>(EventPriority::Normal, |ev| {
        ev.order.push("Normal");
    });
    bus.subscribe::<OrderTestEvent, _>(EventPriority::Highest, |ev| {
        ev.order.push("Highest");
    });
    bus.subscribe::<OrderTestEvent, _>(EventPriority::Low, |ev| {
        ev.order.push("Low");
    });
    bus.subscribe::<OrderTestEvent, _>(EventPriority::High, |ev| {
        ev.order.push("High");
    });

    let mut event = OrderTestEvent::default();
    bus.dispatch(&mut event);

    assert_eq!(
        event.order,
        vec!["Highest", "High", "Normal", "Low", "Lowest", "Monitor"],
        "Handlers must execute in order: Highest -> High -> Normal -> Low -> Lowest -> Monitor"
    );
}

#[test]
fn test_same_priority_fifo_order() {
    let bus = EventBus::new();

    bus.subscribe::<OrderTestEvent, _>(EventPriority::Normal, |ev| {
        ev.order.push("Normal_1");
    });
    bus.subscribe::<OrderTestEvent, _>(EventPriority::Normal, |ev| {
        ev.order.push("Normal_2");
    });
    bus.subscribe::<OrderTestEvent, _>(EventPriority::Normal, |ev| {
        ev.order.push("Normal_3");
    });

    let mut event = OrderTestEvent::default();
    bus.dispatch(&mut event);

    assert_eq!(
        event.order,
        vec!["Normal_1", "Normal_2", "Normal_3"],
        "Handlers with identical priority must preserve registration order"
    );
}

#[test]
fn test_cancellation_skips_lower_priorities_except_monitor() {
    let bus = EventBus::new();

    bus.subscribe::<CancellableTestEvent, _>(EventPriority::Highest, |ev| {
        ev.log.push("Highest");
    });

    bus.subscribe::<CancellableTestEvent, _>(EventPriority::High, |ev| {
        ev.log.push("High");
        ev.cancel(); // Cancel at High priority
    });

    bus.subscribe::<CancellableTestEvent, _>(EventPriority::Normal, |ev| {
        ev.log.push("Normal"); // Should be skipped!
    });

    bus.subscribe::<CancellableTestEvent, _>(EventPriority::Low, |ev| {
        ev.log.push("Low"); // Should be skipped!
    });

    bus.subscribe::<CancellableTestEvent, _>(EventPriority::Lowest, |ev| {
        ev.log.push("Lowest"); // Should be skipped!
    });

    bus.subscribe::<CancellableTestEvent, _>(EventPriority::Monitor, |ev| {
        ev.log.push("Monitor"); // Monitor MUST always run!
    });

    let mut event = CancellableTestEvent::default();
    bus.dispatch_cancellable(&mut event);

    assert!(event.is_cancelled(), "Event must be marked cancelled");
    assert_eq!(
        event.log,
        vec!["Highest", "High", "Monitor"],
        "Cancelled event must skip Normal, Low, Lowest but run Monitor"
    );
}

#[test]
fn test_pre_cancelled_event_runs_only_monitor() {
    let bus = EventBus::new();

    bus.subscribe::<CancellableTestEvent, _>(EventPriority::Highest, |ev| {
        ev.log.push("Highest");
    });
    bus.subscribe::<CancellableTestEvent, _>(EventPriority::Normal, |ev| {
        ev.log.push("Normal");
    });
    bus.subscribe::<CancellableTestEvent, _>(EventPriority::Monitor, |ev| {
        ev.log.push("Monitor");
    });

    let mut event = CancellableTestEvent {
        log: Vec::new(),
        cancelled: true,
    };
    bus.dispatch_cancellable(&mut event);

    assert_eq!(
        event.log,
        vec!["Monitor"],
        "Pre-cancelled event must only execute Monitor priority handlers"
    );
}

#[test]
fn test_unsubscribe() {
    let bus = EventBus::new();

    let sub_id = bus.subscribe::<OrderTestEvent, _>(EventPriority::Normal, |ev| {
        ev.order.push("Subscribed");
    });

    let mut event1 = OrderTestEvent::default();
    bus.dispatch(&mut event1);
    assert_eq!(event1.order, vec!["Subscribed"]);

    let removed = bus.unsubscribe(sub_id);
    assert!(removed, "Unsubscribe should return true for existing subscription");

    let mut event2 = OrderTestEvent::default();
    bus.dispatch(&mut event2);
    assert!(event2.order.is_empty(), "Handler must not run after unsubscribe");

    let removed_again = bus.unsubscribe(sub_id);
    assert!(!removed_again, "Unsubscribe should return false for already removed subscription");
}

#[test]
fn test_multithreaded_dispatch() {
    let bus = Arc::new(EventBus::new());
    let counter = Arc::new(AtomicUsize::new(0));

    let counter_clone = Arc::clone(&counter);
    bus.subscribe::<OrderTestEvent, _>(EventPriority::Normal, move |_| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    });

    let mut handles = Vec::new();
    for _ in 0..8 {
        let bus_clone = Arc::clone(&bus);
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                let mut ev = OrderTestEvent::default();
                bus_clone.dispatch(&mut ev);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("thread join failed");
    }

    assert_eq!(counter.load(Ordering::SeqCst), 800);
}
