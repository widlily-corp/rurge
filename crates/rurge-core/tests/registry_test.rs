use std::str::FromStr;
use rurge_core::{Identifier, ParseIdentifierError, Registry, RegistryError};

#[derive(Debug, PartialEq, Eq, Clone)]
struct BlockType {
    name: String,
    hardness: u32,
}

#[test]
fn test_identifier_parsing_and_display() {
    let id = Identifier::new("minecraft", "diamond_block");
    assert_eq!(id.namespace, "minecraft");
    assert_eq!(id.path, "diamond_block");
    assert_eq!(id.to_string(), "minecraft:diamond_block");

    // FromStr
    let parsed: Identifier = "rurge:copper_ore".parse().expect("valid identifier");
    assert_eq!(parsed.namespace, "rurge");
    assert_eq!(parsed.path, "copper_ore");

    // Errors
    let err_no_colon = Identifier::from_str("invalid_no_colon");
    assert!(matches!(err_no_colon, Err(ParseIdentifierError::MissingSeparator(_))));

    let err_empty_ns = Identifier::from_str(":path_only");
    assert!(matches!(err_empty_ns, Err(ParseIdentifierError::EmptyNamespace(_))));

    let err_empty_path = Identifier::from_str("ns_only:");
    assert!(matches!(err_empty_path, Err(ParseIdentifierError::EmptyPath(_))));
}

#[test]
fn test_dense_sequential_id_allocation() {
    let mut reg: Registry<BlockType> = Registry::new("blocks");

    let id1 = Identifier::new("test", "stone");
    let id2 = Identifier::new("test", "dirt");
    let id3 = Identifier::new("test", "grass");

    let raw1 = reg
        .register(id1.clone(), BlockType { name: "Stone".into(), hardness: 15 })
        .expect("register stone");
    let raw2 = reg
        .register(id2.clone(), BlockType { name: "Dirt".into(), hardness: 5 })
        .expect("register dirt");
    let raw3 = reg
        .register(id3.clone(), BlockType { name: "Grass".into(), hardness: 6 })
        .expect("register grass");

    assert_eq!(raw1, 0, "First entry must receive raw ID 0");
    assert_eq!(raw2, 1, "Second entry must receive raw ID 1");
    assert_eq!(raw3, 2, "Third entry must receive raw ID 2");
    assert_eq!(reg.len(), 3);
    assert!(!reg.is_empty());
}

#[test]
fn test_dual_lookup_by_id_and_raw_id() {
    let mut reg: Registry<BlockType> = Registry::new("blocks");

    let id = Identifier::new("rurge", "adamantite");
    let raw = reg
        .register(
            id.clone(),
            BlockType {
                name: "Adamantite Ore".into(),
                hardness: 100,
            },
        )
        .expect("register");

    // Lookup by identifier
    let entry_by_id = reg.get_by_id(&id).expect("found by id");
    assert_eq!(entry_by_id.name, "Adamantite Ore");
    assert_eq!(entry_by_id.hardness, 100);

    // Lookup by raw ID
    let entry_by_raw = reg.get_by_raw_id(raw).expect("found by raw id");
    assert_eq!(entry_by_raw.name, "Adamantite Ore");

    // Lookup ID by raw ID
    let id_by_raw = reg.get_id_by_raw_id(raw).expect("found id by raw");
    assert_eq!(id_by_raw, &id);

    // Reverse raw ID lookup
    assert_eq!(reg.get_raw_id(&id), Some(raw));
}

#[test]
fn test_duplicate_registration_rejected() {
    let mut reg: Registry<BlockType> = Registry::new("blocks");

    let id = Identifier::new("mod", "item");
    let raw0 = reg
        .register(
            id.clone(),
            BlockType {
                name: "Item V1".into(),
                hardness: 1,
            },
        )
        .expect("first registration succeeds");
    assert_eq!(raw0, 0);

    // Second registration with same ID must fail
    let err = reg
        .register(
            id.clone(),
            BlockType {
                name: "Item V2".into(),
                hardness: 2,
            },
        )
        .unwrap_err();

    assert_eq!(err, RegistryError::DuplicateIdentifier(id.clone()));
    assert_eq!(reg.len(), 1, "Registry length must remain 1 after duplicate attempt");

    // Original entry must be preserved
    let original = reg.get_by_id(&id).expect("original remains");
    assert_eq!(original.name, "Item V1");
}

#[test]
fn test_nonexistent_lookups() {
    let reg: Registry<BlockType> = Registry::new("blocks");

    let unknown_id = Identifier::new("unknown", "block");
    assert!(reg.get_by_id(&unknown_id).is_none());
    assert!(reg.get_by_raw_id(999).is_none());
    assert!(reg.get_id_by_raw_id(999).is_none());
    assert_eq!(reg.get_raw_id(&unknown_id), None);
    assert!(!reg.contains(&unknown_id));
}
