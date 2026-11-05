use std::sync::Arc;
use abi_stable::{
    abi_stability::check_layout_compatibility,
    StableAbi,
};
use rurge_api::events::{
    BlockBreakEvent, CancellableEvent, EntityDamageEvent, Hand, InteractionHand,
    PlayerInteractEvent, WorldTickEvent,
};
use rurge_api::RurgeContext;

#[test]
fn test_game_events_abi_stability() {
    assert!(
        check_layout_compatibility(
            <BlockBreakEvent as StableAbi>::LAYOUT,
            <BlockBreakEvent as StableAbi>::LAYOUT
        )
        .is_ok(),
        "BlockBreakEvent layout must be ABI compatible with itself"
    );

    assert!(
        check_layout_compatibility(
            <PlayerInteractEvent as StableAbi>::LAYOUT,
            <PlayerInteractEvent as StableAbi>::LAYOUT
        )
        .is_ok(),
        "PlayerInteractEvent layout must be ABI compatible with itself"
    );

    assert!(
        check_layout_compatibility(
            <InteractionHand as StableAbi>::LAYOUT,
            <InteractionHand as StableAbi>::LAYOUT
        )
        .is_ok(),
        "InteractionHand layout must be ABI compatible with itself"
    );

    assert!(
        check_layout_compatibility(
            <EntityDamageEvent as StableAbi>::LAYOUT,
            <EntityDamageEvent as StableAbi>::LAYOUT
        )
        .is_ok(),
        "EntityDamageEvent layout must be ABI compatible with itself"
    );

    assert!(
        check_layout_compatibility(
            <WorldTickEvent as StableAbi>::LAYOUT,
            <WorldTickEvent as StableAbi>::LAYOUT
        )
        .is_ok(),
        "WorldTickEvent layout must be ABI compatible with itself"
    );
}

#[test]
fn test_block_break_event_properties_and_cancellation() {
    let mut event = BlockBreakEvent::new("player_test", [10, 64, -20]);
    assert_eq!(event.player_id.as_str(), "player_test");
    assert_eq!(event.pos, [10, 64, -20]);
    assert!(!event.is_cancelled());

    event.cancel();
    assert!(event.is_cancelled());
}

#[test]
fn test_player_interact_event_properties_and_cancellation() {
    let mut event = PlayerInteractEvent::new("alex", InteractionHand::Off, [5, 12, 100]);
    assert_eq!(event.player_id.as_str(), "alex");
    assert_eq!(event.hand, Hand::Off);
    assert_eq!(event.target_pos, [5, 12, 100]);
    assert!(!event.is_cancelled());

    event.cancel();
    assert!(event.is_cancelled());
}

#[test]
fn test_entity_damage_event_properties_and_cancellation() {
    let mut event = EntityDamageEvent::new(42, "minecraft:fall", 5.0);
    assert_eq!(event.entity_id, 42);
    assert_eq!(event.source.as_str(), "minecraft:fall");
    assert_eq!(event.damage, 5.0);
    assert!(!event.is_cancelled());

    event.cancel();
    assert!(event.is_cancelled());
}

#[test]
fn test_world_tick_event_properties() {
    let event = WorldTickEvent::new(1200, 0.05);
    assert_eq!(event.tick_count, 1200);
    assert!((event.delta_time - 0.05).abs() < 1e-6);
}

#[test]
fn test_cancellable_trait_polymorphism() {
    fn cancel_event(e: &mut dyn CancellableEvent) {
        e.cancel();
    }

    let mut b_event = BlockBreakEvent::new("steve", [0, 0, 0]);
    let mut p_event = PlayerInteractEvent::new("steve", InteractionHand::Main, [1, 1, 1]);
    let mut d_event = EntityDamageEvent::new(1, "arrow", 4.0);

    cancel_event(&mut b_event);
    cancel_event(&mut p_event);
    cancel_event(&mut d_event);

    assert!(b_event.is_cancelled());
    assert!(p_event.is_cancelled());
    assert!(d_event.is_cancelled());
}

#[test]
fn test_rurge_context_central_event_bus_registry() {
    #[derive(Debug, PartialEq, Eq)]
    struct DummyBus(u32);

    RurgeContext::clear_event_bus();
    assert!(RurgeContext::event_bus::<DummyBus>().is_none());

    let bus = Arc::new(DummyBus(42));
    RurgeContext::set_event_bus(bus.clone());

    let retrieved = RurgeContext::event_bus::<DummyBus>().expect("Bus must be present");
    assert_eq!(*retrieved, DummyBus(42));

    let ctx = RurgeContext::new("0.2.0", "26.3", "mods", "config");
    let ctx_bus = ctx.get_event_bus::<DummyBus>().expect("Bus from ctx instance");
    assert_eq!(*ctx_bus, DummyBus(42));

    RurgeContext::clear_event_bus();
    assert!(RurgeContext::event_bus::<DummyBus>().is_none());
}

#[test]
fn test_game_events_serialization_roundtrip() {
    let event = BlockBreakEvent::new("player_1", [10, 20, 30]);
    let json = serde_json::to_string(&event).expect("Serialization failed");
    let deserialized: BlockBreakEvent = serde_json::from_str(&json).expect("Deserialization failed");
    assert_eq!(event, deserialized);

    let p_event = PlayerInteractEvent::new("player_2", InteractionHand::Main, [-1, 5, 99]);
    let p_json = serde_json::to_string(&p_event).expect("Serialization failed");
    let p_deserialized: PlayerInteractEvent = serde_json::from_str(&p_json).expect("Deserialization failed");
    assert_eq!(p_event, p_deserialized);

    let d_event = EntityDamageEvent::new(100, "magic", 7.5);
    let d_json = serde_json::to_string(&d_event).expect("Serialization failed");
    let d_deserialized: EntityDamageEvent = serde_json::from_str(&d_json).expect("Deserialization failed");
    assert_eq!(d_event, d_deserialized);

    let w_event = WorldTickEvent::new(24000, 0.05);
    let w_json = serde_json::to_string(&w_event).expect("Serialization failed");
    let w_deserialized: WorldTickEvent = serde_json::from_str(&w_json).expect("Deserialization failed");
    assert_eq!(w_event, w_deserialized);
}
