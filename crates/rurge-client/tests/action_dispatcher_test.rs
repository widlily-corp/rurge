use horizon_client::mod_api::{Flow, Host, NativeServerPlayer, Text};
use rurge_client::action_dispatcher::ActionDispatcher;
use rurge_converter::ir::{ActionEffect, ItemActionDef, ItemTrigger};
use rurge_converter::schema::ContentSchema;
use rurge_runtime::engine::RurgeEngine;
use std::sync::Arc;

struct MockHost {
    pub commands: Vec<String>,
    pub action_bars: Vec<String>,
    pub current_block: String,
    pub player: Option<NativeServerPlayer>,
}

impl MockHost {
    fn new() -> Self {
        let mut p = NativeServerPlayer::dummy();
        p.position = [100.0, 70.0, 200.0];
        Self {
            commands: Vec::new(),
            action_bars: Vec::new(),
            current_block: "minecraft:air".to_string(),
            player: Some(p),
        }
    }
}

impl Host for MockHost {
    fn run_command(&mut self, line: &str) -> bool {
        self.commands.push(line.to_string());
        true
    }

    fn action_bar(&mut self, _uuid: u128, line: Text) -> bool {
        self.action_bars.push(line.plain());
        true
    }

    fn broadcast(&self, _line: Text) {}

    fn block(&self, _pos: [i32; 3]) -> String {
        self.current_block.clone()
    }

    fn acting_player(&self) -> Option<NativeServerPlayer> {
        self.player.clone()
    }
}

#[test]
fn test_action_dispatcher_ignores_vanilla_items() {
    let engine = Arc::new(RurgeEngine::new());
    let mut host = MockHost::new();

    let flow = ActionDispatcher::handle_item_use(
        &engine,
        &mut host,
        "minecraft:stick",
        None,
        None,
    );

    assert_eq!(flow, Flow::Continue);
    assert!(host.commands.is_empty());
}

#[test]
fn test_action_dispatcher_dispatches_explicit_registered_actions() {
    let engine = Arc::new(RurgeEngine::new());
    
    // Register custom action for mod item
    let schema = ContentSchema {
        schema_version: "1.0".to_string(),
        mod_id: "testmagic".to_string(),
        blocks: vec![],
        items: vec![],
        recipes: vec![],
        item_actions: vec![ItemActionDef::new(
            ItemTrigger::RightClick,
            vec![
                ActionEffect::PlaySound {
                    sound: "testmagic:spell.cast".to_string(),
                    volume: 1.0,
                    pitch: 1.0,
                },
                ActionEffect::Heal { amount: 8.0 },
            ],
        ).with_item_id("testmagic:healing_wand")],
        block_triggers: vec![],
    };

    engine.content().register_schema("testmagic", &schema).unwrap();

    let mut host = MockHost::new();
    let flow = ActionDispatcher::handle_item_use(
        &engine,
        &mut host,
        "testmagic:healing_wand",
        None,
        None,
    );

    assert_eq!(flow, Flow::Cancel);
    assert!(host.commands.iter().any(|c| c.contains("playsound testmagic:spell.cast")));
    assert!(host.commands.iter().any(|c| c.contains("effect give @p instant_health")));
}

#[test]
fn test_action_dispatcher_semantic_wand_casting() {
    let engine = Arc::new(RurgeEngine::new());
    let mut host = MockHost::new();

    let flow = ActionDispatcher::handle_item_use(
        &engine,
        &mut host,
        "ebwizardry:wand_master_fire",
        None,
        None,
    );

    assert_eq!(flow, Flow::Cancel);
    assert!(host.commands.iter().any(|c| c.contains("playsound item.wand.blast")));
    assert!(host.commands.iter().any(|c| c.contains("particle flame")));
    assert!(host.commands.iter().any(|c| c.contains("damage @e")));
    assert!(host.action_bars.iter().any(|a| a.contains("Cast Wand")));
}

#[test]
fn test_action_dispatcher_semantic_scroll_consumption() {
    let engine = Arc::new(RurgeEngine::new());
    let mut host = MockHost::new();

    let flow = ActionDispatcher::handle_item_use(
        &engine,
        &mut host,
        "ebwizardry:scroll_lightning_bolt",
        None,
        None,
    );

    assert_eq!(flow, Flow::Cancel);
    assert!(host.commands.iter().any(|c| c.contains("playsound entity.evoker.cast_spell")));
    assert!(host.commands.iter().any(|c| c.contains("clear @p ebwizardry:scroll_lightning_bolt 1")));
    assert!(host.action_bars.iter().any(|a| a.contains("Inscribed")));
}

#[test]
fn test_action_dispatcher_semantic_bomb_explosion() {
    let engine = Arc::new(RurgeEngine::new());
    let mut host = MockHost::new();

    let flow = ActionDispatcher::handle_item_use(
        &engine,
        &mut host,
        "ebwizardry:firebomb",
        None,
        None,
    );

    assert_eq!(flow, Flow::Cancel);
    assert!(host.commands.iter().any(|c| c.contains("particle explosion_emitter")));
    assert!(host.commands.iter().any(|c| c.contains("playsound entity.generic.explode")));
    assert!(host.commands.iter().any(|c| c.contains("clear @p ebwizardry:firebomb 1")));
}

#[test]
fn test_action_dispatcher_semantic_flask_drinking() {
    let engine = Arc::new(RurgeEngine::new());
    let mut host = MockHost::new();

    let flow = ActionDispatcher::handle_item_use(
        &engine,
        &mut host,
        "ebwizardry:mana_flask_large",
        None,
        None,
    );

    assert_eq!(flow, Flow::Cancel);
    assert!(host.commands.iter().any(|c| c.contains("playsound entity.generic.drink")));
    assert!(host.commands.iter().any(|c| c.contains("effect give @p regeneration")));
    assert!(host.commands.iter().any(|c| c.contains("give @p minecraft:glass_bottle 1")));
}

#[test]
fn test_action_dispatcher_block_interaction_arcane_workbench() {
    let engine = Arc::new(RurgeEngine::new());
    let mut host = MockHost::new();
    host.current_block = "ebwizardry:arcane_workbench".to_string();

    let flow = ActionDispatcher::handle_block_use(
        &engine,
        &mut host,
        "ebwizardry:arcane_workbench",
        [10, 64, 20],
    );

    assert_eq!(flow, Flow::Cancel);
    assert!(host.commands.iter().any(|c| c.contains("playsound block.enchantment_table.use")));
    assert!(host.commands.iter().any(|c| c.contains("particle enchant")));
    assert!(host.action_bars.iter().any(|a| a.contains("Interacted with Arcane Workbench")));
}
