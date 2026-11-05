//! Trait and utilities for cancellable events.

/// Trait for events that can be cancelled by handlers.
///
/// When an event is marked as cancelled, downstream default gameplay behavior
/// is prevented, and subsequent handlers with priority lower than `Monitor`
/// can be skipped. Handlers with `Monitor` priority will still run to observe
/// the final cancellation state.
pub trait CancellableEvent {
    /// Cancels this event.
    fn cancel(&mut self);

    /// Checks whether this event has been cancelled.
    fn is_cancelled(&self) -> bool;
}
