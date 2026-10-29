use parking_lot::RwLock;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

/// Priority levels determining the order in which event handlers are called.
///
/// Dispatch order: `Highest` -> `High` -> `Normal` -> `Low` -> `Lowest` -> `Monitor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventPriority {
    Lowest,
    Low,
    Normal,
    High,
    Highest,
    Monitor,
}

impl EventPriority {
    /// The strict dispatch order for event execution.
    pub const DISPATCH_ORDER: [EventPriority; 6] = [
        EventPriority::Highest,
        EventPriority::High,
        EventPriority::Normal,
        EventPriority::Low,
        EventPriority::Lowest,
        EventPriority::Monitor,
    ];
}

#[inline]
fn priority_rank(priority: EventPriority) -> u8 {
    match priority {
        EventPriority::Highest => 0,
        EventPriority::High => 1,
        EventPriority::Normal => 2,
        EventPriority::Low => 3,
        EventPriority::Lowest => 4,
        EventPriority::Monitor => 5,
    }
}

/// Trait for events that can be cancelled by handlers.
///
/// When an event is cancelled, subsequent handlers with priority lower than `Monitor`
/// will be skipped. Handlers with `EventPriority::Monitor` always run to observe the final state.
pub trait CancellableEvent {
    /// Cancels this event.
    fn cancel(&mut self);

    /// Checks whether this event has been cancelled.
    fn is_cancelled(&self) -> bool;
}

/// Unique identifier for an event bus subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubscriptionId(pub u64);

type HandlerFn = Arc<dyn Fn(&mut dyn Any) + Send + Sync>;
type CancellationCheckerFn = Arc<dyn Fn(&dyn Any) -> bool + Send + Sync>;

#[derive(Clone)]
struct HandlerEntry {
    id: SubscriptionId,
    priority: EventPriority,
    seq: u64,
    handler: HandlerFn,
}

/// A thread-safe, prioritized event bus for publishing and subscribing to typed events.
pub struct EventBus {
    next_id: AtomicU64,
    next_seq: AtomicU64,
    handlers: RwLock<HashMap<TypeId, Vec<HandlerEntry>>>,
    cancellation_checkers: RwLock<HashMap<TypeId, CancellationCheckerFn>>,
}

impl EventBus {
    /// Creates a new, empty `EventBus`.
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            next_seq: AtomicU64::new(1),
            handlers: RwLock::new(HashMap::new()),
            cancellation_checkers: RwLock::new(HashMap::new()),
        }
    }

    /// Subscribes a handler to events of type `E` with a given priority.
    pub fn subscribe<E: 'static, F>(&self, priority: EventPriority, handler: F) -> SubscriptionId
    where
        F: Fn(&mut E) + Send + Sync + 'static,
    {
        let id = SubscriptionId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);

        let entry = HandlerEntry {
            id,
            priority,
            seq,
            handler: Arc::new(move |any: &mut dyn Any| {
                if let Some(event) = any.downcast_mut::<E>() {
                    handler(event);
                }
            }),
        };

        let mut lock = self.handlers.write();
        let list = lock.entry(TypeId::of::<E>()).or_default();
        list.push(entry);
        list.sort_by_key(|e| (priority_rank(e.priority), e.seq));

        id
    }

    /// Subscribes a handler for a cancellable event type `E`, automatically registering its cancellation checker.
    pub fn subscribe_cancellable<E: CancellableEvent + 'static, F>(
        &self,
        priority: EventPriority,
        handler: F,
    ) -> SubscriptionId
    where
        F: Fn(&mut E) + Send + Sync + 'static,
    {
        self.register_cancellable::<E>();
        self.subscribe(priority, handler)
    }

    /// Explicitly registers a type `E` as a cancellable event.
    pub fn register_cancellable<E: CancellableEvent + 'static>(&self) {
        let mut checkers = self.cancellation_checkers.write();
        checkers.entry(TypeId::of::<E>()).or_insert_with(|| {
            Arc::new(|any: &dyn Any| {
                any.downcast_ref::<E>()
                    .map(|e| e.is_cancelled())
                    .unwrap_or(false)
            })
        });
    }

    /// Unsubscribes a previously registered handler by its subscription ID.
    pub fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut lock = self.handlers.write();
        let mut found = false;
        for list in lock.values_mut() {
            if let Some(pos) = list.iter().position(|e| e.id == id) {
                list.remove(pos);
                found = true;
                break;
            }
        }
        found
    }

    /// Dispatches an event through all registered handlers in priority order.
    pub fn dispatch<E: 'static>(&self, event: &mut E) {
        self.dispatch_internal(event, None);
    }

    /// Dispatches a cancellable event, ensuring cancellation state is monitored after each handler.
    pub fn dispatch_cancellable<E: CancellableEvent + 'static>(&self, event: &mut E) {
        self.register_cancellable::<E>();
        let checker: CancellationCheckerFn = Arc::new(|any: &dyn Any| {
            any.downcast_ref::<E>()
                .map(|e| e.is_cancelled())
                .unwrap_or(false)
        });
        self.dispatch_internal(event, Some(checker));
    }

    fn dispatch_internal<E: 'static>(
        &self,
        event: &mut E,
        explicit_checker: Option<CancellationCheckerFn>,
    ) {
        let handlers = {
            let lock = self.handlers.read();
            match lock.get(&TypeId::of::<E>()) {
                Some(list) => list.clone(),
                None => return,
            }
        };

        let checker = explicit_checker.or_else(|| {
            let checkers = self.cancellation_checkers.read();
            checkers.get(&TypeId::of::<E>()).cloned()
        });

        for entry in handlers {
            if entry.priority != EventPriority::Monitor
                && checker.as_ref().is_some_and(|c| c(event as &dyn Any))
            {
                continue;
            }

            (entry.handler)(event as &mut dyn Any);
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
