struct Templates {
    env: Environment<'static>,
}

impl Templates {
    fn new(root: &Path) -> Result<Self> {
        let mut env = Environment::new();
        // Templates render config files (package.json, Cargo.toml, ...) with raw
        // substitution; disable auto-escaping so values are not JSON/HTML encoded.
        env.set_auto_escape_callback(|_| AutoEscape::None);
        let template_root = root.join("templates");
        add_templates(&mut env, &template_root.join("rust"), "rust")?;
        add_templates(&mut env, &template_root.join("python"), "python")?;
        add_templates(&mut env, &template_root.join("js"), "js")?;
        add_templates(&mut env, &template_root.join("c"), "c")?;
        add_templates(&mut env, &template_root.join("cpp"), "cpp")?;
        add_templates(&mut env, &template_root.join("xtask"), "xtask")?;
        Ok(Self { env })
    }

    fn render(&self, name: &str, context: Value) -> Result<String> {
        Ok(self.env.get_template(name)?.render(context)?)
    }

    fn render_to_file(&self, name: &str, context: Value, path: &Path) -> Result<()> {
        write_file(path, &self.render(name, context)?)
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
    let src = root.join("templates").join(package);
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
        "python" => rel.starts_with("synapse") || rel == Path::new("mcap.py"),
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
    templates: &Templates,
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
    templates.render_to_file(
        "xtask/checksums.jinja",
        context! {
            entries => [ChecksumEntry {
                hash,
                path: file_name.to_string(),
            }]
        },
        &artifacts.join(format!("{archive_name}.sha256")),
    )?;

    Ok(())
}

fn smoke_cpp_archive(root: &Path, templates: &Templates, cpp_root: &Path) -> Result<()> {
    println!("smoke-testing C++ archive");

    let smoke = cpp_root.join("smoke.cpp");
    templates.render_to_file("xtask/smoke.cpp.jinja", context! {}, &smoke)?;

    let cxx = env::var("CXX").unwrap_or_else(|_| "c++".to_string());
    run(Command::new(&cxx)
        .arg("-std=c++17")
        .arg("-I")
        .arg(cpp_root.join("include"))
        .arg("-c")
        .arg(&smoke)
        .arg("-o")
        .arg(cpp_root.join("smoke.o")))?;

    remove_file_if_exists(&smoke)?;
    remove_file_if_exists(&cpp_root.join("smoke.o"))?;

    let mcap_smoke = cpp_root.join("mcap_smoke");
    run(Command::new(&cxx)
        .arg("-std=c++17")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-Werror")
        .arg("-I")
        .arg(cpp_root.join("include"))
        .arg(cpp_root.join("tests/mcap_smoke.cpp"))
        .arg(cpp_root.join("src/mcap.cpp"))
        .arg(cpp_root.join("src/bfbs/state.cpp"))
        .arg("-o")
        .arg(&mcap_smoke))?;
    let mcap_path = cpp_root.join("cpp-writer.mcap");
    run(Command::new(&mcap_smoke).arg(&mcap_path))?;
    validate_mcap_with_rust(root, &mcap_path)?;
    remove_file_if_exists(&mcap_smoke)?;
    remove_file_if_exists(&mcap_path)?;

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

    let mcap_smoke = c_root.join("mcap_smoke");
    run(Command::new(&cc)
        .arg("-std=c11")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-Werror")
        .arg("-I")
        .arg(c_root.join("include"))
        .arg(c_root.join("tests/mcap_smoke.c"))
        .arg(c_root.join("src/mcap.c"))
        .arg(c_root.join("src/bfbs/state.c"))
        .arg("-o")
        .arg(&mcap_smoke))?;
    run(Command::new(&mcap_smoke).arg(c_root.join("c-writer.mcap")))?;
    remove_file_if_exists(&mcap_smoke)?;

    Ok(())
}

fn smoke_c_to_rust_mcap(root: &Path, c_root: &Path) -> Result<()> {
    println!("validating C MCAP output with the Rust reader");
    validate_mcap_with_rust(root, &c_root.join("c-writer.mcap"))?;
    remove_file_if_exists(&c_root.join("c-writer.mcap"))?;
    Ok(())
}

fn validate_mcap_with_rust(root: &Path, path: &Path) -> Result<()> {
    let package_root = root.join("target/xtask/packages/rust");
    let target_dir = package_root.join("target");
    run(Command::new("cargo")
        .env("CARGO_TARGET_DIR", &target_dir)
        .arg("run")
        .arg("--quiet")
        .arg("--features")
        .arg("mcap")
        .arg("--example")
        .arg("validate_mcap")
        .arg("--manifest-path")
        .arg(package_root.join("Cargo.toml"))
        .arg("--")
        .arg(path))?;
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

fn smoke_cmake_find_package_c(
    templates: &Templates,
    tools: &Tools,
    workdir: &Path,
    package_root: &Path,
) -> Result<()> {
    println!("smoke-testing CMake find_package C archive usage");

    let smoke_dir = workdir.join("find-package-c-smoke");
    reset_dir(&smoke_dir)?;
    templates.render_to_file(
        "xtask/cmake_find_package_c_smoke/CMakeLists.txt.jinja",
        context! {
            package_version => tools.package_version.as_str(),
        },
        &smoke_dir.join("CMakeLists.txt"),
    )?;
    templates.render_to_file(
        "xtask/smoke.c.jinja",
        context! {},
        &smoke_dir.join("main.c"),
    )?;

    run(Command::new("cmake")
        .arg("-S")
        .arg(&smoke_dir)
        .arg("-B")
        .arg(smoke_dir.join("build"))
        .arg(format!(
            "-DCMAKE_PREFIX_PATH={}",
            package_root.to_string_lossy()
        )))?;
    run(Command::new("cmake")
        .arg("--build")
        .arg(smoke_dir.join("build"))
        .arg("--parallel")
        .arg("2"))?;

    Ok(())
}

fn smoke_cmake_find_package_cpp(
    templates: &Templates,
    tools: &Tools,
    workdir: &Path,
    package_root: &Path,
) -> Result<()> {
    println!("smoke-testing CMake find_package C++ archive usage");

    let smoke_dir = workdir.join("find-package-cpp-smoke");
    reset_dir(&smoke_dir)?;
    templates.render_to_file(
        "xtask/cmake_find_package_cpp_smoke/CMakeLists.txt.jinja",
        context! {
            package_version => tools.package_version.as_str(),
        },
        &smoke_dir.join("CMakeLists.txt"),
    )?;
    templates.render_to_file(
        "xtask/smoke.cpp.jinja",
        context! {},
        &smoke_dir.join("main.cpp"),
    )?;

    run(Command::new("cmake")
        .arg("-S")
        .arg(&smoke_dir)
        .arg("-B")
        .arg(smoke_dir.join("build"))
        .arg(format!(
            "-DCMAKE_PREFIX_PATH={}",
            package_root.to_string_lossy()
        )))?;
    run(Command::new("cmake")
        .arg("--build")
        .arg(smoke_dir.join("build"))
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
