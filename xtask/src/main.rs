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

include!("protocol.rs");
#[derive(Debug)]
struct Tools {
    package_version: String,
    flatbuffers_version: String,
    flatbuffers_commit: String,
    flatbuffers_build_version: String,
    flatcc_version: String,
    flatcc_commit: String,
    flatcc_binary: PathBuf,
    flatcc_source: PathBuf,
    mcap_rust_version: String,
    mcap_python_version: String,
    mcap_javascript_version: String,
    mcap_cpp_version: String,
    mcap_cpp_commit: String,
    typescript_version: String,
}

#[derive(Debug, serde::Deserialize)]
struct ToolsFile {
    package: PackageTools,
    flatbuffers: FlatbuffersTools,
    #[serde(rename = "flatbuffers-build")]
    flatbuffers_build: FlatbuffersBuildTools,
    flatcc: FlatccTools,
    mcap: McapTools,
    typescript: TypescriptTools,
}

#[derive(Debug, serde::Deserialize)]
struct PackageTools {
    version: String,
}

#[derive(Debug, serde::Deserialize)]
struct FlatbuffersTools {
    version: String,
    commit: String,
}

#[derive(Debug, serde::Deserialize)]
struct FlatbuffersBuildTools {
    version: String,
}

#[derive(Debug, serde::Deserialize)]
struct FlatccTools {
    version: String,
    commit: String,
}

#[derive(Debug, serde::Deserialize)]
struct McapTools {
    rust: String,
    python: String,
    javascript: String,
    cpp: McapCppTools,
}

#[derive(Debug, serde::Deserialize)]
struct McapCppTools {
    version: String,
    commit: String,
}

#[derive(Debug, serde::Deserialize)]
struct TypescriptTools {
    version: String,
}

#[derive(Debug)]
struct Options {
    release_name: String,
    update: bool,
}

fn main() -> Result<()> {
    let root = find_repo_root(&env::current_dir()?)?;
    let (command, options) = parse_args()?;

    match command.as_str() {
        "build" => build(&root, &options),
        "ci" => ci(&root, &options),
        "js" => js(&root),
        "check" => check(&root),
        "wire-check" => wire_check_command(&root, &options),
        _ => fail(format!(
            "unknown command '{command}'. expected: build, ci, js, check, or wire-check"
        )),
    }
}

fn build(root: &Path, options: &Options) -> Result<()> {
    let tools = read_tools(root)?;
    check_release_version(&tools, &options.release_name)?;
    let templates = Templates::new(root)?;

    let packages = stage_packages(root, &templates, &tools)?;
    check_pins(&packages, &tools)?;
    let flatc = build_flatc(&tools)?;
    let flatcc = flatcc_tool(&tools)?;
    generate_bindings(root, &flatc, &flatcc.binary, &templates, &packages)?;
    build_js_package(
        root,
        &templates,
        &packages.js,
        &flatcc.binary,
        &tools,
        false,
    )?;
    build_archives(root, &tools, &flatc, &flatcc, &options.release_name, true)?;

    Ok(())
}

fn check(root: &Path) -> Result<()> {
    let tools = read_tools(root)?;
    let flatcc = flatcc_tool(&tools)?;
    let check_dir = root.join("target/xtask/check");
    reset_dir(&check_dir)?;
    let bfbs_dir = check_dir.join("bfbs");
    generate_reflection_schemas(root, &flatcc.binary, &bfbs_dir)?;
    let schema = load_compiled_schema(&bfbs_dir)?;
    validate_protocol(&schema)?;
    let topics = topic_entries(&schema)?;
    let wire = build_wire_descriptors(&bfbs_dir)?;
    wire_check(root, &wire)?;

    let templates = Templates::new(root)?;
    let context = topic_catalog_context(&schema, &topics)?;
    for (template, output) in [
        ("xtask/topic_catalog/topics.json.jinja", "topics.json"),
        (
            "xtask/topic_catalog/topic_catalog.js.jinja",
            "topic_catalog.js",
        ),
        (
            "xtask/topic_catalog/topic_catalog.d.ts.jinja",
            "topic_catalog.d.ts",
        ),
        (
            "xtask/topic_catalog/topic_catalog.py.jinja",
            "topic_catalog.py",
        ),
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
/// (`--update`). Uses the same Nix-pinned FlatCC as `check` and `ci`.
fn wire_check_command(root: &Path, options: &Options) -> Result<()> {
    let tools = read_tools(root)?;
    let flatcc = flatcc_tool(&tools)?;
    let dir = root.join("target/xtask/wire-check");
    reset_dir(&dir)?;
    let bfbs_dir = dir.join("bfbs");
    generate_reflection_schemas(root, &flatcc.binary, &bfbs_dir)?;
    let current = build_wire_descriptors(&bfbs_dir)?;
    if options.update {
        update_wire_baseline(root, &current)
    } else {
        wire_check(root, &current)
    }
}

fn ci(root: &Path, options: &Options) -> Result<()> {
    let tools = read_tools(root)?;
    check_release_version(&tools, &options.release_name)?;
    let templates = Templates::new(root)?;

    let packages = stage_packages(root, &templates, &tools)?;
    check_pins(&packages, &tools)?;
    let flatc = build_flatc(&tools)?;
    let flatcc = flatcc_tool(&tools)?;
    generate_bindings(root, &flatc, &flatcc.binary, &templates, &packages)?;
    check_rust_package(&templates, &packages.rust)?;
    build_python_package(root, &templates, &packages.python, &tools)?;
    build_js_package(root, &templates, &packages.js, &flatcc.binary, &tools, true)?;
    build_archives(root, &tools, &flatc, &flatcc, &options.release_name, false)?;
    Ok(())
}

const MCAP_PROFILE_PATH: &str = "docs/MCAP.md";

/// Hash the full catalog contract a constrained link relies on after the
/// one-time handshake: topic id and key routing, payload interpretation
/// (encoding, wire type, transitive schema hash, instance key grammar), and
/// command ids with their request/reply contracts. Policy and documentation
/// fields (scope, descriptions) are deliberately excluded. Endpoints that
/// agree on this hash agree on how every frame is routed, decoded, and
/// restored to canonical Zenoh form.
fn schema_set_hash(topics: &[TopicEntry], commands: &[CommandEntry]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"synapse-schema-set-v3\n");
    let mut sorted_topics: Vec<&TopicEntry> = topics.iter().collect();
    sorted_topics.sort_by_key(|topic| topic.id);
    for topic in sorted_topics {
        digest.update(b"topic\t");
        digest.update(topic.id.to_string());
        digest.update(b"\t");
        digest.update(topic.key.as_bytes());
        digest.update(if topic.multi_instance {
            b"\t1\t"
        } else {
            b"\t0\t"
        });
        digest.update(topic.encoding.as_bytes());
        digest.update(b"\t");
        digest.update(topic.wire_type.as_bytes());
        digest.update(b"\t");
        digest.update(topic.schema_hash.as_bytes());
        digest.update(b"\n");
    }
    let mut sorted_commands: Vec<&CommandEntry> = commands.iter().collect();
    sorted_commands.sort_by_key(|command| command.id);
    for command in sorted_commands {
        digest.update(b"cmd\t");
        digest.update(command.id.to_string());
        for field in [
            command.name.as_str(),
            command.request_encoding,
            command.request_type.as_str(),
            command.request_schema_hash.as_str(),
            command.reply_encoding,
            command.reply_type.as_str(),
            command.reply_schema_hash.as_str(),
        ] {
            digest.update(b"\t");
            digest.update(field.as_bytes());
        }
        digest.update(b"\n");
    }
    let digest = digest.finalize();
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn js(root: &Path) -> Result<()> {
    let tools = read_tools(root)?;
    let templates = Templates::new(root)?;

    let package = root.join("target/xtask/packages/js");
    stage_template_tree(root, "js", &package, &templates, package_context(&tools))?;
    let flatcc = flatcc_tool(&tools)?;
    build_js_package(root, &templates, &package, &flatcc.binary, &tools, false)?;

    println!("staged npm package at {}", package.display());
    Ok(())
}

fn parse_args() -> Result<(String, Options)> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "ci".to_string());
    let mut release_name = env::var("GITHUB_REF_NAME").unwrap_or_else(|_| "local".to_string());
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

fn read_tools(_root: &Path) -> Result<Tools> {
    let Some(path) = env::var_os("SYNAPSE_FBS_TOOLS_TOML").map(PathBuf::from) else {
        return fail("SYNAPSE_FBS_TOOLS_TOML is not set. Run inside `nix develop`.");
    };
    if !path.is_file() {
        return fail(format!(
            "could not find Nix-generated tool manifest at {}",
            path.display()
        ));
    }
    let content = fs::read_to_string(&path)?;
    let parsed: ToolsFile = toml::from_str(&content)
        .map_err(|err| io::Error::other(format!("invalid {}: {err}", path.display())))?;

    Ok(Tools {
        package_version: parsed.package.version,
        flatbuffers_version: parsed.flatbuffers.version,
        flatbuffers_commit: parsed.flatbuffers.commit,
        flatbuffers_build_version: parsed.flatbuffers_build.version,
        flatcc_version: parsed.flatcc.version,
        flatcc_commit: parsed.flatcc.commit,
        flatcc_binary: required_env_path("SYNAPSE_FBS_FLATCC")?,
        flatcc_source: required_env_path("SYNAPSE_FBS_FLATCC_SOURCE")?,
        mcap_rust_version: parsed.mcap.rust,
        mcap_python_version: parsed.mcap.python,
        mcap_javascript_version: parsed.mcap.javascript,
        mcap_cpp_version: parsed.mcap.cpp.version,
        mcap_cpp_commit: parsed.mcap.cpp.commit,
        typescript_version: parsed.typescript.version,
    })
}

fn required_env_path(name: &str) -> Result<PathBuf> {
    Ok(env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other(format!("{name} is not set. Run through Nix.")))?)
}

/// Exercise the rendered catalog helpers with whichever toolchains are
/// available locally; each check is skipped when its tool is missing.
fn smoke_catalog_helpers(templates: &Templates, check_dir: &Path) -> Result<()> {
    if command_succeeds(Command::new("node").arg("--version")) {
        let script = templates.render("xtask/smoke/catalog.js.jinja", context! {})?;
        run(Command::new("node")
            .current_dir(check_dir)
            .arg("--input-type=module")
            .arg("-e")
            .arg(script))?;
    }

    if let Ok(python) = python_bin() {
        let code = templates.render("xtask/smoke/catalog.py.jinja", context! {})?;
        run(Command::new(&python)
            .current_dir(check_dir)
            .arg("-c")
            .arg(code))?;
    }

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

fn check_release_version(tools: &Tools, release_name: &str) -> Result<()> {
    // Only enforce for tag builds: GITHUB_REF_NAME is the branch name on
    // branch pushes, and branches may legitimately be named v2-wip etc.
    if env::var("GITHUB_REF_TYPE").as_deref() != Ok("tag") {
        return Ok(());
    }
    let Some(version) = release_name.strip_prefix('v') else {
        return Ok(());
    };
    if !version.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return Ok(());
    }
    if version != tools.package_version {
        return fail(format!(
            "release tag '{release_name}' does not match package.version={} in flake.nix",
            tools.package_version
        ));
    }
    Ok(())
}

fn check_pins(packages: &PackagePaths, tools: &Tools) -> Result<()> {
    println!("checking pinned generator and runtime versions");

    require_git_sha("FLATBUFFERS_COMMIT", &tools.flatbuffers_commit)?;
    require_git_sha("FLATCC_COMMIT", &tools.flatcc_commit)?;
    require_git_sha("MCAP_CPP_COMMIT", &tools.mcap_cpp_commit)?;

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

    let pyproject = fs::read_to_string(packages.python.join("pyproject.toml"))?;
    let python_pin = format!("flatbuffers=={}", tools.flatbuffers_version);
    if !pyproject.contains(&python_pin) {
        return fail(format!(
            "staged python/pyproject.toml must contain {python_pin}"
        ));
    }
    let python_mcap_pin = format!("mcap=={}", tools.mcap_python_version);
    if !pyproject.contains(&python_mcap_pin) {
        return fail(format!(
            "staged python/pyproject.toml must contain {python_mcap_pin}"
        ));
    }

    let js_package = fs::read_to_string(packages.js.join("package.json"))?;
    let js_mcap_pin = format!("\"@mcap/core\": \"{}\"", tools.mcap_javascript_version);
    if !js_package.contains(&js_mcap_pin) {
        return fail(format!("staged js/package.json must contain {js_mcap_pin}"));
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
