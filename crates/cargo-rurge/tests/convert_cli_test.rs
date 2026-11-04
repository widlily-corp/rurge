use cargo_rurge::commands::convert::{self, ConvertArgs};
use cargo_rurge::{CargoRurgeError, Cli, Commands};
use clap::Parser;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::tempdir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[test]
fn test_cli_parsing_convert_direct() {
    let args = vec!["cargo-rurge", "convert", "my-mod.jar"];
    let cli = Cli::try_parse_from(args).expect("should parse convert subcommand");
    match cli {
        Cli::Convert(ConvertArgs {
            jar_path,
            output,
            inspect,
        }) => {
            assert_eq!(jar_path, PathBuf::from("my-mod.jar"));
            assert_eq!(output, None);
            assert!(!inspect);
        }
        _ => panic!("Expected Cli::Convert"),
    }
}

#[test]
fn test_cli_parsing_convert_with_options() {
    let args = vec![
        "cargo-rurge",
        "convert",
        "path/to/mod.jar",
        "-o",
        "output/mod.rmod",
        "--inspect",
    ];
    let cli = Cli::try_parse_from(args).expect("should parse convert with options");
    match cli {
        Cli::Convert(ConvertArgs {
            jar_path,
            output,
            inspect,
        }) => {
            assert_eq!(jar_path, PathBuf::from("path/to/mod.jar"));
            assert_eq!(output, Some(PathBuf::from("output/mod.rmod")));
            assert!(inspect);
        }
        _ => panic!("Expected Cli::Convert"),
    }
}

#[test]
fn test_cli_parsing_cargo_subcommand_convert() {
    let args = vec![
        "cargo-rurge",
        "rurge",
        "convert",
        "mod.jar",
        "--output",
        "custom.rmod",
    ];
    let cli = Cli::try_parse_from(args).expect("should parse cargo rurge convert");
    match cli {
        Cli::Rurge(Commands::Convert(ConvertArgs {
            jar_path,
            output,
            inspect,
        })) => {
            assert_eq!(jar_path, PathBuf::from("mod.jar"));
            assert_eq!(output, Some(PathBuf::from("custom.rmod")));
            assert!(!inspect);
        }
        _ => panic!("Expected Cli::Rurge(Commands::Convert)"),
    }
}

fn create_minimal_test_jar(path: &std::path::Path) {
    let file = File::create(path).expect("create test jar");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    let manifest = r#"{
        "schemaVersion": 1,
        "id": "cli_test_mod",
        "name": "CLI Test Mod",
        "version": "1.0.0",
        "description": "CLI converter integration test.",
        "authors": ["Tester"]
    }"#;
    zip.start_file("fabric.mod.json", options).unwrap();
    zip.write_all(manifest.as_bytes()).unwrap();

    let lang = r#"{"item.cli_test_mod.test_item": "Test Item"}"#;
    zip.start_file("assets/cli_test_mod/lang/en_us.json", options).unwrap();
    zip.write_all(lang.as_bytes()).unwrap();

    zip.finish().unwrap();
}

#[test]
fn test_convert_cli_execution_generates_package() {
    let temp = tempdir().expect("temp dir");
    let jar_path = temp.path().join("input.jar");
    let out_rmod = temp.path().join("output.rmod");

    create_minimal_test_jar(&jar_path);

    let args = ConvertArgs {
        jar_path,
        output: Some(out_rmod.clone()),
        inspect: false,
    };

    convert::execute(args).expect("convert execute should succeed");
    assert!(out_rmod.is_file());

    // Verify .rmod contents
    let rmod_file = File::open(&out_rmod).expect("open rmod");
    let mut archive = zip::ZipArchive::new(rmod_file).expect("open zip");
    assert!(archive.by_name("manifest.toml").is_ok());
    assert!(archive.by_name("content.json").is_ok());
    assert!(archive.by_name("compatibility-report.json").is_ok());
}

#[test]
fn test_convert_cli_inspect_does_not_generate_file() {
    let temp = tempdir().expect("temp dir");
    let jar_path = temp.path().join("inspect_mod.jar");
    let out_rmod = temp.path().join("should_not_exist.rmod");

    create_minimal_test_jar(&jar_path);

    let args = ConvertArgs {
        jar_path,
        output: Some(out_rmod.clone()),
        inspect: true,
    };

    convert::execute(args).expect("convert inspect should succeed");
    assert!(!out_rmod.exists());
}

#[test]
fn test_convert_cli_nonexistent_jar_returns_error() {
    let temp = tempdir().expect("temp dir");
    let non_existent = temp.path().join("non_existent.jar");

    let args = ConvertArgs {
        jar_path: non_existent,
        output: None,
        inspect: false,
    };

    let res = convert::execute(args);
    assert!(res.is_err());
    match res {
        Err(CargoRurgeError::Converter(_)) => {}
        other => panic!("Expected Converter error, got: {other:?}"),
    }
}
