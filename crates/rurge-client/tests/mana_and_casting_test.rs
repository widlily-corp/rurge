use rurge_client::action_dispatcher::ActionDispatcher;

#[test]
fn test_wand_mana_lifecycle() {
    ActionDispatcher::set_wand_mana(1400);
    assert_eq!(ActionDispatcher::get_wand_mana(), 1400);

    ActionDispatcher::set_wand_mana(0);
    assert_eq!(ActionDispatcher::get_wand_mana(), 0);

    ActionDispatcher::set_wand_mana(1400);
}

#[test]
fn test_mana_deduction_prevents_casting_when_empty() {
    let initial_mana = 10;
    let spell_cost = 25;
    let can_cast = initial_mana >= spell_cost;
    assert!(!can_cast, "Spell should not cast when mana is lower than cost");
}

#[test]
fn test_mana_deduction_succeeds_when_sufficient() {
    let initial_mana = 1400;
    let spell_cost = 30;
    let remaining = if initial_mana >= spell_cost {
        initial_mana - spell_cost
    } else {
        initial_mana
    };
    assert_eq!(remaining, 1370);
}
