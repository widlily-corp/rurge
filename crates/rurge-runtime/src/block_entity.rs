use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::{Mutex, RwLock};
use rurge_api::block_entity::{
    BlockEntity, BlockEntityTickContext, BlockPos, NbtCompound as ApiNbtCompound, NbtTag,
};
use rurge_blamer::{safe_invoke, Blamer};

use crate::persistence::{NbtCompound as RuntimeNbtCompound, PersistenceError};

/// Tag name used when storing block entities inside runtime / chunk NBT compounds.
pub const BLOCK_ENTITIES_NBT_KEY: &str = "rurge:block_entities";

/// Re-export API types for runtime consumers.
pub use rurge_api::block_entity::{
    BlockEntity as BlockEntityTrait, BlockEntityTickContext as TickContext, BlockPos as BlockPosition,
};

/// An entry representing a registered block entity in the manager.
#[derive(Clone)]
pub struct BlockEntityEntry {
    /// ID of the mod that owns this block entity.
    pub mod_id: String,
    /// Thread-safe interior-mutable instance of the block entity.
    pub entity: Arc<Mutex<Box<dyn BlockEntity>>>,
}

/// Factory function to reconstruct a block entity instance from its type ID during NBT deserialization.
pub type BlockEntityFactory = Arc<dyn Fn() -> Box<dyn BlockEntity> + Send + Sync + 'static>;

/// Converts an API [`NbtTag`] into a [`serde_json::Value`].
pub fn nbt_tag_to_json(tag: &NbtTag) -> serde_json::Value {
    match tag {
        NbtTag::Byte(b) => serde_json::Value::Number((*b).into()),
        NbtTag::Short(s) => serde_json::Value::Number((*s).into()),
        NbtTag::Int(i) => serde_json::Value::Number((*i).into()),
        NbtTag::Long(l) => serde_json::Value::Number((*l).into()),
        NbtTag::Float(f) => serde_json::Number::from_f64(*f as f64)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        NbtTag::Double(d) => serde_json::Number::from_f64(*d)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        NbtTag::String(s) => serde_json::Value::String(s.clone()),
        NbtTag::Boolean(b) => serde_json::Value::Bool(*b),
        NbtTag::List(list) => {
            serde_json::Value::Array(list.iter().map(nbt_tag_to_json).collect())
        }
        NbtTag::Compound(compound) => {
            let mut map = serde_json::Map::new();
            for (k, v) in &compound.values {
                map.insert(k.clone(), nbt_tag_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        NbtTag::ByteArray(bytes) => {
            serde_json::Value::Array(bytes.iter().map(|b| serde_json::Value::Number((*b).into())).collect())
        }
        NbtTag::IntArray(ints) => {
            serde_json::Value::Array(ints.iter().map(|i| serde_json::Value::Number((*i).into())).collect())
        }
        NbtTag::LongArray(longs) => {
            serde_json::Value::Array(longs.iter().map(|l| serde_json::Value::Number((*l).into())).collect())
        }
    }
}

/// Converts a [`serde_json::Value`] into an API [`NbtTag`].
pub fn json_to_nbt_tag(val: &serde_json::Value) -> NbtTag {
    match val {
        serde_json::Value::Null => NbtTag::Compound(ApiNbtCompound::new()),
        serde_json::Value::Bool(b) => NbtTag::Boolean(*b),
        serde_json::Value::Number(num) => {
            if let Some(i) = num.as_i64() {
                if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                    NbtTag::Int(i as i32)
                } else {
                    NbtTag::Long(i)
                }
            } else if let Some(f) = num.as_f64() {
                NbtTag::Double(f)
            } else {
                NbtTag::Int(0)
            }
        }
        serde_json::Value::String(s) => NbtTag::String(s.clone()),
        serde_json::Value::Array(arr) => {
            NbtTag::List(arr.iter().map(json_to_nbt_tag).collect())
        }
        serde_json::Value::Object(map) => {
            let mut compound = ApiNbtCompound::new();
            for (k, v) in map {
                compound.insert(k.clone(), json_to_nbt_tag(v));
            }
            NbtTag::Compound(compound)
        }
    }
}

/// Converts an API [`ApiNbtCompound`] to a runtime [`RuntimeNbtCompound`].
pub fn api_to_runtime_nbt(api: &ApiNbtCompound) -> RuntimeNbtCompound {
    let mut runtime = RuntimeNbtCompound::new();
    for (k, v) in &api.values {
        runtime.insert(k.clone(), nbt_tag_to_json(v));
    }
    runtime
}

/// Converts a runtime [`RuntimeNbtCompound`] to an API [`ApiNbtCompound`].
pub fn runtime_to_api_nbt(runtime: &RuntimeNbtCompound) -> ApiNbtCompound {
    let mut api = ApiNbtCompound::new();
    for (k, v) in &runtime.values {
        api.insert(k.clone(), json_to_nbt_tag(v));
    }
    api
}

impl From<ApiNbtCompound> for RuntimeNbtCompound {
    fn from(api: ApiNbtCompound) -> Self {
        api_to_runtime_nbt(&api)
    }
}

impl From<&ApiNbtCompound> for RuntimeNbtCompound {
    fn from(api: &ApiNbtCompound) -> Self {
        api_to_runtime_nbt(api)
    }
}

/// Thread-safe manager responsible for block entity registration, NBT persistence,
/// and 20 TPS tick execution with panic isolation via [`safe_invoke`].
#[derive(Default)]
pub struct BlockEntityManager {
    tiles: RwLock<HashMap<BlockPos, BlockEntityEntry>>,
    factories: RwLock<HashMap<String, BlockEntityFactory>>,
}

impl BlockEntityManager {
    /// Creates a new empty `BlockEntityManager`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new block entity at the given world coordinate, attributed to `mod_id`.
    pub fn register<T: BlockEntity + 'static>(
        &self,
        mod_id: impl Into<String>,
        pos: BlockPos,
        entity: T,
    ) {
        self.register_boxed(mod_id, pos, Box::new(entity));
    }

    /// Registers a boxed block entity at the given coordinate, attributed to `mod_id`.
    pub fn register_boxed(
        &self,
        mod_id: impl Into<String>,
        pos: BlockPos,
        entity: Box<dyn BlockEntity>,
    ) {
        let entry = BlockEntityEntry {
            mod_id: mod_id.into(),
            entity: Arc::new(Mutex::new(entity)),
        };
        self.tiles.write().insert(pos, entry);
    }

    /// Registers a block entity using its own `mod_id()` method for attribution.
    pub fn register_entity(&self, pos: BlockPos, entity: Box<dyn BlockEntity>) {
        let mod_id = entity.mod_id().to_string();
        self.register_boxed(mod_id, pos, entity);
    }

    /// Unregisters and returns the block entity at `pos`, if present.
    pub fn unregister(&self, pos: &BlockPos) -> Option<Arc<Mutex<Box<dyn BlockEntity>>>> {
        self.tiles.write().remove(pos).map(|entry| entry.entity)
    }

    /// Removes the block entity at `pos`, equivalent to [`unregister`](BlockEntityManager::unregister).
    pub fn remove(&self, pos: &BlockPos) -> Option<Arc<Mutex<Box<dyn BlockEntity>>>> {
        self.unregister(pos)
    }

    /// Retrieves a shared, thread-safe reference to the block entity at `pos`.
    pub fn get(&self, pos: &BlockPos) -> Option<Arc<Mutex<Box<dyn BlockEntity>>>> {
        self.tiles.read().get(pos).map(|entry| Arc::clone(&entry.entity))
    }

    /// Retrieves the mod ID owning the block entity at `pos`.
    pub fn get_mod_id(&self, pos: &BlockPos) -> Option<String> {
        self.tiles.read().get(pos).map(|entry| entry.mod_id.clone())
    }

    /// Returns whether a block entity is registered at `pos`.
    pub fn contains(&self, pos: &BlockPos) -> bool {
        self.tiles.read().contains_key(pos)
    }

    /// Returns the number of currently registered block entities.
    pub fn len(&self) -> usize {
        self.tiles.read().len()
    }

    /// Returns whether there are no block entities registered.
    pub fn is_empty(&self) -> bool {
        self.tiles.read().is_empty()
    }

    /// Clears all registered block entities.
    pub fn clear(&self) {
        self.tiles.write().clear();
    }

    /// Returns a list of all coordinates with registered block entities.
    pub fn all_positions(&self) -> Vec<BlockPos> {
        self.tiles.read().keys().copied().collect()
    }

    /// Returns all positions registered under the specified `mod_id`.
    pub fn positions_for_mod(&self, mod_id: &str) -> Vec<BlockPos> {
        self.tiles
            .read()
            .iter()
            .filter(|(_, entry)| entry.mod_id == mod_id)
            .map(|(pos, _)| *pos)
            .collect()
    }

    /// Registers a factory function used to reconstruct block entities of `type_id` during NBT loading.
    pub fn register_factory<F>(&self, type_id: impl Into<String>, factory: F)
    where
        F: Fn() -> Box<dyn BlockEntity> + Send + Sync + 'static,
    {
        self.factories.write().insert(type_id.into(), Arc::new(factory));
    }

    /// Checks if a factory function is registered for the specified `type_id`.
    pub fn has_factory(&self, type_id: &str) -> bool {
        self.factories.read().contains_key(type_id)
    }

    /// Instantiates a new block entity using the registered factory for `type_id`.
    pub fn create_entity(&self, type_id: &str) -> Option<Box<dyn BlockEntity>> {
        self.factories.read().get(type_id).map(|f| f())
    }

    /// Executes a single world tick cycle across all active registered block entities.
    ///
    /// Each block entity's `tick()` method is executed inside [`safe_invoke`] to catch and blame
    /// any panics to the respective mod without crashing the game loop.
    pub fn tick_all(&self, tick_count: u64, blamer: &Blamer) {
        // 1. Take a snapshot of candidate entities to tick without holding the map lock.
        let snapshot: Vec<(BlockPos, String, Arc<Mutex<Box<dyn BlockEntity>>>)> = {
            let tiles = self.tiles.read();
            tiles
                .iter()
                .filter_map(|(pos, entry)| {
                    let guard = entry.entity.lock();
                    if guard.can_tick() {
                        Some((*pos, entry.mod_id.clone(), Arc::clone(&entry.entity)))
                    } else {
                        None
                    }
                })
                .collect()
        };

        let mut to_remove = Vec::new();

        // 2. Iterate snapshot and tick each entity inside safe_invoke panic boundaries.
        for (pos, mod_id, entity_arc) in snapshot {
            // Verify entity was not uninstalled or removed by an earlier tick in this batch.
            if !self.contains(&pos) {
                continue;
            }

            let mut ctx = BlockEntityTickContext::new(pos, tick_count);
            let entity_clone = Arc::clone(&entity_arc);

            let res = safe_invoke(
                &mod_id,
                "block_entity_tick",
                blamer,
                std::panic::AssertUnwindSafe(|| {
                    let mut entity = entity_clone.lock();
                    entity.tick(&mut ctx);
                }),
            );

            if let Err(panic_err) = res {
                log::warn!(
                    "Panic caught in block entity at {pos} belonging to mod '{}' during tick {tick_count}: {}",
                    panic_err.mod_id,
                    panic_err.message
                );
            }

            if ctx.is_removed {
                to_remove.push(pos);
            }
        }

        // 3. Remove entities that marked themselves as removed during ticking.
        if !to_remove.is_empty() {
            let mut tiles = self.tiles.write();
            for pos in to_remove {
                tiles.remove(&pos);
            }
        }
    }

    /// Ticks a single block entity at `pos` within safe panic boundaries.
    ///
    /// Returns `true` if the entity existed and executed without panic, `false` otherwise.
    pub fn tick_at(&self, pos: &BlockPos, tick_count: u64, blamer: &Blamer) -> bool {
        let entry_opt = self.tiles.read().get(pos).cloned();
        if let Some(entry) = entry_opt {
            let mut ctx = BlockEntityTickContext::new(*pos, tick_count);
            let entity_clone = Arc::clone(&entry.entity);

            let res = safe_invoke(
                &entry.mod_id,
                "block_entity_tick",
                blamer,
                std::panic::AssertUnwindSafe(|| {
                    let mut entity = entity_clone.lock();
                    entity.tick(&mut ctx);
                }),
            );

            if ctx.is_removed {
                self.tiles.write().remove(pos);
            }

            res.is_ok()
        } else {
            false
        }
    }

    /// Serializes the state of a single block entity at `pos` into an NBT compound.
    ///
    /// Automatically attaches `"x"`, `"y"`, `"z"`, `"mod_id"`, and `"type_id"` metadata tags.
    pub fn save_entity_to_nbt(&self, pos: &BlockPos) -> Option<ApiNbtCompound> {
        let entry_opt = self.tiles.read().get(pos).cloned();
        entry_opt.map(|entry| {
            let mut compound = ApiNbtCompound::new();
            let guard = entry.entity.lock();
            guard.write_nbt(&mut compound);

            compound.set_i32("x", pos.x);
            compound.set_i32("y", pos.y);
            compound.set_i32("z", pos.z);
            compound.set_string("mod_id", &entry.mod_id);
            compound.set_string("type_id", guard.type_id());

            compound
        })
    }

    /// Serializes all currently registered block entities into an API NBT compound.
    ///
    /// The compound contains a `"block_entities"` list containing each serialized tile entity.
    pub fn save_all_to_nbt(&self) -> ApiNbtCompound {
        let positions = self.all_positions();
        let mut list = Vec::with_capacity(positions.len());

        for pos in positions {
            if let Some(entity_nbt) = self.save_entity_to_nbt(&pos) {
                list.push(NbtTag::Compound(entity_nbt));
            }
        }

        let mut root = ApiNbtCompound::new();
        root.set_list("block_entities", list);
        root
    }

    /// Restores or updates a block entity from an NBT compound.
    ///
    /// If an entity already exists at `(x, y, z)`, its `read_nbt` method is called.
    /// If not, and a factory is registered for `"type_id"`, a new entity is created and registered.
    ///
    /// Returns `true` if the entity was successfully updated or created.
    pub fn load_entity_from_nbt(&self, nbt: &ApiNbtCompound) -> bool {
        let x = match nbt.get_i32("x") {
            Some(x) => x,
            None => return false,
        };
        let y = match nbt.get_i32("y") {
            Some(y) => y,
            None => return false,
        };
        let z = match nbt.get_i32("z") {
            Some(z) => z,
            None => return false,
        };
        let pos = BlockPos::new(x, y, z);

        // Check if an entity already exists at this coordinate
        if let Some(entity_arc) = self.get(&pos) {
            let mut guard = entity_arc.lock();
            guard.read_nbt(nbt);
            return true;
        }

        // Otherwise, attempt reconstruction via registered factory
        if let Some(type_id) = nbt.get_string("type_id") {
            if let Some(mut entity) = self.create_entity(type_id) {
                entity.read_nbt(nbt);
                let mod_id = nbt
                    .get_string("mod_id")
                    .unwrap_or_else(|| entity.mod_id())
                    .to_string();
                self.register_boxed(mod_id, pos, entity);
                return true;
            }
        }

        false
    }

    /// Loads multiple block entities from an NBT compound containing a `"block_entities"` list.
    ///
    /// Returns the count of successfully loaded or updated entities.
    pub fn load_all_from_nbt(&self, root: &ApiNbtCompound) -> usize {
        let mut loaded = 0;
        if let Some(list) = root.get_list("block_entities") {
            for tag in list {
                if let NbtTag::Compound(compound) = tag {
                    if self.load_entity_from_nbt(compound) {
                        loaded += 1;
                    }
                }
            }
        }
        loaded
    }

    /// Saves all block entities into the runtime NBT compound under the [`BLOCK_ENTITIES_NBT_KEY`] tag.
    pub fn save_to_runtime_nbt(
        &self,
        root: &mut RuntimeNbtCompound,
    ) -> Result<(), PersistenceError> {
        let api_root = self.save_all_to_nbt();
        let json_val = nbt_tag_to_json(&NbtTag::Compound(api_root));
        root.insert(BLOCK_ENTITIES_NBT_KEY, json_val);
        Ok(())
    }

    /// Deserializes and loads block entities from the runtime NBT compound's [`BLOCK_ENTITIES_NBT_KEY`] tag.
    pub fn load_from_runtime_nbt(
        &self,
        root: &RuntimeNbtCompound,
    ) -> Result<usize, PersistenceError> {
        if let Some(json_val) = root.get(BLOCK_ENTITIES_NBT_KEY) {
            if let NbtTag::Compound(api_root) = json_to_nbt_tag(json_val) {
                return Ok(self.load_all_from_nbt(&api_root));
            }
        }
        Ok(0)
    }
}
