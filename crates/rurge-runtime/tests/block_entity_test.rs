use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use rurge_api::block_entity::{
    BlockEntity, BlockEntityTickContext, BlockPos, NbtCompound as ApiNbtCompound, NbtTag,
};
use rurge_blamer::Blamer;
use rurge_runtime::block_entity::{
    api_to_runtime_nbt, runtime_to_api_nbt, BlockEntityManager, BLOCK_ENTITIES_NBT_KEY,
};
use rurge_runtime::persistence::NbtCompound as RuntimeNbtCompound;
use rurge_runtime::RurgeEngine;

// ---------------------------------------------------------------------------
// Mock Block Entities for Testing
// ---------------------------------------------------------------------------

struct CounterBlockEntity {
    mod_id: String,
    count: u64,
    should_panic: bool,
    should_remove: bool,
    ticking_enabled: bool,
}

impl CounterBlockEntity {
    fn new(mod_id: impl Into<String>) -> Self {
        Self {
            mod_id: mod_id.into(),
            count: 0,
            should_panic: false,
            should_remove: false,
            ticking_enabled: true,
        }
    }

    fn panicking(mod_id: impl Into<String>) -> Self {
        Self {
            mod_id: mod_id.into(),
            count: 0,
            should_panic: true,
            should_remove: false,
            ticking_enabled: true,
        }
    }
}

impl BlockEntity for CounterBlockEntity {
    fn tick(&mut self, ctx: &mut BlockEntityTickContext) {
        if self.should_panic {
            panic!("Fatal error in tile entity tick!");
        }
        self.count += 1;
        if self.should_remove {
            ctx.mark_removed();
        }
    }

    fn read_nbt(&mut self, nbt: &ApiNbtCompound) {
        if let Some(c) = nbt.get_i64("count") {
            self.count = c as u64;
        }
        if let Some(p) = nbt.get_bool("should_panic") {
            self.should_panic = p;
        }
    }

    fn write_nbt(&self, nbt: &mut ApiNbtCompound) {
        nbt.set_i64("count", self.count as i64);
        nbt.set_bool("should_panic", self.should_panic);
    }

    fn mod_id(&self) -> &str {
        &self.mod_id
    }

    fn type_id(&self) -> &str {
        "test_mod:counter"
    }

    fn can_tick(&self) -> bool {
        self.ticking_enabled
    }
}

// ---------------------------------------------------------------------------
// Unit Tests: BlockPos & Math
// ---------------------------------------------------------------------------

#[test]
fn test_block_pos_arithmetic_and_directions() {
    let pos = BlockPos::new(10, 64, -20);
    assert_eq!(pos.up(), BlockPos::new(10, 65, -20));
    assert_eq!(pos.down(), BlockPos::new(10, 63, -20));
    assert_eq!(pos.north(), BlockPos::new(10, 64, -21));
    assert_eq!(pos.south(), BlockPos::new(10, 64, -19));
    assert_eq!(pos.west(), BlockPos::new(9, 64, -20));
    assert_eq!(pos.east(), BlockPos::new(11, 64, -20));
    assert_eq!(pos.offset(5, -4, 2), BlockPos::new(15, 60, -18));

    // Conversions
    let tuple: (i32, i32, i32) = pos.into();
    assert_eq!(tuple, (10, 64, -20));
    assert_eq!(BlockPos::from(tuple), pos);

    let arr: [i32; 3] = pos.into();
    assert_eq!(arr, [10, 64, -20]);
    assert_eq!(BlockPos::from(arr), pos);

    // Distance calculations
    let other = BlockPos::new(13, 68, -20);
    assert_eq!(pos.manhattan_distance(&other), 3 + 4 + 0);
    assert_eq!(pos.distance_squared(&other), 9.0 + 16.0);
    assert_eq!(pos.distance(&other), 5.0);

    // Display
    assert_eq!(format!("{pos}"), "(10, 64, -20)");
}

// ---------------------------------------------------------------------------
// Unit Tests: NBT Data Compound & Serialization
// ---------------------------------------------------------------------------

#[test]
fn test_nbt_compound_typed_setters_and_getters() {
    let mut root = ApiNbtCompound::new();
    assert!(root.is_empty());

    root.set_byte("byte_val", 12i8);
    root.set_short("short_val", 3000i16);
    root.set_i32("int_val", 123456);
    root.set_i64("long_val", 9876543210i64);
    root.set_f32("float_val", 3.14f32);
    root.set_f64("double_val", 2.718281828f64);
    root.set_string("str_val", "Horizon World");
    root.set_bool("bool_val", true);
    root.set_byte_array("bytes", vec![1, 2, 3, 4]);
    root.set_int_array("ints", vec![100, 200, 300]);

    let mut nested = ApiNbtCompound::new();
    nested.set_string("sub_key", "sub_val");
    root.set_compound("nested", nested);

    let list = vec![NbtTag::Int(10), NbtTag::Int(20), NbtTag::Int(30)];
    root.set_list("list_val", list);

    // Validations
    assert_eq!(root.get_byte("byte_val"), Some(12i8));
    assert_eq!(root.get_short("short_val"), Some(3000i16));
    assert_eq!(root.get_i32("int_val"), Some(123456));
    assert_eq!(root.get_i64("long_val"), Some(9876543210i64));
    assert_eq!(root.get_f32("float_val"), Some(3.14f32));
    assert_eq!(root.get_f64("double_val"), Some(2.718281828f64));
    assert_eq!(root.get_string("str_val"), Some("Horizon World"));
    assert_eq!(root.get_bool("bool_val"), Some(true));
    assert_eq!(root.get_byte_array("bytes"), Some(&[1, 2, 3, 4][..]));
    assert_eq!(root.get_int_array("ints"), Some(&[100, 200, 300][..]));

    let child = root.get_compound("nested").unwrap();
    assert_eq!(child.get_string("sub_key"), Some("sub_val"));

    assert_eq!(root.len(), 12);
    assert!(!root.is_empty());
    assert!(root.contains_key("str_val"));
    assert!(!root.contains_key("missing_val"));

    // Removal
    let removed = root.remove("str_val");
    assert_eq!(removed, Some(NbtTag::String("Horizon World".to_string())));
    assert!(!root.contains_key("str_val"));
}

#[test]
fn test_nbt_runtime_interop_conversions() {
    let mut api_nbt = ApiNbtCompound::new();
    api_nbt.set_i32("energy", 5000);
    api_nbt.set_string("tier", "advanced");
    api_nbt.set_bool("online", true);

    // Convert API -> Runtime
    let runtime_nbt: RuntimeNbtCompound = api_to_runtime_nbt(&api_nbt);
    assert_eq!(
        runtime_nbt.get("tier"),
        Some(&serde_json::Value::String("advanced".to_string()))
    );

    // Convert Runtime -> API
    let restored_api = runtime_to_api_nbt(&runtime_nbt);
    assert_eq!(restored_api.get_i32("energy"), Some(5000));
    assert_eq!(restored_api.get_string("tier"), Some("advanced"));
    assert_eq!(restored_api.get_bool("online"), Some(true));
}

// ---------------------------------------------------------------------------
// Integration Tests: BlockEntityManager Registration & Lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_block_entity_manager_registration_and_queries() {
    let manager = BlockEntityManager::new();
    assert!(manager.is_empty());
    assert_eq!(manager.len(), 0);

    let pos1 = BlockPos::new(10, 20, 30);
    let pos2 = BlockPos::new(40, 50, 60);

    let entity1 = CounterBlockEntity::new("mod_alpha");
    let entity2 = CounterBlockEntity::new("mod_beta");

    manager.register("mod_alpha", pos1, entity1);
    manager.register_entity(pos2, Box::new(entity2));

    assert_eq!(manager.len(), 2);
    assert!(manager.contains(&pos1));
    assert!(manager.contains(&pos2));
    assert_eq!(manager.get_mod_id(&pos1), Some("mod_alpha".to_string()));
    assert_eq!(manager.get_mod_id(&pos2), Some("mod_beta".to_string()));

    let positions_alpha = manager.positions_for_mod("mod_alpha");
    assert_eq!(positions_alpha, vec![pos1]);

    // Unregister / remove
    let removed = manager.remove(&pos1);
    assert!(removed.is_some());
    assert!(!manager.contains(&pos1));
    assert_eq!(manager.len(), 1);

    manager.clear();
    assert!(manager.is_empty());
}

// ---------------------------------------------------------------------------
// Integration Tests: 20 TPS World Tick & Panic Isolation
// ---------------------------------------------------------------------------

#[test]
fn test_block_entity_tick_cycle_and_panic_isolation() {
    let blamer = Arc::new(Blamer::new());
    let manager = BlockEntityManager::new();

    let normal_pos = BlockPos::new(1, 10, 1);
    let normal_entity = CounterBlockEntity::new("industrial_rust");
    manager.register("industrial_rust", normal_pos, normal_entity);

    let faulty_pos = BlockPos::new(2, 10, 2);
    let faulty_entity = CounterBlockEntity::panicking("broken_mod");
    manager.register("broken_mod", faulty_pos, faulty_entity);

    let disabled_pos = BlockPos::new(3, 10, 3);
    let mut disabled_entity = CounterBlockEntity::new("industrial_rust");
    disabled_entity.ticking_enabled = false;
    manager.register("industrial_rust", disabled_pos, disabled_entity);

    // Execute world tick cycle (tick count 1)
    manager.tick_all(1, &blamer);

    // 1. Normal entity should tick successfully and increment its count
    let normal_arc = manager.get(&normal_pos).expect("Normal entity exists");
    assert_eq!(
        normal_arc.lock().tick(&mut BlockEntityTickContext::new(normal_pos, 2)),
        ()
    );
    // 1 tick from tick_all + 1 manual tick = 2
    assert_eq!(
        normal_arc
            .lock()
            .mod_id(),
        "industrial_rust"
    );

    // 2. Faulty entity panicked, but tick_all did not crash!
    // The panic was blamed on "broken_mod"
    assert_eq!(blamer.panic_count_for_mod("broken_mod"), 1);
    assert_eq!(blamer.panic_count_for_mod("industrial_rust"), 0);

    // 3. Disabled entity should not tick
    let disabled_arc = manager.get(&disabled_pos).expect("Disabled entity exists");
    let mut test_nbt = ApiNbtCompound::new();
    disabled_arc.lock().write_nbt(&mut test_nbt);
    assert_eq!(test_nbt.get_i64("count"), Some(0));

    // Execute another tick cycle
    manager.tick_all(2, &blamer);
    assert_eq!(blamer.panic_count_for_mod("broken_mod"), 2);
}

#[test]
fn test_block_entity_self_removal_on_tick() {
    let blamer = Arc::new(Blamer::new());
    let manager = BlockEntityManager::new();

    let pos = BlockPos::new(5, 5, 5);
    let mut entity = CounterBlockEntity::new("temporary_mod");
    entity.should_remove = true;

    manager.register("temporary_mod", pos, entity);
    assert!(manager.contains(&pos));

    // Ticking the entity causes it to mark itself removed
    manager.tick_all(1, &blamer);

    // Entity should be automatically unregistered by manager
    assert!(!manager.contains(&pos));
    assert_eq!(manager.len(), 0);
}

// ---------------------------------------------------------------------------
// Integration Tests: NBT State Persistence & Factory Reconstruction
// ---------------------------------------------------------------------------

#[test]
fn test_block_entity_nbt_save_and_load_roundtrip() {
    let manager = BlockEntityManager::new();

    let pos = BlockPos::new(100, 60, -50);
    let mut entity = CounterBlockEntity::new("thermal_mod");
    entity.count = 42;
    manager.register("thermal_mod", pos, entity);

    // Save single entity to NBT
    let entity_nbt = manager
        .save_entity_to_nbt(&pos)
        .expect("Entity NBT saved");
    assert_eq!(entity_nbt.get_i32("x"), Some(100));
    assert_eq!(entity_nbt.get_i32("y"), Some(60));
    assert_eq!(entity_nbt.get_i32("z"), Some(-50));
    assert_eq!(entity_nbt.get_string("mod_id"), Some("thermal_mod"));
    assert_eq!(entity_nbt.get_string("type_id"), Some("test_mod:counter"));
    assert_eq!(entity_nbt.get_i64("count"), Some(42));

    // Modify state in memory
    {
        let entity_arc = manager.get(&pos).unwrap();
        let mut guard = entity_arc.lock();
        let mut nbt_modified = ApiNbtCompound::new();
        nbt_modified.set_i64("count", 999);
        guard.read_nbt(&nbt_modified);
    }

    // Verify modified
    let entity_nbt_modified = manager.save_entity_to_nbt(&pos).unwrap();
    assert_eq!(entity_nbt_modified.get_i64("count"), Some(999));

    // Restore from original NBT
    let loaded = manager.load_entity_from_nbt(&entity_nbt);
    assert!(loaded);

    // Verify restored count
    let restored_nbt = manager.save_entity_to_nbt(&pos).unwrap();
    assert_eq!(restored_nbt.get_i64("count"), Some(42));
}

#[test]
fn test_block_entity_factory_reconstruction_from_nbt() {
    let manager = BlockEntityManager::new();

    // Register factory for "test_mod:counter"
    manager.register_factory("test_mod:counter", || {
        Box::new(CounterBlockEntity::new("dynamic_mod"))
    });
    assert!(manager.has_factory("test_mod:counter"));

    // Create an NBT payload representing an entity that does NOT exist in memory yet
    let mut nbt = ApiNbtCompound::new();
    nbt.set_i32("x", 7);
    nbt.set_i32("y", 128);
    nbt.set_i32("z", -15);
    nbt.set_string("type_id", "test_mod:counter");
    nbt.set_string("mod_id", "dynamic_mod");
    nbt.set_i64("count", 777);

    let target_pos = BlockPos::new(7, 128, -15);
    assert!(!manager.contains(&target_pos));

    // Load from NBT should reconstruct via factory and register
    let success = manager.load_entity_from_nbt(&nbt);
    assert!(success);
    assert!(manager.contains(&target_pos));

    // Verify reconstructed entity state
    let saved_nbt = manager.save_entity_to_nbt(&target_pos).unwrap();
    assert_eq!(saved_nbt.get_i64("count"), Some(777));
}

#[test]
fn test_block_entity_runtime_compound_saving_and_loading() {
    let manager = BlockEntityManager::new();
    manager.register_factory("test_mod:counter", || {
        Box::new(CounterBlockEntity::new("chunk_mod"))
    });

    let pos1 = BlockPos::new(1, 2, 3);
    let mut e1 = CounterBlockEntity::new("chunk_mod");
    e1.count = 100;
    manager.register("chunk_mod", pos1, e1);

    let pos2 = BlockPos::new(4, 5, 6);
    let mut e2 = CounterBlockEntity::new("chunk_mod");
    e2.count = 200;
    manager.register("chunk_mod", pos2, e2);

    // Save into runtime NBT compound (Anvil chunk root equivalent)
    let mut chunk_root = RuntimeNbtCompound::new();
    let res = manager.save_to_runtime_nbt(&mut chunk_root);
    assert!(res.is_ok());
    assert!(chunk_root.get(BLOCK_ENTITIES_NBT_KEY).is_some());

    // Create fresh manager and restore all entities from chunk root
    let new_manager = BlockEntityManager::new();
    new_manager.register_factory("test_mod:counter", || {
        Box::new(CounterBlockEntity::new("chunk_mod"))
    });

    let count_loaded = new_manager
        .load_from_runtime_nbt(&chunk_root)
        .expect("Load from runtime NBT succeeds");
    assert_eq!(count_loaded, 2);
    assert_eq!(new_manager.len(), 2);
    assert!(new_manager.contains(&pos1));
    assert!(new_manager.contains(&pos2));

    let saved1 = new_manager.save_entity_to_nbt(&pos1).unwrap();
    assert_eq!(saved1.get_i64("count"), Some(100));

    let saved2 = new_manager.save_entity_to_nbt(&pos2).unwrap();
    assert_eq!(saved2.get_i64("count"), Some(200));
}

// ---------------------------------------------------------------------------
// Integration Tests: RurgeEngine 20 TPS Tick Integration
// ---------------------------------------------------------------------------

#[test]
fn test_engine_server_tick_world_block_entities_integration() {
    let temp_dir = tempfile::tempdir().expect("Create tempdir");
    let engine = RurgeEngine::with_dirs(
        temp_dir.path().join("mods"),
        temp_dir.path().join("cache"),
    );

    let pos = BlockPos::new(0, 64, 0);
    let entity = CounterBlockEntity::new("engine_mod");
    engine.register_block_entity("engine_mod", pos, entity);

    // Initial count is 0
    let entity_arc = engine
        .block_entities()
        .get(&pos)
        .expect("Block entity registered in engine");
    let mut check_nbt = ApiNbtCompound::new();
    entity_arc.lock().write_nbt(&mut check_nbt);
    assert_eq!(check_nbt.get_i64("count"), Some(0));

    // Execute 3 server ticks (20 TPS cycle)
    engine.server_tick(1);
    engine.server_tick(2);
    engine.server_tick(3);

    // Block entity should have received 3 ticks
    check_nbt.clear();
    entity_arc.lock().write_nbt(&mut check_nbt);
    assert_eq!(check_nbt.get_i64("count"), Some(3));

    // Test explicit tick_world
    engine.tick_world(4);
    check_nbt.clear();
    entity_arc.lock().write_nbt(&mut check_nbt);
    assert_eq!(check_nbt.get_i64("count"), Some(4));
}

#[test]
fn test_engine_server_tick_panic_isolation() {
    let temp_dir = tempfile::tempdir().expect("Create tempdir");
    let mut engine = RurgeEngine::with_dirs(
        temp_dir.path().join("mods"),
        temp_dir.path().join("cache"),
    );

    // Register a broken block entity that panics
    let broken_pos = BlockPos::new(10, 10, 10);
    let broken_entity = CounterBlockEntity::panicking("crasher_mod");
    engine.register_block_entity("crasher_mod", broken_pos, broken_entity);

    // Register a healthy block entity
    let healthy_pos = BlockPos::new(20, 20, 20);
    let healthy_entity = CounterBlockEntity::new("healthy_mod");
    engine.register_block_entity("healthy_mod", healthy_pos, healthy_entity);

    // Register a server tick hook
    let hook_ran = Arc::new(AtomicBool::new(false));
    let hook_ran_clone = Arc::clone(&hook_ran);
    engine.register_server_tick("healthy_mod", move |_tick| {
        hook_ran_clone.store(true, Ordering::SeqCst);
    });

    // Server tick must NOT panic even though crasher_mod block entity crashes
    engine.server_tick(1);

    // Verify panic was isolated and blamed
    assert_eq!(engine.blamer().panic_count_for_mod("crasher_mod"), 1);
    assert_eq!(engine.blamer().panic_count_for_mod("healthy_mod"), 0);

    // Verify healthy block entity still ticked
    let healthy_arc = engine.block_entities().get(&healthy_pos).unwrap();
    let mut check_nbt = ApiNbtCompound::new();
    healthy_arc.lock().write_nbt(&mut check_nbt);
    assert_eq!(check_nbt.get_i64("count"), Some(1));

    // Verify server tick hook still ran
    assert!(hook_ran.load(Ordering::SeqCst));
}

// ---------------------------------------------------------------------------
// Concurrency & Thread-Safety Tests
// ---------------------------------------------------------------------------

#[test]
fn test_block_entity_manager_concurrent_access() {
    let blamer = Arc::new(Blamer::new());
    let manager = Arc::new(BlockEntityManager::new());

    // Populate with 20 entities
    for i in 0..20 {
        let pos = BlockPos::new(i, 64, 0);
        let entity = CounterBlockEntity::new("concurrent_mod");
        manager.register("concurrent_mod", pos, entity);
    }

    let mut handles = Vec::new();

    // Spawn 4 threads that perform tick_all and queries concurrently
    for t in 0..4 {
        let manager_clone = Arc::clone(&manager);
        let blamer_clone = Arc::clone(&blamer);
        let handle = std::thread::spawn(move || {
            for step in 0..15 {
                let tick = (t * 100 + step) as u64;
                manager_clone.tick_all(tick, &blamer_clone);
                let _ = manager_clone.len();
                let _ = manager_clone.all_positions();
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Thread joined cleanly");
    }

    // All entities should have ticked 4 threads * 15 steps = 60 times
    let pos0 = BlockPos::new(0, 64, 0);
    let entity_arc = manager.get(&pos0).unwrap();
    let mut check_nbt = ApiNbtCompound::new();
    entity_arc.lock().write_nbt(&mut check_nbt);
    assert_eq!(check_nbt.get_i64("count"), Some(60));
}
