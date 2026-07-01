use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use minijinja::{AutoEscape, Environment, Value, context};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const SCHEMAS: &[&str] = &[
    "fbs/types.fbs",
    "fbs/sensors.fbs",
    "fbs/state.fbs",
    "fbs/control.fbs",
    "fbs/transport.fbs",
    "fbs/optical_flow.fbs",
    "fbs/mocap.fbs",
    "fbs/log.fbs",
    "fbs/sil.fbs",
    "fbs/all.fbs",
];

#[derive(Debug)]
struct Tools {
    package_version: String,
    flatbuffers_version: String,
    flatbuffers_commit: String,
    flatbuffers_build_version: String,
    flatcc_version: String,
    flatcc_commit: String,
}

#[derive(Debug)]
struct Options {
    release_name: String,
    docs_version: Option<String>,
    docs_out_dir: Option<PathBuf>,
}

fn main() -> Result<()> {
    let root = find_repo_root(&env::current_dir()?)?;
    let (command, options) = parse_args()?;

    match command.as_str() {
        "ci" => ci(&root, &options),
        "js" => js(&root),
        "docs" => docs(&root, &options),
        _ => fail(format!(
            "unknown command '{command}'. expected: ci, js, or docs"
        )),
    }
}

fn ci(root: &Path, options: &Options) -> Result<()> {
    let tools = read_tools(root)?;
    let templates = Templates::new(root)?;

    let packages = stage_packages(root, &templates, &tools)?;
    check_pins(&packages, &tools)?;
    let flatc = build_flatc(root, &tools)?;
    let flatcc = build_flatcc(root, &tools)?;
    generate_bindings(root, &flatc, &packages)?;
    check_rust_package(&packages.rust)?;
    build_python_package(root, &packages.python, &tools)?;
    build_js_package(root, &packages.js, &flatc)?;
    build_archives(root, &tools, &flatc, &flatcc, &options.release_name)?;
    generate_docs_site(
        root,
        &default_docs_version(&tools, options),
        &root.join("target/xtask/docs"),
    )?;

    Ok(())
}

fn js(root: &Path) -> Result<()> {
    let tools = read_tools(root)?;
    let templates = Templates::new(root)?;

    let package = root.join("target/xtask/packages/js");
    stage_template_tree(root, "js", &package, &templates, package_context(&tools))?;
    let flatc = build_flatc(root, &tools)?;
    build_js_package(root, &package, &flatc)?;

    println!("staged npm package at {}", package.display());
    Ok(())
}

fn docs(root: &Path, options: &Options) -> Result<()> {
    let tools = read_tools(root)?;
    let version = options
        .docs_version
        .clone()
        .or_else(|| env::var("DOCS_VERSION").ok())
        .or_else(|| env::var("GITHUB_REF_NAME").ok())
        .unwrap_or_else(|| tools.package_version.clone());
    let out_dir = options
        .docs_out_dir
        .clone()
        .unwrap_or_else(|| root.join("target/xtask/docs"));

    generate_docs_site(root, &version, &out_dir)
}

fn parse_args() -> Result<(String, Options)> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "ci".to_string());
    let mut release_name = env::var("GITHUB_REF_NAME").unwrap_or_else(|_| "local".to_string());
    let mut docs_version = None;
    let mut docs_out_dir = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--release-name" => {
                release_name = args
                    .next()
                    .ok_or_else(|| io::Error::other("--release-name requires a value"))?;
            }
            "--version" => {
                docs_version = Some(
                    args.next()
                        .ok_or_else(|| io::Error::other("--version requires a value"))?,
                );
            }
            "--out-dir" => {
                docs_out_dir =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        io::Error::other("--out-dir requires a value")
                    })?));
            }
            other => return fail(format!("unknown argument '{other}'")),
        }
    }

    Ok((
        command,
        Options {
            release_name,
            docs_version,
            docs_out_dir,
        },
    ))
}

fn find_repo_root(start: &Path) -> Result<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("tools.lock").is_file() && dir.join("fbs/all.fbs").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return fail("could not find repository root containing tools.lock and fbs/all.fbs");
        }
    }
}

fn read_tools(root: &Path) -> Result<Tools> {
    let content = fs::read_to_string(root.join("tools.lock"))?;
    let mut values = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return fail(format!("invalid tools.lock line: {line}"));
        };
        values.insert(key.to_string(), value.to_string());
    }

    Ok(Tools {
        package_version: required_value(&values, "PACKAGE_VERSION")?,
        flatbuffers_version: required_value(&values, "FLATBUFFERS_VERSION")?,
        flatbuffers_commit: required_value(&values, "FLATBUFFERS_COMMIT")?,
        flatbuffers_build_version: required_value(&values, "FLATBUFFERS_BUILD_VERSION")?,
        flatcc_version: required_value(&values, "FLATCC_VERSION")?,
        flatcc_commit: required_value(&values, "FLATCC_COMMIT")?,
    })
}

fn required_value(values: &BTreeMap<String, String>, key: &str) -> Result<String> {
    values
        .get(key)
        .cloned()
        .ok_or_else(|| io::Error::other(format!("missing {key} in tools.lock")).into())
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

    let pyproject = fs::read_to_string(packages.python.join("pyproject.toml"))?;
    let python_pin = format!("flatbuffers=={}", tools.flatbuffers_version);
    if !pyproject.contains(&python_pin) {
        return fail(format!(
            "staged python/pyproject.toml must contain {python_pin}"
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

#[derive(Debug)]
struct PackagePaths {
    rust: PathBuf,
    python: PathBuf,
    js: PathBuf,
}

fn stage_packages(root: &Path, templates: &Templates, tools: &Tools) -> Result<PackagePaths> {
    println!("staging package roots");

    let packages = PackagePaths {
        rust: root.join("target/xtask/packages/rust"),
        python: root.join("target/xtask/packages/python"),
        js: root.join("target/xtask/packages/js"),
    };
    let context = package_context(tools);

    stage_template_tree(root, "rust", &packages.rust, templates, context.clone())?;
    stage_template_tree(root, "python", &packages.python, templates, context.clone())?;
    stage_template_tree(root, "js", &packages.js, templates, context)?;

    Ok(packages)
}

fn package_context(tools: &Tools) -> Value {
    context! {
        package_version => tools.package_version.as_str(),
        flatbuffers_version => tools.flatbuffers_version.as_str(),
    }
}

fn archive_context(
    artifact: &str,
    release_name: &str,
    tools: &Tools,
    schema_sha256: &str,
    bfbs_sha256: &str,
    runtime_source_paths: &str,
) -> Value {
    context! {
        artifact => artifact,
        release_name => release_name,
        package_version => tools.package_version.as_str(),
        flatbuffers_version => tools.flatbuffers_version.as_str(),
        flatbuffers_commit => tools.flatbuffers_commit.as_str(),
        flatbuffers_build_version => tools.flatbuffers_build_version.as_str(),
        flatcc_version => tools.flatcc_version.as_str(),
        flatcc_commit => tools.flatcc_commit.as_str(),
        schema_sha256 => schema_sha256,
        bfbs_sha256 => bfbs_sha256,
        runtime_source_paths => runtime_source_paths,
    }
}

fn build_flatc(root: &Path, tools: &Tools) -> Result<PathBuf> {
    println!("building pinned flatc {}", tools.flatbuffers_version);

    let workdir = root.join("target/xtask/flatc-bootstrap");
    reset_dir(&workdir)?;
    fs::create_dir_all(workdir.join("src"))?;

    write_file(
        &workdir.join("Cargo.toml"),
        &format!(
            r#"[package]
name = "synapse-fbs-flatc-bootstrap"
version = "0.0.0"
edition = "2024"
publish = false

[build-dependencies]
flatbuffers-build = {{ version = "={}", features = ["vendored"] }}
"#,
            tools.flatbuffers_build_version
        ),
    )?;
    write_file(
        &workdir.join("build.rs"),
        &format!(
            r#"fn main() {{
    assert_eq!(flatbuffers_build::SUPPORTED_FLATC_VERSION, "{}");
}}
"#,
            tools.flatbuffers_version
        ),
    )?;
    write_file(&workdir.join("src/lib.rs"), "pub fn flatc_bootstrap() {}\n")?;

    run(Command::new("cargo")
        .arg("generate-lockfile")
        .arg("--manifest-path")
        .arg(workdir.join("Cargo.toml")))?;

    let cargo_lock = fs::read_to_string(workdir.join("Cargo.lock"))?;
    let lock_pin = format!("version = \"{}\"", tools.flatbuffers_build_version);
    if !cargo_lock.contains(&lock_pin) {
        return fail(format!(
            "flatc bootstrap Cargo.lock does not contain {lock_pin}"
        ));
    }

    run(Command::new("cargo")
        .env("CARGO_TARGET_DIR", workdir.join("target"))
        .arg("check")
        .arg("--locked")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(workdir.join("Cargo.toml")))?;

    let flatc = find_file(&workdir.join("target/debug/build"), "flatc")?;
    let version = output(Command::new(&flatc).arg("--version"))?;
    let expected = format!("flatc version {}", tools.flatbuffers_version);
    if version.trim() != expected {
        return fail(format!(
            "unexpected flatc version '{}', expected '{}'",
            version.trim(),
            expected
        ));
    }

    Ok(flatc)
}

#[derive(Debug)]
struct FlatccBuild {
    binary: PathBuf,
    source: PathBuf,
}

fn build_flatcc(root: &Path, tools: &Tools) -> Result<FlatccBuild> {
    println!("building pinned flatcc {}", tools.flatcc_version);

    let workdir = root.join("target/xtask/flatcc");
    let source = workdir.join("src");
    let build = workdir.join("build");
    reset_dir(&workdir)?;
    fetch_git_commit(
        "https://github.com/dvidelabs/flatcc.git",
        &tools.flatcc_commit,
        &source,
    )?;

    run(Command::new("cmake")
        .arg("-S")
        .arg(&source)
        .arg("-B")
        .arg(&build)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg("-DFLATCC_TEST=OFF")
        .arg("-DFLATCC_ALLOW_WERROR=OFF"))?;
    run(Command::new("cmake")
        .arg("--build")
        .arg(&build)
        .arg("--target")
        .arg("flatcc_cli")
        .arg("--parallel")
        .arg("2"))?;

    let binary = source.join("bin/flatcc");
    if !binary.is_file() {
        return fail(format!(
            "flatcc binary was not created at {}",
            binary.display()
        ));
    }

    let version_text = output_combined(Command::new(&binary).arg("--version"))?;
    let version = version_text
        .lines()
        .find_map(|line| line.split_once("version:").map(|(_, value)| value.trim()))
        .ok_or_else(|| io::Error::other("flatcc --version did not report a version"))?;
    if version != tools.flatcc_version {
        return fail(format!(
            "unexpected flatcc version '{version}', expected '{}'",
            tools.flatcc_version
        ));
    }

    Ok(FlatccBuild { binary, source })
}

fn generate_bindings(root: &Path, flatc: &Path, packages: &PackagePaths) -> Result<()> {
    println!("generating Rust and Python bindings");

    reset_dir(&packages.rust.join("src/generated"))?;
    remove_dir_if_exists(&packages.python.join("synapse"))?;

    let mut rust_cmd = Command::new(flatc);
    rust_cmd
        .current_dir(root)
        .arg("--rust")
        .arg("--rust-module-root-file")
        .arg("-I")
        .arg("fbs")
        .arg("-o")
        .arg(packages.rust.join("src/generated"))
        .args(SCHEMAS);
    run(&mut rust_cmd)?;

    let mut python_cmd = Command::new(flatc);
    python_cmd
        .current_dir(root)
        .arg("--python")
        .arg("-I")
        .arg("fbs")
        .arg("-o")
        .arg(&packages.python)
        .args(SCHEMAS);
    run(&mut python_cmd)?;

    Ok(())
}

fn check_rust_package(package_root: &Path) -> Result<()> {
    println!("checking Rust crate");

    let target_dir = package_root.join("target");
    run(Command::new("cargo")
        .env("CARGO_TARGET_DIR", &target_dir)
        .arg("check")
        .arg("--manifest-path")
        .arg(package_root.join("Cargo.toml")))?;

    let mut package = Command::new("cargo");
    package
        .env("CARGO_TARGET_DIR", &target_dir)
        .arg("package")
        .arg("--manifest-path")
        .arg(package_root.join("Cargo.toml"))
        .arg("--allow-dirty");
    run(&mut package)?;

    Ok(())
}

fn build_python_package(root: &Path, package_root: &Path, tools: &Tools) -> Result<()> {
    println!("building Python package");

    let python = python_bin()?;
    remove_dir_if_exists(&package_root.join("build"))?;
    remove_dir_if_exists(&package_root.join("dist"))?;
    remove_dir_if_exists(&package_root.join("synapse_fbs.egg-info"))?;

    run(Command::new(&python)
        .arg("-m")
        .arg("build")
        .arg(package_root))?;

    let dist_files = python_dist_files(package_root)?;
    if dist_files.is_empty() {
        return fail("python build did not produce any dist files");
    }

    if command_succeeds(
        Command::new(&python)
            .arg("-m")
            .arg("twine")
            .arg("--version"),
    ) {
        let mut twine = Command::new(&python);
        twine.arg("-m").arg("twine").arg("check").args(&dist_files);
        run(&mut twine)?;
    } else {
        let mut twine = Command::new("twine");
        twine.arg("check").args(&dist_files);
        run(&mut twine)?;
    }

    smoke_python_package(root, package_root, &python, tools)?;

    Ok(())
}

fn python_bin() -> Result<PathBuf> {
    if let Ok(value) = env::var("PYTHON") {
        let python = PathBuf::from(value);
        if command_succeeds(Command::new(&python).arg("-c").arg("import sys")) {
            return Ok(python);
        }
    }

    for candidate in ["python", "python3"] {
        if command_succeeds(Command::new(candidate).arg("-c").arg("import sys")) {
            return Ok(PathBuf::from(candidate));
        }
    }

    fail("could not find python or python3")
}

fn python_dist_files(package_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let dist = package_root.join("dist");
    if dist.is_dir() {
        for entry in fs::read_dir(dist)? {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if name.ends_with(".whl") || name.ends_with(".tar.gz") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn smoke_python_package(
    root: &Path,
    package_root: &Path,
    python: &Path,
    tools: &Tools,
) -> Result<()> {
    println!("smoke-testing Python wheel");

    let wheel = python_dist_files(package_root)?
        .into_iter()
        .find(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.ends_with(".whl"))
        })
        .ok_or_else(|| io::Error::other("python build did not produce a wheel"))?;

    let venv = root.join("target/xtask/python-smoke");
    reset_dir(&venv)?;
    run(Command::new(python).arg("-m").arg("venv").arg(&venv))?;

    let venv_python = if cfg!(windows) {
        venv.join("Scripts/python.exe")
    } else {
        venv.join("bin/python")
    };

    run(Command::new(&venv_python)
        .arg("-m")
        .arg("pip")
        .arg("install")
        .arg(&wheel))?;

    let code = format!(
        r#"import importlib.metadata as metadata
from synapse.topic.Vec3f import Vec3f
from synapse.log.LogRecord import LogRecord
assert metadata.version("flatbuffers") == "{}"
assert Vec3f is not None and LogRecord is not None
"#,
        tools.flatbuffers_version
    );
    run(Command::new(&venv_python).arg("-c").arg(code))?;

    Ok(())
}

fn build_js_package(root: &Path, package_root: &Path, flatc: &Path) -> Result<()> {
    println!("building JavaScript schema-assets package");

    remove_dir_if_exists(&package_root.join("fbs"))?;
    remove_dir_if_exists(&package_root.join("bfbs"))?;
    copy_common_archive_files(root, package_root)?;
    generate_reflection_schemas(root, flatc, &package_root.join("bfbs"))?;
    write_schema_hashes(root, &package_root.join("schema.sha256"))?;
    write_bfbs_hashes(package_root, &package_root.join("bfbs.sha256"))?;

    smoke_js_package(package_root)?;

    Ok(())
}

fn smoke_js_package(package_root: &Path) -> Result<()> {
    println!("smoke-testing JavaScript package");

    let node = node_bin()?;
    let script = r#"import { fbsDir, bfbsDir, schemaFiles, schemaPath } from './index.js';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
for (const name of schemaFiles) {
  if (!existsSync(schemaPath(name))) throw new Error('missing schema ' + name);
}
if (!existsSync(join(fbsDir, 'log.fbs'))) throw new Error('missing fbsDir');
if (!existsSync(join(bfbsDir, 'log.bfbs'))) throw new Error('missing reflection schema');
console.log('synapse-fbs js package ok');
"#;

    run(Command::new(&node)
        .current_dir(package_root)
        .arg("--input-type=module")
        .arg("-e")
        .arg(script))?;

    // Validate the published file set when npm is available.
    if command_succeeds(Command::new("npm").arg("--version")) {
        run(Command::new("npm")
            .current_dir(package_root)
            .arg("pack")
            .arg("--dry-run"))?;
    }

    Ok(())
}

fn node_bin() -> Result<PathBuf> {
    if let Ok(value) = env::var("NODE") {
        let node = PathBuf::from(value);
        if command_succeeds(Command::new(&node).arg("--version")) {
            return Ok(node);
        }
    }

    if command_succeeds(Command::new("node").arg("--version")) {
        return Ok(PathBuf::from("node"));
    }

    fail("could not find node")
}

fn build_archives(
    root: &Path,
    tools: &Tools,
    flatc: &Path,
    flatcc: &FlatccBuild,
    release_name: &str,
) -> Result<()> {
    println!("building generated C and C++ archives");

    let artifacts = root.join("target/xtask/artifacts");
    let workdir = root.join("target/xtask/artifacts-work");
    reset_dir(&artifacts)?;
    reset_dir(&workdir)?;
    let templates = Templates::new(root)?;

    let flatbuffers_source = workdir.join("flatbuffers");
    fetch_git_commit(
        "https://github.com/google/flatbuffers.git",
        &tools.flatbuffers_commit,
        &flatbuffers_source,
    )?;

    let cpp_root = workdir.join("synapse_fbs-cpp");
    fs::create_dir_all(cpp_root.join("include/synapse"))?;
    fs::create_dir_all(cpp_root.join("include"))?;
    fs::create_dir_all(cpp_root.join("third_party/flatbuffers"))?;
    fs::create_dir_all(cpp_root.join("fbs"))?;
    fs::create_dir_all(cpp_root.join("bfbs"))?;

    let mut cpp_gen = Command::new(flatc);
    cpp_gen
        .current_dir(root)
        .arg("--cpp")
        .arg("-I")
        .arg("fbs")
        .arg("-o")
        .arg(cpp_root.join("include/synapse"))
        .args(SCHEMAS);
    run(&mut cpp_gen)?;
    generate_reflection_schemas(root, flatc, &cpp_root.join("bfbs"))?;

    copy_dir_all(
        &flatbuffers_source.join("include/flatbuffers"),
        &cpp_root.join("include/flatbuffers"),
    )?;
    fs::copy(
        flatbuffers_source.join("LICENSE"),
        cpp_root.join("third_party/flatbuffers/LICENSE"),
    )?;
    copy_common_archive_files(root, &cpp_root)?;
    write_schema_hashes(root, &cpp_root.join("schema.sha256"))?;
    write_bfbs_hashes(&cpp_root, &cpp_root.join("bfbs.sha256"))?;
    copy_render_template_tree(
        "cpp",
        &root.join("cpp"),
        &cpp_root,
        &templates,
        archive_context(
            "cpp",
            release_name,
            tools,
            &sha256_hex(&cpp_root.join("schema.sha256"))?,
            &sha256_hex(&cpp_root.join("bfbs.sha256"))?,
            "",
        ),
    )?;
    write_tar_gz(
        &workdir,
        &artifacts,
        "synapse_fbs-cpp",
        "synapse_fbs-cpp.tar.gz",
    )?;

    let c_root = workdir.join("synapse_fbs-c");
    fs::create_dir_all(c_root.join("include/synapse"))?;
    fs::create_dir_all(c_root.join("include"))?;
    fs::create_dir_all(c_root.join("src/flatcc-runtime"))?;
    fs::create_dir_all(c_root.join("third_party/flatcc"))?;
    fs::create_dir_all(c_root.join("fbs"))?;
    fs::create_dir_all(c_root.join("bfbs"))?;

    let mut c_gen = Command::new(&flatcc.binary);
    c_gen
        .current_dir(root)
        .arg("-a")
        .arg("-I")
        .arg("fbs")
        .arg("-o")
        .arg(c_root.join("include/synapse"))
        .args(SCHEMAS);
    run(&mut c_gen)?;
    generate_reflection_schemas(root, flatc, &c_root.join("bfbs"))?;

    copy_dir_all(
        &flatcc.source.join("include/flatcc"),
        &c_root.join("include/flatcc"),
    )?;
    copy_files_with_extension(
        &flatcc.source.join("src/runtime"),
        &c_root.join("src/flatcc-runtime"),
        "c",
    )?;
    fs::copy(
        flatcc.source.join("LICENSE"),
        c_root.join("third_party/flatcc/LICENSE"),
    )?;
    fs::copy(
        flatcc.source.join("NOTICE"),
        c_root.join("third_party/flatcc/NOTICE"),
    )?;
    copy_common_archive_files(root, &c_root)?;
    write_schema_hashes(root, &c_root.join("schema.sha256"))?;
    write_bfbs_hashes(&c_root, &c_root.join("bfbs.sha256"))?;
    let runtime_source_paths = runtime_source_names(&c_root.join("src/flatcc-runtime"))?
        .into_iter()
        .map(|source| format!("  \"${{CMAKE_CURRENT_LIST_DIR}}/src/flatcc-runtime/{source}\""))
        .collect::<Vec<_>>()
        .join("\n");
    copy_render_template_tree(
        "c",
        &root.join("c"),
        &c_root,
        &templates,
        archive_context(
            "c",
            release_name,
            tools,
            &sha256_hex(&c_root.join("schema.sha256"))?,
            &sha256_hex(&c_root.join("bfbs.sha256"))?,
            &runtime_source_paths,
        ),
    )?;
    write_tar_gz(
        &workdir,
        &artifacts,
        "synapse_fbs-c",
        "synapse_fbs-c.tar.gz",
    )?;

    smoke_cpp_archive(&templates, &cpp_root)?;
    smoke_c_archive(&templates, &c_root)?;
    smoke_cmake_fetch(
        &templates,
        &workdir,
        &artifacts.join("synapse_fbs-c.tar.gz"),
    )?;
    print_artifacts(&artifacts)?;

    Ok(())
}

fn generate_reflection_schemas(root: &Path, flatc: &Path, output_dir: &Path) -> Result<()> {
    println!(
        "generating FlatBuffers reflection schemas into {}",
        output_dir.display()
    );
    fs::create_dir_all(output_dir)?;

    let mut bfbs_gen = Command::new(flatc);
    bfbs_gen
        .current_dir(root)
        .arg("--schema")
        .arg("-b")
        .arg("-I")
        .arg("fbs")
        .arg("-o")
        .arg(output_dir)
        .args(SCHEMAS);
    run(&mut bfbs_gen)?;

    for schema in SCHEMAS {
        let stem = Path::new(schema)
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| io::Error::other(format!("schema path has no file stem: {schema}")))?;
        let bfbs = output_dir.join(format!("{stem}.bfbs"));
        if !bfbs.is_file() {
            return fail(format!(
                "flatc did not generate expected reflection schema {}",
                bfbs.display()
            ));
        }
    }

    Ok(())
}

fn copy_common_archive_files(root: &Path, archive_root: &Path) -> Result<()> {
    fs::copy(root.join("LICENSE"), archive_root.join("LICENSE"))?;
    copy_dir_all(&root.join("fbs"), &archive_root.join("fbs"))?;
    Ok(())
}

fn write_schema_hashes(root: &Path, output_path: &Path) -> Result<()> {
    let mut content = String::new();
    for file in schema_files(root)? {
        let hash = sha256_hex(&file)?;
        let rel = file.strip_prefix(root)?;
        content.push_str(&format!("{}  {}\n", hash, rel.display()));
    }
    write_file(output_path, &content)
}

fn write_bfbs_hashes(archive_root: &Path, output_path: &Path) -> Result<()> {
    let mut content = String::new();
    for file in files_with_extension(&archive_root.join("bfbs"), "bfbs")? {
        let hash = sha256_hex(&file)?;
        let rel = file.strip_prefix(archive_root)?;
        content.push_str(&format!("{}  {}\n", hash, rel.display()));
    }
    write_file(output_path, &content)
}

fn schema_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_fbs_files(&root.join("fbs"), &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_fbs_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_fbs_files(&path, files)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("fbs") {
            files.push(path);
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct SchemaDoc {
    files: Vec<SchemaFileDoc>,
}

#[derive(Clone, Debug)]
struct SchemaFileDoc {
    name: String,
    namespace: String,
    includes: Vec<String>,
    entities: Vec<SchemaEntityDoc>,
}

#[derive(Clone, Debug)]
struct SchemaEntityDoc {
    kind: SchemaEntityKind,
    name: String,
    value_type: Option<String>,
    comments: Vec<String>,
    members: Vec<SchemaMemberDoc>,
}

#[derive(Clone, Debug)]
struct SchemaMemberDoc {
    name: String,
    type_name: Option<String>,
    value: Option<String>,
    comments: Vec<String>,
    unit_scale: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SchemaEntityKind {
    Struct,
    Table,
    Enum,
    Union,
}

impl SchemaEntityKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Struct => "struct",
            Self::Table => "table",
            Self::Enum => "enum",
            Self::Union => "union",
        }
    }
}

fn default_docs_version(tools: &Tools, options: &Options) -> String {
    options
        .docs_version
        .clone()
        .unwrap_or_else(|| match options.release_name.as_str() {
            "local" => tools.package_version.clone(),
            release_name => normalize_docs_version(release_name),
        })
}

fn generate_docs_site(root: &Path, version: &str, out_dir: &Path) -> Result<()> {
    let version = normalize_docs_version(version);
    let version_dir_name = docs_dir_name(&version);
    let version_dir = out_dir.join(&version_dir_name);

    println!(
        "generating schema docs for {version} into {}",
        version_dir.display()
    );

    let docs = parse_schema_docs(root)?;
    reset_dir(&version_dir)?;
    copy_dir_all(&root.join("fbs"), &version_dir.join("fbs"))?;
    write_file(&out_dir.join(".nojekyll"), "")?;
    write_file(&out_dir.join("style.css"), DOCS_CSS)?;
    write_file(
        &version_dir.join("index.html"),
        &render_schema_docs(&docs, &version, &version_dir_name),
    )?;
    write_docs_root_index(out_dir, &version_dir_name)?;

    Ok(())
}

fn parse_schema_docs(root: &Path) -> Result<SchemaDoc> {
    let mut files = Vec::new();
    for path in schema_files(root)? {
        let rel = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        files.push(parse_schema_file(&path, &rel)?);
    }
    Ok(SchemaDoc { files })
}

fn parse_schema_file(path: &Path, name: &str) -> Result<SchemaFileDoc> {
    let content = fs::read_to_string(path)?;
    let mut namespace = String::new();
    let mut includes = Vec::new();
    let mut entities = Vec::new();
    let mut current: Option<SchemaEntityDoc> = None;
    let mut pending_comments = Vec::new();

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        if let Some(comment) = trimmed.strip_prefix("//") {
            pending_comments.push(comment.trim().to_string());
            continue;
        }

        let code = trimmed
            .split_once("//")
            .map(|(before, _)| before)
            .unwrap_or(trimmed)
            .trim();
        if code.is_empty() {
            continue;
        }

        if let Some(entity) = current.as_mut() {
            if code.starts_with('}') {
                entities.push(current.take().expect("entity exists"));
                pending_comments.clear();
                continue;
            }

            if let Some(member) = parse_schema_member(entity.kind, code, &mut pending_comments) {
                entity.members.push(member);
            } else {
                pending_comments.clear();
            }
            continue;
        }

        if let Some(rest) = code.strip_prefix("namespace ") {
            namespace = rest.trim_end_matches(';').trim().to_string();
            pending_comments.clear();
            continue;
        }

        if let Some(rest) = code.strip_prefix("include ") {
            includes.push(
                rest.trim_end_matches(';')
                    .trim()
                    .trim_matches('"')
                    .to_string(),
            );
            pending_comments.clear();
            continue;
        }

        if let Some((kind, entity_name, value_type)) = parse_schema_entity_start(code) {
            current = Some(SchemaEntityDoc {
                kind,
                name: entity_name,
                value_type,
                comments: take_comments(&mut pending_comments),
                members: Vec::new(),
            });
            continue;
        }

        pending_comments.clear();
    }

    if let Some(entity) = current {
        return fail(format!(
            "{} ended while parsing {} {}",
            path.display(),
            entity.kind.as_str(),
            entity.name
        ));
    }

    Ok(SchemaFileDoc {
        name: name.to_string(),
        namespace,
        includes,
        entities,
    })
}

fn parse_schema_entity_start(code: &str) -> Option<(SchemaEntityKind, String, Option<String>)> {
    for (prefix, kind) in [
        ("struct ", SchemaEntityKind::Struct),
        ("table ", SchemaEntityKind::Table),
        ("enum ", SchemaEntityKind::Enum),
        ("union ", SchemaEntityKind::Union),
    ] {
        let Some(rest) = code.strip_prefix(prefix) else {
            continue;
        };
        let head = rest.split_once('{')?.0.trim();
        let (name, value_type) = head
            .split_once(':')
            .map(|(name, value_type)| {
                (name.trim().to_string(), Some(value_type.trim().to_string()))
            })
            .unwrap_or_else(|| (head.to_string(), None));
        return Some((kind, name, value_type));
    }

    None
}

fn parse_schema_member(
    kind: SchemaEntityKind,
    code: &str,
    pending_comments: &mut Vec<String>,
) -> Option<SchemaMemberDoc> {
    match kind {
        SchemaEntityKind::Struct | SchemaEntityKind::Table => {
            let member = code.trim_end_matches(';').trim();
            let (name, rest) = member.split_once(':')?;
            let (type_name, value) = rest
                .split_once('=')
                .map(|(type_name, value)| {
                    (
                        type_name.trim().to_string(),
                        Some(value.trim().trim_end_matches(';').to_string()),
                    )
                })
                .unwrap_or_else(|| (rest.trim().to_string(), None));
            let name = name.trim().to_string();
            Some(SchemaMemberDoc {
                unit_scale: unit_scale_note(&name),
                name,
                type_name: Some(type_name),
                value,
                comments: take_comments(pending_comments),
            })
        }
        SchemaEntityKind::Enum => {
            let member = code.trim_end_matches(',').trim();
            if member.is_empty() {
                return None;
            }
            let (name, value) = member
                .split_once('=')
                .map(|(name, value)| (name.trim().to_string(), Some(value.trim().to_string())))
                .unwrap_or_else(|| (member.to_string(), None));
            Some(SchemaMemberDoc {
                unit_scale: unit_scale_note(&name),
                name,
                type_name: None,
                value,
                comments: take_comments(pending_comments),
            })
        }
        SchemaEntityKind::Union => {
            let member = code.trim_end_matches(',').trim();
            if member.is_empty() {
                return None;
            }
            let (name, type_name) = member
                .split_once(':')
                .map(|(name, type_name)| {
                    (name.trim().to_string(), Some(type_name.trim().to_string()))
                })
                .unwrap_or_else(|| (member.to_string(), Some(member.to_string())));
            Some(SchemaMemberDoc {
                unit_scale: None,
                name,
                type_name,
                value: None,
                comments: take_comments(pending_comments),
            })
        }
    }
}

fn take_comments(comments: &mut Vec<String>) -> Vec<String> {
    std::mem::take(comments)
        .into_iter()
        .filter(|comment| !comment.is_empty())
        .collect()
}

fn unit_scale_note(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let note = if lower.contains("_enu_") && lower.ends_with("_m_s2") {
        "ENU frame (x east, y north, z up), meters per second squared"
    } else if lower.contains("_enu_") && lower.ends_with("_m_s") {
        "ENU frame (x east, y north, z up), meters per second"
    } else if lower.contains("_enu_") && lower.ends_with("_m") {
        "ENU frame (x east, y north, z up), meters"
    } else if lower.contains("_flu_") && lower.ends_with("_m_s2") {
        "FLU body frame (x forward, y left, z up), meters per second squared"
    } else if lower.contains("_flu_") && lower.ends_with("_rad_s") {
        "FLU body frame (x forward, y left, z up), radians per second"
    } else if lower.contains("_flu_") && lower.ends_with("_tesla") {
        "FLU body frame (x forward, y left, z up), tesla"
    } else if lower.contains("latitude_deg_e7") || lower.contains("longitude_deg_e7") {
        "degrees scaled by 1e7; int32 preserves global-coordinate precision"
    } else if lower.ends_with("_deg_e7") {
        "degrees scaled by 1e7"
    } else if lower.ends_with("_deg_e5") {
        "degrees scaled by 1e5"
    } else if lower.ends_with("_cdegc") || lower.ends_with("_cdeg") && lower.contains("temperature")
    {
        "centi-degrees Celsius; degC = value / 100"
    } else if lower.ends_with("_cdeg") {
        "centidegrees; degrees = value / 100"
    } else if lower.ends_with("_milli") {
        "normalized milli-units; value / 1000, usually [-1, 1]"
    } else if lower.ends_with("_centi") {
        "centi-units; value / 100"
    } else if lower.ends_with("_dpermille") {
        "deci-percent; percent = value / 10, 1000 means 100%"
    } else if lower.ends_with("_cpercent") {
        "centi-percent; percent = value / 100"
    } else if lower.ends_with("_raw_us") {
        "raw pulse width in microseconds"
    } else if lower.ends_with("_us") {
        "microseconds"
    } else if lower.ends_with("_ms") {
        "milliseconds"
    } else if lower.ends_with("_mm_s") {
        "millimeters per second"
    } else if lower.ends_with("_cm_s") {
        "centimeters per second"
    } else if lower.ends_with("_m_s2") {
        "meters per second squared"
    } else if lower.ends_with("_m_s") {
        "meters per second"
    } else if lower.ends_with("_rad_s") {
        "radians per second"
    } else if lower.ends_with("_rad") {
        "radians"
    } else if lower.ends_with("_mm") {
        "millimeters"
    } else if lower.ends_with("_mv") {
        "millivolts"
    } else if lower.ends_with("_ca") {
        "centi-amps; amps = value / 100"
    } else if lower.ends_with("_mah") {
        "milliamp-hours"
    } else if lower.ends_with("_hj") {
        "hecto-joules; joules = value * 100"
    } else if lower.ends_with("_pct") {
        "percent"
    } else if lower.ends_with("_hpa") {
        "hectopascals"
    } else if lower.ends_with("_tesla") {
        "tesla"
    } else if lower.ends_with("_deg") {
        "degrees"
    } else if lower.ends_with("_m") {
        "meters"
    } else if lower.ends_with("_s") {
        "seconds"
    } else if lower.contains("flags") || lower.contains("bitmask") || lower.ends_with("_mask") {
        "bitmask"
    } else {
        return None;
    };

    Some(note.to_string())
}

fn normalize_docs_version(version: &str) -> String {
    let version = version
        .strip_prefix("refs/tags/")
        .or_else(|| version.strip_prefix("refs/heads/"))
        .unwrap_or(version)
        .trim();
    let version = if version
        .strip_prefix('v')
        .is_some_and(|rest| rest.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
    {
        &version[1..]
    } else {
        version
    };

    if version.is_empty() {
        "local".to_string()
    } else {
        version.to_string()
    }
}

fn docs_dir_name(version: &str) -> String {
    let sanitized = version
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "local".to_string()
    } else {
        sanitized
    }
}

fn render_schema_docs(docs: &SchemaDoc, version: &str, version_dir_name: &str) -> String {
    let mut html = String::new();
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str(&format!(
        "<title>synapse_fbs {}</title><link rel=\"stylesheet\" href=\"../style.css\">",
        escape_html(version)
    ));
    html.push_str("</head><body><main>");
    html.push_str(&format!(
        "<header><p class=\"eyebrow\">synapse_fbs</p><h1>Schema Documentation <span>{}</span></h1>",
        escape_html(version)
    ));
    html.push_str("<p>Generated from the FlatBuffers schemas in <code>fbs/</code>. High-rate payloads use fixed-layout structs wrapped by small tables so they can be published as FlatBuffers roots while keeping the payload layout stable. Coordinate frames follow <a href=\"https://www.ros.org/reps/rep-0103.html\">ROS REP-0103</a>: local/world vectors use ENU and body vectors use FLU.</p></header>");
    html.push_str("<section class=\"notice\"><h2>Unit And Scale Rules</h2>");
    html.push_str("<p>Fields encode units and frames in their names. Local/world vectors use <code>_enu_</code>; body vectors use <code>_flu_</code>. Global coordinates use <code>_deg_e7</code>, altitudes use <code>_mm</code>, speeds commonly use <code>_cm_s</code> or <code>_mm_s</code>, temperatures use <code>_cdeg</code>, currents use <code>_ca</code>, magnetic field uses <code>_tesla</code>, and normalized manual-control axes use <code>_milli</code>. The scale column below is generated from those suffixes.</p></section>");
    html.push_str("<nav class=\"toc\"><h2>Schemas</h2><ul>");
    for file in &docs.files {
        html.push_str(&format!(
            "<li><a href=\"#{}\">{}</a><ul>",
            escape_attr(&anchor_id(&file.name, "")),
            escape_html(&file.name)
        ));
        for entity in &file.entities {
            html.push_str(&format!(
                "<li><a href=\"#{}\"><code>{}</code> {}</a></li>",
                escape_attr(&anchor_id(&file.name, &entity.name)),
                escape_html(entity.kind.as_str()),
                escape_html(&entity.name)
            ));
        }
        html.push_str("</ul></li>");
    }
    html.push_str("</ul></nav>");

    for file in &docs.files {
        html.push_str(&format!(
            "<section class=\"schema-file\" id=\"{}\"><h2>{}</h2>",
            escape_attr(&anchor_id(&file.name, "")),
            escape_html(&file.name)
        ));
        if !file.namespace.is_empty() {
            html.push_str(&format!(
                "<p><strong>Namespace:</strong> <code>{}</code></p>",
                escape_html(&file.namespace)
            ));
        }
        if !file.includes.is_empty() {
            html.push_str("<p><strong>Includes:</strong> ");
            for (index, include) in file.includes.iter().enumerate() {
                if index > 0 {
                    html.push_str(", ");
                }
                html.push_str(&format!("<code>{}</code>", escape_html(include)));
            }
            html.push_str("</p>");
        }
        let source_href = file.name.strip_prefix("fbs/").unwrap_or(&file.name);
        html.push_str(&format!(
            "<p><a href=\"fbs/{}\">Source schema</a></p>",
            escape_attr(source_href)
        ));

        for entity in &file.entities {
            render_entity_docs(&mut html, file, entity);
        }

        html.push_str("</section>");
    }

    html.push_str(&format!(
        "<footer><p>Version path: <code>{}</code>. Generated by <code>cargo run --locked --manifest-path xtask/Cargo.toml -- docs</code>.</p></footer>",
        escape_html(version_dir_name)
    ));
    html.push_str("</main></body></html>");
    html
}

fn render_entity_docs(html: &mut String, file: &SchemaFileDoc, entity: &SchemaEntityDoc) {
    html.push_str(&format!(
        "<article class=\"entity\" id=\"{}\"><h3><code>{}</code> {}</h3>",
        escape_attr(&anchor_id(&file.name, &entity.name)),
        escape_html(entity.kind.as_str()),
        escape_html(&entity.name)
    ));
    if let Some(value_type) = &entity.value_type {
        html.push_str(&format!(
            "<p><strong>Backing type:</strong> <code>{}</code></p>",
            escape_html(value_type)
        ));
    }
    render_comments(html, &entity.comments);

    html.push_str("<div class=\"table-wrap\"><table><thead><tr>");
    html.push_str("<th>Name</th><th>Type</th><th>Value</th><th>Unit / Scale</th><th>Notes</th>");
    html.push_str("</tr></thead><tbody>");
    for member in &entity.members {
        html.push_str("<tr>");
        html.push_str(&format!(
            "<td><code>{}</code></td>",
            escape_html(&member.name)
        ));
        html.push_str(&format!(
            "<td>{}</td>",
            member
                .type_name
                .as_ref()
                .map(|value| format!("<code>{}</code>", escape_html(value)))
                .unwrap_or_default()
        ));
        html.push_str(&format!(
            "<td>{}</td>",
            member
                .value
                .as_ref()
                .map(|value| format!("<code>{}</code>", escape_html(value)))
                .unwrap_or_default()
        ));
        html.push_str(&format!(
            "<td>{}</td>",
            member
                .unit_scale
                .as_ref()
                .map(|value| escape_html(value))
                .unwrap_or_default()
        ));
        html.push_str("<td>");
        render_comments(html, &member.comments);
        html.push_str("</td></tr>");
    }
    html.push_str("</tbody></table></div></article>");
}

fn render_comments(html: &mut String, comments: &[String]) {
    if comments.is_empty() {
        return;
    }
    html.push_str("<p>");
    for (index, comment) in comments.iter().enumerate() {
        if index > 0 {
            html.push(' ');
        }
        html.push_str(&escape_html(comment));
    }
    html.push_str("</p>");
}

fn write_docs_root_index(out_dir: &Path, current_version_dir: &str) -> Result<()> {
    let mut versions = Vec::new();
    for entry in fs::read_dir(out_dir)? {
        let path = entry?.path();
        if path.is_dir() && path.join("index.html").is_file() {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                versions.push(name.to_string());
            }
        }
    }
    versions.sort();
    versions.dedup();
    versions.sort_by(|left, right| match (*left == "main", *right == "main") {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => right.cmp(left),
    });

    let mut html = String::new();
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str(
        "<title>synapse_fbs schema docs</title><link rel=\"stylesheet\" href=\"style.css\">",
    );
    html.push_str("</head><body><main><header><p class=\"eyebrow\">synapse_fbs</p><h1>Schema Documentation</h1>");
    html.push_str("<p>Versioned FlatBuffers schema documentation generated from the source schemas.</p></header>");
    html.push_str("<section class=\"versions\"><h2>Versions</h2><ul>");
    for version in versions {
        let marker = if version == current_version_dir {
            " <span class=\"current\">updated</span>"
        } else {
            ""
        };
        html.push_str(&format!(
            "<li><a href=\"{0}/\">{0}</a>{1}</li>",
            escape_html(&version),
            marker
        ));
    }
    html.push_str("</ul></section></main></body></html>");
    write_file(&out_dir.join("index.html"), &html)
}

fn anchor_id(file_name: &str, entity_name: &str) -> String {
    docs_dir_name(&format!(
        "{}-{}",
        file_name.replace('/', "-").trim_end_matches(".fbs"),
        entity_name
    ))
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn escape_attr(value: &str) -> String {
    escape_html(value)
}

const DOCS_CSS: &str = r#":root {
  color-scheme: light;
  --bg: #f7f8fa;
  --panel: #ffffff;
  --text: #1f2933;
  --muted: #5f6b7a;
  --border: #d8dee8;
  --accent: #0f766e;
  --code: #eff3f7;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  line-height: 1.5;
}

main {
  max-width: 1180px;
  margin: 0 auto;
  padding: 32px 20px 64px;
}

h1,
h2,
h3 {
  line-height: 1.2;
}

h1 {
  margin: 0;
  font-size: 2.25rem;
}

h1 span,
.current {
  color: var(--accent);
}

.eyebrow {
  margin: 0 0 8px;
  color: var(--muted);
  font-size: 0.8rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.notice,
.toc,
.schema-file,
.entity,
.versions {
  margin-top: 24px;
  padding: 20px;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 8px;
}

.entity {
  padding: 16px;
}

.toc ul,
.versions ul {
  padding-left: 1.2rem;
}

a {
  color: var(--accent);
}

code {
  padding: 0.1rem 0.3rem;
  border-radius: 4px;
  background: var(--code);
  font-size: 0.92em;
}

.table-wrap {
  overflow-x: auto;
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.92rem;
}

th,
td {
  padding: 10px 12px;
  border-top: 1px solid var(--border);
  text-align: left;
  vertical-align: top;
}

th {
  color: var(--muted);
  font-size: 0.78rem;
  text-transform: uppercase;
}

td p {
  margin: 0;
}
"#;

struct Templates {
    env: Environment<'static>,
}

impl Templates {
    fn new(root: &Path) -> Result<Self> {
        let mut env = Environment::new();
        // Templates render config files (package.json, Cargo.toml, ...) with raw
        // substitution; disable auto-escaping so values are not JSON/HTML encoded.
        env.set_auto_escape_callback(|_| AutoEscape::None);
        add_templates(&mut env, &root.join("rust"), "rust")?;
        add_templates(&mut env, &root.join("python"), "python")?;
        add_templates(&mut env, &root.join("js"), "js")?;
        add_templates(&mut env, &root.join("c"), "c")?;
        add_templates(&mut env, &root.join("cpp"), "cpp")?;
        add_templates(&mut env, &root.join("xtask/templates"), "xtask")?;
        Ok(Self { env })
    }

    fn render_to_file(&self, name: &str, context: Value, path: &Path) -> Result<()> {
        let template = self.env.get_template(name)?;
        write_file(path, &template.render(context)?)
    }
}

fn add_templates(env: &mut Environment<'static>, dir: &Path, prefix: &str) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    add_templates_recursive(env, dir, dir, prefix)
}

fn add_templates_recursive(
    env: &mut Environment<'static>,
    root: &Path,
    dir: &Path,
    prefix: &str,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            add_templates_recursive(env, root, &path, prefix)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("jinja") {
            let rel = path.strip_prefix(root)?;
            let name = format!("{prefix}/{}", rel.to_string_lossy().replace('\\', "/"));
            let source = fs::read_to_string(&path)?;
            env.add_template_owned(name, source)?;
        }
    }
    Ok(())
}

fn stage_template_tree(
    root: &Path,
    package: &str,
    dst: &Path,
    templates: &Templates,
    context: Value,
) -> Result<()> {
    let src = root.join(package);
    reset_dir(dst)?;
    copy_render_template_tree(package, &src, dst, templates, context)
}

fn copy_render_template_tree(
    package: &str,
    src: &Path,
    dst: &Path,
    templates: &Templates,
    context: Value,
) -> Result<()> {
    copy_render_template_dir(package, src, src, dst, templates, context)
}

fn copy_render_template_dir(
    package: &str,
    src_root: &Path,
    src_dir: &Path,
    dst_root: &Path,
    templates: &Templates,
    context: Value,
) -> Result<()> {
    for entry in fs::read_dir(src_dir)? {
        let path = entry?.path();
        let rel = path.strip_prefix(src_root)?;
        if should_skip_staged_path(package, rel) {
            continue;
        }

        let dst = dst_root.join(rel);
        if path.is_dir() {
            copy_render_template_dir(
                package,
                src_root,
                &path,
                dst_root,
                templates,
                context.clone(),
            )?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("jinja") {
            let rel_name = rel.to_string_lossy().replace('\\', "/");
            let template_name = format!("{package}/{rel_name}");
            let output_name = rel_name
                .strip_suffix(".jinja")
                .ok_or_else(|| io::Error::other("template file must end with .jinja"))?;
            templates.render_to_file(
                &template_name,
                context.clone(),
                &dst_root.join(output_name),
            )?;
        } else {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&path, &dst)?;
        }
    }
    Ok(())
}

fn should_skip_staged_path(package: &str, rel: &Path) -> bool {
    let first = rel
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .unwrap_or_default();
    let file_name = rel
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    if matches!(
        first,
        "target" | "build" | "dist" | "node_modules" | "__pycache__" | ".pytest_cache"
    ) || first.ends_with(".egg-info")
    {
        return true;
    }

    if matches!(file_name, "Cargo.lock" | "Cargo.toml" | "pyproject.toml")
        || file_name.ends_with(".pyc")
    {
        return true;
    }

    match package {
        "rust" => rel.starts_with("src/generated"),
        "python" => rel.starts_with("synapse"),
        "js" => {
            rel.starts_with("fbs")
                || rel.starts_with("bfbs")
                || matches!(file_name, "schema.sha256" | "bfbs.sha256")
        }
        _ => false,
    }
}

fn runtime_source_names(runtime_dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for file in files_with_extension(runtime_dir, "c")? {
        let name = file
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| io::Error::other("runtime source path has no file name"))?
            .to_string();
        names.push(name);
    }
    names.sort();
    Ok(names)
}

fn write_tar_gz(
    workdir: &Path,
    artifacts: &Path,
    package_dir: &str,
    archive_name: &str,
) -> Result<()> {
    let tar_name = archive_name
        .strip_suffix(".gz")
        .ok_or_else(|| io::Error::other("archive name must end with .gz"))?;
    let archive = artifacts.join(archive_name);
    let tar_path = artifacts.join(tar_name);

    remove_file_if_exists(&archive)?;
    remove_file_if_exists(&tar_path)?;

    run(Command::new("tar")
        .arg("--sort=name")
        .arg("--owner=0")
        .arg("--group=0")
        .arg("--numeric-owner")
        .arg("--mtime=UTC 2020-01-01")
        .arg("-C")
        .arg(workdir)
        .arg("-cf")
        .arg(&tar_path)
        .arg(package_dir))?;
    run(Command::new("gzip").arg("-n").arg("-f").arg(&tar_path))?;

    let hash = sha256_hex(&archive)?;
    let file_name = archive
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| io::Error::other("archive path has no file name"))?;
    write_file(
        &artifacts.join(format!("{archive_name}.sha256")),
        &format!("{hash}  {file_name}\n"),
    )?;

    Ok(())
}

fn smoke_cpp_archive(templates: &Templates, cpp_root: &Path) -> Result<()> {
    println!("smoke-testing C++ archive");

    let smoke = cpp_root.join("smoke.cpp");
    templates.render_to_file("xtask/smoke.cpp.jinja", context! {}, &smoke)?;

    let cxx = env::var("CXX").unwrap_or_else(|_| "c++".to_string());
    run(Command::new(cxx)
        .arg("-std=c++11")
        .arg("-I")
        .arg(cpp_root.join("include"))
        .arg("-c")
        .arg(&smoke)
        .arg("-o")
        .arg(cpp_root.join("smoke.o")))?;

    remove_file_if_exists(&smoke)?;
    remove_file_if_exists(&cpp_root.join("smoke.o"))?;

    Ok(())
}

fn smoke_c_archive(templates: &Templates, c_root: &Path) -> Result<()> {
    println!("smoke-testing C archive");

    let smoke = c_root.join("smoke.c");
    templates.render_to_file("xtask/smoke.c.jinja", context! {}, &smoke)?;

    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    run(Command::new(&cc)
        .arg("-std=c11")
        .arg("-I")
        .arg(c_root.join("include"))
        .arg("-c")
        .arg(&smoke)
        .arg("-o")
        .arg(c_root.join("smoke.o")))?;

    for source in files_with_extension(&c_root.join("src/flatcc-runtime"), "c")? {
        let object = c_root.join(format!(
            "{}.o",
            source
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("runtime")
        ));
        run(Command::new(&cc)
            .arg("-std=c11")
            .arg("-I")
            .arg(c_root.join("include"))
            .arg("-c")
            .arg(&source)
            .arg("-o")
            .arg(&object))?;
        remove_file_if_exists(&object)?;
    }

    remove_file_if_exists(&smoke)?;
    remove_file_if_exists(&c_root.join("smoke.o"))?;

    Ok(())
}

fn smoke_cmake_fetch(templates: &Templates, workdir: &Path, archive: &Path) -> Result<()> {
    println!("smoke-testing CMake FetchContent archive usage");

    let fetch_smoke = workdir.join("fetch-smoke");
    reset_dir(&fetch_smoke)?;
    let archive_hash = sha256_hex(archive)?;

    templates.render_to_file(
        "xtask/cmake_fetch_smoke/CMakeLists.txt.jinja",
        context! {
            archive_path => archive.display().to_string(),
            archive_sha256 => archive_hash,
        },
        &fetch_smoke.join("CMakeLists.txt"),
    )?;
    templates.render_to_file(
        "xtask/smoke.c.jinja",
        context! {},
        &fetch_smoke.join("main.c"),
    )?;

    run(Command::new("cmake")
        .arg("-S")
        .arg(&fetch_smoke)
        .arg("-B")
        .arg(fetch_smoke.join("build")))?;
    run(Command::new("cmake")
        .arg("--build")
        .arg(fetch_smoke.join("build"))
        .arg("--parallel")
        .arg("2"))?;

    Ok(())
}

fn print_artifacts(artifacts: &Path) -> Result<()> {
    println!("release artifacts:");
    let mut entries = Vec::new();
    for entry in fs::read_dir(artifacts)? {
        entries.push(entry?.path());
    }
    entries.sort();
    for path in entries {
        let metadata = fs::metadata(&path)?;
        println!(
            "  {} ({} bytes)",
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("<unknown>"),
            metadata.len()
        );
    }
    Ok(())
}

fn fetch_git_commit(url: &str, commit: &str, dest: &Path) -> Result<()> {
    remove_dir_if_exists(dest)?;
    fs::create_dir_all(dest)?;

    run(Command::new("git").arg("init").arg(dest))?;
    run(Command::new("git")
        .arg("-C")
        .arg(dest)
        .arg("remote")
        .arg("add")
        .arg("origin")
        .arg(url))?;
    run(Command::new("git")
        .arg("-C")
        .arg(dest)
        .arg("fetch")
        .arg("--depth")
        .arg("1")
        .arg("origin")
        .arg(commit))?;
    run(Command::new("git")
        .arg("-C")
        .arg(dest)
        .arg("checkout")
        .arg("--detach")
        .arg("FETCH_HEAD"))?;

    let actual = output(
        Command::new("git")
            .arg("-C")
            .arg(dest)
            .arg("rev-parse")
            .arg("HEAD"),
    )?;
    if actual.trim() != commit {
        return fail(format!(
            "git checkout of {url} produced {}, expected {commit}",
            actual.trim()
        ));
    }

    Ok(())
}

fn copy_files_with_extension(src: &Path, dst: &Path, extension: &str) -> Result<()> {
    fs::create_dir_all(dst)?;
    for file in files_with_extension(src, extension)? {
        let name = file
            .file_name()
            .ok_or_else(|| io::Error::other("path has no file name"))?;
        fs::copy(&file, dst.join(name))?;
    }
    Ok(())
}

fn files_with_extension(src: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(src)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

fn find_file(root: &Path, file_name: &str) -> Result<PathBuf> {
    if !root.is_dir() {
        return fail(format!("{} is not a directory", root.display()));
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Ok(found) = find_file(&path, file_name) {
                return Ok(found);
            }
        } else if path.file_name().and_then(|value| value.to_str()) == Some(file_name) {
            return Ok(path);
        }
    }

    fail(format!(
        "could not find file named {file_name} under {}",
        root.display()
    ))
}

fn reset_dir(path: &Path) -> Result<()> {
    remove_dir_if_exists(path)?;
    fs::create_dir_all(path)?;
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

fn sha256_hex(path: &Path) -> Result<String> {
    let output = output(Command::new("sha256sum").arg(path))?;
    output
        .split_whitespace()
        .next()
        .map(ToString::to_string)
        .ok_or_else(|| {
            io::Error::other(format!(
                "sha256sum produced no output for {}",
                path.display()
            ))
            .into()
        })
}

fn command_succeeds(command: &mut Command) -> bool {
    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn run(command: &mut Command) -> Result<()> {
    let line = command_line(command);
    println!("+ {line}");
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        fail(format!("command failed with {status}: {line}"))
    }
}

fn output(command: &mut Command) -> Result<String> {
    let line = command_line(command);
    let output = command.output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        fail(format!("command failed: {line}\n{stderr}"))
    }
}

fn output_combined(command: &mut Command) -> Result<String> {
    let line = command_line(command);
    let output = command.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    if output.status.success() {
        Ok(combined)
    } else {
        fail(format!("command failed: {line}\n{combined}"))
    }
}

fn command_line(command: &Command) -> String {
    let mut parts = vec![command.get_program().to_string_lossy().to_string()];
    parts.extend(
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string()),
    );
    parts.join(" ")
}

fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(io::Error::other(message.into()).into())
}
