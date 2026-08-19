use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use flatbuffers_reflection::reflection::{self, BaseType};
use minijinja::{AutoEscape, Environment, Value, context};
use serde::Serialize;
use sha2::{Digest, Sha256};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[allow(dead_code)]
#[path = "../../templates/rust/src/actuator_outputs_contract.rs"]
mod actuator_outputs_contract;

include!("protocol.rs");
include!("identity.rs");
include!("cdr.rs");
include!("actuator_vectors.rs");
#[derive(Debug)]
struct Tools {
    package_version: String,
    flatbuffers_version: String,
    flatbuffers_commit: String,
    flatcc_version: String,
    flatcc_commit: String,
    mcap_rust_version: String,
}

const LOCAL_PACKAGE_VERSION: &str = "0.0.0";
const FLATBUFFERS_VERSION: &str = "25.12.19";
const FLATBUFFERS_COMMIT: &str = "7e163021e59cca4f8e1e35a7c828b5c6b7915953";
const FLATCC_VERSION: &str = "0.6.1";
const FLATCC_COMMIT: &str = "d17e324e7e595272da486c5b9b20e848b78ba9ba";
const MCAP_RUST_VERSION: &str = "0.25.0";

#[derive(Debug)]
struct Options {
    release_name: String,
    update: bool,
}

fn main() -> Result<()> {
    let root = find_repo_root(&env::current_dir()?)?;
    let (command, options) = parse_args()?;
    enforce_binding_language_policy(&root)?;
    validate_actuator_output_vectors(&root)?;

    match command.as_str() {
        "build" => build(&root, &options),
        "ci" => ci(&root, &options),
        "check" => check(&root),
        "wire-check" => wire_check_command(&root, &options),
        _ => fail(format!(
            "unknown command '{command}'. expected: build, ci, check, or wire-check"
        )),
    }
}

fn build(root: &Path, options: &Options) -> Result<()> {
    let tools = tools(package_version(&options.release_name)?);
    let templates = Templates::new(root)?;

    let packages = stage_packages(root, &templates, &tools)?;
    check_pins(&packages, &tools)?;
    let flatc = build_flatc(&tools)?;
    let flatcc = flatcc_tool(&tools)?;
    generate_bindings(root, &flatc, &flatcc.binary, &templates, &packages)?;
    build_archives(root, &tools, &flatcc, &options.release_name, true)?;

    Ok(())
}

fn check(root: &Path) -> Result<()> {
    let tools = tools(LOCAL_PACKAGE_VERSION);
    let flatcc = flatcc_binary(&tools)?;
    let check_dir = root.join("target/xtask/check");
    reset_dir(&check_dir)?;
    let bfbs_dir = check_dir.join("bfbs");
    generate_reflection_schemas(root, &flatcc, &bfbs_dir)?;
    let schema = load_compiled_schema(&bfbs_dir)?;
    validate_protocol(&schema)?;
    let topics = topic_entries(&schema)?;
    let wire = build_wire_descriptors(&bfbs_dir)?;
    wire_check(root, &wire)?;

    let templates = Templates::new(root)?;
    validate_cdr_idl_sources(root, &templates, &schema)?;
    let context = topic_catalog_context(&schema, &topics)?;
    for (template, output) in [
        ("xtask/topic_catalog/topics.json.jinja", "topics.json"),
        (
            "xtask/topic_catalog/topic_catalog.rs.jinja",
            "topic_catalog.rs",
        ),
        (
            "xtask/topic_catalog/topic_catalog.h.jinja",
            "topic_catalog.h",
        ),
    ] {
        templates.render_to_file(template, context.clone(), &check_dir.join(output))?;
    }
    write_c_topic_print(
        &templates,
        &schema,
        &topics,
        &check_dir.join("synapse/topic_print.h"),
        &check_dir.join("topic_print.c"),
    )?;
    templates.render_to_file(
        "xtask/topic_catalog/schemas.rs.jinja",
        embedded_schemas_context(&schema)?,
        &check_dir.join("schemas.rs"),
    )?;
    templates.render_to_file(
        "xtask/topic_catalog/topic_decode.rs.jinja",
        context.clone(),
        &check_dir.join("topic_decode.rs"),
    )?;
    smoke_catalog_helpers(&templates, &check_dir)?;

    println!("schema checks passed for {} topics", topics.len());
    println!(
        "{:<24} {:>4} {:>6}  {:<8} payload",
        "topic", "id", "bytes", "scope"
    );
    for topic in &topics {
        println!(
            "{:<24} {:>4} {:>6}  {:<8} {}",
            topic.name,
            topic.id,
            topic
                .payload_size
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            topic.scope,
            topic.payload_type.as_deref().unwrap_or("-"),
        );
    }
    Ok(())
}

/// Regenerate the per-type wire descriptors from the pinned schema and either
/// compare them against the committed baseline (default) or rewrite it
/// (`--update`). Uses the same version-checked FlatCC as `check` and `ci`.
fn wire_check_command(root: &Path, options: &Options) -> Result<()> {
    let tools = tools(LOCAL_PACKAGE_VERSION);
    let flatcc = flatcc_binary(&tools)?;
    let dir = root.join("target/xtask/wire-check");
    reset_dir(&dir)?;
    let bfbs_dir = dir.join("bfbs");
    generate_reflection_schemas(root, &flatcc, &bfbs_dir)?;
    let current = build_wire_descriptors(&bfbs_dir)?;
    if options.update {
        update_wire_baseline(root, &current)
    } else {
        wire_check(root, &current)
    }
}

fn ci(root: &Path, options: &Options) -> Result<()> {
    let tools = tools(package_version(&options.release_name)?);
    let templates = Templates::new(root)?;

    let packages = stage_packages(root, &templates, &tools)?;
    check_pins(&packages, &tools)?;
    let flatc = build_flatc(&tools)?;
    let flatcc = flatcc_tool(&tools)?;
    generate_bindings(root, &flatc, &flatcc.binary, &templates, &packages)?;
    check_rust_package(&templates, &packages.rust)?;
    build_archives(root, &tools, &flatcc, &options.release_name, false)?;
    Ok(())
}

const MCAP_PROFILE_PATH: &str = "docs/MCAP.md";

fn parse_args() -> Result<(String, Options)> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "ci".to_string());
    let mut release_name = "local".to_string();
    let mut update = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--release-name" => {
                release_name = args
                    .next()
                    .ok_or_else(|| io::Error::other("--release-name requires a value"))?;
            }
            "--update" => update = true,
            other => return fail(format!("unknown argument '{other}'")),
        }
    }

    Ok((
        command,
        Options {
            release_name,
            update,
        },
    ))
}

fn find_repo_root(start: &Path) -> Result<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("xtask/Cargo.toml").is_file() && dir.join("fbs/all.fbs").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return fail(
                "could not find repository root containing xtask/Cargo.toml and fbs/all.fbs",
            );
        }
    }
}

fn tools(package_version: impl Into<String>) -> Tools {
    Tools {
        package_version: package_version.into(),
        flatbuffers_version: FLATBUFFERS_VERSION.to_string(),
        flatbuffers_commit: FLATBUFFERS_COMMIT.to_string(),
        flatcc_version: FLATCC_VERSION.to_string(),
        flatcc_commit: FLATCC_COMMIT.to_string(),
        mcap_rust_version: MCAP_RUST_VERSION.to_string(),
    }
}

/// Exercise the rendered native catalog helpers with whichever toolchains are
/// available locally; each check is skipped when its tool is missing.
fn smoke_catalog_helpers(templates: &Templates, check_dir: &Path) -> Result<()> {
    if command_succeeds(Command::new("cc").arg("--version")) {
        templates.render_to_file(
            "xtask/smoke/catalog.c.jinja",
            context! {},
            &check_dir.join("catalog_test.c"),
        )?;
        run(Command::new("cc")
            .current_dir(check_dir)
            .arg("-std=c11")
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-Werror")
            .arg("catalog_test.c")
            .arg("-o")
            .arg("catalog_test_c"))?;
        run(&mut Command::new(check_dir.join("catalog_test_c")))?;
        println!("catalog c helpers ok");

        templates.render_to_file(
            "xtask/smoke/topic-print.c.jinja",
            context! {},
            &check_dir.join("print_test.c"),
        )?;
        run(Command::new("cc")
            .current_dir(check_dir)
            .arg("-std=c11")
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-Werror")
            .arg("-I")
            .arg(".")
            .arg("print_test.c")
            .arg("topic_print.c")
            .arg("-o")
            .arg("print_test_c"))?;
        run(&mut Command::new(check_dir.join("print_test_c")))?;
        println!("topic print c helpers ok");
    }

    if command_succeeds(Command::new("rustc").arg("--version")) {
        templates.render_to_file(
            "xtask/smoke/catalog.rs.jinja",
            context! {},
            &check_dir.join("catalog_test.rs"),
        )?;
        run(Command::new("rustc")
            .current_dir(check_dir)
            .arg("--edition")
            .arg("2021")
            .arg("catalog_test.rs")
            .arg("-o")
            .arg("catalog_test_rs"))?;
        run(&mut Command::new(check_dir.join("catalog_test_rs")))?;
        println!("catalog rust helpers ok");
    }

    Ok(())
}

fn package_version(release_name: &str) -> Result<String> {
    if release_name == "local" {
        return Ok(LOCAL_PACKAGE_VERSION.to_string());
    }

    let Some(version) = release_name.strip_prefix('v') else {
        return fail(format!(
            "invalid release name '{release_name}'; expected 'local' or a tag like 'v1.2.3'"
        ));
    };
    let mut components = version.split('.');
    let valid = matches!(
        (components.next(), components.next(), components.next(), components.next()),
        (Some(major), Some(minor), Some(patch), None)
            if [major, minor, patch].into_iter().all(valid_version_component)
    );
    if !valid {
        return fail(format!(
            "invalid release tag '{release_name}'; expected a stable semantic version like 'v1.2.3'"
        ));
    }

    Ok(version.to_string())
}

fn valid_version_component(component: &str) -> bool {
    !component.is_empty()
        && component.bytes().all(|byte| byte.is_ascii_digit())
        && (component == "0" || !component.starts_with('0'))
}

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn local_builds_use_development_version() {
        assert_eq!(package_version("local").unwrap(), "0.0.0");
    }

    #[test]
    fn release_version_comes_from_tag() {
        assert_eq!(package_version("v12.3.45").unwrap(), "12.3.45");
    }

    #[test]
    fn malformed_release_tags_are_rejected() {
        for tag in ["1.2.3", "v1.2", "v1.2.3.4", "v01.2.3", "v1.2.3-rc.1"] {
            assert!(
                package_version(tag).is_err(),
                "tag should be rejected: {tag}"
            );
        }
    }
}

fn check_pins(packages: &PackagePaths, tools: &Tools) -> Result<()> {
    println!("checking pinned generator and runtime versions");

    require_git_sha("FLATBUFFERS_COMMIT", &tools.flatbuffers_commit)?;
    require_git_sha("FLATCC_COMMIT", &tools.flatcc_commit)?;

    let rust_cargo = fs::read_to_string(packages.rust.join("Cargo.toml"))?;
    let rust_pin = format!("flatbuffers = \"={}\"", tools.flatbuffers_version);
    if !rust_cargo.contains(&rust_pin) {
        return fail(format!("staged rust/Cargo.toml must contain {rust_pin}"));
    }
    let rust_mcap_pin = format!("mcap = {{ version = \"={}\"", tools.mcap_rust_version);
    if !rust_cargo.contains(&rust_mcap_pin) {
        return fail(format!(
            "staged rust/Cargo.toml must contain {rust_mcap_pin}"
        ));
    }

    Ok(())
}

fn require_git_sha(name: &str, value: &str) -> Result<()> {
    if value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        fail(format!("{name} must be a 40-character git SHA"))
    }
}

include!("packaging.rs");
include!("support.rs");
include!("schema.rs");
