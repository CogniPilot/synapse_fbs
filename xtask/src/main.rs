use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use minijinja::{Environment, Value, context};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const SCHEMAS: &[&str] = &[
    "fbs/synapse_topics.fbs",
    "fbs/synapse_optical_flow.fbs",
    "fbs/synapse_mocap.fbs",
    "fbs/synapse_log.fbs",
    "fbs/synapse_sil.fbs",
    "fbs/synapse_all.fbs",
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
}

fn main() -> Result<()> {
    let root = find_repo_root(&env::current_dir()?)?;
    let (command, options) = parse_args()?;

    match command.as_str() {
        "ci" => ci(&root, &options),
        _ => fail(format!(
            "unknown command '{command}'. expected: cargo run --manifest-path xtask/Cargo.toml -- ci"
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
    build_archives(root, &tools, &flatc, &flatcc, &options.release_name)?;

    Ok(())
}

fn parse_args() -> Result<(String, Options)> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "ci".to_string());
    let mut release_name = env::var("GITHUB_REF_NAME").unwrap_or_else(|_| "local".to_string());

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--release-name" => {
                release_name = args
                    .next()
                    .ok_or_else(|| io::Error::other("--release-name requires a value"))?;
            }
            other => return fail(format!("unknown argument '{other}'")),
        }
    }

    Ok((command, Options { release_name }))
}

fn find_repo_root(start: &Path) -> Result<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("tools.lock").is_file() && dir.join("fbs/synapse_all.fbs").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return fail(
                "could not find repository root containing tools.lock and fbs/synapse_all.fbs",
            );
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
}

fn stage_packages(root: &Path, templates: &Templates, tools: &Tools) -> Result<PackagePaths> {
    println!("staging package roots");

    let packages = PackagePaths {
        rust: root.join("target/xtask/packages/rust"),
        python: root.join("target/xtask/packages/python"),
    };
    let context = package_context(tools);

    stage_template_tree(root, "rust", &packages.rust, templates, context.clone())?;
    stage_template_tree(root, "python", &packages.python, templates, context)?;

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

struct Templates {
    env: Environment<'static>,
}

impl Templates {
    fn new(root: &Path) -> Result<Self> {
        let mut env = Environment::new();
        add_templates(&mut env, &root.join("rust"), "rust")?;
        add_templates(&mut env, &root.join("python"), "python")?;
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
        "target" | "build" | "dist" | "__pycache__" | ".pytest_cache"
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
