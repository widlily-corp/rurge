use std::fs::{self, File};
use std::path::PathBuf;
use tempfile::tempdir;

use cargo_rurge::commands::new::{self, NewArgs, TemplateFlavor};
use cargo_rurge::commands::pack::{self, PackArgs};
use cargo_rurge::commands::run::{self, RunArgs};
use cargo_rurge::{CargoRurgeError, Cli, Commands};
use clap::Parser;
use rurge_core::manifest::ModManifest;
use rurge_loader::{current_target_triple, RmodArchive};

#[test]
fn test_cli_parsing_direct_and_cargo_subcommand() {
    // Direct invocation: cargo-rurge new my_mod
    let direct = Cli::try_parse_from(["cargo-rurge", "new", "my_mod"]).expect("Failed to parse direct CLI");
    match direct {
        Cli::New(args) => {
            assert_eq!(args.name, "my_mod");
            assert_eq!(args.template, TemplateFlavor::Hud);
        }
        _ => panic!("Expected Cli::New"),
    }

    // Cargo invocation: cargo-rurge rurge new my_mod --template block
    let cargo_sub = Cli::try_parse_from(["cargo-rurge", "rurge", "new", "my_mod", "--template", "block"])
        .expect("Failed to parse cargo subcommand");
    match cargo_sub {
        Cli::Rurge(Commands::New(args)) => {
            assert_eq!(args.name, "my_mod");
            assert_eq!(args.template, TemplateFlavor::Block);
        }
        _ => panic!("Expected Cli::Rurge(Commands::New)"),
    }

    // Direct pack invocation
    let pack_cli = Cli::try_parse_from(["cargo-rurge", "pack", "--release"]).expect("Failed to parse pack CLI");
    match pack_cli {
        Cli::Pack(args) => {
            assert!(args.release);
        }
        _ => panic!("Expected Cli::Pack"),
    }

    // Direct run invocation
    let run_cli = Cli::try_parse_from(["cargo-rurge", "run", "--release", "--", "--custom-arg"])
        .expect("Failed to parse run CLI");
    match run_cli {
        Cli::Run(args) => {
            assert!(args.release);
            assert_eq!(args.args, vec!["--custom-arg"]);
        }
        _ => panic!("Expected Cli::Run"),
    }

    // Cargo subcommand run invocation
    let cargo_run = Cli::try_parse_from(["cargo-rurge", "rurge", "run"])
        .expect("Failed to parse cargo rurge run");
    match cargo_run {
        Cli::Rurge(Commands::Run(args)) => {
            assert!(!args.release);
            assert!(args.args.is_empty());
        }
        _ => panic!("Expected Cli::Rurge(Commands::Run)"),
    }
}

#[test]
fn test_cli_parsing_run_options() {
    let parsed = Cli::try_parse_from([
        "cargo-rurge",
        "run",
        "--release",
        "--dry-run",
        "--no-build",
        "--mods-dir",
        "./custom_mods",
        "--path",
        "./my_mod",
        "--target",
        "x86_64-pc-windows-msvc",
        "--",
        "--windowed",
    ])
    .expect("Failed to parse full run options");

    match parsed {
        Cli::Run(args) => {
            assert!(args.release);
            assert!(args.dry_run);
            assert!(args.no_build);
            assert_eq!(args.mods_dir, Some(PathBuf::from("./custom_mods")));
            assert_eq!(args.path, Some(PathBuf::from("./my_mod")));
            assert_eq!(args.target, Some("x86_64-pc-windows-msvc".to_string()));
            assert_eq!(args.args, vec!["--windowed"]);
        }
        _ => panic!("Expected Cli::Run"),
    }
}

#[test]
fn test_scaffold_new_mod_hud_default() {
    let tmp = tempdir().expect("Failed to create tempdir");
    let mod_dir = tmp.path().join("hud_compass");

    let args = NewArgs {
        name: "hud_compass".to_string(),
        template: TemplateFlavor::Hud,
        path: Some(mod_dir.clone()),
        force: false,
    };

    new::execute(args).expect("new::execute failed");

    // Verify files exist
    assert!(mod_dir.join("Cargo.toml").exists());
    assert!(mod_dir.join("rurge.toml").exists());
    assert!(mod_dir.join("src/lib.rs").exists());
    assert!(mod_dir.join("assets").is_dir());
    assert!(mod_dir.join("data").is_dir());

    // Verify rurge.toml manifest
    let manifest = ModManifest::from_file(mod_dir.join("rurge.toml")).expect("Failed to parse generated rurge.toml");
    assert_eq!(manifest.id, "hud_compass");
    assert_eq!(manifest.name, "hud_compass");
    assert_eq!(manifest.version, semver::Version::new(0, 1, 0));

    // Verify Cargo.toml contains cdylib crate-type
    let cargo_toml = fs::read_to_string(mod_dir.join("Cargo.toml")).expect("Failed to read Cargo.toml");
    let parsed_cargo: toml::Value = toml::from_str(&cargo_toml).expect("Cargo.toml is not valid TOML");
    assert_eq!(
        parsed_cargo["package"]["name"].as_str(),
        Some("hud_compass")
    );
    let crate_types = parsed_cargo["lib"]["crate-type"]
        .as_array()
        .expect("lib.crate-type should be an array");
    let crate_types_str: Vec<&str> = crate_types.iter().filter_map(|v| v.as_str()).collect();
    assert!(crate_types_str.contains(&"cdylib"));
    assert!(crate_types_str.contains(&"rlib"));

    // Verify src/lib.rs contains RurgePlugin and export_rurge_plugin!
    let lib_rs = fs::read_to_string(mod_dir.join("src/lib.rs")).expect("Failed to read src/lib.rs");
    assert!(lib_rs.contains("RurgePlugin"));
    assert!(lib_rs.contains("export_rurge_plugin!"));
}

#[test]
fn test_scaffold_new_mod_flavors() {
    for (flavor, keyword) in [
        (TemplateFlavor::Hud, "HUD"),
        (TemplateFlavor::Block, "Block"),
        (TemplateFlavor::Shader, "Shader"),
    ] {
        let tmp = tempdir().expect("Failed to create tempdir");
        let mod_dir = tmp.path().join("flavor_test");

        let args = NewArgs {
            name: "flavor_test".to_string(),
            template: flavor,
            path: Some(mod_dir.clone()),
            force: false,
        };

        new::execute(args).expect("Failed to scaffold flavor");
        let lib_rs = fs::read_to_string(mod_dir.join("src/lib.rs")).expect("Failed to read lib.rs");
        assert!(lib_rs.contains(keyword));
    }
}

#[test]
fn test_scaffold_invalid_mod_id() {
    let tmp = tempdir().expect("Failed to create tempdir");
    let mod_dir = tmp.path().join("invalid_dir");

    let args = NewArgs {
        name: "invalid id with spaces!".to_string(),
        template: TemplateFlavor::Hud,
        path: Some(mod_dir),
        force: false,
    };

    let result = new::execute(args);
    assert!(result.is_err(), "Expected error for invalid mod ID");
}

#[test]
fn test_scaffold_non_empty_dir_without_force() {
    let tmp = tempdir().expect("Failed to create tempdir");
    let mod_dir = tmp.path().join("existing_dir");
    fs::create_dir_all(&mod_dir).unwrap();
    File::create(mod_dir.join("existing_file.txt")).unwrap();

    let args = NewArgs {
        name: "test_existing".to_string(),
        template: TemplateFlavor::Hud,
        path: Some(mod_dir.clone()),
        force: false,
    };

    let result = new::execute(args);
    assert!(result.is_err(), "Expected error when targeting non-empty directory without --force");

    // With force = true it should succeed
    let force_args = NewArgs {
        name: "test_existing".to_string(),
        template: TemplateFlavor::Hud,
        path: Some(mod_dir.clone()),
        force: true,
    };
    new::execute(force_args).expect("Scaffolding with --force should succeed");
    assert!(mod_dir.join("rurge.toml").exists());
}

#[test]
fn test_pack_mod_creates_valid_rmod_archive() {
    let tmp = tempdir().expect("Failed to create tempdir");
    let project_dir = tmp.path().join("test_pack_mod");
    fs::create_dir_all(&project_dir).unwrap();

    // 1. Create rurge.toml
    let manifest_toml = r#"
        id = "test_pack_mod"
        name = "Test Pack Mod"
        version = "1.2.3"
        description = "A mod built for pack testing"
        authors = ["Pack Tester"]
    "#;
    fs::write(project_dir.join("rurge.toml"), manifest_toml).unwrap();

    // 2. Create mock compiled binary in target/release
    let target_dir = project_dir.join("target").join("release");
    fs::create_dir_all(&target_dir).unwrap();

    #[cfg(target_os = "windows")]
    let dylib_name = "test_pack_mod.dll";
    #[cfg(target_os = "linux")]
    let dylib_name = "libtest_pack_mod.so";
    #[cfg(target_os = "macos")]
    let dylib_name = "libtest_pack_mod.dylib";

    let binary_bytes = b"fake-compiled-cdylib-bytes";
    fs::write(target_dir.join(dylib_name), binary_bytes).unwrap();

    // 3. Create assets and data files
    let assets_dir = project_dir.join("assets").join("textures");
    fs::create_dir_all(&assets_dir).unwrap();
    fs::write(assets_dir.join("compass.png"), b"compass-texture-data").unwrap();

    let data_dir = project_dir.join("data");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("hud_layout.json"), b"{\"x\": 10, \"y\": 20}").unwrap();

    // 4. Pack into out_dir
    let out_dir = tmp.path().join("dist");
    let pack_args = PackArgs {
        path: Some(project_dir.clone()),
        out: Some(out_dir.clone()),
        release: true,
        target: None,
        binary: None,
    };

    let rmod_path = pack::execute(pack_args).expect("pack::execute failed");
    assert!(rmod_path.exists());
    assert_eq!(rmod_path.file_name().unwrap(), "test_pack_mod-1.2.3.rmod");

    // 5. Verify using rurge-loader RmodArchive
    let archive = RmodArchive::open(&rmod_path).expect("Failed to open packed .rmod with RmodArchive");
    let manifest = archive.read_manifest().expect("Failed to read manifest from packed archive");
    assert_eq!(manifest.id, "test_pack_mod");
    assert_eq!(manifest.version, semver::Version::new(1, 2, 3));

    // 6. Verify binary extraction
    let extract_dir = tmp.path().join("extracted");
    let extracted_bin = archive
        .extract_binary(current_target_triple(), &extract_dir)
        .expect("Failed to extract binary matching host triple");
    assert!(extracted_bin.exists());
    let extracted_bytes = fs::read(extracted_bin).unwrap();
    assert_eq!(extracted_bytes, binary_bytes);
}

#[test]
fn test_run_command_dev_link_and_dry_run() {
    let tmp = tempdir().expect("Failed to create tempdir");
    let project_dir = tmp.path().join("my_dev_mod");
    fs::create_dir_all(&project_dir).unwrap();

    let manifest_toml = r#"
        id = "my_dev_mod"
        name = "My Dev Mod"
        version = "0.5.0"
        description = "Dev mod for run testing"
        authors = ["Dev"]
    "#;
    fs::write(project_dir.join("rurge.toml"), manifest_toml).unwrap();

    let target_debug_dir = project_dir.join("target").join("debug");
    fs::create_dir_all(&target_debug_dir).unwrap();

    #[cfg(target_os = "windows")]
    let dylib_name = "my_dev_mod.dll";
    #[cfg(target_os = "linux")]
    let dylib_name = "libmy_dev_mod.so";
    #[cfg(target_os = "macos")]
    let dylib_name = "libmy_dev_mod.dylib";

    let fake_binary = b"dev-mode-binary-payload";
    fs::write(target_debug_dir.join(dylib_name), fake_binary).unwrap();

    let mods_dir = tmp.path().join("dev_mods");

    let run_args = RunArgs {
        release: false,
        path: Some(project_dir.clone()),
        mods_dir: Some(mods_dir.clone()),
        target: None,
        no_build: true, // skip invoking cargo build on mock
        dry_run: true,  // do not launch real client binary
        args: vec![],
    };

    run::execute(run_args).expect("run::execute failed in dry-run mode");

    let linked_rmod = mods_dir.join("my_dev_mod-0.5.0.rmod");
    assert!(linked_rmod.is_file(), "Linked .rmod package was not found in mods_dir");

    // Verify the deployed archive
    let archive = RmodArchive::open(&linked_rmod).expect("Failed to open linked .rmod package");
    let manifest = archive.read_manifest().expect("Failed to read manifest");
    assert_eq!(manifest.id, "my_dev_mod");
    assert_eq!(manifest.version, semver::Version::new(0, 5, 0));

    let extract_dir = tmp.path().join("extracted_dev");
    let extracted_bin = archive
        .extract_binary(current_target_triple(), &extract_dir)
        .expect("Failed to extract deployed binary");
    assert_eq!(fs::read(extracted_bin).unwrap(), fake_binary);
}

#[test]
fn test_run_command_link_file_fallback() {
    let tmp = tempdir().expect("Failed to create tempdir");
    let src = tmp.path().join("source.txt");
    let dst = tmp.path().join("dest").join("linked.txt");

    fs::write(&src, b"link-test-data-v1").unwrap();
    run::link_file(&src, &dst).expect("link_file failed");
    assert_eq!(fs::read(&dst).unwrap(), b"link-test-data-v1");

    // Overwriting source and re-linking replaces destination cleanly
    fs::write(&src, b"link-test-data-v2").unwrap();
    run::link_file(&src, &dst).expect("re-link failed");
    assert_eq!(fs::read(&dst).unwrap(), b"link-test-data-v2");
}

#[test]
fn test_run_command_missing_manifest_error() {
    let tmp = tempdir().expect("Failed to create tempdir");
    let empty_dir = tmp.path().join("empty_crate");
    fs::create_dir_all(&empty_dir).unwrap();

    let run_args = RunArgs {
        release: false,
        path: Some(empty_dir.clone()),
        mods_dir: None,
        target: None,
        no_build: true,
        dry_run: true,
        args: vec![],
    };

    let result = run::execute(run_args);
    match result {
        Err(CargoRurgeError::ManifestNotFound(path)) => {
            assert_eq!(path, empty_dir.join("rurge.toml"));
        }
        other => panic!("Expected ManifestNotFound, got {other:?}"),
    }
}
