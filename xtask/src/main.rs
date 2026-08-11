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

const SCHEMAS: &[&str] = &[
    "fbs/types.fbs",
    "fbs/sensors.fbs",
    "fbs/state.fbs",
    "fbs/control.fbs",
    "fbs/optical_flow.fbs",
    "fbs/mocap.fbs",
    "fbs/telemetry.fbs",
    "fbs/sim.fbs",
    "fbs/trajectory.fbs",
    "fbs/transport.fbs",
    "fbs/transfer.fbs",
    "fbs/firmware.fbs",
    "fbs/all.fbs",
];

const CMD_KEY_PREFIX: &str = "cmd";
const META_KEY_PREFIX: &str = "meta";
const LIVELINESS_KEY_PREFIX: &str = "live";

// Frozen literals from the normative MCAP.md `synapse/1` profile. Keep these
// in one place so every generated language catalog gives writers the exact
// same contract instead of requiring string literals in each implementation.
const MCAP_PROFILE: &str = "synapse/1";
const MCAP_SCHEMA_ENCODING: &str = "flatbuffer";
const MCAP_MESSAGE_ENCODING: &str = "flatbuffer";
const MCAP_METADATA_NAME: &str = "synapse";
const MCAP_SCHEMA_SET_HASH_KEY: &str = "synapse.schema_set_hash";
const MCAP_SESSION_ID_KEY: &str = "synapse.session_id";
const MCAP_SOURCE_KEY: &str = "synapse.source";
const MCAP_TIME_BASIS_KEY: &str = "synapse.time_basis";
const MCAP_TIME_BASIS_MONOTONIC_BOOT: &str = "monotonic_boot";
const MCAP_TIME_BASIS_UNIX_EPOCH: &str = "unix_epoch";
const MCAP_TIME_BASIS_CORRELATED: &str = "correlated";
const MCAP_TOPIC_ID_KEY: &str = "synapse.topic_id";

/// Key segments reserved for non-topic key spaces; topic keys must not
/// collide with them.
const RESERVED_KEY_SEGMENTS: &[&str] = &[CMD_KEY_PREFIX, META_KEY_PREFIX, LIVELINESS_KEY_PREFIX];

/// Curated short key for every TopicId member. Keys are the human API:
/// `[<namespace>/]<key>[/<instance>]`, for example `cub1/odom`,
/// `qualisys/cub1/external_pose`, or `cub1/imu/0`. Everything before the key
/// is the deployment namespace; the payload contract comes from the value
/// metadata, never the key. Keys are lowercase snake_case, must start with a
/// letter, and must not use a reserved segment. Renaming a key is a breaking
/// catalog change caught by the schema-set hash.
/// (TopicId member, key)
const TOPIC_KEYS: &[(&str, &str)] = &[
    ("VehicleHealth", "health"),
    ("TimeReference", "time"),
    ("RadioControl", "rc"),
    ("ManualControlCommand", "manual"),
    ("InertialSample", "imu"),
    ("MagneticField", "mag"),
    ("AirData", "air"),
    ("PowerStatus", "power"),
    ("GnssFix", "gnss"),
    ("OpticalFlow", "flow"),
    ("OpticalFlowVelocity", "flow_vel"),
    ("AttitudeEstimate", "att"),
    ("LocalPositionEstimate", "local_pos"),
    ("GlobalPositionEstimate", "global_pos"),
    ("OdometryEstimate", "odom_estimate"),
    ("EstimatorHealth", "est_health"),
    ("MissionProgress", "mission"),
    ("NavigationTarget", "nav"),
    ("HomeReference", "home"),
    ("AttitudeCommand", "att_sp"),
    ("RateCommand", "rates_sp"),
    ("LocalPositionCommand", "pos_sp"),
    ("TrajectorySegment", "traj"),
    ("ActuatorCommand", "act_cmd"),
    ("ActuatorFeedback", "act_fb"),
    ("PwmSignalOutputs", "pwm"),
    ("ControlLoopMetrics", "loop"),
    ("TextStatus", "text"),
    ("GcsStatus", "gcs"),
    ("ExternalOdometry", "external_pose"),
    ("ExternalOdometryCovariance", "external_pose_cov"),
    ("MocapFrame", "mocap_matrix"),
    ("LockstepTick", "tick"),
    ("LockstepStatus", "tick_status"),
    ("FirmwareProgress", "fw"),
    ("RawPose", "pose_raw"),
    ("Pose", "pose"),
    ("PoseWithCovariance", "pose_cov"),
    ("Twist", "twist"),
    ("TwistWithCovariance", "twist_cov"),
    ("Odometry", "odom"),
    ("OdometryWithCovariance", "odom_cov"),
    ("MocapPoseFrame", "mocap"),
];

/// Queryable command and transfer services on the cmd key space. Ids mirror
/// the CmdId enum in fbs/transfer.fbs so non-Zenoh request/reply transports
/// can select a service numerically. Type names are fully qualified.
/// (id, name, request type, reply type, description)
const COMMANDS: &[(u16, &str, &str, &str, &str)] = &[
    (
        1,
        "param_get",
        "synapse.cmd.ParamGetRequest",
        "synapse.cmd.ParamGetReply",
        "Fetch one parameter by name, or a paged parameter catalog chunk.",
    ),
    (
        2,
        "param_set",
        "synapse.cmd.ParamSetRequest",
        "synapse.cmd.ParamSetReply",
        "Set one parameter.",
    ),
    (
        3,
        "mission_get",
        "synapse.cmd.MissionGetRequest",
        "synapse.cmd.MissionGetReply",
        "Fetch a paged GPS mission chunk.",
    ),
    (
        4,
        "mission_set",
        "synapse.cmd.MissionSetRequest",
        "synapse.cmd.MissionSetReply",
        "Replace or patch a GPS mission in bounded chunks.",
    ),
    (
        5,
        "trajectory_get",
        "synapse.cmd.TrajectoryGetRequest",
        "synapse.cmd.TrajectoryGetReply",
        "Fetch a paged trajectory segment chunk.",
    ),
    (
        6,
        "trajectory_set",
        "synapse.cmd.TrajectorySetRequest",
        "synapse.cmd.TrajectorySetReply",
        "Replace or patch a trajectory in bounded chunks.",
    ),
    (
        7,
        "firmware_info",
        "synapse.cmd.FirmwareInfoRequest",
        "synapse.cmd.FirmwareInfoReply",
        "Fetch firmware/update capabilities.",
    ),
    (
        8,
        "firmware_status",
        "synapse.cmd.FirmwareStatusRequest",
        "synapse.cmd.FirmwareStatusReply",
        "Fetch firmware update state/progress.",
    ),
    (
        9,
        "firmware_prepare",
        "synapse.cmd.FirmwarePrepareRequest",
        "synapse.cmd.FirmwarePrepareReply",
        "Prepare a maintenance-mode firmware update.",
    ),
    (
        10,
        "firmware_chunk",
        "synapse.cmd.FirmwareChunkRequest",
        "synapse.cmd.FirmwareChunkReply",
        "Transfer one staged firmware image chunk.",
    ),
    (
        11,
        "firmware_commit",
        "synapse.cmd.FirmwareCommitRequest",
        "synapse.cmd.FirmwareCommitReply",
        "Commit a staged image for bootloader test boot.",
    ),
    (
        12,
        "firmware_abort",
        "synapse.cmd.FirmwareAbortRequest",
        "synapse.cmd.FirmwareAbortReply",
        "Abort a staged firmware update.",
    ),
];

/// Topics that never leave the vehicle network segment. Everything else is
/// scope "any" and may be bridged over the air subject to rate policy.
const VEHICLE_SCOPE_TOPICS: &[&str] = &[
    "RadioControl",
    "InertialSample",
    "AirData",
    "OpticalFlow",
    "OpticalFlowVelocity",
    "ExternalOdometry",
    "RawPose",
    "Pose",
    "PoseWithCovariance",
    "Twist",
    "TwistWithCovariance",
    "Odometry",
    "OdometryWithCovariance",
    "MocapFrame",
    "MocapPoseFrame",
    "TrajectorySegment",
    "ActuatorCommand",
    "ActuatorFeedback",
    "PwmSignalOutputs",
    "ControlLoopMetrics",
    "LockstepTick",
    "LockstepStatus",
];

#[derive(Debug)]
struct Tools {
    package_version: String,
    flatbuffers_version: String,
    flatbuffers_commit: String,
    flatbuffers_build_version: String,
    flatcc_version: String,
    flatcc_commit: String,
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
}

fn main() -> Result<()> {
    let root = find_repo_root(&env::current_dir()?)?;
    let (command, options) = parse_args()?;

    match command.as_str() {
        "build" => build(&root, &options),
        "ci" => ci(&root, &options),
        "js" => js(&root),
        "check" => check(&root),
        _ => fail(format!(
            "unknown command '{command}'. expected: build, ci, js, or check"
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
    let flatcc = build_flatcc(root, &tools)?;
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
    let flatcc = build_flatcc(root, &tools)?;
    let check_dir = root.join("target/xtask/check");
    reset_dir(&check_dir)?;
    let bfbs_dir = check_dir.join("bfbs");
    generate_reflection_schemas(root, &flatcc.binary, &bfbs_dir)?;
    let schema = load_compiled_schema(&bfbs_dir)?;
    validate_protocol(&schema)?;
    let topics = topic_entries(&schema)?;

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

fn ci(root: &Path, options: &Options) -> Result<()> {
    let tools = read_tools(root)?;
    check_release_version(&tools, &options.release_name)?;
    let templates = Templates::new(root)?;

    let packages = stage_packages(root, &templates, &tools)?;
    check_pins(&packages, &tools)?;
    let flatc = build_flatc(&tools)?;
    let flatcc = build_flatcc(root, &tools)?;
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
    let flatcc = build_flatcc(root, &tools)?;
    build_js_package(root, &templates, &package, &flatcc.binary, &tools, false)?;

    println!("staged npm package at {}", package.display());
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
        mcap_rust_version: parsed.mcap.rust,
        mcap_python_version: parsed.mcap.python,
        mcap_javascript_version: parsed.mcap.javascript,
        mcap_cpp_version: parsed.mcap.cpp.version,
        mcap_cpp_commit: parsed.mcap.cpp.commit,
        typescript_version: parsed.typescript.version,
    })
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
    println!("using Nix-pinned flatc {}", tools.flatbuffers_version);

    let flatc = env::var_os("SYNAPSE_FBS_FLATC")
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::other("SYNAPSE_FBS_FLATC is not set. Run inside `nix develop`.")
        })?;
    if !flatc.is_file() {
        return fail(format!(
            "SYNAPSE_FBS_FLATC does not point to a file: {}",
            flatc.display()
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
struct FlatccBuild {
    binary: PathBuf,
    source: PathBuf,
}

fn build_flatcc(root: &Path, tools: &Tools) -> Result<FlatccBuild> {
    println!("building pinned flatcc {}", tools.flatcc_version);

    let workdir = root.join("target/xtask/flatcc");
    let source = workdir.join("src");
    let build = workdir.join("build");
    let binary = source.join("bin/flatcc");

    let cached_commit = Command::new("git")
        .arg("-C")
        .arg(&source)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .ok()
        .filter(|result| result.status.success())
        .and_then(|result| String::from_utf8(result.stdout).ok());
    let cached_version = Command::new(&binary)
        .arg("--version")
        .output()
        .ok()
        .filter(|result| result.status.success())
        .and_then(|result| String::from_utf8(result.stdout).ok())
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.split_once("version:")
                    .map(|(_, value)| value.trim().to_owned())
            })
        });

    if cached_commit.as_deref().map(str::trim) == Some(tools.flatcc_commit.as_str())
        && cached_version.as_deref() == Some(tools.flatcc_version.as_str())
        && binary.is_file()
    {
        println!("reusing cached flatcc {}", tools.flatcc_version);
        return Ok(FlatccBuild { binary, source });
    }

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
        .arg("-DCMAKE_POLICY_VERSION_MINIMUM=3.5")
        .arg("-DFLATCC_TEST=OFF")
        .arg("-DFLATCC_ALLOW_WERROR=OFF"))?;
    run(Command::new("cmake")
        .arg("--build")
        .arg(&build)
        .arg("--target")
        .arg("flatcc_cli")
        .arg("--parallel")
        .arg("2"))?;

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
    flatcc: &FlatccBuild,
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
    if !development_only {
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

#[derive(Clone, Debug)]
struct CompiledSchema {
    files: Vec<SchemaFile>,
}

#[derive(Clone, Debug)]
struct SchemaFile {
    name: String,
    entities: Vec<SchemaEntity>,
    root_type: Option<String>,
    file_identifier: Option<String>,
}

#[derive(Clone, Debug)]
struct SchemaEntity {
    kind: SchemaEntityKind,
    name: String,
    namespace: String,
    value_type: Option<String>,
    members: Vec<SchemaMember>,
    byte_size: Option<usize>,
}

#[derive(Clone, Debug)]
struct SchemaMember {
    name: String,
    type_name: Option<String>,
    value: Option<String>,
    offset: Option<usize>,
}

#[derive(Clone, Debug)]
struct TopicEntry {
    id: u16,
    name: String,
    key: String,
    root_table: String,
    root_table_namespace: String,
    payload_type: Option<String>,
    payload_type_namespace: Option<String>,
    payload_size: Option<usize>,
    schema_file: String,
    wire_type: String,
    schema_hash: String,
    fixed_layout: bool,
    multi_instance: bool,
    scope: &'static str,
    encoding: &'static str,
    description: String,
}

#[derive(Clone, Copy, Debug)]
struct CommandPayloadMetadata {
    encoding: &'static str,
    size: Option<usize>,
}

/// One queryable command service with its full wire contract: request and
/// reply types each carry the same transitive schema hash topics use, so the
/// compatibility allowlist and the schema-set hash cover command payloads.
#[derive(Clone, Debug, Serialize)]
struct CommandEntry {
    id: u16,
    name: String,
    key: String,
    request_type: String,
    request_schema_hash: String,
    request_encoding: &'static str,
    request_size: Option<usize>,
    reply_type: String,
    reply_schema_hash: String,
    reply_encoding: &'static str,
    reply_size: Option<usize>,
    description: String,
}

#[derive(Clone, Debug, Serialize)]
struct TopicCatalogContext {
    version: u8,
    schema_set_hash: String,
    mcap_profile: &'static str,
    mcap_schema_encoding: &'static str,
    mcap_message_encoding: &'static str,
    mcap_metadata_name: &'static str,
    mcap_schema_set_hash_key: &'static str,
    mcap_session_id_key: &'static str,
    mcap_source_key: &'static str,
    mcap_time_basis_key: &'static str,
    mcap_time_basis_monotonic_boot: &'static str,
    mcap_time_basis_unix_epoch: &'static str,
    mcap_time_basis_correlated: &'static str,
    mcap_topic_id_key: &'static str,
    cmd_key_prefix: &'static str,
    meta_key_prefix: &'static str,
    liveliness_key_prefix: &'static str,
    mcap_schemas: Vec<McapSchemaTemplateEntry>,
    topics: Vec<TopicTemplateEntry>,
    commands: Vec<CommandEntry>,
}

#[derive(Clone, Debug, Serialize)]
struct McapSchemaTemplateEntry {
    symbol: String,
    schema_file: String,
}

#[derive(Clone, Debug, Serialize)]
struct TopicTemplateEntry {
    id: u16,
    name: String,
    key: String,
    root_table: String,
    payload_type: Option<String>,
    payload_size: Option<usize>,
    schema_file: String,
    mcap_schema_name: String,
    mcap_schema_file: String,
    mcap_schema_symbol: String,
    wire_type: String,
    schema_hash: String,
    fixed_layout: bool,
    multi_instance: bool,
    scope: &'static str,
    encoding: &'static str,
    description: String,
    root_table_rust_path: String,
    payload_type_rust_path: String,
    root_table_qualified: String,
    payload_type_qualified: Option<String>,
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

fn load_compiled_schema(bfbs_dir: &Path) -> Result<CompiledSchema> {
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();

    // SCHEMAS is dependency ordered. FlatCC emits each top-level schema with
    // all of its includes, so the declarations first seen in each BFBS belong
    // to that source file. xtask only adapts compiler reflection data; it does
    // not read or interpret FBS source.
    for schema_file in SCHEMAS {
        let stem = Path::new(schema_file)
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| io::Error::other(format!("schema path has no stem: {schema_file}")))?;
        let path = bfbs_dir.join(format!("{stem}.bfbs"));
        let bytes = fs::read(&path)?;
        let schema = reflection::root_as_schema(&bytes)
            .map_err(|err| io::Error::other(format!("invalid {}: {err}", path.display())))?;
        let mut entities = Vec::new();

        for object in schema.objects() {
            if seen.insert(object.name().to_string()) {
                entities.push(reflected_object(&schema, object)?);
            }
        }
        for reflected_enum in schema.enums() {
            if seen.insert(reflected_enum.name().to_string()) {
                entities.push(reflected_enum_entity(&schema, reflected_enum)?);
            }
        }
        entities.sort_by(|left, right| left.name.cmp(&right.name));

        files.push(SchemaFile {
            name: (*schema_file).to_string(),
            entities,
            root_type: schema.root_table().map(|root| root.name().to_string()),
            file_identifier: schema
                .file_ident()
                .filter(|identifier| !identifier.is_empty())
                .map(str::to_string),
        });
    }

    Ok(CompiledSchema { files })
}

fn reflected_object(
    schema: &reflection::Schema<'_>,
    object: reflection::Object<'_>,
) -> Result<SchemaEntity> {
    let (namespace, name) = split_qualified_name(object.name());
    let mut fields = object
        .fields()
        .into_iter()
        .filter(|field| field.type_().base_type() != BaseType::UType)
        .collect::<Vec<_>>();
    if object.is_struct() {
        fields.sort_by_key(|field| field.offset());
    } else {
        fields.sort_by_key(|field| field.id());
    }
    let members = fields
        .into_iter()
        .map(|field| {
            let type_name = reflected_type_name(schema, field.type_(), &namespace)?;
            Ok(SchemaMember {
                name: field.name().to_string(),
                value: reflected_default(schema, field, &type_name),
                type_name: Some(type_name),
                offset: object.is_struct().then(|| usize::from(field.offset())),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(SchemaEntity {
        kind: if object.is_struct() {
            SchemaEntityKind::Struct
        } else {
            SchemaEntityKind::Table
        },
        name,
        namespace,
        value_type: None,
        members,
        byte_size: object
            .is_struct()
            .then(|| usize::try_from(object.bytesize()))
            .transpose()?,
    })
}

fn reflected_enum_entity(
    schema: &reflection::Schema<'_>,
    reflected_enum: reflection::Enum<'_>,
) -> Result<SchemaEntity> {
    let (namespace, name) = split_qualified_name(reflected_enum.name());
    let bit_flags = reflected_enum.attributes().is_some_and(|attributes| {
        attributes
            .into_iter()
            .any(|attribute| attribute.key() == "bit_flags")
    });
    let members = reflected_enum
        .values()
        .into_iter()
        .filter(|value| !(reflected_enum.is_union() && value.name() == "NONE"))
        .map(|value| {
            let type_name = value
                .union_type()
                .filter(|type_| type_.base_type() != BaseType::None)
                .map(|type_| reflected_type_name(schema, type_, &namespace))
                .transpose()?;
            Ok(SchemaMember {
                name: value.name().to_string(),
                type_name,
                value: (!reflected_enum.is_union() && !bit_flags)
                    .then(|| value.value().to_string()),
                offset: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let value_type = if reflected_enum.is_union() {
        None
    } else {
        let base = reflected_scalar_name(reflected_enum.underlying_type().base_type())?;
        Some(if bit_flags {
            format!("{base} (bit_flags)")
        } else {
            base.to_string()
        })
    };

    Ok(SchemaEntity {
        kind: if reflected_enum.is_union() {
            SchemaEntityKind::Union
        } else {
            SchemaEntityKind::Enum
        },
        name,
        namespace,
        value_type,
        members,
        byte_size: None,
    })
}

fn reflected_type_name(
    schema: &reflection::Schema<'_>,
    type_: reflection::Type<'_>,
    namespace: &str,
) -> Result<String> {
    let base = if type_.base_type() == BaseType::Vector {
        type_.element()
    } else {
        type_.base_type()
    };
    let atom = if type_.index() >= 0 {
        let qualified = if base == BaseType::Obj {
            schema.objects().get(usize::try_from(type_.index())?).name()
        } else {
            schema.enums().get(usize::try_from(type_.index())?).name()
        };
        local_type_name(namespace, qualified)
    } else {
        reflected_scalar_name(base)?.to_string()
    };

    Ok(if type_.base_type() == BaseType::Vector {
        format!("[{atom}]")
    } else {
        atom
    })
}

fn reflected_default(
    schema: &reflection::Schema<'_>,
    field: reflection::Field<'_>,
    type_name: &str,
) -> Option<String> {
    if field.default_integer() == 0 && field.default_real() == 0.0 {
        return None;
    }
    if field.type_().index() >= 0 && field.type_().base_type() != BaseType::Obj {
        let reflected_enum = schema
            .enums()
            .get(usize::try_from(field.type_().index()).ok()?);
        return reflected_enum
            .values()
            .into_iter()
            .find(|value| value.value() == field.default_integer())
            .map(|value| value.name().to_string());
    }
    if matches!(
        field.type_().base_type(),
        BaseType::Float | BaseType::Double
    ) {
        Some(field.default_real().to_string())
    } else if is_scalar_type(type_name) {
        Some(field.default_integer().to_string())
    } else {
        None
    }
}

fn reflected_scalar_name(base: BaseType) -> Result<&'static str> {
    match base {
        BaseType::Bool => Ok("bool"),
        BaseType::Byte => Ok("byte"),
        BaseType::UByte => Ok("ubyte"),
        BaseType::Short => Ok("short"),
        BaseType::UShort => Ok("ushort"),
        BaseType::Int => Ok("int"),
        BaseType::UInt => Ok("uint"),
        BaseType::Long => Ok("long"),
        BaseType::ULong => Ok("ulong"),
        BaseType::Float => Ok("float"),
        BaseType::Double => Ok("double"),
        BaseType::String => Ok("string"),
        _ => fail(format!("unsupported reflected FlatBuffers type {base:?}")),
    }
}

fn split_qualified_name(qualified: &str) -> (String, String) {
    qualified
        .rsplit_once('.')
        .map(|(namespace, name)| (namespace.to_string(), name.to_string()))
        .unwrap_or_else(|| (String::new(), qualified.to_string()))
}

fn local_type_name(namespace: &str, qualified: &str) -> String {
    let (target_namespace, name) = split_qualified_name(qualified);
    if target_namespace == namespace {
        name
    } else {
        qualified.to_string()
    }
}

/// Check TOPIC_KEYS is a valid, exhaustive map for the TopicId enum: one
/// well-formed, unique, non-reserved key per topic and no stale entries.
fn validate_topic_keys(topic_enum: &SchemaEntity) -> Result<()> {
    let mut problems = Vec::new();
    let members: BTreeSet<&str> = topic_enum
        .members
        .iter()
        .map(|member| member.name.as_str())
        .filter(|name| *name != "Unknown")
        .collect();

    let mut seen_names = BTreeSet::new();
    let mut seen_keys = BTreeSet::new();
    for (name, key) in TOPIC_KEYS {
        if !seen_names.insert(*name) {
            problems.push(format!("TOPIC_KEYS lists {name} more than once"));
        }
        if !seen_keys.insert(*key) {
            problems.push(format!("topic key '{key}' is used more than once"));
        }
        if !members.contains(name) {
            problems.push(format!("TOPIC_KEYS entry {name} is not a TopicId member"));
        }
        if RESERVED_KEY_SEGMENTS.contains(key) {
            problems.push(format!("topic key '{key}' is a reserved key segment"));
        }
        let well_formed = key.starts_with(|c: char| c.is_ascii_lowercase())
            && key
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !well_formed {
            problems.push(format!(
                "topic key '{key}' must be lowercase snake_case starting with a letter"
            ));
        }
    }
    for member in &members {
        if !seen_names.contains(member) {
            problems.push(format!("TopicId {member} has no TOPIC_KEYS entry"));
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        fail(format!(
            "topic key table is invalid:\n{}",
            problems.join("\n")
        ))
    }
}

fn topic_key(member: &str) -> Result<&'static str> {
    TOPIC_KEYS
        .iter()
        .find(|(name, _)| *name == member)
        .map(|(_, key)| *key)
        .ok_or_else(|| io::Error::other(format!("TopicId {member} has no TOPIC_KEYS entry")).into())
}

fn topic_entries(schema: &CompiledSchema) -> Result<Vec<TopicEntry>> {
    let topic_enum = schema
        .files
        .iter()
        .flat_map(|file| &file.entities)
        .find(|entity| entity.kind == SchemaEntityKind::Enum && entity.name == "TopicId")
        .ok_or_else(|| io::Error::other("TopicId enum not found"))?;
    validate_topic_keys(topic_enum)?;

    let mut topics = Vec::new();
    for member in &topic_enum.members {
        let Some(value) = &member.value else {
            return fail(format!(
                "TopicId {} is missing an explicit value",
                member.name
            ));
        };
        let id = value.parse::<u16>().map_err(|err| {
            io::Error::other(format!(
                "TopicId {} has invalid value {value}: {err}",
                member.name
            ))
        })?;
        if id == 0 || member.name == "Unknown" {
            continue;
        }

        let (schema_file, root_table) =
            find_schema_entity(schema, &member.name).ok_or_else(|| {
                io::Error::other(format!(
                    "TopicId {} does not match a root table in the schema",
                    member.name
                ))
            })?;
        if root_table.kind != SchemaEntityKind::Table {
            return fail(format!(
                "TopicId {} resolves to {} {}, expected a table",
                member.name,
                root_table.kind.as_str(),
                root_table.name
            ));
        }

        let payload_type = thin_root_wrapper_payload(root_table).map(type_lookup_name);
        let payload_entity = payload_type
            .as_ref()
            .and_then(|payload| find_schema_entity(schema, payload));
        let payload_type_namespace = payload_entity.map(|(_, entity)| entity.namespace.clone());
        let fixed_layout =
            payload_entity.is_some_and(|(_, entity)| entity.kind == SchemaEntityKind::Struct);
        let payload_size = payload_entity.and_then(|(_, entity)| entity.byte_size);
        let multi_instance = fixed_layout
            && payload_entity
                .is_some_and(|(_, entity)| entity.members.iter().any(|member| member.name == "id"));
        let scope = if VEHICLE_SCOPE_TOPICS.contains(&member.name.as_str()) {
            "vehicle"
        } else {
            "any"
        };
        let encoding = if fixed_layout { "struct" } else { "table" };
        let wire_entity = if fixed_layout {
            payload_entity
                .map(|(_, entity)| entity)
                .expect("fixed-layout topic has a payload entity")
        } else {
            root_table
        };
        let wire_type = qualified_name(&wire_entity.namespace, &wire_entity.name);
        let schema_hash = wire_schema_hash(schema, wire_entity)?;
        topics.push(TopicEntry {
            id,
            name: member.name.clone(),
            key: topic_key(&member.name)?.to_string(),
            root_table: root_table.name.clone(),
            root_table_namespace: root_table.namespace.clone(),
            payload_type,
            payload_type_namespace,
            payload_size,
            schema_file: schema_file.name.clone(),
            wire_type,
            schema_hash,
            fixed_layout,
            multi_instance,
            scope,
            encoding,
            description: String::new(),
        });
    }

    Ok(topics)
}

/// Hash the exact transitive FlatBuffers contract reachable from one wire
/// type. Comments, source files, and unrelated declarations are deliberately
/// excluded so documentation and unrelated message changes do not invalidate
/// compatible values.
fn wire_schema_hash(schema: &CompiledSchema, root: &SchemaEntity) -> Result<String> {
    let mut reachable = BTreeMap::<String, &SchemaEntity>::new();
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        let qualified = qualified_name(&entity.namespace, &entity.name);
        if reachable.insert(qualified, entity).is_some() {
            continue;
        }
        if let Some(value_type) = entity.value_type.as_deref()
            && let Some(referenced) = resolve_schema_type(schema, &entity.namespace, value_type)
        {
            pending.push(referenced);
        }
        for member in &entity.members {
            if let Some(type_name) = member.type_name.as_deref()
                && let Some(referenced) = resolve_schema_type(schema, &entity.namespace, type_name)
            {
                pending.push(referenced);
            }
        }
    }

    let mut digest = Sha256::new();
    digest.update(b"synapse-flatbuffers-wire-schema-v1\n");
    for (qualified, entity) in reachable {
        digest.update(b"entity\t");
        digest.update(entity.kind.as_str().as_bytes());
        digest.update(b"\t");
        digest.update(qualified.as_bytes());
        digest.update(b"\t");
        digest.update(entity.value_type.as_deref().unwrap_or("").as_bytes());
        digest.update(b"\n");
        for member in &entity.members {
            digest.update(b"member\t");
            digest.update(member.name.as_bytes());
            digest.update(b"\t");
            digest.update(member.type_name.as_deref().unwrap_or("").as_bytes());
            digest.update(b"\t");
            if let Some(type_name) = member.type_name.as_deref()
                && let Some(target) = resolve_schema_type(schema, &entity.namespace, type_name)
            {
                digest.update(qualified_name(&target.namespace, &target.name).as_bytes());
            }
            digest.update(b"\t");
            digest.update(member.value.as_deref().unwrap_or("").as_bytes());
            digest.update(b"\n");
        }
    }
    let digest = digest.finalize();
    Ok(digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn resolve_schema_type<'a>(
    schema: &'a CompiledSchema,
    namespace: &str,
    type_name: &str,
) -> Option<&'a SchemaEntity> {
    let atom = type_name
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(':')
        .next()
        .unwrap_or(type_name)
        .trim();
    if is_scalar_type(atom) || matches!(atom, "string") {
        return None;
    }
    let qualified = if atom.contains('.') || namespace.is_empty() {
        atom.to_string()
    } else {
        format!("{namespace}.{atom}")
    };
    schema
        .files
        .iter()
        .flat_map(|file| &file.entities)
        .find(|entity| qualified_name(&entity.namespace, &entity.name) == qualified)
        .or_else(|| {
            schema
                .files
                .iter()
                .flat_map(|file| &file.entities)
                .find(|entity| entity.name == type_lookup_name(atom))
        })
}

fn command_payload_metadata(
    schema: &CompiledSchema,
    type_name: &str,
) -> Result<CommandPayloadMetadata> {
    let lookup = type_lookup_name(type_name);
    let Some((_, entity)) = find_schema_entity(schema, &lookup) else {
        return fail(format!(
            "command type {type_name} does not resolve to a schema entity"
        ));
    };
    match entity.kind {
        SchemaEntityKind::Struct => Ok(CommandPayloadMetadata {
            encoding: "struct",
            size: entity.byte_size,
        }),
        SchemaEntityKind::Table => Ok(CommandPayloadMetadata {
            encoding: "table",
            size: None,
        }),
        _ => fail(format!(
            "command type {type_name} resolves to {} {}, expected struct or table",
            entity.kind.as_str(),
            entity.name
        )),
    }
}

/// Resolve the COMMANDS table against compiler reflection, computing each
/// request and reply type's payload metadata and transitive schema hash.
fn command_entries(schema: &CompiledSchema) -> Result<Vec<CommandEntry>> {
    let mut commands = Vec::new();
    for (id, name, request_type, reply_type, description) in COMMANDS {
        let request_meta = command_payload_metadata(schema, request_type)?;
        let reply_meta = command_payload_metadata(schema, reply_type)?;
        commands.push(CommandEntry {
            id: *id,
            name: (*name).to_string(),
            key: format!("{CMD_KEY_PREFIX}/{name}"),
            request_type: (*request_type).to_string(),
            request_schema_hash: command_schema_hash(schema, request_type)?,
            request_encoding: request_meta.encoding,
            request_size: request_meta.size,
            reply_type: (*reply_type).to_string(),
            reply_schema_hash: command_schema_hash(schema, reply_type)?,
            reply_encoding: reply_meta.encoding,
            reply_size: reply_meta.size,
            description: (*description).to_string(),
        });
    }
    Ok(commands)
}

fn command_schema_hash(schema: &CompiledSchema, type_name: &str) -> Result<String> {
    let Some((_, entity)) = find_schema_entity(schema, &type_lookup_name(type_name)) else {
        return fail(format!(
            "command type {type_name} does not resolve to a schema entity"
        ));
    };
    let qualified = qualified_name(&entity.namespace, &entity.name);
    if qualified != type_name {
        return fail(format!(
            "command type {type_name} resolves to {qualified}; declare the fully qualified name"
        ));
    }
    wire_schema_hash(schema, entity)
}

fn is_scalar_type(type_name: &str) -> bool {
    matches!(
        type_name,
        "bool"
            | "byte"
            | "int8"
            | "ubyte"
            | "uint8"
            | "short"
            | "int16"
            | "ushort"
            | "uint16"
            | "int"
            | "int32"
            | "uint"
            | "uint32"
            | "float"
            | "float32"
            | "long"
            | "int64"
            | "ulong"
            | "uint64"
            | "double"
            | "float64"
    )
}

fn enum_base_type(value_type: &str) -> &str {
    value_type.split_whitespace().next().unwrap_or(value_type)
}

#[derive(Clone, Debug, Serialize)]
struct FieldDescTemplateEntry {
    name: String,
    offset: usize,
    kind: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct TopicFieldsTemplateEntry {
    payload_type: String,
    topic_id: u16,
    payload_size: usize,
    field_count: usize,
    fields: Vec<FieldDescTemplateEntry>,
}

fn scalar_field_kind(type_name: &str) -> Option<&'static str> {
    Some(match type_name {
        "bool" => "SYNAPSE_FIELD_BOOL",
        "byte" | "int8" => "SYNAPSE_FIELD_I8",
        "ubyte" | "uint8" => "SYNAPSE_FIELD_U8",
        "short" | "int16" => "SYNAPSE_FIELD_I16",
        "ushort" | "uint16" => "SYNAPSE_FIELD_U16",
        "int" | "int32" => "SYNAPSE_FIELD_I32",
        "uint" | "uint32" => "SYNAPSE_FIELD_U32",
        "long" | "int64" => "SYNAPSE_FIELD_I64",
        "ulong" | "uint64" => "SYNAPSE_FIELD_U64",
        "float" | "float32" => "SYNAPSE_FIELD_F32",
        "double" | "float64" => "SYNAPSE_FIELD_F64",
        _ => return None,
    })
}

/// Flatten FlatCC-reflected fixed-layout fields into scalar descriptors.
/// Nested struct members become dotted names ("attitude.w").
fn collect_field_descs(
    schema: &CompiledSchema,
    type_name: &str,
    prefix: &str,
    base_offset: usize,
    out: &mut Vec<FieldDescTemplateEntry>,
) -> Result<()> {
    let lookup = type_lookup_name(type_name);
    if let Some(kind) = scalar_field_kind(&lookup) {
        out.push(FieldDescTemplateEntry {
            name: prefix.to_string(),
            offset: base_offset,
            kind,
        });
        return Ok(());
    }

    let Some((_, entity)) = find_schema_entity(schema, &lookup) else {
        return fail(format!(
            "cannot collect field descriptors for unknown type {lookup}"
        ));
    };
    match entity.kind {
        SchemaEntityKind::Enum => {
            let base = entity
                .value_type
                .as_deref()
                .map(enum_base_type)
                .unwrap_or_default();
            let Some(kind) = scalar_field_kind(base) else {
                return fail(format!("enum {lookup} has unsupported base type '{base}'"));
            };
            out.push(FieldDescTemplateEntry {
                name: prefix.to_string(),
                offset: base_offset,
                kind,
            });
        }
        SchemaEntityKind::Struct => {
            for member in &entity.members {
                let Some(member_type) = member.type_name.as_deref() else {
                    return fail(format!(
                        "struct {lookup} member {} is missing a type",
                        member.name
                    ));
                };
                let member_offset = member.offset.ok_or_else(|| {
                    io::Error::other(format!(
                        "FlatCC reflection omitted the offset for {lookup}.{}",
                        member.name
                    ))
                })?;
                let name = if prefix.is_empty() {
                    member.name.clone()
                } else {
                    format!("{prefix}.{}", member.name)
                };
                collect_field_descs(schema, member_type, &name, base_offset + member_offset, out)?;
            }
        }
        _ => {
            return fail(format!(
                "{lookup} is a {}, not a fixed-layout struct or enum",
                entity.kind.as_str()
            ));
        }
    }
    Ok(())
}

fn topic_print_context(schema: &CompiledSchema, topics: &[TopicEntry]) -> Result<Value> {
    let mut structs = Vec::new();
    for topic in topics {
        if !topic.fixed_layout {
            continue;
        }
        let payload = topic
            .payload_type
            .as_deref()
            .expect("fixed-layout topic has a payload type");
        let payload_size = topic
            .payload_size
            .expect("fixed-layout topic has a payload size");
        let mut fields = Vec::new();
        collect_field_descs(schema, payload, "", 0, &mut fields)?;
        structs.push(TopicFieldsTemplateEntry {
            payload_type: type_lookup_name(payload),
            topic_id: topic.id,
            payload_size,
            field_count: fields.len(),
            fields,
        });
    }
    Ok(context! { structs => structs })
}

fn write_c_topic_print(
    templates: &Templates,
    schema: &CompiledSchema,
    topics: &[TopicEntry],
    header_path: &Path,
    source_path: &Path,
) -> Result<()> {
    let context = topic_print_context(schema, topics)?;
    templates.render_to_file(
        "xtask/topic_catalog/topic_print.h.jinja",
        context.clone(),
        header_path,
    )?;
    templates.render_to_file(
        "xtask/topic_catalog/topic_print.c.jinja",
        context,
        source_path,
    )
}

/// Protocol-level consistency checks over FlatCC reflection: TopicId
/// contiguity, TopicId/union agreement, and command type resolution.
fn validate_protocol(schema: &CompiledSchema) -> Result<()> {
    let mut problems = Vec::new();

    let topic_enum = schema
        .files
        .iter()
        .flat_map(|file| &file.entities)
        .find(|entity| entity.kind == SchemaEntityKind::Enum && entity.name == "TopicId")
        .ok_or_else(|| io::Error::other("TopicId enum not found"))?;
    let topic_names = topic_enum
        .members
        .iter()
        .filter(|member| member.name != "Unknown")
        .map(|member| member.name.as_str())
        .collect::<Vec<_>>();

    for (index, member) in topic_enum
        .members
        .iter()
        .filter(|member| member.name != "Unknown")
        .enumerate()
    {
        let expected = (index + 1).to_string();
        if member.value.as_deref() != Some(expected.as_str()) {
            problems.push(format!(
                "TopicId {} has value {}, expected contiguous value {expected}",
                member.name,
                member.value.as_deref().unwrap_or("<none>")
            ));
        }
    }

    match schema
        .files
        .iter()
        .flat_map(|file| &file.entities)
        .find(|entity| entity.kind == SchemaEntityKind::Union && entity.name == "SynapseMessage")
    {
        Some(union_entity) => {
            let union_names = union_entity
                .members
                .iter()
                .map(|member| member.name.as_str())
                .collect::<Vec<_>>();
            if union_names != topic_names {
                problems.push(format!(
                    "SynapseMessage union does not mirror TopicId.\n  TopicId: {}\n  union:   {}",
                    topic_names.join(", "),
                    union_names.join(", ")
                ));
            }
        }
        None => problems.push("SynapseMessage union not found".to_string()),
    }

    for (_, name, request_type, reply_type, _) in COMMANDS {
        for type_name in [request_type, reply_type] {
            if find_schema_entity(schema, type_name).is_none() {
                problems.push(format!(
                    "command {name} references unknown type {type_name}"
                ));
            }
        }
    }

    // CmdId in fbs/transfer.fbs must mirror the COMMANDS table.
    match schema
        .files
        .iter()
        .flat_map(|file| &file.entities)
        .find(|entity| entity.kind == SchemaEntityKind::Enum && entity.name == "CmdId")
    {
        Some(cmd_enum) => {
            let enum_entries = cmd_enum
                .members
                .iter()
                .filter(|member| member.name != "Unknown")
                .map(|member| member.value.clone().unwrap_or_default())
                .collect::<Vec<_>>();
            let command_entries = COMMANDS
                .iter()
                .map(|(id, _, _, _, _)| id.to_string())
                .collect::<Vec<_>>();
            if enum_entries != command_entries {
                problems.push(format!(
                    "CmdId enum does not mirror the xtask COMMANDS table.\n  CmdId:    {enum_entries:?}\n  COMMANDS: {command_entries:?}"
                ));
            }
        }
        None => problems.push("CmdId enum not found in fbs/transfer.fbs".to_string()),
    }

    if problems.is_empty() {
        Ok(())
    } else {
        fail(format!(
            "protocol validation failed:\n{}",
            problems.join("\n")
        ))
    }
}

fn find_schema_entity<'a>(
    schema: &'a CompiledSchema,
    name: &str,
) -> Option<(&'a SchemaFile, &'a SchemaEntity)> {
    let lookup = type_lookup_name(name);
    schema.files.iter().find_map(|file| {
        file.entities
            .iter()
            .find(|entity| entity.name == lookup)
            .map(|entity| (file, entity))
    })
}

fn thin_root_wrapper_payload(entity: &SchemaEntity) -> Option<&str> {
    if entity.kind != SchemaEntityKind::Table || entity.members.len() != 1 {
        return None;
    }
    let member = &entity.members[0];
    if member.name != "data" {
        return None;
    }
    member
        .type_name
        .as_deref()
        .filter(|type_name| type_lookup_name(type_name).ends_with("Data"))
}

/// Path of a schema entity inside the flatc-generated Rust module tree, for
/// example ("synapse.topic", "VehicleHealthData") -> "synapse::topic::VehicleHealthData".
fn rust_module_path(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", namespace.replace('.', "::"))
    }
}

/// Fully qualified FlatBuffers name, for example "synapse.topic.VehicleHealthData".
fn qualified_name(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{namespace}.{name}")
    }
}

fn type_lookup_name(type_name: &str) -> String {
    type_name
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .rsplit('.')
        .next()
        .unwrap_or(type_name)
        .trim()
        .to_string()
}

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
