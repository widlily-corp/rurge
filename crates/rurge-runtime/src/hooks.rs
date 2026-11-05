use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use rurge_api::events::{
    BlockBreakEvent, EntityDamageEvent, PlayerInteractEvent, WorldTickEvent,
};
use rurge_blamer::{safe_invoke, Blamer};
use rurge_core::EventPriority;

/// Event fired during client game loop tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientTickEvent {
    pub tick_count: u64,
}

/// Event fired during server game loop tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerTickEvent {
    pub tick_count: u64,
}

type TickFn = Arc<dyn Fn(u64) + Send + Sync + 'static>;

#[derive(Clone)]
struct TickHandler {
    mod_id: String,
    handler: TickFn,
}

/// Collection of client and server tick hooks registered by mods.
#[derive(Default)]
pub struct TickHooks {
    client_handlers: Vec<TickHandler>,
    server_handlers: Vec<TickHandler>,
}

impl TickHooks {
    /// Creates a new empty `TickHooks` registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a client tick hook callback attributed to `mod_id`.
    pub fn register_client_tick<F>(&mut self, mod_id: impl Into<String>, handler: F)
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        self.client_handlers.push(TickHandler {
            mod_id: mod_id.into(),
            handler: Arc::new(handler),
        });
    }

    /// Registers a server tick hook callback attributed to `mod_id`.
    pub fn register_server_tick<F>(&mut self, mod_id: impl Into<String>, handler: F)
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        self.server_handlers.push(TickHandler {
            mod_id: mod_id.into(),
            handler: Arc::new(handler),
        });
    }

    /// Executes all client tick hooks within safe panic boundaries without atomic ref-count overhead.
    pub fn execute_client_ticks(&self, tick_count: u64, blamer: &Blamer) {
        for entry in &self.client_handlers {
            let _ = safe_invoke(
                &entry.mod_id,
                "client_tick",
                blamer,
                AssertUnwindSafe(|| {
                    (entry.handler)(tick_count);
                }),
            );
        }
    }

    /// Executes all server tick hooks within safe panic boundaries without atomic ref-count overhead.
    pub fn execute_server_ticks(&self, tick_count: u64, blamer: &Blamer) {
        for entry in &self.server_handlers {
            let _ = safe_invoke(
                &entry.mod_id,
                "server_tick",
                blamer,
                AssertUnwindSafe(|| {
                    (entry.handler)(tick_count);
                }),
            );
        }
    }
}

/// Alias for `TickHooks`.
pub type GameLoopHooks = TickHooks;

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

type EventHandlerFn<E> = Arc<dyn Fn(&mut E) + Send + Sync + 'static>;

#[derive(Clone)]
struct PrioritizedHandler<E> {
    mod_id: String,
    priority: EventPriority,
    seq: u64,
    handler: EventHandlerFn<E>,
}

/// Registry and safe dispatcher for gameplay event hooks.
///
/// Handlers are executed in strict priority order:
/// `Highest` -> `High` -> `Normal` -> `Low` -> `Lowest` -> `Monitor`.
/// If an event is cancelled by any handler, non-`Monitor` handlers with lower priority
/// will be skipped. Handlers with `Monitor` priority always execute to observe final state.
#[derive(Default)]
pub struct GameEventHooks {
    next_seq: AtomicU64,
    block_break_handlers: Vec<PrioritizedHandler<BlockBreakEvent>>,
    player_interact_handlers: Vec<PrioritizedHandler<PlayerInteractEvent>>,
    entity_damage_handlers: Vec<PrioritizedHandler<EntityDamageEvent>>,
    world_tick_handlers: Vec<PrioritizedHandler<WorldTickEvent>>,
}

impl GameEventHooks {
    /// Creates a new empty `GameEventHooks` registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a handler for `BlockBreakEvent` with a priority and attribution.
    pub fn register_block_break<F>(
        &mut self,
        mod_id: impl Into<String>,
        priority: EventPriority,
        handler: F,
    ) where
        F: Fn(&mut BlockBreakEvent) + Send + Sync + 'static,
    {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        self.block_break_handlers.push(PrioritizedHandler {
            mod_id: mod_id.into(),
            priority,
            seq,
            handler: Arc::new(handler),
        });
        self.block_break_handlers
            .sort_by_key(|e| (priority_rank(e.priority), e.seq));
    }

    /// Registers a handler for `PlayerInteractEvent` with a priority and attribution.
    pub fn register_player_interact<F>(
        &mut self,
        mod_id: impl Into<String>,
        priority: EventPriority,
        handler: F,
    ) where
        F: Fn(&mut PlayerInteractEvent) + Send + Sync + 'static,
    {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        self.player_interact_handlers.push(PrioritizedHandler {
            mod_id: mod_id.into(),
            priority,
            seq,
            handler: Arc::new(handler),
        });
        self.player_interact_handlers
            .sort_by_key(|e| (priority_rank(e.priority), e.seq));
    }

    /// Registers a handler for `EntityDamageEvent` with a priority and attribution.
    pub fn register_entity_damage<F>(
        &mut self,
        mod_id: impl Into<String>,
        priority: EventPriority,
        handler: F,
    ) where
        F: Fn(&mut EntityDamageEvent) + Send + Sync + 'static,
    {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        self.entity_damage_handlers.push(PrioritizedHandler {
            mod_id: mod_id.into(),
            priority,
            seq,
            handler: Arc::new(handler),
        });
        self.entity_damage_handlers
            .sort_by_key(|e| (priority_rank(e.priority), e.seq));
    }

    /// Registers a handler for `WorldTickEvent` with a priority and attribution.
    pub fn register_world_tick<F>(
        &mut self,
        mod_id: impl Into<String>,
        priority: EventPriority,
        handler: F,
    ) where
        F: Fn(&mut WorldTickEvent) + Send + Sync + 'static,
    {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        self.world_tick_handlers.push(PrioritizedHandler {
            mod_id: mod_id.into(),
            priority,
            seq,
            handler: Arc::new(handler),
        });
        self.world_tick_handlers
            .sort_by_key(|e| (priority_rank(e.priority), e.seq));
    }

    /// Executes `BlockBreakEvent` handlers in priority order, skipping non-Monitor handlers if cancelled.
    pub fn dispatch_block_break(&self, event: &mut BlockBreakEvent, blamer: &Blamer) {
        for entry in &self.block_break_handlers {
            if entry.priority != EventPriority::Monitor && event.is_cancelled() {
                continue;
            }
            let _ = safe_invoke(
                &entry.mod_id,
                "block_break",
                blamer,
                AssertUnwindSafe(|| {
                    (entry.handler)(event);
                }),
            );
        }
    }

    /// Executes `PlayerInteractEvent` handlers in priority order, skipping non-Monitor handlers if cancelled.
    pub fn dispatch_player_interact(&self, event: &mut PlayerInteractEvent, blamer: &Blamer) {
        for entry in &self.player_interact_handlers {
            if entry.priority != EventPriority::Monitor && event.is_cancelled() {
                continue;
            }
            let _ = safe_invoke(
                &entry.mod_id,
                "player_interact",
                blamer,
                AssertUnwindSafe(|| {
                    (entry.handler)(event);
                }),
            );
        }
    }

    /// Executes `EntityDamageEvent` handlers in priority order, skipping non-Monitor handlers if cancelled.
    pub fn dispatch_entity_damage(&self, event: &mut EntityDamageEvent, blamer: &Blamer) {
        for entry in &self.entity_damage_handlers {
            if entry.priority != EventPriority::Monitor && event.is_cancelled() {
                continue;
            }
            let _ = safe_invoke(
                &entry.mod_id,
                "entity_damage",
                blamer,
                AssertUnwindSafe(|| {
                    (entry.handler)(event);
                }),
            );
        }
    }

    /// Executes `WorldTickEvent` handlers in priority order.
    pub fn dispatch_world_tick(&self, event: &mut WorldTickEvent, blamer: &Blamer) {
        for entry in &self.world_tick_handlers {
            let _ = safe_invoke(
                &entry.mod_id,
                "world_tick",
                blamer,
                AssertUnwindSafe(|| {
                    (entry.handler)(event);
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use rurge_api::events::InteractionHand;
    use rurge_api::RurgeContext;
    use rurge_core::EventBus;

    #[test]
    fn test_game_hooks_block_break_cancellation_flow() {
        let mut hooks = GameEventHooks::new();
        let blamer = Blamer::new();
        let log = Arc::new(Mutex::new(Vec::new()));

        let l1 = log.clone();
        hooks.register_block_break("mod_high", EventPriority::High, move |e| {
            l1.lock().unwrap().push("High");
            e.cancel();
        });

        let l2 = log.clone();
        hooks.register_block_break("mod_normal", EventPriority::Normal, move |_e| {
            l2.lock().unwrap().push("Normal");
        });

        let l3 = log.clone();
        hooks.register_block_break("mod_low", EventPriority::Low, move |_e| {
            l3.lock().unwrap().push("Low");
        });

        let l4 = log.clone();
        hooks.register_block_break("mod_monitor", EventPriority::Monitor, move |e| {
            l4.lock().unwrap().push(if e.is_cancelled() { "Monitor_Cancelled" } else { "Monitor_Open" });
        });

        let mut event = BlockBreakEvent::new("steve", [10, 64, 20]);
        hooks.dispatch_block_break(&mut event, &blamer);

        assert!(event.is_cancelled());
        let entries = log.lock().unwrap().clone();
        assert_eq!(entries, vec!["High", "Monitor_Cancelled"]);
    }

    #[test]
    fn test_game_hooks_player_interact_cancellation_flow() {
        let mut hooks = GameEventHooks::new();
        let blamer = Blamer::new();
        let log = Arc::new(Mutex::new(Vec::new()));

        let l1 = log.clone();
        hooks.register_player_interact("mod_a", EventPriority::Highest, move |_e| {
            l1.lock().unwrap().push("Highest");
        });

        let l2 = log.clone();
        hooks.register_player_interact("mod_b", EventPriority::Normal, move |e| {
            l2.lock().unwrap().push("Normal_Cancel");
            e.cancel();
        });

        let l3 = log.clone();
        hooks.register_player_interact("mod_c", EventPriority::Lowest, move |_e| {
            l3.lock().unwrap().push("Lowest");
        });

        let l4 = log.clone();
        hooks.register_player_interact("mod_d", EventPriority::Monitor, move |e| {
            assert!(e.is_cancelled());
            l4.lock().unwrap().push("Monitor");
        });

        let mut event = PlayerInteractEvent::new("alex", InteractionHand::Main, [0, 1, 0]);
        hooks.dispatch_player_interact(&mut event, &blamer);

        assert!(event.is_cancelled());
        let entries = log.lock().unwrap().clone();
        assert_eq!(entries, vec!["Highest", "Normal_Cancel", "Monitor"]);
    }

    #[test]
    fn test_game_hooks_entity_damage_modification_and_cancellation() {
        let mut hooks = GameEventHooks::new();
        let blamer = Blamer::new();

        hooks.register_entity_damage("armor_mod", EventPriority::High, move |e| {
            e.damage *= 0.5; // 50% damage reduction
        });

        hooks.register_entity_damage("invulnerable_mod", EventPriority::Normal, move |e| {
            if e.entity_id == 999 {
                e.cancel();
            }
        });

        let mut normal_damage = EntityDamageEvent::new(1, "mob", 10.0);
        hooks.dispatch_entity_damage(&mut normal_damage, &blamer);
        assert!(!normal_damage.is_cancelled());
        assert_eq!(normal_damage.damage, 5.0);

        let mut cancelled_damage = EntityDamageEvent::new(999, "mob", 10.0);
        hooks.dispatch_entity_damage(&mut cancelled_damage, &blamer);
        assert!(cancelled_damage.is_cancelled());
    }

    #[test]
    fn test_game_hooks_world_tick_dispatch() {
        let mut hooks = GameEventHooks::new();
        let blamer = Blamer::new();
        let tick_record = Arc::new(Mutex::new((0u64, 0.0f32)));

        let tr = tick_record.clone();
        hooks.register_world_tick("stats_mod", EventPriority::Normal, move |e| {
            *tr.lock().unwrap() = (e.tick_count, e.delta_time);
        });

        let mut event = WorldTickEvent::new(500, 0.05);
        hooks.dispatch_world_tick(&mut event, &blamer);

        let (count, dt) = *tick_record.lock().unwrap();
        assert_eq!(count, 500);
        assert!((dt - 0.05).abs() < 1e-6);
    }

    #[test]
    fn test_event_bus_game_events_dispatch_and_cancellation() {
        let bus = EventBus::new();
        let cancelled_seen = Arc::new(Mutex::new(false));
        let cs = cancelled_seen.clone();

        bus.subscribe(EventPriority::High, |e: &mut BlockBreakEvent| {
            if e.player_id.as_str() == "griefer" {
                e.cancel();
            }
        });

        bus.subscribe(EventPriority::Monitor, move |e: &mut BlockBreakEvent| {
            if e.is_cancelled() {
                *cs.lock().unwrap() = true;
            }
        });

        let mut event = BlockBreakEvent::new("griefer", [1, 2, 3]);
        bus.dispatch(&mut event);

        assert!(event.is_cancelled());
        assert!(*cancelled_seen.lock().unwrap());
    }

    #[test]
    fn test_game_hooks_panic_isolation() {
        let mut hooks = GameEventHooks::new();
        let blamer = Blamer::new();
        let monitor_ran = Arc::new(Mutex::new(false));
        let mr = monitor_ran.clone();

        hooks.register_block_break("faulty_mod", EventPriority::Normal, |_e| {
            panic!("Intentional crash in block break hook");
        });

        hooks.register_block_break("safe_monitor", EventPriority::Monitor, move |_e| {
            *mr.lock().unwrap() = true;
        });

        let mut event = BlockBreakEvent::new("steve", [0, 0, 0]);
        hooks.dispatch_block_break(&mut event, &blamer);

        assert!(*monitor_ran.lock().unwrap(), "Monitor must still run despite panic in earlier hook");
        assert_eq!(blamer.panic_count_for_mod("faulty_mod"), 1);
    }

    #[test]
    fn test_central_event_bus_context_integration() {
        let bus = Arc::new(EventBus::new());
        RurgeContext::set_event_bus(bus.clone());

        let retrieved = RurgeContext::event_bus::<EventBus>().expect("Central event bus must be set");
        let interacted = Arc::new(Mutex::new(false));
        let int_clone = interacted.clone();

        retrieved.subscribe(EventPriority::Normal, move |_e: &mut PlayerInteractEvent| {
            *int_clone.lock().unwrap() = true;
        });

        let mut event = PlayerInteractEvent::new("hero", InteractionHand::Main, [10, 20, 30]);
        bus.dispatch(&mut event);

        assert!(*interacted.lock().unwrap(), "Listener registered on central bus must receive event");
        RurgeContext::clear_event_bus();
    }
}
