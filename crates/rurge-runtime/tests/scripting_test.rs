use rurge_converter::ir::{
    ActionCondition, ActionEffect, BlockTriggerDef, BlockTriggerType, ContentIR, ItemActionDef,
    ItemTrigger,
};
use rurge_converter::manifest::{ModLoader, ModMetadata};
use rurge_converter::schema::ContentSchema;
use rurge_runtime::scripting::{
    BlockState, ExecutionContext, MechanicsEvent, MechanicsExecutor, PlayerState,
};
use std::collections::HashMap;

#[test]
fn test_parse_and_serialize_schema_with_mechanics() {
    let json = r#"{
        "schema_version": "1.0",
        "mod_id": "elemental_craft",
        "blocks": [
            {
                "id": "fire_altar",
                "hardness": 3.0,
                "resistance": 12.0,
                "harvest_level": 2,
                "sound_group": "stone",
                "light_emission": 7,
                "opaque": true,
                "triggers": [
                    {
                        "trigger": "interact",
                        "actions": [
                            {
                                "action": "set_block_state",
                                "properties": { "active": "true" }
                            },
                            {
                                "action": "play_sound",
                                "sound": "elemental_craft:altar_ignite",
                                "volume": 1.0,
                                "pitch": 1.2
                            }
                        ]
                    }
                ]
            }
        ],
        "items": [
            {
                "id": "blaze_wand",
                "max_stack_size": 1,
                "durability": 150,
                "rarity": "RARE",
                "actions": [
                    {
                        "trigger": "right_click",
                        "cooldown_ticks": 40,
                        "actions": [
                            {
                                "action": "apply_effect",
                                "effect": "fire_resistance",
                                "duration_ticks": 300,
                                "amplifier": 0
                            },
                            {
                                "action": "explode",
                                "power": 2.5,
                                "causes_fire": true,
                                "break_blocks": false
                            }
                        ]
                    }
                ]
            }
        ],
        "recipes": []
    }"#;

    let schema = ContentSchema::from_json_str(json).expect("ContentSchema should parse valid JSON");
    assert_eq!(schema.mod_id, "elemental_craft");
    assert_eq!(schema.blocks.len(), 1);
    assert_eq!(schema.items.len(), 1);

    // Verify parsed block trigger
    let block = &schema.blocks[0];
    assert_eq!(block.id, "fire_altar");
    assert_eq!(block.triggers.len(), 1);
    assert_eq!(block.triggers[0].trigger, BlockTriggerType::Interact);
    assert_eq!(block.triggers[0].actions.len(), 2);

    // Verify parsed item action
    let item = &schema.items[0];
    assert_eq!(item.id, "blaze_wand");
    assert_eq!(item.actions.len(), 1);
    assert_eq!(item.actions[0].trigger, ItemTrigger::RightClick);
    assert_eq!(item.actions[0].cooldown_ticks, Some(40));
    assert_eq!(item.actions[0].actions.len(), 2);

    // Test serialization roundtrip
    let serialized = schema
        .to_json_pretty()
        .expect("Serialization to JSON should succeed");
    let roundtrip = ContentSchema::from_json_str(&serialized)
        .expect("Roundtrip deserialization should succeed");
    assert_eq!(roundtrip.mod_id, "elemental_craft");
    assert_eq!(roundtrip.blocks[0].triggers.len(), 1);
    assert_eq!(roundtrip.items[0].actions.len(), 1);
}

#[test]
fn test_content_ir_to_schema_conversion_preserves_mechanics() {
    let mut ir = ContentIR::new(ModMetadata {
        id: "alchemy".into(),
        name: "Alchemy Mod".into(),
        version: "1.0.0".into(),
        authors: vec!["Wizard".into()],
        description: "".into(),
        loader: ModLoader::Fabric,
    });

    let action = ItemActionDef::new(
        ItemTrigger::RightClick,
        vec![
            ActionEffect::Heal { amount: 6.0 },
            ActionEffect::ApplyEffect {
                effect: "regeneration".into(),
                duration_ticks: 100,
                amplifier: 1,
            },
        ],
    )
    .with_cooldown(20);

    let mut item = rurge_converter::ir::ItemDef::new("healing_potion");
    item.actions.push(action);
    ir.add_item(item);

    let trigger = BlockTriggerDef::new(
        BlockTriggerType::StepOn,
        vec![ActionEffect::Damage {
            amount: 4.0,
            damage_type: Some("magic".into()),
        }],
    );

    let mut block = rurge_converter::ir::BlockDef::new("spiked_floor");
    block.triggers.push(trigger);
    ir.add_block(block);

    let schema = ContentSchema::from_ir(&ir);
    assert_eq!(schema.items.len(), 1);
    assert_eq!(schema.items[0].actions.len(), 1);
    assert_eq!(
        schema.items[0].actions[0].trigger,
        ItemTrigger::RightClick
    );
    assert_eq!(schema.blocks.len(), 1);
    assert_eq!(schema.blocks[0].triggers.len(), 1);
    assert_eq!(
        schema.blocks[0].triggers[0].trigger,
        BlockTriggerType::StepOn
    );
}

#[test]
fn test_mechanics_executor_right_click_apply_effect_and_explode() {
    let executor = MechanicsExecutor::new();

    let wand_action = ItemActionDef::new(
        ItemTrigger::RightClick,
        vec![
            ActionEffect::ApplyEffect {
                effect: "speed".into(),
                duration_ticks: 200,
                amplifier: 1,
            },
            ActionEffect::Explode {
                power: 3.0,
                causes_fire: false,
                break_blocks: true,
            },
        ],
    );
    executor.register_item_action("magic:spell_wand", wand_action);

    // Place blocks in the world around origin
    executor.set_block([1, 64, 0], BlockState::new("minecraft:stone"));
    executor.set_block([0, 64, 1], BlockState::new("minecraft:dirt"));
    executor.set_block([100, 64, 100], BlockState::new("minecraft:diamond_block")); // far away

    let mut player = PlayerState::new("player_alice");
    player.position = [0.0, 64.0, 0.0];
    player.held_item = Some("magic:spell_wand".into());
    executor.set_player(player);

    let result = executor.execute_item_action("player_alice", "magic:spell_wand", ItemTrigger::RightClick);
    assert!(result.is_success(), "Item action execution should succeed");
    assert_eq!(result.actions_executed, 2);

    // Verify effect was applied to the player
    let updated_player = executor.get_player("player_alice").expect("Player should exist");
    let effect = updated_player
        .active_effects
        .get("speed")
        .expect("Player should have speed effect");
    assert_eq!(effect.amplifier, 1);
    assert_eq!(effect.remaining_ticks, 200);

    // Verify blocks within explosion radius were destroyed and distant block remains
    assert!(executor.get_block([1, 64, 0]).is_none());
    assert!(executor.get_block([0, 64, 1]).is_none());
    assert!(executor.get_block([100, 64, 100]).is_some());

    // Verify events were logged
    assert!(result.has_event(|e| matches!(e, MechanicsEvent::EffectApplied { effect, .. } if effect == "speed")));
    assert!(result.has_event(|e| matches!(e, MechanicsEvent::Exploded { power, break_blocks, .. } if *power == 3.0 && *break_blocks)));
}

#[test]
fn test_mechanics_executor_item_cooldown_lifecycle() {
    let executor = MechanicsExecutor::new();

    let action = ItemActionDef::new(
        ItemTrigger::RightClick,
        vec![ActionEffect::PlaySound {
            sound: "entity.lightning.thunder".into(),
            volume: 1.0,
            pitch: 1.0,
        }],
    )
    .with_cooldown(10);

    executor.register_item_action("mod:horn", action);

    let player = PlayerState::new("player_bob");
    executor.set_player(player);

    // First use: must succeed and engage cooldown
    let res1 = executor.execute_item_action("player_bob", "mod:horn", ItemTrigger::RightClick);
    assert!(res1.is_success());
    assert!(executor.is_on_cooldown("player_bob", "mod:horn"));
    assert_eq!(executor.get_remaining_cooldown("player_bob", "mod:horn"), 10);

    // Immediate second use: must fail due to cooldown
    let res2 = executor.execute_item_action("player_bob", "mod:horn", ItemTrigger::RightClick);
    assert!(!res2.is_success());
    assert!(res2.error_message.as_ref().unwrap().contains("cooldown"));

    // Advance 5 ticks
    for _ in 0..5 {
        executor.tick();
    }
    assert!(executor.is_on_cooldown("player_bob", "mod:horn"));
    assert_eq!(executor.get_remaining_cooldown("player_bob", "mod:horn"), 5);

    // Advance 5 more ticks (total 10 ticks passed)
    for _ in 0..5 {
        executor.tick();
    }
    assert!(!executor.is_on_cooldown("player_bob", "mod:horn"));
    assert_eq!(executor.get_remaining_cooldown("player_bob", "mod:horn"), 0);

    // Third use now succeeds
    let res3 = executor.execute_item_action("player_bob", "mod:horn", ItemTrigger::RightClick);
    assert!(res3.is_success());
}

#[test]
fn test_mechanics_executor_block_interact_changes_state_and_plays_sound() {
    let executor = MechanicsExecutor::new();

    let trigger = BlockTriggerDef::new(
        BlockTriggerType::Interact,
        vec![
            ActionEffect::SetBlockState {
                block_id: None,
                properties: {
                    let mut p = HashMap::new();
                    p.insert("powered".into(), "true".into());
                    p
                },
            },
            ActionEffect::PlaySound {
                sound: "block.lever.click".into(),
                volume: 1.0,
                pitch: 1.0,
            },
        ],
    );
    executor.register_block_trigger("custom_tech:lever", trigger);

    let initial_pos = [10, 65, 10];
    let initial_block = BlockState::new("custom_tech:lever").with_property("powered", "false");
    executor.set_block(initial_pos, initial_block);

    let player = PlayerState::new("player_charlie");
    executor.set_player(player);

    let res = executor.execute_block_trigger("player_charlie", initial_pos, BlockTriggerType::Interact);
    assert!(res.is_success(), "Block interaction trigger should succeed");

    // Verify block state property mutated
    let updated_block = executor.get_block(initial_pos).expect("Block should exist");
    assert_eq!(updated_block.id, "custom_tech:lever");
    assert_eq!(updated_block.get_property("powered"), Some("true"));

    // Verify events
    assert!(res.has_event(|e| matches!(
        e,
        MechanicsEvent::BlockStateChanged { properties, .. } if properties.get("powered") == Some(&"true".to_string())
    )));
    assert!(res.has_event(|e| matches!(
        e,
        MechanicsEvent::SoundPlayed { sound, .. } if sound == "block.lever.click"
    )));
}

#[test]
fn test_mechanics_executor_conditions_filtering() {
    let executor = MechanicsExecutor::new();

    // Action requiring sneaking
    let sneak_action = ItemActionDef::new(
        ItemTrigger::RightClick,
        vec![ActionEffect::SendMessage {
            message: "Secret activated".into(),
        }],
    )
    .with_condition(ActionCondition::Sneaking { sneaking: true });

    executor.register_item_action("rpg:secret_ring", sneak_action);

    let mut player = PlayerState::new("player_dan");
    player.sneaking = false;
    executor.set_player(player.clone());

    // Without sneaking, execution fails
    let fail_res = executor.execute_item_action("player_dan", "rpg:secret_ring", ItemTrigger::RightClick);
    assert!(!fail_res.is_success());

    // With sneaking, execution succeeds
    player.sneaking = true;
    executor.set_player(player);

    let success_res = executor.execute_item_action("player_dan", "rpg:secret_ring", ItemTrigger::RightClick);
    assert!(success_res.is_success());
    assert!(success_res.has_event(|e| matches!(e, MechanicsEvent::MessageSent { message, .. } if message == "Secret activated")));
}

#[test]
fn test_mechanics_executor_direct_action_effects_execution() {
    let executor = MechanicsExecutor::new();

    let mut player = PlayerState::new("player_eve");
    player.health = 10.0;
    player.position = [10.0, 70.0, 10.0];
    executor.set_player(player);

    let effects = vec![
        ActionEffect::Heal { amount: 5.0 },
        ActionEffect::Damage {
            amount: 2.0,
            damage_type: Some("fire".into()),
        },
        ActionEffect::GiveItem {
            item_id: "minecraft:gold_ingot".into(),
            count: 3,
        },
        ActionEffect::Teleport {
            offset_x: 0.0,
            offset_y: 10.0,
            offset_z: 0.0,
        },
    ];

    let mut ctx = ExecutionContext::new();
    ctx.player_id = Some("player_eve".into());
    ctx.position = Some([10.0, 70.0, 10.0]);

    let res = executor.execute_action_effects(&effects, &ctx);
    assert!(res.is_success());
    assert_eq!(res.actions_executed, 4);

    let p = executor.get_player("player_eve").unwrap();
    // 10.0 + 5.0 - 2.0 = 13.0 HP
    assert_eq!(p.health, 13.0);
    // Inventory received 3 gold ingots
    assert_eq!(p.inventory.get("minecraft:gold_ingot"), Some(&3));
    // Teleported Y by +10.0 (from 70 to 80)
    assert_eq!(p.position, [10.0, 80.0, 10.0]);
}

#[test]
fn test_mechanics_executor_register_schema_full_integration() {
    let executor = MechanicsExecutor::new();

    let schema = ContentSchema {
        schema_version: "1.0".to_string(),
        mod_id: "techno".to_string(),
        blocks: vec![rurge_converter::schema::SchemaBlock {
            id: "generator".into(),
            hardness: 2.0,
            resistance: 5.0,
            tool: None,
            harvest_level: 0,
            sound_group: "metal".into(),
            light_emission: 0,
            opaque: true,
            model: None,
            triggers: vec![BlockTriggerDef::new(
                BlockTriggerType::Redstone,
                vec![ActionEffect::SetBlockState {
                    block_id: Some("techno:active_generator".into()),
                    properties: HashMap::new(),
                }],
            )],
        }],
        items: vec![rurge_converter::schema::SchemaItem {
            id: "wrench".into(),
            max_stack_size: 1,
            durability: 100,
            tool: None,
            food: None,
            rarity: "COMMON".into(),
            model: None,
            actions: vec![ItemActionDef::new(
                ItemTrigger::RightClick,
                vec![ActionEffect::PlaySound {
                    sound: "techno:wrench_crank".into(),
                    volume: 1.0,
                    pitch: 1.0,
                }],
            )],
        }],
        recipes: vec![],
        item_actions: vec![],
        block_triggers: vec![],
    };

    executor
        .register_schema("techno", &schema)
        .expect("Registering schema into MechanicsExecutor should succeed");

    // Execute wrench by canonical ID or short ID
    let player = PlayerState::new("mechanic");
    executor.set_player(player);

    let res_item = executor.execute_item_action("mechanic", "techno:wrench", ItemTrigger::RightClick);
    assert!(res_item.is_success());

    // Execute generator block trigger
    executor.set_block([5, 64, 5], BlockState::new("techno:generator"));
    let res_block = executor.execute_block_trigger("mechanic", [5, 64, 5], BlockTriggerType::Redstone);
    assert!(res_block.is_success());

    let transformed_block = executor.get_block([5, 64, 5]).unwrap();
    assert_eq!(transformed_block.id, "techno:active_generator");
}
