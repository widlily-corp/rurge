use abi_stable::{
    std_types::RString,
    StableAbi,
};
use serde::{Deserialize, Serialize};

/// Event fired before mods start their main initialization phase.
#[repr(C)]
#[derive(StableAbi, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreInitEvent {
    pub mod_id: RString,
}

impl PreInitEvent {
    pub fn new(mod_id: impl Into<RString>) -> Self {
        Self {
            mod_id: mod_id.into(),
        }
    }
}

/// Event fired during main initialization phase.
#[repr(C)]
#[derive(StableAbi, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitEvent {
    pub mod_id: RString,
}

impl InitEvent {
    pub fn new(mod_id: impl Into<RString>) -> Self {
        Self {
            mod_id: mod_id.into(),
        }
    }
}

/// Event fired after all mods have finished basic initialization.
#[repr(C)]
#[derive(StableAbi, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostInitEvent {
    pub mod_id: RString,
}

impl PostInitEvent {
    pub fn new(mod_id: impl Into<RString>) -> Self {
        Self {
            mod_id: mod_id.into(),
        }
    }
}

/// Event fired when the game server is preparing to start.
#[repr(C)]
#[derive(StableAbi, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerStartingEvent {
    pub server_name: RString,
}

impl ServerStartingEvent {
    pub fn new(server_name: impl Into<RString>) -> Self {
        Self {
            server_name: server_name.into(),
        }
    }
}

/// Event fired when the game server is stopping.
#[repr(C)]
#[derive(StableAbi, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerStoppingEvent;
