use std::collections::BTreeMap;
use std::fmt;
use serde::{Deserialize, Serialize};

/// 3D integer coordinate representing a block position in the world.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BlockPos {
    /// X coordinate (East-West axis).
    pub x: i32,
    /// Y coordinate (Up-Down axis).
    pub y: i32,
    /// Z coordinate (South-North axis).
    pub z: i32,
}

impl BlockPos {
    /// Origin coordinate (0, 0, 0).
    pub const ZERO: Self = Self::new(0, 0, 0);

    /// Creates a new `BlockPos` with the given coordinates.
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Returns the position offset by 1 block along the +Y axis.
    pub const fn up(self) -> Self {
        Self::new(self.x, self.y + 1, self.z)
    }

    /// Returns the position offset by 1 block along the -Y axis.
    pub const fn down(self) -> Self {
        Self::new(self.x, self.y - 1, self.z)
    }

    /// Returns the position offset by 1 block North (-Z axis).
    pub const fn north(self) -> Self {
        Self::new(self.x, self.y, self.z - 1)
    }

    /// Returns the position offset by 1 block South (+Z axis).
    pub const fn south(self) -> Self {
        Self::new(self.x, self.y, self.z + 1)
    }

    /// Returns the position offset by 1 block West (-X axis).
    pub const fn west(self) -> Self {
        Self::new(self.x - 1, self.y, self.z)
    }

    /// Returns the position offset by 1 block East (+X axis).
    pub const fn east(self) -> Self {
        Self::new(self.x + 1, self.y, self.z)
    }

    /// Returns the position offset by the specified delta coordinates.
    pub const fn offset(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }

    /// Computes the Manhattan distance between two block positions.
    pub fn manhattan_distance(&self, other: &Self) -> u32 {
        (self.x.abs_diff(other.x)) + (self.y.abs_diff(other.y)) + (self.z.abs_diff(other.z))
    }

    /// Computes the squared Euclidean distance between two block positions.
    pub fn distance_squared(&self, other: &Self) -> f64 {
        let dx = (self.x - other.x) as f64;
        let dy = (self.y - other.y) as f64;
        let dz = (self.z - other.z) as f64;
        dx * dx + dy * dy + dz * dz
    }

    /// Computes the Euclidean distance between two block positions.
    pub fn distance(&self, other: &Self) -> f64 {
        self.distance_squared(other).sqrt()
    }
}

impl fmt::Display for BlockPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}

impl From<(i32, i32, i32)> for BlockPos {
    fn from((x, y, z): (i32, i32, i32)) -> Self {
        Self::new(x, y, z)
    }
}

impl From<[i32; 3]> for BlockPos {
    fn from([x, y, z]: [i32; 3]) -> Self {
        Self::new(x, y, z)
    }
}

impl From<BlockPos> for (i32, i32, i32) {
    fn from(pos: BlockPos) -> Self {
        (pos.x, pos.y, pos.z)
    }
}

impl From<BlockPos> for [i32; 3] {
    fn from(pos: BlockPos) -> Self {
        [pos.x, pos.y, pos.z]
    }
}

/// Context provided to a [`BlockEntity`] during each tick cycle.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockEntityTickContext {
    /// World position of the block entity currently ticking.
    pub pos: BlockPos,
    /// Absolute engine/world tick counter (20 TPS).
    pub tick_count: u64,
    /// Delta time elapsed since the previous tick in seconds (defaults to 0.05s for 20 TPS).
    pub delta_time: f32,
    /// Flag indicating whether the block entity modified state and needs persistence saving.
    pub is_dirty: bool,
    /// Flag indicating whether the block entity has been destroyed or invalidated.
    pub is_removed: bool,
}

impl BlockEntityTickContext {
    /// Creates a new tick context for the given position and tick count.
    pub fn new(pos: BlockPos, tick_count: u64) -> Self {
        Self {
            pos,
            tick_count,
            delta_time: 0.05,
            is_dirty: false,
            is_removed: false,
        }
    }

    /// Creates a new tick context with explicit delta time.
    pub fn with_delta(pos: BlockPos, tick_count: u64, delta_time: f32) -> Self {
        Self {
            pos,
            tick_count,
            delta_time,
            is_dirty: false,
            is_removed: false,
        }
    }

    /// Marks the block entity as dirty, indicating that its state changed and should be saved.
    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
    }

    /// Marks the block entity as removed, causing the manager to unregister it.
    pub fn mark_removed(&mut self) {
        self.is_removed = true;
    }
}

/// Strongly-typed NBT value tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NbtTag {
    /// 8-bit signed integer.
    Byte(i8),
    /// 16-bit signed integer.
    Short(i16),
    /// 32-bit signed integer.
    Int(i32),
    /// 64-bit signed integer.
    Long(i64),
    /// 32-bit floating point number.
    Float(f32),
    /// 64-bit floating point number.
    Double(f64),
    /// UTF-8 string value.
    String(String),
    /// Boolean flag value.
    Boolean(bool),
    /// Homogeneous or heterogeneous list of NBT tags.
    List(Vec<NbtTag>),
    /// Nested NBT compound mapping string keys to tags.
    Compound(NbtCompound),
    /// Raw byte array.
    ByteArray(Vec<u8>),
    /// Raw 32-bit integer array.
    IntArray(Vec<i32>),
    /// Raw 64-bit integer array.
    LongArray(Vec<i64>),
}

impl From<i8> for NbtTag {
    fn from(v: i8) -> Self {
        Self::Byte(v)
    }
}

impl From<i16> for NbtTag {
    fn from(v: i16) -> Self {
        Self::Short(v)
    }
}

impl From<i32> for NbtTag {
    fn from(v: i32) -> Self {
        Self::Int(v)
    }
}

impl From<i64> for NbtTag {
    fn from(v: i64) -> Self {
        Self::Long(v)
    }
}

impl From<f32> for NbtTag {
    fn from(v: f32) -> Self {
        Self::Float(v)
    }
}

impl From<f64> for NbtTag {
    fn from(v: f64) -> Self {
        Self::Double(v)
    }
}

impl From<bool> for NbtTag {
    fn from(v: bool) -> Self {
        Self::Boolean(v)
    }
}

impl From<String> for NbtTag {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl From<&str> for NbtTag {
    fn from(v: &str) -> Self {
        Self::String(v.to_string())
    }
}

impl From<NbtCompound> for NbtTag {
    fn from(v: NbtCompound) -> Self {
        Self::Compound(v)
    }
}

impl From<Vec<NbtTag>> for NbtTag {
    fn from(v: Vec<NbtTag>) -> Self {
        Self::List(v)
    }
}

impl From<Vec<u8>> for NbtTag {
    fn from(v: Vec<u8>) -> Self {
        Self::ByteArray(v)
    }
}

impl From<Vec<i32>> for NbtTag {
    fn from(v: Vec<i32>) -> Self {
        Self::IntArray(v)
    }
}

impl From<Vec<i64>> for NbtTag {
    fn from(v: Vec<i64>) -> Self {
        Self::LongArray(v)
    }
}

/// NBT compound structure representing serialized block entity or mod state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NbtCompound {
    /// Stored tag values keyed by tag name.
    pub values: BTreeMap<String, NbtTag>,
}

impl NbtCompound {
    /// Creates a new empty `NbtCompound`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a tag value by key.
    pub fn insert(&mut self, key: impl Into<String>, tag: impl Into<NbtTag>) {
        self.values.insert(key.into(), tag.into());
    }

    /// Returns a reference to the tag associated with `key`.
    pub fn get(&self, key: &str) -> Option<&NbtTag> {
        self.values.get(key)
    }

    /// Returns a mutable reference to the tag associated with `key`.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut NbtTag> {
        self.values.get_mut(key)
    }

    /// Removes a tag by key, returning the removed tag if present.
    pub fn remove(&mut self, key: &str) -> Option<NbtTag> {
        self.values.remove(key)
    }

    /// Returns whether the compound contains a tag with the specified key.
    pub fn contains_key(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// Returns the number of tags in this compound.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether the compound contains no tags.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Clears all tags from this compound.
    pub fn clear(&mut self) {
        self.values.clear();
    }

    /// Sets an 8-bit integer value.
    pub fn set_byte(&mut self, key: impl Into<String>, val: i8) {
        self.insert(key, NbtTag::Byte(val));
    }

    /// Retrieves an 8-bit integer value, widening or narrowing from other integer types if fitting.
    pub fn get_byte(&self, key: &str) -> Option<i8> {
        match self.get(key) {
            Some(NbtTag::Byte(b)) => Some(*b),
            Some(NbtTag::Short(s)) if *s >= i8::MIN as i16 && *s <= i8::MAX as i16 => {
                Some(*s as i8)
            }
            Some(NbtTag::Int(i)) if *i >= i8::MIN as i32 && *i <= i8::MAX as i32 => {
                Some(*i as i8)
            }
            Some(NbtTag::Long(l)) if *l >= i8::MIN as i64 && *l <= i8::MAX as i64 => {
                Some(*l as i8)
            }
            _ => None,
        }
    }

    /// Sets a 16-bit integer value.
    pub fn set_short(&mut self, key: impl Into<String>, val: i16) {
        self.insert(key, NbtTag::Short(val));
    }

    /// Retrieves a 16-bit integer value, widening or narrowing from other integer types if fitting.
    pub fn get_short(&self, key: &str) -> Option<i16> {
        match self.get(key) {
            Some(NbtTag::Short(s)) => Some(*s),
            Some(NbtTag::Byte(b)) => Some(*b as i16),
            Some(NbtTag::Int(i)) if *i >= i16::MIN as i32 && *i <= i16::MAX as i32 => {
                Some(*i as i16)
            }
            Some(NbtTag::Long(l)) if *l >= i16::MIN as i64 && *l <= i16::MAX as i64 => {
                Some(*l as i16)
            }
            _ => None,
        }
    }

    /// Sets a 32-bit integer value.
    pub fn set_i32(&mut self, key: impl Into<String>, val: i32) {
        self.insert(key, NbtTag::Int(val));
    }

    /// Retrieves a 32-bit integer value, widening or narrowing from other integer types if fitting.
    pub fn get_i32(&self, key: &str) -> Option<i32> {
        match self.get(key) {
            Some(NbtTag::Int(i)) => Some(*i),
            Some(NbtTag::Short(s)) => Some(*s as i32),
            Some(NbtTag::Byte(b)) => Some(*b as i32),
            Some(NbtTag::Long(l)) if *l >= i32::MIN as i64 && *l <= i32::MAX as i64 => {
                Some(*l as i32)
            }
            _ => None,
        }
    }

    /// Sets a 64-bit integer value.
    pub fn set_i64(&mut self, key: impl Into<String>, val: i64) {
        self.insert(key, NbtTag::Long(val));
    }

    /// Retrieves a 64-bit integer value, widening from smaller integer types if necessary.
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        match self.get(key) {
            Some(NbtTag::Long(l)) => Some(*l),
            Some(NbtTag::Int(i)) => Some(*i as i64),
            Some(NbtTag::Short(s)) => Some(*s as i64),
            Some(NbtTag::Byte(b)) => Some(*b as i64),
            _ => None,
        }
    }

    /// Sets a 32-bit float value.
    pub fn set_f32(&mut self, key: impl Into<String>, val: f32) {
        self.insert(key, NbtTag::Float(val));
    }

    /// Retrieves a 32-bit float value, converting from 64-bit double if present.
    pub fn get_f32(&self, key: &str) -> Option<f32> {
        match self.get(key) {
            Some(NbtTag::Float(f)) => Some(*f),
            Some(NbtTag::Double(d)) => Some(*d as f32),
            _ => None,
        }
    }

    /// Sets a 64-bit float value.
    pub fn set_f64(&mut self, key: impl Into<String>, val: f64) {
        self.insert(key, NbtTag::Double(val));
    }

    /// Retrieves a 64-bit float value, widening from 32-bit float if necessary.
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        match self.get(key) {
            Some(NbtTag::Double(d)) => Some(*d),
            Some(NbtTag::Float(f)) => Some(*f as f64),
            _ => None,
        }
    }

    /// Sets a string value.
    pub fn set_string(&mut self, key: impl Into<String>, val: impl Into<String>) {
        self.insert(key, NbtTag::String(val.into()));
    }

    /// Retrieves a string slice.
    pub fn get_string(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(NbtTag::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Sets a boolean value.
    pub fn set_bool(&mut self, key: impl Into<String>, val: bool) {
        self.insert(key, NbtTag::Boolean(val));
    }

    /// Retrieves a boolean value.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match self.get(key) {
            Some(NbtTag::Boolean(b)) => Some(*b),
            _ => None,
        }
    }

    /// Sets a nested compound value.
    pub fn set_compound(&mut self, key: impl Into<String>, val: NbtCompound) {
        self.insert(key, NbtTag::Compound(val));
    }

    /// Retrieves a reference to a nested compound.
    pub fn get_compound(&self, key: &str) -> Option<&NbtCompound> {
        match self.get(key) {
            Some(NbtTag::Compound(c)) => Some(c),
            _ => None,
        }
    }

    /// Retrieves a mutable reference to a nested compound.
    pub fn get_compound_mut(&mut self, key: &str) -> Option<&mut NbtCompound> {
        match self.get_mut(key) {
            Some(NbtTag::Compound(c)) => Some(c),
            _ => None,
        }
    }

    /// Sets a raw byte array value.
    pub fn set_byte_array(&mut self, key: impl Into<String>, val: Vec<u8>) {
        self.insert(key, NbtTag::ByteArray(val));
    }

    /// Retrieves a byte array slice.
    pub fn get_byte_array(&self, key: &str) -> Option<&[u8]> {
        match self.get(key) {
            Some(NbtTag::ByteArray(arr)) => Some(arr.as_slice()),
            _ => None,
        }
    }

    /// Sets an integer array value.
    pub fn set_int_array(&mut self, key: impl Into<String>, val: Vec<i32>) {
        self.insert(key, NbtTag::IntArray(val));
    }

    /// Retrieves an integer array slice.
    pub fn get_int_array(&self, key: &str) -> Option<&[i32]> {
        match self.get(key) {
            Some(NbtTag::IntArray(arr)) => Some(arr.as_slice()),
            _ => None,
        }
    }

    /// Sets a list of NBT tags.
    pub fn set_list(&mut self, key: impl Into<String>, val: Vec<NbtTag>) {
        self.insert(key, NbtTag::List(val));
    }

    /// Retrieves an NBT tag list slice.
    pub fn get_list(&self, key: &str) -> Option<&[NbtTag]> {
        match self.get(key) {
            Some(NbtTag::List(l)) => Some(l.as_slice()),
            _ => None,
        }
    }
}

/// Trait implemented by world block entities (tile entities) in Rurge mods.
///
/// Block entities hold persistent, ticking state tied to a specific world position ([`BlockPos`]).
pub trait BlockEntity: Send + Sync {
    /// Ticks the block entity state once per game tick (typically 20 TPS).
    fn tick(&mut self, ctx: &mut BlockEntityTickContext);

    /// Reads and restores block entity state from an NBT compound tag.
    fn read_nbt(&mut self, nbt: &NbtCompound);

    /// Serializes block entity state into an NBT compound tag for world saving.
    fn write_nbt(&self, nbt: &mut NbtCompound);

    /// Returns the ID of the mod that registered or owns this block entity.
    fn mod_id(&self) -> &str {
        "unknown"
    }

    /// Returns the type identifier for this block entity (e.g. `"industrial_rust:quarry"`).
    fn type_id(&self) -> &str {
        "rurge:block_entity"
    }

    /// Returns whether this block entity is active and should receive [`tick`](BlockEntity::tick) calls.
    fn can_tick(&self) -> bool {
        true
    }
}
