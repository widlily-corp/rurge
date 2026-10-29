use std::collections::HashMap;
use rurge_core::manifest::{ModEnvironment, ModManifest};
use rurge_core::resolver::{DependencyResolver, ResolverError};

fn helper_manifest(
    id: &str,
    version: &str,
    deps: &[(&str, &str)],
    incompat: &[(&str, &str)],
) -> ModManifest {
    let mut dependencies = HashMap::new();
    for (k, v) in deps {
        dependencies.insert(k.to_string(), semver::VersionReq::parse(v).unwrap());
    }

    let mut incompatibilities = HashMap::new();
    for (k, v) in incompat {
        incompatibilities.insert(k.to_string(), semver::VersionReq::parse(v).unwrap());
    }

    ModManifest {
        id: id.to_string(),
        name: format!("Mod {id}"),
        version: semver::Version::parse(version).unwrap(),
        description: Some(format!("Description for {id}")),
        authors: vec!["Author".to_string()],
        entrypoint: None,
        environment: ModEnvironment::Both,
        dependencies,
        incompatibilities,
    }
}

#[test]
fn test_manifest_parse_valid_full() {
    let toml_content = r#"
        id = "create_rust"
        name = "Create: Rust Edition"
        version = "1.2.3"
        description = "A mechanical engineering mod"
        authors = ["Alice", "Bob"]
        entrypoint = "create_rust_init"
        environment = "Both"

        [dependencies]
        base_lib = "^0.5.0"
        physics_core = ">=1.0.0"

        [incompatibilities]
        old_engine = "<2.0.0"
    "#;

    let manifest = ModManifest::from_toml_str(toml_content).expect("Failed to parse manifest");
    assert_eq!(manifest.id, "create_rust");
    assert_eq!(manifest.name, "Create: Rust Edition");
    assert_eq!(manifest.version, semver::Version::parse("1.2.3").unwrap());
    assert_eq!(manifest.description.as_deref(), Some("A mechanical engineering mod"));
    assert_eq!(manifest.authors, vec!["Alice", "Bob"]);
    assert_eq!(manifest.entrypoint.as_deref(), Some("create_rust_init"));
    assert_eq!(manifest.environment, ModEnvironment::Both);
    assert_eq!(manifest.dependencies.len(), 2);
    assert_eq!(manifest.incompatibilities.len(), 1);
}

#[test]
fn test_manifest_parse_minimal_defaults() {
    let toml_content = r#"
        id = "minimal_mod"
        name = "Minimal Mod"
        version = "0.1.0"
    "#;

    let manifest = ModManifest::from_toml_str(toml_content).expect("Failed to parse minimal manifest");
    assert_eq!(manifest.id, "minimal_mod");
    assert_eq!(manifest.name, "Minimal Mod");
    assert_eq!(manifest.version, semver::Version::parse("0.1.0").unwrap());
    assert_eq!(manifest.environment, ModEnvironment::Both);
    assert!(manifest.description.is_none());
    assert!(manifest.authors.is_empty());
    assert!(manifest.entrypoint.is_none());
    assert!(manifest.dependencies.is_empty());
    assert!(manifest.incompatibilities.is_empty());
}

#[test]
fn test_manifest_invalid_id() {
    let toml_content = r#"
        id = "invalid mod with spaces!"
        name = "Invalid Mod"
        version = "1.0.0"
    "#;

    let result = ModManifest::from_toml_str(toml_content);
    assert!(result.is_err(), "Manifest with spaces in ID should fail");
}

#[test]
fn test_resolver_linear_dependency() {
    // A depends on B, B depends on C
    // Expected order: [C, B, A]
    let mod_a = helper_manifest("mod_a", "1.0.0", &[("mod_b", "^1.0")], &[]);
    let mod_b = helper_manifest("mod_b", "1.2.0", &[("mod_c", "^1.0")], &[]);
    let mod_c = helper_manifest("mod_c", "1.5.0", &[], &[]);

    let manifests = vec![mod_a, mod_b, mod_c];
    let resolved = DependencyResolver::resolve(manifests).expect("Linear resolution should succeed");

    let ids: Vec<&str> = resolved.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["mod_c", "mod_b", "mod_a"]);
}

#[test]
fn test_resolver_diamond_dependency() {
    // A depends on B and C.
    // B and C depend on D.
    // Expected: D comes first, B and C come before A, A comes last.
    let mod_a = helper_manifest("mod_a", "1.0.0", &[("mod_b", "^1.0"), ("mod_c", "^1.0")], &[]);
    let mod_b = helper_manifest("mod_b", "1.0.0", &[("mod_d", "^1.0")], &[]);
    let mod_c = helper_manifest("mod_c", "1.0.0", &[("mod_d", "^1.0")], &[]);
    let mod_d = helper_manifest("mod_d", "1.0.0", &[], &[]);

    let manifests = vec![mod_a, mod_b, mod_c, mod_d];
    let resolved = DependencyResolver::resolve(manifests).expect("Diamond resolution should succeed");

    let ids: Vec<&str> = resolved.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids.len(), 4);
    assert_eq!(ids[0], "mod_d");
    assert_eq!(ids[3], "mod_a");

    let pos_b = ids.iter().position(|&x| x == "mod_b").unwrap();
    let pos_c = ids.iter().position(|&x| x == "mod_c").unwrap();
    let pos_d = ids.iter().position(|&x| x == "mod_d").unwrap();
    let pos_a = ids.iter().position(|&x| x == "mod_a").unwrap();

    assert!(pos_d < pos_b);
    assert!(pos_d < pos_c);
    assert!(pos_b < pos_a);
    assert!(pos_c < pos_a);
}

#[test]
fn test_resolver_missing_dependency() {
    let mod_a = helper_manifest("mod_a", "1.0.0", &[("missing_lib", "^2.0.0")], &[]);
    let manifests = vec![mod_a];

    let result = DependencyResolver::resolve(manifests);
    match result {
        Err(ResolverError::MissingDependency { dependent, required, requirement }) => {
            assert_eq!(dependent, "mod_a");
            assert_eq!(required, "missing_lib");
            assert_eq!(requirement, "^2.0.0");
        }
        other => panic!("Expected MissingDependency, got {other:?}"),
    }
}

#[test]
fn test_resolver_version_mismatch() {
    let mod_a = helper_manifest("mod_a", "1.0.0", &[("mod_b", "^2.0.0")], &[]);
    let mod_b = helper_manifest("mod_b", "1.5.0", &[], &[]);
    let manifests = vec![mod_a, mod_b];

    let result = DependencyResolver::resolve(manifests);
    match result {
        Err(ResolverError::MissingDependency { dependent, required, requirement }) => {
            assert_eq!(dependent, "mod_a");
            assert_eq!(required, "mod_b");
            assert_eq!(requirement, "^2.0.0");
        }
        other => panic!("Expected MissingDependency for version mismatch, got {other:?}"),
    }
}

#[test]
fn test_resolver_incompatibility_conflict() {
    let mod_a = helper_manifest("mod_a", "1.0.0", &[], &[("mod_b", ">=1.0.0")]);
    let mod_b = helper_manifest("mod_b", "1.2.0", &[], &[]);
    let manifests = vec![mod_a, mod_b];

    let result = DependencyResolver::resolve(manifests);
    match result {
        Err(ResolverError::IncompatibleMod { mod_a, mod_b, version_b }) => {
            assert_eq!(mod_a, "mod_a");
            assert_eq!(mod_b, "mod_b");
            assert_eq!(version_b, "1.2.0");
        }
        other => panic!("Expected IncompatibleMod error, got {other:?}"),
    }
}

#[test]
fn test_resolver_circular_dependency() {
    // A depends on B, B depends on A
    let mod_a = helper_manifest("mod_a", "1.0.0", &[("mod_b", "^1.0")], &[]);
    let mod_b = helper_manifest("mod_b", "1.0.0", &[("mod_a", "^1.0")], &[]);
    let manifests = vec![mod_a, mod_b];

    let result = DependencyResolver::resolve(manifests);
    match result {
        Err(ResolverError::CircularDependency { cycle }) => {
            assert!(cycle.contains(&"mod_a".to_string()));
            assert!(cycle.contains(&"mod_b".to_string()));
        }
        other => panic!("Expected CircularDependency error, got {other:?}"),
    }
}

#[test]
fn test_resolver_duplicate_mod_id() {
    let mod_a1 = helper_manifest("mod_a", "1.0.0", &[], &[]);
    let mod_a2 = helper_manifest("mod_a", "2.0.0", &[], &[]);
    let manifests = vec![mod_a1, mod_a2];

    let result = DependencyResolver::resolve(manifests);
    match result {
        Err(ResolverError::DuplicateModId { id }) => {
            assert_eq!(id, "mod_a");
        }
        other => panic!("Expected DuplicateModId error, got {other:?}"),
    }
}

#[test]
fn test_resolver_self_dependency_cycle() {
    let mod_a = helper_manifest("mod_a", "1.0.0", &[("mod_a", "^1.0")], &[]);
    let manifests = vec![mod_a];

    let result = DependencyResolver::resolve(manifests);
    match result {
        Err(ResolverError::CircularDependency { cycle }) => {
            assert!(cycle.contains(&"mod_a".to_string()));
        }
        other => panic!("Expected CircularDependency error for self-dependency, got {other:?}"),
    }
}

#[test]
fn test_resolver_three_node_cycle() {
    // A -> B -> C -> A
    let mod_a = helper_manifest("mod_a", "1.0.0", &[("mod_b", "^1.0")], &[]);
    let mod_b = helper_manifest("mod_b", "1.0.0", &[("mod_c", "^1.0")], &[]);
    let mod_c = helper_manifest("mod_c", "1.0.0", &[("mod_a", "^1.0")], &[]);
    let manifests = vec![mod_a, mod_b, mod_c];

    let result = DependencyResolver::resolve(manifests);
    match result {
        Err(ResolverError::CircularDependency { cycle }) => {
            assert!(cycle.contains(&"mod_a".to_string()));
            assert!(cycle.contains(&"mod_b".to_string()));
            assert!(cycle.contains(&"mod_c".to_string()));
        }
        other => panic!("Expected CircularDependency error for 3-node cycle, got {other:?}"),
    }
}

#[test]
fn test_resolver_disconnected_components() {
    // A depends on B. C is independent.
    let mod_a = helper_manifest("mod_a", "1.0.0", &[("mod_b", "^1.0")], &[]);
    let mod_b = helper_manifest("mod_b", "1.0.0", &[], &[]);
    let mod_c = helper_manifest("mod_c", "1.0.0", &[], &[]);
    let manifests = vec![mod_a, mod_c, mod_b];

    let resolved = DependencyResolver::resolve(manifests).expect("Disconnected resolution should succeed");
    let ids: Vec<&str> = resolved.iter().map(|m| m.id.as_str()).collect();

    let pos_a = ids.iter().position(|&x| x == "mod_a").unwrap();
    let pos_b = ids.iter().position(|&x| x == "mod_b").unwrap();
    assert!(pos_b < pos_a);
    assert_eq!(ids.len(), 3);
}

#[test]
fn test_manifest_from_file() {
    let temp_dir = std::env::temp_dir();
    let manifest_path = temp_dir.join(format!("rurge_test_manifest_{}.toml", std::process::id()));
    let content = r#"
        id = "file_mod"
        name = "File Mod"
        version = "1.0.0"
        environment = "Client"
    "#;
    std::fs::write(&manifest_path, content).unwrap();

    let manifest = ModManifest::from_file(&manifest_path).expect("from_file should parse");
    assert_eq!(manifest.id, "file_mod");
    assert_eq!(manifest.environment, ModEnvironment::Client);

    let _ = std::fs::remove_file(&manifest_path);
}
