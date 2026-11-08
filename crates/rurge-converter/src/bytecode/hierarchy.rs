//! Class hierarchy analysis and type deduction for JVM bytecode.
//!
//! Builds an inheritance graph from parsed [`ClassFile`] instances to trace
//! superclasses and interfaces, augmented with built-in knowledge of Minecraft vanilla hierarchies.

use crate::bytecode::ClassFile;
use std::collections::{HashMap, HashSet, VecDeque};

/// Normalized representation of class inheritance and interface implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassNode {
    /// Fully qualified normalized binary class name (e.g. `"net/minecraft/world/item/SwordItem"`).
    pub name: String,
    /// Optional fully qualified normalized superclass name.
    pub super_class: Option<String>,
    /// List of fully qualified normalized interface names implemented by this class.
    pub interfaces: Vec<String>,
}

/// Represents the JVM class hierarchy graph extracted from mod classes,
/// augmented with built-in knowledge of Minecraft vanilla classes.
#[derive(Debug, Clone, Default)]
pub struct ClassHierarchy {
    /// Mapping of normalized class name to its inheritance info.
    classes: HashMap<String, ClassNode>,
}

/// Normalizes a JVM class name or type descriptor into a slash-separated binary name.
///
/// Handles:
/// - `"[Lnet/minecraft/world/item/Item;"` -> `"net/minecraft/world/item/Item"`
/// - `"Lnet/minecraft/world/item/Item;"` -> `"net/minecraft/world/item/Item"`
/// - `"net.minecraft.world.item.Item"` -> `"net/minecraft/world/item/Item"`
/// - `"net/minecraft/world/item/Item"` -> `"net/minecraft/world/item/Item"`
pub fn normalize_class_name(name: &str) -> String {
    let mut s = name.trim();
    while s.starts_with('[') {
        s = &s[1..];
    }
    if s.starts_with('L') && s.ends_with(';') {
        s = &s[1..s.len() - 1];
    }
    s.replace('.', "/")
}

fn simple_name(name: &str) -> &str {
    let norm = name.trim();
    norm.rsplit('/').next().unwrap_or(norm)
}

impl ClassHierarchy {
    /// Creates an empty class hierarchy.
    pub fn new() -> Self {
        Self {
            classes: HashMap::new(),
        }
    }

    /// Builds a class hierarchy graph from a slice of parsed JVM classfiles.
    pub fn from_classes(classes: &[ClassFile]) -> Self {
        let mut hierarchy = Self::new();
        for cf in classes {
            let class_name = cf.class_name();
            if class_name.is_empty() {
                continue;
            }
            let norm_name = normalize_class_name(class_name);
            let super_class = cf.super_class_name().map(normalize_class_name);
            let mut interfaces = Vec::with_capacity(cf.interfaces.len());
            for &iface_idx in &cf.interfaces {
                if let Some(iface_name) = cf.constant_pool.get_class_name(iface_idx) {
                    interfaces.push(normalize_class_name(iface_name));
                }
            }
            hierarchy.add_node(norm_name, super_class, interfaces);
        }
        hierarchy
    }

    /// Registers a class node with optional superclass and interfaces.
    pub fn add_class(
        &mut self,
        class_name: &str,
        super_class: Option<&str>,
        interfaces: &[&str],
    ) {
        let norm_name = normalize_class_name(class_name);
        let super_class = super_class.map(normalize_class_name);
        let ifaces = interfaces.iter().map(|&i| normalize_class_name(i)).collect();
        self.add_node(norm_name, super_class, ifaces);
    }

    fn add_node(&mut self, name: String, super_class: Option<String>, interfaces: Vec<String>) {
        self.classes.insert(
            name.clone(),
            ClassNode {
                name,
                super_class,
                interfaces,
            },
        );
    }

    /// Recursively checks whether `class` is equal to or inherits from `target_parent`
    /// across superclasses and implemented interfaces.
    ///
    /// Handles cycles safely with a visited set and traverses both parsed mod classes
    /// and built-in Minecraft vanilla type hierarchies.
    pub fn is_subclass_of(&self, class: &str, target_parent: &str) -> bool {
        let class_norm = normalize_class_name(class);
        let target_norm = normalize_class_name(target_parent);

        if class_norm.is_empty() || target_norm.is_empty() {
            return false;
        }

        if class_norm == target_norm {
            return true;
        }

        let target_is_simple = !target_norm.contains('/');
        if target_is_simple && simple_name(&class_norm) == target_norm {
            return true;
        }

        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();

        visited.insert(class_norm.clone());
        queue.push_back(class_norm);

        while let Some(current) = queue.pop_front() {
            if current == target_norm {
                return true;
            }
            if target_is_simple && simple_name(&current) == target_norm {
                return true;
            }

            // 1. Check mod-defined classes
            if let Some(node) = self.classes.get(&current) {
                if let Some(ref sc) = node.super_class
                    && visited.insert(sc.clone())
                {
                    queue.push_back(sc.clone());
                }
                for iface in &node.interfaces {
                    if visited.insert(iface.clone()) {
                        queue.push_back(iface.clone());
                    }
                }
            }

            // 2. Check built-in vanilla hierarchy knowledge
            if let Some((known_super, known_interfaces)) = get_known_vanilla_parents(&current) {
                if let Some(sc) = known_super {
                    let sc_norm = normalize_class_name(sc);
                    if visited.insert(sc_norm.clone()) {
                        queue.push_back(sc_norm);
                    }
                }
                for &iface in known_interfaces {
                    let iface_norm = normalize_class_name(iface);
                    if visited.insert(iface_norm.clone()) {
                        queue.push_back(iface_norm);
                    }
                }
            }
        }

        false
    }

    /// Checks whether `class` represents an Item.
    pub fn is_item(&self, class: &str) -> bool {
        self.is_subclass_of(class, "net/minecraft/world/item/Item")
            || self.is_subclass_of(class, "net/minecraft/item/Item")
            || self.is_subclass_of(class, "Item")
            || simple_name(class).ends_with("Item")
    }

    /// Checks whether `class` represents a Block.
    pub fn is_block(&self, class: &str) -> bool {
        let s_name = simple_name(class);
        if s_name.ends_with("BlockItem") {
            return false;
        }
        self.is_subclass_of(class, "net/minecraft/world/level/block/Block")
            || self.is_subclass_of(class, "net/minecraft/block/Block")
            || self.is_subclass_of(class, "net/minecraft/world/level/block/state/BlockBehaviour")
            || self.is_subclass_of(class, "Block")
            || s_name.ends_with("Block")
    }

    /// Checks whether `class` represents Armor (helmet, chestplate, leggings, boots).
    pub fn is_armor(&self, class: &str) -> bool {
        let s_name = simple_name(class);
        self.is_subclass_of(class, "net/minecraft/world/item/ArmorItem")
            || self.is_subclass_of(class, "net/minecraft/item/ArmorItem")
            || self.is_subclass_of(class, "ArmorItem")
            || s_name.contains("Armor")
            || s_name.contains("Helmet")
            || s_name.contains("Chestplate")
            || s_name.contains("Leggings")
            || s_name.contains("Boots")
    }

    /// Checks whether `class` represents a Sword.
    pub fn is_sword(&self, class: &str) -> bool {
        let s_name = simple_name(class);
        self.is_subclass_of(class, "net/minecraft/world/item/SwordItem")
            || self.is_subclass_of(class, "net/minecraft/item/SwordItem")
            || self.is_subclass_of(class, "SwordItem")
            || s_name.contains("Sword")
            || s_name.contains("Blade")
            || s_name.contains("Katana")
            || s_name.contains("Saber")
    }

    /// Checks whether `class` represents an Axe.
    pub fn is_axe(&self, class: &str) -> bool {
        let s_name = simple_name(class);
        if self.is_pickaxe(class) {
            return false;
        }
        self.is_subclass_of(class, "net/minecraft/world/item/AxeItem")
            || self.is_subclass_of(class, "net/minecraft/item/AxeItem")
            || self.is_subclass_of(class, "AxeItem")
            || s_name.contains("Battleaxe")
            || s_name.contains("Hatchet")
            || (s_name.contains("Axe") && !s_name.contains("Pickaxe"))
    }

    /// Checks whether `class` represents a Pickaxe.
    pub fn is_pickaxe(&self, class: &str) -> bool {
        let s_name = simple_name(class);
        self.is_subclass_of(class, "net/minecraft/world/item/PickaxeItem")
            || self.is_subclass_of(class, "net/minecraft/item/PickaxeItem")
            || self.is_subclass_of(class, "PickaxeItem")
            || s_name.contains("Pickaxe")
            || s_name.contains("Mattock")
    }

    /// Checks whether `class` represents a Bow or Crossbow.
    pub fn is_bow(&self, class: &str) -> bool {
        let s_name = simple_name(class);
        self.is_subclass_of(class, "net/minecraft/world/item/BowItem")
            || self.is_subclass_of(class, "net/minecraft/item/BowItem")
            || self.is_subclass_of(class, "net/minecraft/world/item/CrossbowItem")
            || self.is_subclass_of(class, "net/minecraft/item/CrossbowItem")
            || self.is_subclass_of(class, "BowItem")
            || self.is_subclass_of(class, "CrossbowItem")
            || s_name.contains("Bow")
            || s_name.contains("Crossbow")
    }

    /// Checks whether `class` represents a Projectile or Throwing Entity / Item.
    pub fn is_projectile(&self, class: &str) -> bool {
        let s_name = simple_name(class);
        self.is_subclass_of(class, "net/minecraft/world/entity/projectile/Projectile")
            || self.is_subclass_of(class, "net/minecraft/world/item/ArrowItem")
            || self.is_subclass_of(class, "net/minecraft/item/ArrowItem")
            || self.is_subclass_of(class, "net/minecraft/entity/projectile/ProjectileEntity")
            || s_name.contains("Projectile")
            || s_name.contains("Arrow")
            || s_name.contains("Bullet")
    }

    /// Checks whether `class` represents a BlockEntity (TileEntity).
    pub fn is_block_entity(&self, class: &str) -> bool {
        let s_name = simple_name(class);
        self.is_subclass_of(class, "net/minecraft/world/level/block/entity/BlockEntity")
            || self.is_subclass_of(class, "net/minecraft/block/entity/BlockEntity")
            || self.is_subclass_of(class, "net/minecraft/tileentity/TileEntity")
            || self.is_subclass_of(class, "BlockEntity")
            || self.is_subclass_of(class, "TileEntity")
            || s_name.contains("BlockEntity")
            || s_name.contains("TileEntity")
    }
}

/// Known vanilla superclasses and interfaces for standard Minecraft types.
fn get_known_vanilla_parents(class: &str) -> Option<(Option<&'static str>, &'static [&'static str])> {
    match class {
        // Items (Mojang)
        "net/minecraft/world/item/SwordItem" => {
            Some((Some("net/minecraft/world/item/TieredItem"), &[]))
        }
        "net/minecraft/world/item/AxeItem" => {
            Some((Some("net/minecraft/world/item/DiggerItem"), &[]))
        }
        "net/minecraft/world/item/PickaxeItem" => {
            Some((Some("net/minecraft/world/item/DiggerItem"), &[]))
        }
        "net/minecraft/world/item/ShovelItem" => {
            Some((Some("net/minecraft/world/item/DiggerItem"), &[]))
        }
        "net/minecraft/world/item/HoeItem" => {
            Some((Some("net/minecraft/world/item/DiggerItem"), &[]))
        }
        "net/minecraft/world/item/DiggerItem" => {
            Some((Some("net/minecraft/world/item/TieredItem"), &[]))
        }
        "net/minecraft/world/item/TieredItem" => {
            Some((Some("net/minecraft/world/item/Item"), &[]))
        }
        "net/minecraft/world/item/ArmorItem" => {
            Some((Some("net/minecraft/world/item/Item"), &[]))
        }
        "net/minecraft/world/item/BlockItem" => {
            Some((Some("net/minecraft/world/item/Item"), &[]))
        }
        "net/minecraft/world/item/BowItem" => {
            Some((Some("net/minecraft/world/item/ProjectileWeaponItem"), &[]))
        }
        "net/minecraft/world/item/CrossbowItem" => {
            Some((Some("net/minecraft/world/item/ProjectileWeaponItem"), &[]))
        }
        "net/minecraft/world/item/ProjectileWeaponItem" => {
            Some((Some("net/minecraft/world/item/Item"), &[]))
        }
        "net/minecraft/world/item/ArrowItem" => {
            Some((Some("net/minecraft/world/item/Item"), &[]))
        }
        "net/minecraft/world/item/ShieldItem" => {
            Some((Some("net/minecraft/world/item/Item"), &[]))
        }
        "net/minecraft/world/item/PotionItem" => {
            Some((Some("net/minecraft/world/item/Item"), &[]))
        }
        "net/minecraft/world/item/Item" => {
            Some((Some("java/lang/Object"), &[]))
        }

        // Items (Fabric / Yarn)
        "net/minecraft/item/SwordItem" => {
            Some((Some("net/minecraft/item/ToolItem"), &[]))
        }
        "net/minecraft/item/AxeItem" => {
            Some((Some("net/minecraft/item/MiningToolItem"), &[]))
        }
        "net/minecraft/item/PickaxeItem" => {
            Some((Some("net/minecraft/item/MiningToolItem"), &[]))
        }
        "net/minecraft/item/ShovelItem" => {
            Some((Some("net/minecraft/item/MiningToolItem"), &[]))
        }
        "net/minecraft/item/HoeItem" => {
            Some((Some("net/minecraft/item/MiningToolItem"), &[]))
        }
        "net/minecraft/item/MiningToolItem" => {
            Some((Some("net/minecraft/item/ToolItem"), &[]))
        }
        "net/minecraft/item/ToolItem" => {
            Some((Some("net/minecraft/item/Item"), &[]))
        }
        "net/minecraft/item/ArmorItem" => {
            Some((Some("net/minecraft/item/Item"), &[]))
        }
        "net/minecraft/item/BlockItem" => {
            Some((Some("net/minecraft/item/Item"), &[]))
        }
        "net/minecraft/item/BowItem" => {
            Some((Some("net/minecraft/item/RangedWeaponItem"), &[]))
        }
        "net/minecraft/item/CrossbowItem" => {
            Some((Some("net/minecraft/item/RangedWeaponItem"), &[]))
        }
        "net/minecraft/item/RangedWeaponItem" => {
            Some((Some("net/minecraft/item/Item"), &[]))
        }
        "net/minecraft/item/ArrowItem" => {
            Some((Some("net/minecraft/item/Item"), &[]))
        }
        "net/minecraft/item/Item" => {
            Some((Some("java/lang/Object"), &[]))
        }

        // Blocks (Mojang)
        "net/minecraft/world/level/block/Block" => {
            Some((Some("net/minecraft/world/level/block/state/BlockBehaviour"), &[]))
        }
        "net/minecraft/world/level/block/state/BlockBehaviour" => {
            Some((Some("java/lang/Object"), &[]))
        }
        "net/minecraft/world/level/block/OreBlock" => {
            Some((Some("net/minecraft/world/level/block/Block"), &[]))
        }
        "net/minecraft/world/level/block/LeavesBlock" => {
            Some((Some("net/minecraft/world/level/block/Block"), &[]))
        }
        "net/minecraft/world/level/block/RotatedPillarBlock" => {
            Some((Some("net/minecraft/world/level/block/Block"), &[]))
        }
        "net/minecraft/world/level/block/GlassBlock" => {
            Some((Some("net/minecraft/world/level/block/AbstractGlassBlock"), &[]))
        }
        "net/minecraft/world/level/block/AbstractGlassBlock" => {
            Some((Some("net/minecraft/world/level/block/Block"), &[]))
        }
        "net/minecraft/world/level/block/BaseEntityBlock" => {
            Some((
                Some("net/minecraft/world/level/block/Block"),
                &["net/minecraft/world/level/block/EntityBlock"],
            ))
        }

        // Blocks (Fabric / Yarn)
        "net/minecraft/block/Block" => {
            Some((Some("net/minecraft/block/AbstractBlock"), &[]))
        }
        "net/minecraft/block/AbstractBlock" => {
            Some((Some("java/lang/Object"), &[]))
        }
        "net/minecraft/block/OreBlock" => {
            Some((Some("net/minecraft/block/Block"), &[]))
        }
        "net/minecraft/block/LeavesBlock" => {
            Some((Some("net/minecraft/block/Block"), &[]))
        }
        "net/minecraft/block/PillarBlock" => {
            Some((Some("net/minecraft/block/Block"), &[]))
        }
        "net/minecraft/block/GlassBlock" => {
            Some((Some("net/minecraft/block/AbstractGlassBlock"), &[]))
        }
        "net/minecraft/block/AbstractGlassBlock" => {
            Some((Some("net/minecraft/block/Block"), &[]))
        }
        "net/minecraft/block/BlockWithEntity" => {
            Some((Some("net/minecraft/block/Block"), &[]))
        }

        // Block Entities
        "net/minecraft/world/level/block/entity/BlockEntity" => {
            Some((Some("java/lang/Object"), &[]))
        }
        "net/minecraft/block/entity/BlockEntity" => {
            Some((Some("java/lang/Object"), &[]))
        }
        "net/minecraft/tileentity/TileEntity" => {
            Some((Some("java/lang/Object"), &[]))
        }

        // Entities / Projectiles
        "net/minecraft/world/entity/projectile/Projectile" => {
            Some((Some("net/minecraft/world/entity/Entity"), &[]))
        }
        "net/minecraft/world/entity/projectile/AbstractArrow" => {
            Some((Some("net/minecraft/world/entity/projectile/Projectile"), &[]))
        }
        "net/minecraft/world/entity/projectile/Arrow" => {
            Some((Some("net/minecraft/world/entity/projectile/AbstractArrow"), &[]))
        }
        "net/minecraft/world/entity/projectile/ThrowableItemProjectile" => {
            Some((Some("net/minecraft/world/entity/projectile/Projectile"), &[]))
        }
        "net/minecraft/entity/projectile/ProjectileEntity" => {
            Some((Some("net/minecraft/entity/Entity"), &[]))
        }
        "net/minecraft/entity/projectile/PersistentProjectileEntity" => {
            Some((Some("net/minecraft/entity/projectile/ProjectileEntity"), &[]))
        }
        "net/minecraft/entity/projectile/ArrowEntity" => {
            Some((Some("net/minecraft/entity/projectile/PersistentProjectileEntity"), &[]))
        }
        "net/minecraft/entity/projectile/thrown/ThrownItemEntity" => {
            Some((Some("net/minecraft/entity/projectile/ProjectileEntity"), &[]))
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_class_name() {
        assert_eq!(
            normalize_class_name("Lnet/minecraft/world/item/Item;"),
            "net/minecraft/world/item/Item"
        );
        assert_eq!(
            normalize_class_name("[Lnet.minecraft.world.item.SwordItem;"),
            "net/minecraft/world/item/SwordItem"
        );
        assert_eq!(
            normalize_class_name("com.example.mod.RubySword"),
            "com/example/mod/RubySword"
        );
    }

    #[test]
    fn test_subclass_hierarchy_traversal() {
        let mut hierarchy = ClassHierarchy::new();
        hierarchy.add_class(
            "com/example/mod/RubySword",
            Some("net/minecraft/world/item/SwordItem"),
            &[],
        );

        assert!(hierarchy.is_subclass_of("com/example/mod/RubySword", "net/minecraft/world/item/SwordItem"));
        assert!(hierarchy.is_subclass_of("com/example/mod/RubySword", "net/minecraft/world/item/TieredItem"));
        assert!(hierarchy.is_subclass_of("com/example/mod/RubySword", "net/minecraft/world/item/Item"));
        assert!(hierarchy.is_subclass_of("com/example/mod/RubySword", "Item"));
        assert!(hierarchy.is_subclass_of("com/example/mod/RubySword", "SwordItem"));
        assert!(!hierarchy.is_subclass_of("com/example/mod/RubySword", "net/minecraft/world/level/block/Block"));
    }

    #[test]
    fn test_interface_traversal() {
        let mut hierarchy = ClassHierarchy::new();
        hierarchy.add_class(
            "com/example/mod/MagicBlock",
            Some("net/minecraft/world/level/block/Block"),
            &["com/example/mod/Ticking", "com/example/mod/Enchantable"],
        );
        hierarchy.add_class(
            "com/example/mod/Enchantable",
            None,
            &["com/example/mod/MagicMarker"],
        );

        assert!(hierarchy.is_subclass_of("com/example/mod/MagicBlock", "com/example/mod/Ticking"));
        assert!(hierarchy.is_subclass_of("com/example/mod/MagicBlock", "com/example/mod/Enchantable"));
        assert!(hierarchy.is_subclass_of("com/example/mod/MagicBlock", "com/example/mod/MagicMarker"));
        assert!(hierarchy.is_subclass_of("com/example/mod/MagicBlock", "Block"));
    }

    #[test]
    fn test_cycle_safety() {
        let mut hierarchy = ClassHierarchy::new();
        hierarchy.add_class("ClassA", Some("ClassB"), &[]);
        hierarchy.add_class("ClassB", Some("ClassA"), &[]);

        // Should return false and terminate without stack overflow
        assert!(!hierarchy.is_subclass_of("ClassA", "TargetClass"));
    }

    #[test]
    fn test_helper_predicates() {
        let mut hierarchy = ClassHierarchy::new();
        hierarchy.add_class(
            "com/example/mod/TitaniumAxe",
            Some("net/minecraft/world/item/AxeItem"),
            &[],
        );
        hierarchy.add_class(
            "com/example/mod/ObsidianChestplate",
            Some("net/minecraft/world/item/ArmorItem"),
            &[],
        );
        hierarchy.add_class(
            "com/example/mod/CopperFurnaceBlock",
            Some("net/minecraft/world/level/block/Block"),
            &[],
        );

        assert!(hierarchy.is_axe("com/example/mod/TitaniumAxe"));
        assert!(hierarchy.is_item("com/example/mod/TitaniumAxe"));
        assert!(!hierarchy.is_pickaxe("com/example/mod/TitaniumAxe"));

        assert!(hierarchy.is_armor("com/example/mod/ObsidianChestplate"));
        assert!(hierarchy.is_item("com/example/mod/ObsidianChestplate"));

        assert!(hierarchy.is_block("com/example/mod/CopperFurnaceBlock"));
        assert!(!hierarchy.is_item("com/example/mod/CopperFurnaceBlock"));
    }
}
