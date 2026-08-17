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
        mcap_rust_version => tools.mcap_rust_version.as_str(),
        mcap_python_version => tools.mcap_python_version.as_str(),
        mcap_javascript_version => tools.mcap_javascript_version.as_str(),
    }
}

fn archive_context(
    artifact: &str,
    release_name: &str,
    tools: &Tools,
    schema_sha256: &str,
    bfbs_sha256: &str,
    runtime_sources: &[String],
    mcap_bfbs_sources: &[String],
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
        mcap_cpp_version => tools.mcap_cpp_version.as_str(),
        mcap_cpp_commit => tools.mcap_cpp_commit.as_str(),
        schema_sha256 => schema_sha256,
        bfbs_sha256 => bfbs_sha256,
        runtime_sources => runtime_sources,
        mcap_bfbs_sources => mcap_bfbs_sources,
    }
}

fn build_flatc(tools: &Tools) -> Result<PathBuf> {
    println!("using pinned flatc {}", tools.flatbuffers_version);

    verify_git_commit(&tools.flatbuffers_source, &tools.flatbuffers_commit)?;
    let flatc = tools.flatc_binary.clone();
    if !flatc.is_file() {
        return fail(format!(
            "flatc is missing at {}. Run `make bootstrap`.",
            flatc.display(),
        ));
    }

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
struct FlatccTool {
    binary: PathBuf,
    source: PathBuf,
}

fn flatcc_tool(tools: &Tools) -> Result<FlatccTool> {
    let binary = tools.flatcc_binary.clone();
    let source = tools.flatcc_source.clone();
    if !binary.is_file() {
        return fail(format!(
            "flatcc is missing at {}. Run `make bootstrap`.",
            binary.display(),
        ));
    }
    verify_git_commit(&source, &tools.flatcc_commit)?;
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

    if !source.join("src/runtime").is_dir() || !source.join("include/flatcc").is_dir() {
        return fail(format!(
            "pinned flatcc source is incomplete at {}",
            source.display()
        ));
    }

    Ok(FlatccTool { binary, source })
}

const FLATBUFFERS_REPOSITORY: &str = "https://github.com/google/flatbuffers.git";
const FLATCC_REPOSITORY: &str = "https://github.com/dvidelabs/flatcc.git";

fn bootstrap_tools(root: &Path, tools: &Tools, offline: bool) -> Result<()> {
    println!("bootstrapping pinned native schema tools");

    ensure_git_checkout(
        &tools.flatbuffers_source,
        FLATBUFFERS_REPOSITORY,
        &tools.flatbuffers_commit,
        offline,
    )?;
    let flatbuffers_build = root.join("target/xtask/toolchain/flatbuffers/build");
    run(Command::new("cmake")
        .arg("-S")
        .arg(&tools.flatbuffers_source)
        .arg("-B")
        .arg(&flatbuffers_build)
        .arg("-G")
        .arg("Ninja")
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg("-DFLATBUFFERS_BUILD_TESTS=OFF")
        .arg("-DFLATBUFFERS_BUILD_FLATLIB=OFF")
        .arg("-DFLATBUFFERS_BUILD_SHAREDLIB=OFF")
        .arg("-DFLATBUFFERS_INSTALL=OFF"))?;
    run(Command::new("cmake")
        .arg("--build")
        .arg(&flatbuffers_build)
        .arg("--target")
        .arg("flatc")
        .arg("--parallel"))?;

    ensure_git_checkout(
        &tools.flatcc_source,
        FLATCC_REPOSITORY,
        &tools.flatcc_commit,
        offline,
    )?;
    let flatcc_build = root.join("target/xtask/toolchain/flatcc/build");
    run(Command::new("cmake")
        .arg("-S")
        .arg(&tools.flatcc_source)
        .arg("-B")
        .arg(&flatcc_build)
        .arg("-G")
        .arg("Ninja")
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg("-DFLATCC_TEST=OFF")
        .arg("-DFLATCC_ALLOW_WERROR=OFF"))?;
    run(Command::new("cmake")
        .arg("--build")
        .arg(&flatcc_build)
        .arg("--target")
        .arg("flatcc_cli")
        .arg("--parallel"))?;

    build_flatc(tools)?;
    flatcc_tool(tools)?;
    println!("native schema tools are ready");
    Ok(())
}

fn ensure_git_checkout(path: &Path, url: &str, commit: &str, offline: bool) -> Result<()> {
    let new_checkout = !path.join(".git").is_dir();
    if new_checkout {
        if offline {
            return fail(format!(
                "pinned tool source is not cached at {}. Run `make bootstrap` with network access.",
                path.display()
            ));
        }
        if path.exists() && fs::read_dir(path)?.next().is_some() {
            return fail(format!(
                "tool source directory exists but is not a Git checkout: {}",
                path.display()
            ));
        }
        fs::create_dir_all(path)?;
        run(Command::new("git").arg("init").arg(path))?;
        run(Command::new("git")
            .arg("-C")
            .arg(path)
            .arg("remote")
            .arg("add")
            .arg("origin")
            .arg(url))?;
    } else {
        let remote = output(
            Command::new("git")
                .arg("-C")
                .arg(path)
                .arg("remote")
                .arg("get-url")
                .arg("origin"),
        )?;
        if remote.trim() != url {
            return fail(format!(
                "tool source {} has origin {}, expected {url}",
                path.display(),
                remote.trim()
            ));
        }
        verify_git_clean(path)?;
        if command_succeeds(
            Command::new("git")
                .arg("-C")
                .arg(path)
                .arg("rev-parse")
                .arg("--verify")
                .arg("HEAD"),
        ) {
            let current = output(
                Command::new("git")
                    .arg("-C")
                    .arg(path)
                    .arg("rev-parse")
                    .arg("HEAD"),
            )?;
            if current.trim() == commit {
                return Ok(());
            }
        }
        if command_succeeds(
            Command::new("git")
                .arg("-C")
                .arg(path)
                .arg("cat-file")
                .arg("-e")
                .arg(format!("{commit}^{{commit}}")),
        ) {
            run(Command::new("git")
                .arg("-C")
                .arg(path)
                .arg("checkout")
                .arg("--detach")
                .arg(commit))?;
            return verify_git_commit(path, commit);
        }
        if offline {
            return fail(format!(
                "pinned commit {commit} is not cached at {}. Run `make bootstrap` with network access.",
                path.display()
            ));
        }
    }

    run(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("fetch")
        .arg("--depth")
        .arg("1")
        .arg("origin")
        .arg(commit))?;
    run(Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("checkout")
        .arg("--detach")
        .arg("FETCH_HEAD"))?;
    verify_git_commit(path, commit)
}

fn verify_git_commit(path: &Path, commit: &str) -> Result<()> {
    if !path.join(".git").is_dir() {
        return fail(format!(
            "pinned tool source is not a Git checkout: {}",
            path.display()
        ));
    }
    let actual = output(
        Command::new("git")
            .arg("-C")
            .arg(path)
            .arg("rev-parse")
            .arg("HEAD"),
    )?;
    if actual.trim() != commit {
        return fail(format!(
            "tool source {} is at {}, expected {commit}",
            path.display(),
            actual.trim()
        ));
    }
    verify_git_clean(path)?;
    Ok(())
}

fn verify_git_clean(path: &Path) -> Result<()> {
    let status = output(
        Command::new("git")
            .arg("-C")
            .arg(path)
            .arg("status")
            .arg("--porcelain")
            .arg("--untracked-files=all"),
    )?;
    if !status.is_empty() {
        return fail(format!(
            "tool source has tracked or untracked files not ignored: {}",
            path.display()
        ));
    }
    Ok(())
}

fn generate_bindings(
    root: &Path,
    flatc: &Path,
    flatcc: &Path,
    templates: &Templates,
    packages: &PackagePaths,
) -> Result<()> {
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

    fs::copy(
        root.join("templates/python/mcap.py"),
        packages.python.join("synapse/mcap.py"),
    )?;
    write_file(&packages.python.join("synapse/py.typed"), "")?;
    fs::copy(
        root.join(MCAP_PROFILE_PATH),
        packages.python.join("synapse/MCAP.md"),
    )?;
    let bfbs_dir = packages.python.join("synapse/bfbs");
    generate_reflection_schemas(root, flatcc, &bfbs_dir)?;

    let schema = load_compiled_schema(&bfbs_dir)?;
    validate_protocol(&schema)?;
    let topics = topic_entries(&schema)?;
    let wire = build_wire_descriptors(&bfbs_dir)?;
    wire_check(root, &wire)?;
    write_package_topic_catalogs(templates, packages, &schema, &topics)?;

    // The Rust crate ships the wire contract itself: schema sources, compiled
    // binary schemas, and a generated debug decoder, so downstream tools do
    // not vendor schema copies that can drift from the pinned release.
    copy_dir_all(&root.join("fbs"), &packages.rust.join("fbs"))?;
    fs::copy(root.join(MCAP_PROFILE_PATH), packages.rust.join("MCAP.md"))?;
    generate_reflection_schemas(root, flatcc, &packages.rust.join("bfbs"))?;
    write_rust_embedded_schemas(templates, &schema, &packages.rust)?;
    write_rust_topic_decode(templates, &schema, &topics, &packages.rust)?;
    write_rust_mcap_fixed(templates, &schema, &topics, &packages.rust)?;

    Ok(())
}

fn write_package_topic_catalogs(
    templates: &Templates,
    packages: &PackagePaths,
    schema: &CompiledSchema,
    topics: &[TopicEntry],
) -> Result<()> {
    write_js_topic_catalogs(templates, &packages.js, schema, topics)?;
    write_rust_topic_catalog(templates, &packages.rust, schema, topics)?;
    write_python_topic_catalog(templates, &packages.python, schema, topics)?;
    Ok(())
}

fn write_js_topic_catalogs(
    templates: &Templates,
    package_root: &Path,
    schema: &CompiledSchema,
    topics: &[TopicEntry],
) -> Result<()> {
    let context = topic_catalog_context(schema, topics)?;
    templates.render_to_file(
        "xtask/topic_catalog/topics.json.jinja",
        context.clone(),
        &package_root.join("topics.json"),
    )?;
    templates.render_to_file(
        "xtask/topic_catalog/topic_catalog.js.jinja",
        context.clone(),
        &package_root.join("topic_catalog.js"),
    )?;
    templates.render_to_file(
        "xtask/topic_catalog/topic_catalog.d.ts.jinja",
        context,
        &package_root.join("topic_catalog.d.ts"),
    )
}

fn write_rust_topic_catalog(
    templates: &Templates,
    package_root: &Path,
    schema: &CompiledSchema,
    topics: &[TopicEntry],
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/topic_catalog.rs.jinja",
        topic_catalog_context(schema, topics)?,
        &package_root.join("src/topic_catalog.rs"),
    )
}

fn write_python_topic_catalog(
    templates: &Templates,
    package_root: &Path,
    schema: &CompiledSchema,
    topics: &[TopicEntry],
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/topic_catalog.py.jinja",
        topic_catalog_context(schema, topics)?,
        &package_root.join("synapse/topic_catalog.py"),
    )
}

fn write_c_topic_catalogs(
    templates: &Templates,
    package_root: &Path,
    schema: &CompiledSchema,
    topics: &[TopicEntry],
) -> Result<()> {
    let context = topic_catalog_context(schema, topics)?;
    templates.render_to_file(
        "xtask/topic_catalog/topics.json.jinja",
        context.clone(),
        &package_root.join("topics.json"),
    )?;
    templates.render_to_file(
        "xtask/topic_catalog/topic_catalog.h.jinja",
        context,
        &package_root.join("include/synapse/topic_catalog.h"),
    )
}

fn write_c_mcap_topics(
    templates: &Templates,
    package_root: &Path,
    schema: &CompiledSchema,
    topics: &[TopicEntry],
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/mcap_topics.h.jinja",
        topic_catalog_context(schema, topics)?,
        &package_root.join("include/synapse/mcap_topics.h"),
    )
}

fn write_cpp_mcap_topics(
    templates: &Templates,
    package_root: &Path,
    schema: &CompiledSchema,
    topics: &[TopicEntry],
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/mcap_topics.hpp.jinja",
        topic_catalog_context(schema, topics)?,
        &package_root.join("include/synapse/mcap_topics.hpp"),
    )
}

fn topic_catalog_context(schema: &CompiledSchema, topics: &[TopicEntry]) -> Result<Value> {
    let commands = command_entries(schema)?;
    let set_hash = schema_set_hash(topics, &commands);
    let mut mcap_schema_files = BTreeSet::new();
    for topic in topics {
        mcap_schema_files.insert(topic.schema_file.clone());
    }
    let mcap_schemas = mcap_schema_files
        .into_iter()
        .map(|schema_file| {
            let stem = Path::new(&schema_file)
                .file_stem()
                .and_then(|value| value.to_str())
                .expect("validated schema path must have a UTF-8 file stem");
            McapSchemaTemplateEntry {
                symbol: format!("synapse_bfbs_{stem}"),
                schema_file,
            }
        })
        .collect();
    let topics = topics
        .iter()
        .map(|topic| {
            let root_table_rust_path =
                rust_module_path(&topic.root_table_namespace, &topic.root_table);
            let payload_type_rust_path = topic
                .payload_type
                .as_deref()
                .zip(topic.payload_type_namespace.as_deref())
                .map(|(payload, namespace)| rust_module_path(namespace, payload))
                .unwrap_or_default();
            let root_table_qualified =
                qualified_name(&topic.root_table_namespace, &topic.root_table);
            let payload_type_qualified = topic
                .payload_type
                .as_deref()
                .zip(topic.payload_type_namespace.as_deref())
                .map(|(payload, namespace)| qualified_name(namespace, payload));
            let schema_stem = Path::new(&topic.schema_file)
                .file_stem()
                .and_then(|value| value.to_str())
                .expect("validated topic schema path must have a UTF-8 file stem");
            TopicTemplateEntry {
                id: topic.id,
                name: topic.name.clone(),
                key: topic.key.clone(),
                root_table: topic.root_table.clone(),
                payload_type: topic.payload_type.clone(),
                payload_size: topic.payload_size,
                schema_file: topic.schema_file.clone(),
                mcap_schema_name: root_table_qualified.clone(),
                mcap_schema_file: format!("bfbs/{schema_stem}.bfbs"),
                mcap_schema_symbol: format!("synapse_bfbs_{schema_stem}"),
                wire_type: topic.wire_type.clone(),
                schema_hash: topic.schema_hash.clone(),
                fixed_layout: topic.fixed_layout,
                multi_instance: topic.multi_instance,
                scope: topic.scope,
                encoding: topic.encoding,
                description: topic.description.clone(),
                root_table_rust_path,
                payload_type_rust_path,
                root_table_qualified,
                payload_type_qualified,
            }
        })
        .collect();

    Ok(Value::from_serialize(TopicCatalogContext {
        version: 2,
        schema_set_hash: set_hash,
        mcap_profile: MCAP_PROFILE,
        mcap_schema_encoding: MCAP_SCHEMA_ENCODING,
        mcap_message_encoding: MCAP_MESSAGE_ENCODING,
        mcap_metadata_name: MCAP_METADATA_NAME,
        mcap_schema_set_hash_key: MCAP_SCHEMA_SET_HASH_KEY,
        mcap_session_id_key: MCAP_SESSION_ID_KEY,
        mcap_source_key: MCAP_SOURCE_KEY,
        mcap_time_basis_key: MCAP_TIME_BASIS_KEY,
        mcap_time_basis_monotonic_boot: MCAP_TIME_BASIS_MONOTONIC_BOOT,
        mcap_time_basis_unix_epoch: MCAP_TIME_BASIS_UNIX_EPOCH,
        mcap_time_basis_correlated: MCAP_TIME_BASIS_CORRELATED,
        mcap_topic_id_key: MCAP_TOPIC_ID_KEY,
        cmd_key_prefix: CMD_KEY_PREFIX,
        meta_key_prefix: META_KEY_PREFIX,
        liveliness_key_prefix: LIVELINESS_KEY_PREFIX,
        mcap_schemas,
        topics,
        commands,
    }))
}
#[derive(Clone, Debug, Serialize)]
struct EmbeddedSchemaEntry {
    name: String,
    file: String,
    fbs_include: String,
    bfbs_include: String,
    root_type: Option<String>,
    file_identifier: Option<String>,
}

/// Context for the embedded-schemas module: one entry per file in SCHEMAS,
/// pairing the schema source and its compiled binary schema so the staged
/// crate can ship the wire contract instead of consumers vendoring copies.
fn embedded_schemas_context(schema: &CompiledSchema) -> Result<Value> {
    let mut schemas = Vec::new();
    for schema_file in SCHEMAS {
        let file = schema
            .files
            .iter()
            .find(|file| file.name == *schema_file)
            .ok_or_else(|| {
                io::Error::other(format!(
                    "schema {schema_file} is absent from FlatCC reflection"
                ))
            })?;
        let stem = Path::new(schema_file)
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                io::Error::other(format!("schema path has no file stem: {schema_file}"))
            })?;
        schemas.push(EmbeddedSchemaEntry {
            name: stem.to_string(),
            file: (*schema_file).to_string(),
            fbs_include: format!("../{schema_file}"),
            bfbs_include: format!("../bfbs/{stem}.bfbs"),
            root_type: file.root_type.clone(),
            file_identifier: file.file_identifier.clone(),
        });
    }
    Ok(Value::from_serialize(context! { schemas => schemas }))
}

fn write_rust_embedded_schemas(
    templates: &Templates,
    schema: &CompiledSchema,
    package_root: &Path,
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/schemas.rs.jinja",
        embedded_schemas_context(schema)?,
        &package_root.join("src/schemas.rs"),
    )
}

fn write_rust_topic_decode(
    templates: &Templates,
    schema: &CompiledSchema,
    topics: &[TopicEntry],
    package_root: &Path,
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/topic_decode.rs.jinja",
        topic_catalog_context(schema, topics)?,
        &package_root.join("src/topic_decode.rs"),
    )
}

fn write_rust_mcap_fixed(
    templates: &Templates,
    schema: &CompiledSchema,
    topics: &[TopicEntry],
    package_root: &Path,
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/mcap_fixed.rs.jinja",
        topic_catalog_context(schema, topics)?,
        &package_root.join("src/mcap_fixed.rs"),
    )
}

fn check_rust_package(templates: &Templates, package_root: &Path) -> Result<()> {
    println!("checking Rust crate");

    let tests_dir = package_root.join("tests");
    fs::create_dir_all(&tests_dir)?;
    templates.render_to_file(
        "xtask/smoke/firmware-roundtrip.rs.jinja",
        context! {},
        &tests_dir.join("firmware_roundtrip.rs"),
    )?;

    let target_dir = package_root.join("target");
    run(Command::new("cargo")
        .env("CARGO_TARGET_DIR", &target_dir)
        .arg("test")
        .arg("--manifest-path")
        .arg(package_root.join("Cargo.toml")))?;

    run(Command::new("cargo")
        .env("CARGO_TARGET_DIR", &target_dir)
        .arg("test")
        .arg("--features")
        .arg("mcap")
        .arg("--manifest-path")
        .arg(package_root.join("Cargo.toml")))?;

    run(Command::new("cargo")
        .env("CARGO_TARGET_DIR", &target_dir)
        .arg("clippy")
        .arg("--all-targets")
        .arg("--features")
        .arg("mcap")
        .arg("--manifest-path")
        .arg(package_root.join("Cargo.toml"))
        .arg("--")
        .arg("-D")
        .arg("warnings"))?;

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

fn build_python_package(
    root: &Path,
    templates: &Templates,
    package_root: &Path,
    tools: &Tools,
) -> Result<()> {
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

    smoke_python_package(root, templates, package_root, &python, tools)?;

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
    templates: &Templates,
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
        .arg(format!("{}[mcap]", wheel.display())))?;

    let mcap_path = venv.join("python-writer.mcap");

    let code = templates.render(
        "xtask/smoke/python_package.py.jinja",
        context! {
            flatbuffers_version => tools.flatbuffers_version.as_str(),
            mcap_version => tools.mcap_python_version.as_str(),
            mcap_path => mcap_path.display().to_string(),
        },
    )?;
    run(Command::new(&venv_python).arg("-c").arg(code))?;
    validate_mcap_with_rust(root, &mcap_path)?;
    remove_file_if_exists(&mcap_path)?;

    Ok(())
}

fn build_js_package(
    root: &Path,
    templates: &Templates,
    package_root: &Path,
    flatcc: &Path,
    tools: &Tools,
    validate: bool,
) -> Result<()> {
    println!("building JavaScript schema-assets package");

    remove_dir_if_exists(&package_root.join("fbs"))?;
    remove_dir_if_exists(&package_root.join("bfbs"))?;
    copy_common_archive_files(root, package_root)?;
    let bfbs_dir = package_root.join("bfbs");
    generate_reflection_schemas(root, flatcc, &bfbs_dir)?;
    let schema = load_compiled_schema(&bfbs_dir)?;
    validate_protocol(&schema)?;
    let topics = topic_entries(&schema)?;
    write_js_topic_catalogs(templates, package_root, &schema, &topics)?;
    write_schema_hashes(templates, root, &package_root.join("schema.sha256"))?;
    write_bfbs_hashes(templates, package_root, &package_root.join("bfbs.sha256"))?;

    if validate {
        smoke_js_package(root, templates, package_root, tools, true)?;
    }

    Ok(())
}

fn smoke_js_package(
    root: &Path,
    templates: &Templates,
    package_root: &Path,
    tools: &Tools,
    validate_cross_language: bool,
) -> Result<()> {
    println!("smoke-testing JavaScript package");

    let node = node_bin()?;
    if command_succeeds(Command::new("npm").arg("--version")) {
        run(Command::new("npm")
            .current_dir(package_root)
            .arg("install")
            .arg("--ignore-scripts")
            .arg("--no-package-lock")
            .arg("--no-save")
            .arg(format!("@mcap/core@{}", tools.mcap_javascript_version))
            .arg(format!("typescript@{}", tools.typescript_version)))?;
    } else {
        return fail("npm is required to test the optional JavaScript MCAP API");
    }

    let mcap_path = package_root.join("javascript-writer.mcap");
    let script = templates.render(
        "xtask/smoke/javascript_package.js.jinja",
        context! { mcap_path => mcap_path.display().to_string() },
    )?;
    run(Command::new(&node)
        .current_dir(package_root)
        .arg("--input-type=module")
        .arg("-e")
        .arg(script))?;

    let type_smoke = package_root.join("mcap-type-smoke.ts");
    templates.render_to_file(
        "xtask/smoke/mcap-type-smoke.ts.jinja",
        context! {},
        &type_smoke,
    )?;
    run(Command::new(package_root.join("node_modules/.bin/tsc"))
        .current_dir(package_root)
        .arg("--noEmit")
        .arg("--strict")
        .arg("--target")
        .arg("ES2022")
        .arg("--module")
        .arg("NodeNext")
        .arg("--moduleResolution")
        .arg("NodeNext")
        .arg("--skipLibCheck")
        .arg(&type_smoke))?;
    remove_file_if_exists(&type_smoke)?;

    if validate_cross_language {
        validate_mcap_with_rust(root, &mcap_path)?;
    }
    remove_file_if_exists(&mcap_path)?;

    // Validate the published file set when npm is available.
    run(Command::new("npm")
        .current_dir(package_root)
        .arg("pack")
        .arg("--dry-run"))?;

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
    flatcc: &FlatccTool,
    release_name: &str,
    development_only: bool,
) -> Result<()> {
    println!("building generated C and C++ archives");

    let artifacts = root.join("target/xtask/artifacts");
    let workdir = root.join("target/xtask/artifacts-work");
    reset_dir(&artifacts)?;
    reset_dir(&workdir)?;
    let templates = Templates::new(root)?;
    let model_bfbs = workdir.join("model-bfbs");
    generate_reflection_schemas(root, &flatcc.binary, &model_bfbs)?;
    let schema = load_compiled_schema(&model_bfbs)?;
    validate_protocol(&schema)?;
    let topics = topic_entries(&schema)?;

    if !development_only {
        let flatbuffers_source = workdir.join("flatbuffers");
        fetch_git_commit(
            "https://github.com/google/flatbuffers.git",
            &tools.flatbuffers_commit,
            &flatbuffers_source,
        )?;
        let mcap_source = workdir.join("mcap");
        fetch_git_commit(
            "https://github.com/foxglove/mcap.git",
            &tools.mcap_cpp_commit,
            &mcap_source,
        )?;

        let cpp_root = workdir.join("synapse_fbs-cpp");
        fs::create_dir_all(cpp_root.join("include/synapse"))?;
        fs::create_dir_all(cpp_root.join("include"))?;
        fs::create_dir_all(cpp_root.join("third_party/flatbuffers"))?;
        fs::create_dir_all(cpp_root.join("third_party/mcap"))?;
        fs::create_dir_all(cpp_root.join("src/bfbs"))?;
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
        generate_reflection_schemas(root, &flatcc.binary, &cpp_root.join("bfbs"))?;

        copy_dir_all(
            &flatbuffers_source.join("include/flatbuffers"),
            &cpp_root.join("include/flatbuffers"),
        )?;
        fs::copy(
            flatbuffers_source.join("LICENSE"),
            cpp_root.join("third_party/flatbuffers/LICENSE"),
        )?;
        copy_dir_all(
            &mcap_source.join("cpp/mcap/include/mcap"),
            &cpp_root.join("include/mcap"),
        )?;
        fs::copy(
            mcap_source.join("LICENSE"),
            cpp_root.join("third_party/mcap/LICENSE"),
        )?;
        copy_common_archive_files(root, &cpp_root)?;
        write_c_topic_catalogs(&templates, &cpp_root, &schema, &topics)?;
        write_cpp_mcap_topics(&templates, &cpp_root, &schema, &topics)?;
        write_cpp_bfbs_assets(&templates, &cpp_root, &topics)?;
        write_schema_hashes(&templates, root, &cpp_root.join("schema.sha256"))?;
        write_bfbs_hashes(&templates, &cpp_root, &cpp_root.join("bfbs.sha256"))?;
        let cpp_mcap_bfbs_sources = files_with_extension(&cpp_root.join("src/bfbs"), "cpp")?
            .into_iter()
            .map(|source| {
                source
                    .file_name()
                    .and_then(|value| value.to_str())
                    .expect("generated BFBS source must have a UTF-8 name")
                    .to_string()
            })
            .collect::<Vec<_>>();
        copy_render_template_tree(
            "cpp",
            &root.join("templates/cpp"),
            &cpp_root,
            &templates,
            archive_context(
                "cpp",
                release_name,
                tools,
                &sha256_hex(&cpp_root.join("schema.sha256"))?,
                &sha256_hex(&cpp_root.join("bfbs.sha256"))?,
                &[],
                &cpp_mcap_bfbs_sources,
            ),
        )?;
        write_tar_gz(
            &templates,
            &workdir,
            &artifacts,
            "synapse_fbs-cpp",
            "synapse_fbs-cpp.tar.gz",
        )?;
    }

    let c_root = if development_only {
        root.join("target/xtask/packages/c")
    } else {
        workdir.join("synapse_fbs-c")
    };
    reset_dir(&c_root)?;
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
    generate_reflection_schemas(root, &flatcc.binary, &c_root.join("bfbs"))?;
    write_c_bfbs_assets(&templates, &c_root, &topics)?;

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
    write_c_topic_catalogs(&templates, &c_root, &schema, &topics)?;
    write_c_mcap_topics(&templates, &c_root, &schema, &topics)?;
    write_c_topic_print(
        &templates,
        &schema,
        &topics,
        &c_root.join("include/synapse/topic_print.h"),
        &c_root.join("src/topic_print.c"),
    )?;
    write_schema_hashes(&templates, root, &c_root.join("schema.sha256"))?;
    write_bfbs_hashes(&templates, &c_root, &c_root.join("bfbs.sha256"))?;
    let runtime_sources = runtime_source_names(&c_root.join("src/flatcc-runtime"))?;
    let mcap_bfbs_sources = files_with_extension(&c_root.join("src/bfbs"), "c")?
        .into_iter()
        .map(|source| {
            source
                .file_name()
                .and_then(|value| value.to_str())
                .expect("generated BFBS source must have a UTF-8 name")
                .to_string()
        })
        .collect::<Vec<_>>();
    copy_render_template_tree(
        "c",
        &root.join("templates/c"),
        &c_root,
        &templates,
        archive_context(
            "c",
            release_name,
            tools,
            &sha256_hex(&c_root.join("schema.sha256"))?,
            &sha256_hex(&c_root.join("bfbs.sha256"))?,
            &runtime_sources,
            &mcap_bfbs_sources,
        ),
    )?;
    if development_only {
        smoke_c_archive(&templates, &c_root)?;
        smoke_c_to_rust_mcap(root, &c_root)?;
        smoke_cmake_source_override(&templates, &workdir, &c_root)?;
        smoke_cmake_find_package_c(&templates, tools, &workdir, &c_root)?;
        println!("staged local C package at {}", c_root.display());
    } else {
        write_tar_gz(
            &templates,
            &workdir,
            &artifacts,
            "synapse_fbs-c",
            "synapse_fbs-c.tar.gz",
        )?;

        let cpp_root = workdir.join("synapse_fbs-cpp");
        smoke_cpp_archive(root, &templates, &cpp_root)?;
        smoke_c_archive(&templates, &c_root)?;
        smoke_c_to_rust_mcap(root, &c_root)?;
        smoke_cmake_fetch(
            &templates,
            &workdir,
            &artifacts.join("synapse_fbs-c.tar.gz"),
        )?;
        smoke_cmake_find_package_c(&templates, tools, &workdir, &c_root)?;
        smoke_cmake_find_package_cpp(&templates, tools, &workdir, &cpp_root)?;
        print_artifacts(&artifacts)?;
    }

    Ok(())
}

fn generate_reflection_schemas(root: &Path, flatcc: &Path, output_dir: &Path) -> Result<()> {
    println!(
        "generating FlatBuffers reflection schemas into {}",
        output_dir.display()
    );
    reset_dir(output_dir)?;

    let mut bfbs_gen = Command::new(flatcc);
    bfbs_gen
        .current_dir(root)
        .arg("--schema")
        .arg("-Ifbs")
        .arg(format!("-o{}", output_dir.display()))
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
                "flatcc did not generate expected reflection schema {}",
                bfbs.display()
            ));
        }
    }

    Ok(())
}

fn copy_common_archive_files(root: &Path, archive_root: &Path) -> Result<()> {
    fs::copy(root.join("LICENSE"), archive_root.join("LICENSE"))?;
    fs::copy(root.join(MCAP_PROFILE_PATH), archive_root.join("MCAP.md"))?;
    copy_dir_all(&root.join("fbs"), &archive_root.join("fbs"))?;
    Ok(())
}

fn write_c_bfbs_assets(
    templates: &Templates,
    package_root: &Path,
    topics: &[TopicEntry],
) -> Result<()> {
    let output_dir = package_root.join("src/bfbs");
    reset_dir(&output_dir)?;
    let mut schema_files = BTreeSet::new();
    for topic in topics {
        schema_files.insert(topic.schema_file.clone());
    }
    for schema_file in schema_files {
        let stem = Path::new(&schema_file)
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| io::Error::other(format!("invalid schema path: {schema_file}")))?;
        let bytes = fs::read(package_root.join(format!("bfbs/{stem}.bfbs")))?;
        templates.render_to_file(
            "xtask/bfbs/asset.c.jinja",
            context! { stem, bytes },
            &output_dir.join(format!("{stem}.c")),
        )?;
    }
    Ok(())
}

fn write_cpp_bfbs_assets(
    templates: &Templates,
    package_root: &Path,
    topics: &[TopicEntry],
) -> Result<()> {
    let output_dir = package_root.join("src/bfbs");
    reset_dir(&output_dir)?;
    let mut schema_files = BTreeSet::new();
    for topic in topics {
        schema_files.insert(topic.schema_file.clone());
    }
    for schema_file in schema_files {
        let stem = Path::new(&schema_file)
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| io::Error::other("schema file has no UTF-8 stem"))?;
        let bytes = fs::read(package_root.join(format!("bfbs/{stem}.bfbs")))?;
        templates.render_to_file(
            "xtask/bfbs/asset.cpp.jinja",
            context! { stem, bytes },
            &output_dir.join(format!("{stem}.cpp")),
        )?;
    }
    Ok(())
}

#[derive(Serialize)]
struct ChecksumEntry {
    hash: String,
    path: String,
}

fn write_schema_hashes(templates: &Templates, root: &Path, output_path: &Path) -> Result<()> {
    let mut entries = Vec::new();
    for file in schema_files(root)? {
        entries.push(ChecksumEntry {
            hash: sha256_hex(&file)?,
            path: file.strip_prefix(root)?.display().to_string(),
        });
    }
    templates.render_to_file("xtask/checksums.jinja", context! { entries }, output_path)
}

fn write_bfbs_hashes(templates: &Templates, archive_root: &Path, output_path: &Path) -> Result<()> {
    let mut entries = Vec::new();
    for file in files_with_extension(&archive_root.join("bfbs"), "bfbs")? {
        entries.push(ChecksumEntry {
            hash: sha256_hex(&file)?,
            path: file.strip_prefix(archive_root)?.display().to_string(),
        });
    }
    templates.render_to_file("xtask/checksums.jinja", context! { entries }, output_path)
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
