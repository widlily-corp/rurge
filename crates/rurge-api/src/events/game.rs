//! Typed ABI-stable game events dispatched during gameplay.

use abi_stable::{
    std_types::RString,
    StableAbi,
};
use serde::{Deserialize, Serialize};

use super::cancellable::CancellableEvent;

/// Hand used during interaction.
#[repr(u8)]
#[derive(StableAbi, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InteractionHand {
    /// Main hand (primary hand, usually right).
    Main = 0,
    /// Off hand (secondary hand, usually left).
    Off = 1,
}

/// Type alias for `InteractionHand`.
pub type Hand = InteractionHand;

/// Event fired when a player attempts to break a block in the world.
#[repr(C)]
#[derive(StableAbi, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockBreakEvent {
    /// Identifier or name of the player breaking the block.
    pub player_id: RString,
    /// World block coordinate [x, y, z].
    pub pos: [i32; 3],
    /// Whether this block break action has been cancelled.
    pub is_cancelled: bool,
}

impl BlockBreakEvent {
    /// Creates a new `BlockBreakEvent`.
    pub fn new(player_id: impl Into<RString>, pos: [i32; 3]) -> Self {
        Self {
            player_id: player_id.into(),
            pos,
            is_cancelled: false,
        }
    }

    /// Cancels this event, preventing the block from being broken.
    pub fn cancel(&mut self) {
        self.is_cancelled = true;
    }

    /// Checks whether this event has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.is_cancelled
    }
}

impl CancellableEvent for BlockBreakEvent {
    fn cancel(&mut self) {
        self.is_cancelled = true;
    }

    fn is_cancelled(&self) -> bool {
        self.is_cancelled
    }
}

/// Event fired when a player interacts with a target position or block in the world.
#[repr(C)]
#[derive(StableAbi, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerInteractEvent {
    /// Identifier or name of the interacting player.
    pub player_id: RString,
    /// Hand used for the interaction.
    pub hand: InteractionHand,
    /// Target world position [x, y, z].
    pub target_pos: [i32; 3],
    /// Whether this interaction has been cancelled.
    pub is_cancelled: bool,
}

impl PlayerInteractEvent {
    /// Creates a new `PlayerInteractEvent`.
    pub fn new(player_id: impl Into<RString>, hand: InteractionHand, target_pos: [i32; 3]) -> Self {
        Self {
            player_id: player_id.into(),
            hand,
            target_pos,
            is_cancelled: false,
        }
    }

    /// Cancels this event, stopping further interaction processing.
    pub fn cancel(&mut self) {
        self.is_cancelled = true;
    }

    /// Checks whether this event has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.is_cancelled
    }
}

impl CancellableEvent for PlayerInteractEvent {
    fn cancel(&mut self) {
        self.is_cancelled = true;
    }

    fn is_cancelled(&self) -> bool {
        self.is_cancelled
    }
}

/// Event fired when an entity takes damage from a source.
#[repr(C)]
#[derive(StableAbi, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntityDamageEvent {
    /// Network ID of the damaged entity.
    pub entity_id: u32,
    /// Damage source identifier (e.g. "fall", "player", "mob", "magic").
    pub source: RString,
    /// Amount of damage applied in half-hearts.
    pub damage: f32,
    /// Whether the damage has been cancelled.
    pub is_cancelled: bool,
}

impl EntityDamageEvent {
    /// Creates a new `EntityDamageEvent`.
    pub fn new(entity_id: u32, source: impl Into<RString>, damage: f32) -> Self {
        Self {
            entity_id,
            source: source.into(),
            damage,
            is_cancelled: false,
        }
    }

    /// Cancels this event, preventing the damage from being applied.
    pub fn cancel(&mut self) {
        self.is_cancelled = true;
    }

    /// Checks whether this event has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.is_cancelled
    }
}

impl CancellableEvent for EntityDamageEvent {
    fn cancel(&mut self) {
        self.is_cancelled = true;
    }

    fn is_cancelled(&self) -> bool {
        self.is_cancelled
    }
}

/// Event fired during each world or server simulation tick.
#[repr(C)]
#[derive(StableAbi, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldTickEvent {
    /// Total ticks elapsed since world or server startup.
    pub tick_count: u64,
    /// Delta time elapsed since the last tick in seconds.
    pub delta_time: f32,
}

impl WorldTickEvent {
    /// Creates a new `WorldTickEvent`.
    pub fn new(tick_count: u64, delta_time: f32) -> Self {
        Self {
            tick_count,
            delta_time,
        }
    }
}
