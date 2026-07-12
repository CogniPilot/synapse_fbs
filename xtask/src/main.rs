use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

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

const LEGACY_DOC_DIRS: &[&str] = &["0.1.6"];

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
    mdbook_version: String,
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
    docs: DocsTools,
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

#[derive(Debug, serde::Deserialize)]
struct DocsTools {
    mdbook: String,
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
        "check" => check(&root),
        "update-compatibility" => update_compatibility(&root),
        _ => fail(format!(
            "unknown command '{command}'. expected: ci, js, docs, check, or update-compatibility"
        )),
    }
}

fn check(root: &Path) -> Result<()> {
    let docs = parse_schema_docs(root)?;
    validate_schema_docs(&docs)?;
    validate_protocol(&docs)?;
    let topics = topic_entries(&docs)?;
    let commands = command_entries(&docs)?;
    check_schema_compatibility(root, &topics, &commands)?;

    let templates = Templates::new(root)?;
    let check_dir = root.join("target/xtask/check");
    reset_dir(&check_dir)?;
    let context = topic_catalog_context(&docs, &topics)?;
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
        &docs,
        &topics,
        &check_dir.join("synapse/topic_print.h"),
        &check_dir.join("topic_print.c"),
    )?;
    templates.render_to_file(
        "xtask/topic_catalog/schemas.rs.jinja",
        embedded_schemas_context(&docs)?,
        &check_dir.join("schemas.rs"),
    )?;
    templates.render_to_file(
        "xtask/topic_catalog/topic_decode.rs.jinja",
        context.clone(),
        &check_dir.join("topic_decode.rs"),
    )?;
    smoke_catalog_helpers(&check_dir)?;

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
    let flatc = build_flatc(root, &tools)?;
    let flatcc = build_flatcc(root, &tools)?;
    generate_bindings(root, &flatc, &templates, &packages)?;
    check_rust_package(&packages.rust)?;
    build_python_package(root, &packages.python, &tools)?;
    build_js_package(root, &templates, &packages.js, &flatc, &tools, true)?;
    build_archives(root, &tools, &flatc, &flatcc, &options.release_name)?;
    generate_docs_site(
        root,
        &tools,
        &default_docs_version(&tools, options),
        &root.join("target/xtask/docs"),
    )?;

    Ok(())
}

const WIRE_SCHEMA_COMPATIBILITY_PATH: &str = "compatibility/wire-schema.toml";

#[derive(serde::Deserialize, serde::Serialize)]
struct WireSchemaCompatibility {
    wire_types: BTreeMap<String, String>,
}

/// Every wire type an endpoint may decode: topic payloads plus command
/// request and reply types, each mapped to its transitive schema hash.
fn wire_type_compatibility(
    topics: &[TopicEntry],
    commands: &[CommandEntry],
) -> BTreeMap<String, String> {
    let mut wire_types: BTreeMap<String, String> = topics
        .iter()
        .map(|topic| (topic.wire_type.clone(), topic.schema_hash.clone()))
        .collect();
    for command in commands {
        wire_types.insert(
            command.request_type.clone(),
            command.request_schema_hash.clone(),
        );
        wire_types.insert(
            command.reply_type.clone(),
            command.reply_schema_hash.clone(),
        );
    }
    wire_types
}

/// Hash the full catalog contract a constrained link relies on after the
/// one-time handshake: topic id and key routing, payload interpretation
/// (encoding, wire type, transitive schema hash, instance key grammar), and
/// command ids with their request/reply contracts. Policy and documentation
/// fields (scope, descriptions) are deliberately excluded. Endpoints that
/// agree on this hash agree on how every frame is routed, decoded, and
/// restored to canonical Zenoh form.
fn schema_set_hash(topics: &[TopicEntry], commands: &[CommandEntry]) -> String {
    let mut canonical = String::from("synapse-schema-set-v3\n");
    let mut sorted_topics: Vec<&TopicEntry> = topics.iter().collect();
    sorted_topics.sort_by_key(|topic| topic.id);
    for topic in sorted_topics {
        canonical.push_str(&format!(
            "topic\t{}\t{}\t{}\t{}\t{}\t{}\n",
            topic.id,
            topic.key,
            if topic.multi_instance { 1 } else { 0 },
            topic.encoding,
            topic.wire_type,
            topic.schema_hash,
        ));
    }
    let mut sorted_commands: Vec<&CommandEntry> = commands.iter().collect();
    sorted_commands.sort_by_key(|command| command.id);
    for command in sorted_commands {
        canonical.push_str(&format!(
            "cmd\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            command.id,
            command.name,
            command.request_encoding,
            command.request_type,
            command.request_schema_hash,
            command.reply_encoding,
            command.reply_type,
            command.reply_schema_hash,
        ));
    }
    let digest = Sha256::digest(canonical.as_bytes());
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_wire_schema_compatibility(path: &Path) -> Result<BTreeMap<String, String>> {
    let content = fs::read_to_string(path).map_err(|err| {
        io::Error::other(format!(
            "cannot read {}: {err}; run xtask update-compatibility to establish the compatibility baseline",
            path.display()
        ))
    })?;
    let manifest: WireSchemaCompatibility = toml::from_str(&content)
        .map_err(|err| io::Error::other(format!("invalid {}: {err}", path.display())))?;
    for (wire_type, hash) in &manifest.wire_types {
        if hash.len() != 32 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return fail(format!(
                "{} has an invalid 128-bit truncated SHA-256 for {wire_type}",
                path.display(),
            ));
        }
    }
    Ok(manifest.wire_types)
}

fn check_schema_compatibility(
    root: &Path,
    topics: &[TopicEntry],
    commands: &[CommandEntry],
) -> Result<()> {
    let path = root.join(WIRE_SCHEMA_COMPATIBILITY_PATH);
    let expected = parse_wire_schema_compatibility(&path)?;
    let actual = wire_type_compatibility(topics, commands);
    let mut problems = Vec::new();
    for (wire_type, new_hash) in &actual {
        match expected.get(wire_type) {
            Some(old_hash) if new_hash != old_hash => problems.push(format!(
                "BREAKING: {wire_type} changed from {old_hash} to {new_hash}; introduce a new wire type and topic instead"
            )),
            None => problems.push(format!(
                "NEW: {wire_type} ({new_hash}); review it, then run xtask update-compatibility"
            )),
            _ => {}
        }
    }
    for wire_type in expected.keys() {
        if !actual.contains_key(wire_type) {
            problems.push(format!(
                "REMOVED: {wire_type}; run xtask update-compatibility after reviewing the removal"
            ));
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    fail(format!(
        "topic schema compatibility check failed:\n{}",
        problems.join("\n")
    ))
}

fn update_compatibility(root: &Path) -> Result<()> {
    let docs = parse_schema_docs(root)?;
    validate_schema_docs(&docs)?;
    validate_protocol(&docs)?;
    let topics = topic_entries(&docs)?;
    let commands = command_entries(&docs)?;
    let actual = wire_type_compatibility(&topics, &commands);
    let path = root.join(WIRE_SCHEMA_COMPATIBILITY_PATH);
    let existing = if path.is_file() {
        parse_wire_schema_compatibility(&path)?
    } else {
        BTreeMap::new()
    };
    for (wire_type, new_hash) in &actual {
        if let Some(old_hash) = existing.get(wire_type)
            && new_hash != old_hash
        {
            return fail(format!(
                "refusing to rewrite published {wire_type}: {old_hash} -> {new_hash}; introduce a new wire type and topic"
            ));
        }
    }
    let output = format!(
        "# Current Synapse wire-type allowlist. Generated by xtask update-compatibility.\n# Unknown types and hashes are incompatible and must not be decoded.\n{}",
        toml::to_string_pretty(&WireSchemaCompatibility { wire_types: actual })?
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_file(&path, &output)?;
    println!("updated {}", path.display());
    Ok(())
}

fn js(root: &Path) -> Result<()> {
    let tools = read_tools(root)?;
    let templates = Templates::new(root)?;

    let package = root.join("target/xtask/packages/js");
    stage_template_tree(root, "js", &package, &templates, package_context(&tools))?;
    let flatc = build_flatc(root, &tools)?;
    build_js_package(root, &templates, &package, &flatc, &tools, false)?;

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

    generate_docs_site(root, &tools, &version, &out_dir)
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
        mdbook_version: parsed.docs.mdbook,
    })
}

/// Exercise the rendered catalog helpers with whichever toolchains are
/// available locally; each check is skipped when its tool is missing.
fn smoke_catalog_helpers(check_dir: &Path) -> Result<()> {
    if command_succeeds(Command::new("node").arg("--version")) {
        let script = r#"import { parseKey, keyForTopic, topicByKey, topicByName, commandByName, schemaSetHash, mcapProfile, mcapSchemaEncoding, mcapMessageEncoding, mcapMetadataName, mcapTimeBasisMonotonicBoot } from './topic_catalog.js';
if (!/^[0-9a-f]{32}$/.test(schemaSetHash)) throw new Error('bad schema-set hash');
if (mcapProfile !== 'synapse/1' || mcapSchemaEncoding !== 'flatbuffer' || mcapMessageEncoding !== 'flatbuffer') throw new Error('bad MCAP contract');
if (mcapMetadataName !== 'synapse' || mcapTimeBasisMonotonicBoot !== 'monotonic_boot') throw new Error('bad MCAP metadata contract');
if (topicByName('Odometry').mcapSchemaName !== 'synapse.topic.Odometry') throw new Error('bad MCAP schema name');
if (topicByName('Odometry').mcapSchemaFile !== 'bfbs/state.bfbs') throw new Error('bad MCAP schema file');
const parsed = parseKey('cub1/imu/0');
if (!parsed || parsed.namespace !== 'cub1' || parsed.topic.name !== 'InertialSample' || parsed.instance !== 0) throw new Error('bad parseKey');
if (parseKey('health')?.namespace !== '') throw new Error('bad empty namespace');
if (parseKey('qualisys/cub1/external_pose')?.namespace !== 'qualisys/cub1') throw new Error('bad nested namespace');
if (parseKey('cub1/nope') !== undefined) throw new Error('unknown key should fail');
if (parseKey('health/')?.topic.name !== 'VehicleHealth') throw new Error('trailing slash should parse');
if (parseKey('imu/+1') !== undefined) throw new Error('signed instance should fail');
if (parseKey('imu/4294967296') !== undefined) throw new Error('oversized instance should fail');
if (keyForTopic('VehicleHealth') !== 'health') throw new Error('bad key helper');
if (topicByKey('/gnss')?.name !== 'GnssFix') throw new Error('bad topicByKey');
if (commandByName('mission_set')?.key !== 'cmd/mission_set') throw new Error('bad command helper');
if (commandByName('param_get')?.requestType !== 'synapse.cmd.ParamGetRequest') throw new Error('bad command type');
if (commandByName('param_get')?.requestEncoding !== 'table') throw new Error('bad command encoding');
if (commandByName('param_get')?.requestSize !== null) throw new Error('bad command size');
if (!/^[0-9a-f]{32}$/.test(commandByName('param_get')?.requestSchemaHash)) throw new Error('bad command request hash');
if (!/^[0-9a-f]{32}$/.test(commandByName('param_get')?.replySchemaHash)) throw new Error('bad command reply hash');
if (commandByName('firmware_prepare')?.key !== 'cmd/firmware_prepare') throw new Error('bad firmware command key');
if (commandByName('firmware_prepare')?.requestType !== 'synapse.cmd.FirmwarePrepareRequest') throw new Error('bad firmware command type');
if (topicByKey('fw')?.id !== 34) throw new Error('bad firmware progress topic');
console.log('catalog js helpers ok');
"#;
        run(Command::new("node")
            .current_dir(check_dir)
            .arg("--input-type=module")
            .arg("-e")
            .arg(script))?;
    }

    if let Ok(python) = python_bin() {
        let code = r#"import sys
sys.path.insert(0, ".")
import topic_catalog as tc
assert len(tc.SCHEMA_SET_HASH) == 32
assert tc.MCAP_PROFILE == "synapse/1"
assert tc.MCAP_SCHEMA_ENCODING == "flatbuffer"
assert tc.MCAP_MESSAGE_ENCODING == "flatbuffer"
assert tc.MCAP_METADATA_NAME == "synapse"
assert tc.MCAP_TIME_BASIS_MONOTONIC_BOOT == "monotonic_boot"
assert tc.topic_by_name("Odometry").mcap_schema_name == "synapse.topic.Odometry"
assert tc.topic_by_name("Odometry").mcap_schema_file == "bfbs/state.bfbs"
parsed = tc.parse_key("cub1/imu/0")
assert parsed is not None and parsed.namespace == "cub1"
assert parsed.topic.name == "InertialSample" and parsed.instance == 0
assert tc.parse_key("health").namespace == ""
assert tc.parse_key("qualisys/cub1/external_pose").namespace == "qualisys/cub1"
assert tc.parse_key("cub1/nope") is None
assert tc.parse_key("health/").topic.name == "VehicleHealth"
assert tc.parse_key("imu/+1") is None
assert tc.parse_key("imu/4294967296") is None
assert tc.parse_key("imu/٢") is None
assert tc.key_for_topic("VehicleHealth") == "health"
assert tc.topic_by_key("/gnss").name == "GnssFix"
assert tc.command_by_name("mission_set").key == "cmd/mission_set"
assert tc.command_by_name("param_get").request_type == "synapse.cmd.ParamGetRequest"
assert tc.command_by_name("param_get").request_encoding == "table"
assert tc.command_by_name("param_get").request_size is None
assert len(tc.command_by_name("param_get").request_schema_hash) == 32
assert len(tc.command_by_name("param_get").reply_schema_hash) == 32
assert tc.command_by_name("firmware_prepare").key == "cmd/firmware_prepare"
assert tc.command_by_name("firmware_prepare").request_type == "synapse.cmd.FirmwarePrepareRequest"
assert tc.topic_by_key("fw").id == 34
print("catalog python helpers ok")
"#;
        run(Command::new(&python)
            .current_dir(check_dir)
            .arg("-c")
            .arg(code))?;
    }

    if command_succeeds(Command::new("cc").arg("--version")) {
        write_file(&check_dir.join("catalog_test.c"), C_CATALOG_TEST)?;
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

        write_file(&check_dir.join("print_test.c"), C_PRINT_TEST)?;
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
        write_file(&check_dir.join("catalog_test.rs"), RUST_CATALOG_TEST)?;
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

/// Runtime assertions for the C catalog helpers, exercising the shared
/// parse-key grammar (identical cases to the other language smokes).
const C_CATALOG_TEST: &str = r#"#include "topic_catalog.h"
#include <assert.h>
#include <stdio.h>

int main(void) {
    assert(strlen(SYNAPSE_SCHEMA_SET_HASH) == 32);
    assert(strcmp(SYNAPSE_MCAP_PROFILE, "synapse/1") == 0);
    assert(strcmp(SYNAPSE_MCAP_SCHEMA_ENCODING, "flatbuffer") == 0);
    assert(strcmp(SYNAPSE_MCAP_MESSAGE_ENCODING, "flatbuffer") == 0);
    assert(strcmp(SYNAPSE_MCAP_METADATA_NAME, "synapse") == 0);
    assert(strcmp(SYNAPSE_MCAP_TIME_BASIS_MONOTONIC_BOOT,
                  "monotonic_boot") == 0);
    const char *ns = NULL;
    size_t ns_len = 0;
    int32_t instance = -2;

    const synapse_topic_info_t *topic = synapse_topic_parse_key(
        "cub1/imu/0", &ns, &ns_len, &instance);
    assert(topic != NULL && strcmp(topic->name, "InertialSample") == 0);
    assert(ns_len == 4 && strncmp(ns, "cub1", ns_len) == 0);
    assert(instance == 0);

    topic = synapse_topic_parse_key(
        "/cub1/gnss", &ns, &ns_len, &instance);
    assert(topic != NULL && strcmp(topic->name, "GnssFix") == 0);
    assert(ns_len == 4 && strncmp(ns, "cub1", ns_len) == 0);
    assert(instance == -1);

    topic = synapse_topic_parse_key(
        "qualisys/cub1/external_pose", &ns, &ns_len, &instance);
    assert(topic != NULL && strcmp(topic->name, "ExternalOdometry") == 0);
    assert(ns_len == 13 && strncmp(ns, "qualisys/cub1", ns_len) == 0);
    assert(instance == -1);

    topic = synapse_topic_parse_key("health/", NULL, NULL, NULL);
    assert(topic != NULL && strcmp(topic->name, "VehicleHealth") == 0);

    topic = synapse_topic_parse_key("health", &ns, &ns_len, &instance);
    assert(topic != NULL && ns_len == 0 && instance == -1);

    assert(synapse_topic_parse_key("imu/+1", NULL, NULL, NULL) == NULL);
    assert(synapse_topic_parse_key(
        "imu/4294967296", NULL, NULL, NULL) == NULL);
    assert(synapse_topic_parse_key("cub1/nope", NULL, NULL, NULL) == NULL);
    assert(synapse_topic_parse_key("imu/0/extra", NULL, NULL, NULL) == NULL);

    const synapse_topic_info_t *by_key = synapse_topic_by_key("cub1/health");
    assert(by_key != NULL && by_key->id == 1);

    const synapse_topic_info_t *odom = synapse_topic_by_name("Odometry");
    assert(odom != NULL &&
           strcmp(odom->mcap_schema_name, "synapse.topic.Odometry") == 0 &&
           strcmp(odom->mcap_schema_file, "bfbs/state.bfbs") == 0);

    const synapse_command_info_t *command = synapse_command_by_name("param_get");
    assert(command != NULL && command->id == 1 &&
           strcmp(command->request_type, "synapse.cmd.ParamGetRequest") == 0);
    assert(strcmp(command->request_encoding, "table") == 0 &&
           command->request_size == 0);
    assert(strlen(command->request_schema_hash) == 32 &&
           strlen(command->reply_schema_hash) == 32);

    command = synapse_command_by_name("firmware_prepare");
    assert(command != NULL && command->id == 9 &&
           strcmp(command->key, "cmd/firmware_prepare") == 0 &&
           strcmp(command->request_type,
                  "synapse.cmd.FirmwarePrepareRequest") == 0);

    topic = synapse_topic_by_key("fw");
    assert(topic != NULL && topic->id == 34 && !topic->fixed_layout);

    printf("catalog c helpers ok\n");
    return 0;
}
"#;

/// Runtime assertions for the generated C field-descriptor printer:
/// descriptor lookup, dotted nested-struct names, rendering, and the
/// failure paths for table topics and payload size mismatches.
const C_PRINT_TEST: &str = r#"#include "topic_catalog.h"
#include <synapse/topic_print.h>
#include <assert.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    const synapse_topic_info_t *health = synapse_topic_by_name("VehicleHealth");
    assert(health != NULL && health->fixed_layout);

    const synapse_topic_fields_t *fields = synapse_topic_fields_by_id(health->id);
    assert(fields != NULL);
    assert(fields->payload_size == health->payload_size);
    assert(fields->field_count > 0);

    unsigned char payload[256] = {0};
    assert(fields->payload_size <= sizeof(payload));

    const synapse_field_desc_t *flight_mode = NULL;
    for (uint16_t i = 0; i < fields->field_count; ++i) {
        if (strcmp(fields->fields[i].name, "flight_mode") == 0) {
            flight_mode = &fields->fields[i];
        }
    }
    assert(flight_mode != NULL && flight_mode->kind == SYNAPSE_FIELD_U8);
    payload[flight_mode->offset] = 7;

    char line[512];
    int written = synapse_topic_snprint(line, sizeof(line), health->id, payload,
                                        fields->payload_size);
    assert(written > 0 && (size_t)written == strlen(line));
    assert(strstr(line, "flight_mode=7") != NULL);
    assert(strstr(line, "timestamp_us=0") != NULL);

    const synapse_topic_info_t *attitude = synapse_topic_by_name("AttitudeEstimate");
    assert(attitude != NULL);
    const synapse_topic_fields_t *attitude_fields =
        synapse_topic_fields_by_id(attitude->id);
    assert(attitude_fields != NULL);
    bool found_quat_w = false;
    for (uint16_t i = 0; i < attitude_fields->field_count; ++i) {
        if (strcmp(attitude_fields->fields[i].name, "attitude.w") == 0) {
            found_quat_w = true;
            assert(attitude_fields->fields[i].kind == SYNAPSE_FIELD_F32);
        }
    }
    assert(found_quat_w);

    const synapse_topic_info_t *text = synapse_topic_by_name("TextStatus");
    assert(text != NULL && !text->fixed_layout);
    assert(synapse_topic_fields_by_id(text->id) == NULL);
    assert(synapse_topic_snprint(line, sizeof(line), health->id, payload,
                                 fields->payload_size + 1) < 0);

    char tiny[8];
    int full = synapse_topic_snprint(tiny, sizeof(tiny), health->id, payload,
                                     fields->payload_size);
    assert(full == written);
    assert(strlen(tiny) < sizeof(tiny));

    printf("topic print c helpers ok\n");
    return 0;
}
"#;

/// Runtime assertions for the Rust catalog helpers, exercising the shared
/// parse-key grammar (identical cases to the other language smokes).
const RUST_CATALOG_TEST: &str = r#"include!("topic_catalog.rs");

fn main() {
    assert_eq!(SCHEMA_SET_HASH.len(), 32);
    assert_eq!(MCAP_PROFILE, "synapse/1");
    assert_eq!(MCAP_SCHEMA_ENCODING, "flatbuffer");
    assert_eq!(MCAP_MESSAGE_ENCODING, "flatbuffer");
    assert_eq!(MCAP_METADATA_NAME, "synapse");
    assert_eq!(MCAP_TIME_BASIS_MONOTONIC_BOOT, "monotonic_boot");
    let odom = topic_by_name("Odometry").unwrap();
    assert_eq!(odom.mcap_schema_name, "synapse.topic.Odometry");
    assert_eq!(odom.mcap_schema_file, "bfbs/state.bfbs");
    let parsed = parse_key("cub1/imu/0").unwrap();
    assert_eq!(parsed.namespace, "cub1");
    assert_eq!(parsed.topic.name, "InertialSample");
    assert_eq!(parsed.instance, Some(0));

    let parsed = parse_key("/cub1/gnss").unwrap();
    assert_eq!(parsed.namespace, "cub1");
    assert_eq!(parsed.instance, None);

    let parsed = parse_key("qualisys/cub1/external_pose").unwrap();
    assert_eq!(parsed.namespace, "qualisys/cub1");
    assert_eq!(parsed.topic.name, "ExternalOdometry");

    assert_eq!(parse_key("health/").unwrap().topic.name, "VehicleHealth");
    assert_eq!(parse_key("health").unwrap().namespace, "");
    assert!(parse_key("imu/+1").is_none());
    assert!(parse_key("imu/4294967296").is_none());
    assert!(parse_key("cub1/nope").is_none());
    assert!(parse_key("imu/0/extra").is_none());

    assert_eq!(topic_by_key("cub1/health").unwrap().id, 1);
    let command = command_by_name("param_get").unwrap();
    assert_eq!(command.id, 1);
    assert_eq!(command.request_type, "synapse.cmd.ParamGetRequest");
    assert_eq!(command.request_encoding, "table");
    assert_eq!(command.request_size, None);
    assert_eq!(command.request_schema_hash.len(), 32);
    assert_eq!(command.reply_schema_hash.len(), 32);

    let command = command_by_name("firmware_prepare").unwrap();
    assert_eq!(command.id, 9);
    assert_eq!(command.key, "cmd/firmware_prepare");
    assert_eq!(command.request_type, "synapse.cmd.FirmwarePrepareRequest");
    let progress = topic_by_key("fw").unwrap();
    assert_eq!(progress.id, 34);
    assert!(!progress.fixed_layout);

    println!("catalog rust helpers ok");
}
"#;

/// Generated-binding round trips for the firmware tables used by the GCS and
/// receiver service. Every release build therefore verifies the field order
/// and public Rust API without checking generated sources into the repository.
const RUST_FIRMWARE_ROUNDTRIP_TEST: &str = r##"use flatbuffers::FlatBufferBuilder;
use synapse_fbs::{
    cmd::{FirmwarePrepareRequest, FirmwarePrepareRequestArgs, FirmwareUpdateState},
    topic::{FirmwareProgress, FirmwareProgressArgs},
    types::CommandResultCode,
};

#[test]
fn firmware_prepare_request_round_trips() {
    let mut builder = FlatBufferBuilder::new();
    let update_id = builder.create_string("update-42");
    let target = builder.create_string("cubs2");
    let board_id = builder.create_string("mr_vmu_tropic");
    let version = builder.create_string("candidate-1");
    let image_hash = "a".repeat(64);
    let selective_hash = "b".repeat(64);
    let image_sha256 = builder.create_string(&image_hash);
    let selective_sha256 = builder.create_string(&selective_hash);
    let signature = builder.create_vector(&[0x10_u8, 0x20, 0x30, 0x40]);
    let manifest = builder.create_string(r#"{"format":"bin"}"#);
    let request = FirmwarePrepareRequest::create(
        &mut builder,
        &FirmwarePrepareRequestArgs {
            update_id: Some(update_id),
            target: Some(target),
            board_id: Some(board_id),
            version: Some(version),
            image_size: 4096,
            image_sha256: Some(image_sha256),
            selective_sha256: Some(selective_sha256),
            requested_chunk_size: 512,
            chunk_count: 8,
            signature: Some(signature),
            manifest: Some(manifest),
        },
    );
    builder.finish(request, None);

    let decoded = flatbuffers::root::<FirmwarePrepareRequest>(builder.finished_data()).unwrap();
    assert_eq!(decoded.update_id(), Some("update-42"));
    assert_eq!(decoded.target(), Some("cubs2"));
    assert_eq!(decoded.board_id(), Some("mr_vmu_tropic"));
    assert_eq!(decoded.version(), Some("candidate-1"));
    assert_eq!(decoded.image_size(), 4096);
    assert_eq!(decoded.image_sha256(), Some(image_hash.as_str()));
    assert_eq!(decoded.selective_sha256(), Some(selective_hash.as_str()));
    assert_eq!(decoded.requested_chunk_size(), 512);
    assert_eq!(decoded.chunk_count(), 8);
    assert_eq!(
        decoded.signature().unwrap().iter().collect::<Vec<_>>(),
        [0x10, 0x20, 0x30, 0x40]
    );
    assert_eq!(decoded.manifest(), Some(r#"{"format":"bin"}"#));
}

#[test]
fn firmware_progress_round_trips() {
    let mut builder = FlatBufferBuilder::new();
    let update_id = builder.create_string("update-42");
    let message = builder.create_string("chunk accepted");
    let progress = FirmwareProgress::create(
        &mut builder,
        &FirmwareProgressArgs {
            timestamp_us: 123_456,
            update_id: Some(update_id),
            state: FirmwareUpdateState::Receiving,
            result: CommandResultCode::InProgress,
            received_bytes: 512,
            total_bytes: 4096,
            received_chunks: 1,
            total_chunks: 8,
            progress_pct: 12,
            message: Some(message),
            result_detail: -7,
        },
    );
    builder.finish(progress, None);

    let decoded = flatbuffers::root::<FirmwareProgress>(builder.finished_data()).unwrap();
    assert_eq!(decoded.timestamp_us(), 123_456);
    assert_eq!(decoded.update_id(), Some("update-42"));
    assert_eq!(decoded.state(), FirmwareUpdateState::Receiving);
    assert_eq!(decoded.result(), CommandResultCode::InProgress);
    assert_eq!(decoded.received_bytes(), 512);
    assert_eq!(decoded.total_bytes(), 4096);
    assert_eq!(decoded.received_chunks(), 1);
    assert_eq!(decoded.total_chunks(), 8);
    assert_eq!(decoded.progress_pct(), 12);
    assert_eq!(decoded.message(), Some("chunk accepted"));
    assert_eq!(decoded.result_detail(), -7);
}
"##;

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
    runtime_source_paths: &str,
    mcap_bfbs_source_paths: &str,
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
        runtime_source_paths => runtime_source_paths,
        mcap_bfbs_source_paths => mcap_bfbs_source_paths,
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

fn generate_bindings(
    root: &Path,
    flatc: &Path,
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
        root.join("python/mcap.py"),
        packages.python.join("synapse/mcap.py"),
    )?;
    write_file(&packages.python.join("synapse/py.typed"), "")?;
    fs::copy(
        root.join("MCAP.md"),
        packages.python.join("synapse/MCAP.md"),
    )?;
    generate_reflection_schemas(root, flatc, &packages.python.join("synapse/bfbs"))?;

    let docs = parse_schema_docs(root)?;
    validate_schema_docs(&docs)?;
    validate_protocol(&docs)?;
    let topics = topic_entries(&docs)?;
    let commands = command_entries(&docs)?;
    check_schema_compatibility(root, &topics, &commands)?;
    write_package_topic_catalogs(templates, packages, &docs, &topics)?;

    // The Rust crate ships the wire contract itself: schema sources, compiled
    // binary schemas, and a generated debug decoder, so downstream tools do
    // not vendor schema copies that can drift from the pinned release.
    copy_dir_all(&root.join("fbs"), &packages.rust.join("fbs"))?;
    fs::copy(root.join("MCAP.md"), packages.rust.join("MCAP.md"))?;
    generate_reflection_schemas(root, flatc, &packages.rust.join("bfbs"))?;
    write_rust_embedded_schemas(templates, &docs, &packages.rust)?;
    write_rust_topic_decode(templates, &docs, &topics, &packages.rust)?;
    write_rust_mcap_fixed(templates, &docs, &topics, &packages.rust)?;

    Ok(())
}

fn write_package_topic_catalogs(
    templates: &Templates,
    packages: &PackagePaths,
    docs: &SchemaDoc,
    topics: &[TopicEntry],
) -> Result<()> {
    write_js_topic_catalogs(templates, &packages.js, docs, topics)?;
    write_rust_topic_catalog(templates, &packages.rust, docs, topics)?;
    write_python_topic_catalog(templates, &packages.python, docs, topics)?;
    Ok(())
}

fn write_js_topic_catalogs(
    templates: &Templates,
    package_root: &Path,
    docs: &SchemaDoc,
    topics: &[TopicEntry],
) -> Result<()> {
    let context = topic_catalog_context(docs, topics)?;
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
    docs: &SchemaDoc,
    topics: &[TopicEntry],
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/topic_catalog.rs.jinja",
        topic_catalog_context(docs, topics)?,
        &package_root.join("src/topic_catalog.rs"),
    )
}

fn write_python_topic_catalog(
    templates: &Templates,
    package_root: &Path,
    docs: &SchemaDoc,
    topics: &[TopicEntry],
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/topic_catalog.py.jinja",
        topic_catalog_context(docs, topics)?,
        &package_root.join("synapse/topic_catalog.py"),
    )
}

fn write_c_topic_catalogs(
    templates: &Templates,
    package_root: &Path,
    docs: &SchemaDoc,
    topics: &[TopicEntry],
) -> Result<()> {
    let context = topic_catalog_context(docs, topics)?;
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
    docs: &SchemaDoc,
    topics: &[TopicEntry],
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/mcap_topics.h.jinja",
        topic_catalog_context(docs, topics)?,
        &package_root.join("include/synapse/mcap_topics.h"),
    )
}

fn write_cpp_mcap_topics(
    templates: &Templates,
    package_root: &Path,
    docs: &SchemaDoc,
    topics: &[TopicEntry],
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/mcap_topics.hpp.jinja",
        topic_catalog_context(docs, topics)?,
        &package_root.join("include/synapse/mcap_topics.hpp"),
    )
}

fn topic_catalog_context(docs: &SchemaDoc, topics: &[TopicEntry]) -> Result<Value> {
    let command_entries = command_entries(docs)?;
    let commands = command_entries
        .iter()
        .map(|command| CommandTemplateEntry {
            id: command.id,
            name: command.name.clone(),
            key: command.key.clone(),
            request_type: command.request_type.clone(),
            reply_type: command.reply_type.clone(),
            request_schema_hash: command.request_schema_hash.clone(),
            reply_schema_hash: command.reply_schema_hash.clone(),
            request_encoding: command.request_encoding,
            request_size: command.request_size,
            reply_encoding: command.reply_encoding,
            reply_size: command.reply_size,
            description: command.description.clone(),
            name_literal: source_string_literal(&command.name),
            key_literal: source_string_literal(&command.key),
            request_type_literal: source_string_literal(&command.request_type),
            reply_type_literal: source_string_literal(&command.reply_type),
            request_schema_hash_literal: source_string_literal(&command.request_schema_hash),
            reply_schema_hash_literal: source_string_literal(&command.reply_schema_hash),
            request_encoding_literal: source_string_literal(command.request_encoding),
            request_size_c: command
                .request_size
                .map(|value| value.to_string())
                .unwrap_or_else(|| "0".to_string()),
            request_size_python: command
                .request_size
                .map(|value| value.to_string())
                .unwrap_or_else(|| "None".to_string()),
            request_size_rust: command
                .request_size
                .map(|value| format!("Some({value})"))
                .unwrap_or_else(|| "None".to_string()),
            reply_encoding_literal: source_string_literal(command.reply_encoding),
            reply_size_c: command
                .reply_size
                .map(|value| value.to_string())
                .unwrap_or_else(|| "0".to_string()),
            reply_size_python: command
                .reply_size
                .map(|value| value.to_string())
                .unwrap_or_else(|| "None".to_string()),
            reply_size_rust: command
                .reply_size
                .map(|value| format!("Some({value})"))
                .unwrap_or_else(|| "None".to_string()),
            description_literal: source_string_literal(&command.description),
        })
        .collect();

    let set_hash = schema_set_hash(topics, &command_entries);
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
                .expect("validated schema path must have a UTF-8 file stem")
                .to_string();
            McapSchemaTemplateEntry {
                symbol: format!("synapse_bfbs_{stem}"),
                schema_file,
            }
        })
        .collect();
    Ok(Value::from_serialize(TopicCatalogContext {
        version: 2,
        schema_set_hash: set_hash.clone(),
        schema_set_hash_literal: source_string_literal(&set_hash),
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
        mcap_profile_literal: source_string_literal(MCAP_PROFILE),
        mcap_schema_encoding_literal: source_string_literal(MCAP_SCHEMA_ENCODING),
        mcap_message_encoding_literal: source_string_literal(MCAP_MESSAGE_ENCODING),
        mcap_metadata_name_literal: source_string_literal(MCAP_METADATA_NAME),
        mcap_schema_set_hash_key_literal: source_string_literal(MCAP_SCHEMA_SET_HASH_KEY),
        mcap_session_id_key_literal: source_string_literal(MCAP_SESSION_ID_KEY),
        mcap_source_key_literal: source_string_literal(MCAP_SOURCE_KEY),
        mcap_time_basis_key_literal: source_string_literal(MCAP_TIME_BASIS_KEY),
        mcap_time_basis_monotonic_boot_literal: source_string_literal(
            MCAP_TIME_BASIS_MONOTONIC_BOOT,
        ),
        mcap_time_basis_unix_epoch_literal: source_string_literal(MCAP_TIME_BASIS_UNIX_EPOCH),
        mcap_time_basis_correlated_literal: source_string_literal(MCAP_TIME_BASIS_CORRELATED),
        mcap_topic_id_key_literal: source_string_literal(MCAP_TOPIC_ID_KEY),
        cmd_key_prefix: CMD_KEY_PREFIX,
        meta_key_prefix: META_KEY_PREFIX,
        liveliness_key_prefix: LIVELINESS_KEY_PREFIX,
        cmd_key_prefix_literal: source_string_literal(CMD_KEY_PREFIX),
        meta_key_prefix_literal: source_string_literal(META_KEY_PREFIX),
        liveliness_key_prefix_literal: source_string_literal(LIVELINESS_KEY_PREFIX),
        mcap_schemas,
        topics: topics
            .iter()
            .map(|topic| {
                let payload_type_c = topic
                    .payload_type
                    .as_deref()
                    .map(source_string_literal)
                    .unwrap_or_else(|| "NULL".to_string());
                let payload_type_python = topic
                    .payload_type
                    .as_deref()
                    .map(source_string_literal)
                    .unwrap_or_else(|| "None".to_string());
                let payload_type_rust = topic
                    .payload_type
                    .as_deref()
                    .map(|value| format!("Some({})", source_string_literal(value)))
                    .unwrap_or_else(|| "None".to_string());
                let payload_size_c = topic
                    .payload_size
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "0".to_string());
                let payload_size_python = topic
                    .payload_size
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "None".to_string());
                let payload_size_rust = topic
                    .payload_size
                    .map(|value| format!("Some({value})"))
                    .unwrap_or_else(|| "None".to_string());
                let root_table_rust_path =
                    rust_module_path(&topic.root_table_namespace, &topic.root_table);
                let payload_type_rust_path = topic
                    .payload_type
                    .as_deref()
                    .zip(topic.payload_type_namespace.as_deref())
                    .map(|(payload, namespace)| rust_module_path(namespace, payload))
                    .unwrap_or_default();
                let root_table_qualified_literal = source_string_literal(&qualified_name(
                    &topic.root_table_namespace,
                    &topic.root_table,
                ));
                let mcap_schema_name =
                    qualified_name(&topic.root_table_namespace, &topic.root_table);
                let schema_stem = Path::new(&topic.schema_file)
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .expect("validated topic schema path must have a UTF-8 file stem");
                let mcap_schema_file = format!("bfbs/{schema_stem}.bfbs");
                let mcap_schema_symbol = format!("synapse_bfbs_{schema_stem}");
                let payload_type_qualified_literal = topic
                    .payload_type
                    .as_deref()
                    .zip(topic.payload_type_namespace.as_deref())
                    .map(|(payload, namespace)| {
                        source_string_literal(&qualified_name(namespace, payload))
                    })
                    .unwrap_or_default();

                TopicTemplateEntry {
                    id: topic.id,
                    name: topic.name.clone(),
                    key: topic.key.clone(),
                    root_table: topic.root_table.clone(),
                    payload_type: topic.payload_type.clone(),
                    payload_size: topic.payload_size,
                    schema_file: topic.schema_file.clone(),
                    mcap_schema_name: mcap_schema_name.clone(),
                    mcap_schema_file: mcap_schema_file.clone(),
                    mcap_schema_symbol,
                    wire_type: topic.wire_type.clone(),
                    schema_hash: topic.schema_hash.clone(),
                    fixed_layout: topic.fixed_layout,
                    multi_instance: topic.multi_instance,
                    scope: topic.scope,
                    encoding: topic.encoding,
                    description: topic.description.clone(),
                    name_literal: source_string_literal(&topic.name),
                    key_literal: source_string_literal(&topic.key),
                    root_table_literal: source_string_literal(&topic.root_table),
                    payload_type_c,
                    payload_type_python,
                    payload_type_rust,
                    payload_size_c,
                    payload_size_python,
                    payload_size_rust,
                    root_table_rust_path,
                    payload_type_rust_path,
                    root_table_qualified_literal,
                    payload_type_qualified_literal,
                    schema_file_literal: source_string_literal(&topic.schema_file),
                    mcap_schema_name_literal: source_string_literal(&mcap_schema_name),
                    mcap_schema_file_literal: source_string_literal(&mcap_schema_file),
                    wire_type_literal: source_string_literal(&topic.wire_type),
                    schema_hash_literal: source_string_literal(&topic.schema_hash),
                    fixed_layout_python: if topic.fixed_layout { "True" } else { "False" },
                    fixed_layout_c: if topic.fixed_layout { "true" } else { "false" },
                    multi_instance_python: if topic.multi_instance {
                        "True"
                    } else {
                        "False"
                    },
                    multi_instance_c: if topic.multi_instance {
                        "true"
                    } else {
                        "false"
                    },
                    scope_literal: source_string_literal(topic.scope),
                    encoding_literal: source_string_literal(topic.encoding),
                    description_literal: source_string_literal(&topic.description),
                }
            })
            .collect(),
        commands,
    }))
}

#[derive(Clone, Debug, Serialize)]
struct EmbeddedSchemaEntry {
    name_literal: String,
    file_literal: String,
    fbs_include_literal: String,
    bfbs_include_literal: String,
    root_type_rust: String,
    file_identifier_rust: String,
}

/// Context for the embedded-schemas module: one entry per file in SCHEMAS,
/// pairing the schema source and its compiled binary schema so the staged
/// crate can ship the wire contract instead of consumers vendoring copies.
fn embedded_schemas_context(docs: &SchemaDoc) -> Result<Value> {
    let mut schemas = Vec::new();
    for schema in SCHEMAS {
        let file = docs
            .files
            .iter()
            .find(|file| file.name == *schema)
            .ok_or_else(|| {
                io::Error::other(format!("schema {schema} is not part of the parsed docs"))
            })?;
        let stem = Path::new(schema)
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| io::Error::other(format!("schema path has no file stem: {schema}")))?;
        schemas.push(EmbeddedSchemaEntry {
            name_literal: source_string_literal(stem),
            file_literal: source_string_literal(schema),
            fbs_include_literal: source_string_literal(&format!("../{schema}")),
            bfbs_include_literal: source_string_literal(&format!("../bfbs/{stem}.bfbs")),
            root_type_rust: file
                .root_type
                .as_deref()
                .map(|value| format!("Some({})", source_string_literal(value)))
                .unwrap_or_else(|| "None".to_string()),
            file_identifier_rust: file
                .file_identifier
                .as_deref()
                .map(|value| format!("Some({})", source_string_literal(value)))
                .unwrap_or_else(|| "None".to_string()),
        });
    }
    Ok(Value::from_serialize(context! { schemas => schemas }))
}

fn write_rust_embedded_schemas(
    templates: &Templates,
    docs: &SchemaDoc,
    package_root: &Path,
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/schemas.rs.jinja",
        embedded_schemas_context(docs)?,
        &package_root.join("src/schemas.rs"),
    )
}

fn write_rust_topic_decode(
    templates: &Templates,
    docs: &SchemaDoc,
    topics: &[TopicEntry],
    package_root: &Path,
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/topic_decode.rs.jinja",
        topic_catalog_context(docs, topics)?,
        &package_root.join("src/topic_decode.rs"),
    )
}

fn write_rust_mcap_fixed(
    templates: &Templates,
    docs: &SchemaDoc,
    topics: &[TopicEntry],
    package_root: &Path,
) -> Result<()> {
    templates.render_to_file(
        "xtask/topic_catalog/mcap_fixed.rs.jinja",
        topic_catalog_context(docs, topics)?,
        &package_root.join("src/mcap_fixed.rs"),
    )
}

fn check_rust_package(package_root: &Path) -> Result<()> {
    println!("checking Rust crate");

    let tests_dir = package_root.join("tests");
    fs::create_dir_all(&tests_dir)?;
    write_file(
        &tests_dir.join("firmware_roundtrip.rs"),
        RUST_FIRMWARE_ROUNDTRIP_TEST,
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
        .arg(format!("{}[mcap]", wheel.display())))?;

    let mcap_path = venv.join("python-writer.mcap");

    let code = format!(
        r#"import importlib.metadata as metadata
from io import BytesIO
from mcap.writer import CompressionType, IndexType, Writer as ContainerWriter
from synapse import topic_catalog
from synapse.types.Vec3f import Vec3f
from synapse.topic.GnssFixData import GnssFixData
from synapse.cmd.ParamValue import ParamValue
from synapse.cmd.FirmwarePrepareRequest import FirmwarePrepareRequest
from synapse.topic.FirmwareProgress import FirmwareProgress
from synapse.mcap import Reader, TimeBasis, Writer
assert metadata.version("flatbuffers") == "{}"
assert metadata.version("mcap") == "{}"
assert Vec3f is not None and GnssFixData is not None and ParamValue is not None
assert FirmwarePrepareRequest is not None and FirmwareProgress is not None
assert topic_catalog.key_for_topic("VehicleHealth") == "health"
assert topic_catalog.topic_by_id(1).payload_type == "VehicleHealthData"
parsed = topic_catalog.parse_key("cub1/imu/0")
assert parsed is not None and parsed.namespace == "cub1"
assert parsed.topic.name == "InertialSample" and parsed.instance == 0
assert topic_catalog.topic_by_key("cub1/health").id == 1
assert topic_catalog.command_by_name("param_get").key == "cmd/param_get"
assert topic_catalog.command_by_name("firmware_prepare").key == "cmd/firmware_prepare"
assert topic_catalog.topic_by_key("fw").id == 34
assert topic_catalog.topic_by_name("GnssFix").payload_size is not None
assert topic_catalog.MCAP_PROFILE == "synapse/1"
assert topic_catalog.MCAP_SCHEMA_ENCODING == "flatbuffer"
assert topic_catalog.MCAP_MESSAGE_ENCODING == "flatbuffer"
assert topic_catalog.topic_by_name("Odometry").mcap_schema_name == "synapse.topic.Odometry"
assert topic_catalog.topic_by_name("Odometry").mcap_schema_file == "bfbs/state.bfbs"
topic = topic_catalog.topic_by_name("Odometry")
with open({}, "wb") as output:
    writer = Writer(output, "synapse-fbs-python-test/1", "0123456789abcdef0123456789abcdef", "test-python", TimeBasis.MONOTONIC_BOOT)
    channel = writer.add_topic(topic, "test/odom")
    writer.write_fixed(channel, 2000, 1000, bytes(topic.payload_size))
    writer.finish()
with open({}, "rb") as input_file:
    reader = Reader(input_file)
    messages = list(reader.messages(log_time_order=False))
    assert len(messages) == 1
    schema, channel, message = messages[0]
    assert schema.name == "synapse.topic.Odometry" and schema.encoding == "flatbuffer"
    assert channel.topic == "test/odom" and channel.message_encoding == "flatbuffer"
    assert message.log_time == 2000 and message.publish_time == 1000
historical = BytesIO()
container_writer = ContainerWriter(historical, compression=CompressionType.NONE, index_types=IndexType.NONE, repeat_channels=False, repeat_schemas=False, use_chunking=False, use_statistics=False, use_summary_offsets=False, enable_crcs=False)
container_writer.start(profile=topic_catalog.MCAP_PROFILE, library="historical-test")
container_writer.add_metadata(topic_catalog.MCAP_METADATA_NAME, {{
    topic_catalog.MCAP_SCHEMA_SET_HASH_KEY: "0" * 32,
    topic_catalog.MCAP_SESSION_ID_KEY: "fedcba9876543210fedcba9876543210",
    topic_catalog.MCAP_SOURCE_KEY: "historical-test",
    topic_catalog.MCAP_TIME_BASIS_KEY: topic_catalog.MCAP_TIME_BASIS_MONOTONIC_BOOT,
}})
container_writer.finish()
historical.seek(0)
Reader(historical)
"#,
        tools.flatbuffers_version,
        tools.mcap_python_version,
        source_string_literal(&mcap_path.display().to_string()),
        source_string_literal(&mcap_path.display().to_string()),
    );
    run(Command::new(&venv_python).arg("-c").arg(code))?;
    validate_mcap_with_rust(root, &mcap_path)?;
    remove_file_if_exists(&mcap_path)?;

    Ok(())
}

fn build_js_package(
    root: &Path,
    templates: &Templates,
    package_root: &Path,
    flatc: &Path,
    tools: &Tools,
    validate_cross_language: bool,
) -> Result<()> {
    println!("building JavaScript schema-assets package");

    remove_dir_if_exists(&package_root.join("fbs"))?;
    remove_dir_if_exists(&package_root.join("bfbs"))?;
    copy_common_archive_files(root, package_root)?;
    generate_reflection_schemas(root, flatc, &package_root.join("bfbs"))?;
    let docs = parse_schema_docs(root)?;
    validate_schema_docs(&docs)?;
    validate_protocol(&docs)?;
    let topics = topic_entries(&docs)?;
    write_js_topic_catalogs(templates, package_root, &docs, &topics)?;
    write_schema_hashes(root, &package_root.join("schema.sha256"))?;
    write_bfbs_hashes(package_root, &package_root.join("bfbs.sha256"))?;

    smoke_js_package(root, package_root, tools, validate_cross_language)?;

    Ok(())
}

fn smoke_js_package(
    root: &Path,
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
    let script = r#"import { fbsDir, bfbsDir, schemaFiles, schemaPath, keyForTopic, topicById, topicByKey, parseKey, commandByName, topicByName, mcapMetadataName, mcapSchemaSetHashKey, mcapSessionIdKey, mcapSourceKey, mcapTimeBasisKey } from './index.js';
import { container, Reader, TimeBasis, Writer } from './mcap.js';
import { existsSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
for (const name of schemaFiles) {
  if (!existsSync(schemaPath(name))) throw new Error('missing schema ' + name);
}
if (!existsSync(join(fbsDir, 'transport.fbs'))) throw new Error('missing fbsDir');
if (!existsSync(join(bfbsDir, 'transport.bfbs'))) throw new Error('missing reflection schema');
if (!existsSync(join(fbsDir, '..', 'topics.json'))) throw new Error('missing topic catalog');
if (keyForTopic('VehicleHealth') !== 'health') throw new Error('bad topic key helper');
if (topicById(1)?.payloadType !== 'VehicleHealthData') throw new Error('bad topic id helper');
const parsed = parseKey('cub1/imu/0');
if (!parsed || parsed.namespace !== 'cub1' || parsed.topic.name !== 'InertialSample' || parsed.instance !== 0) throw new Error('bad parseKey helper');
if (topicByKey('cub1/health')?.id !== 1) throw new Error('bad namespaced key lookup');
if (commandByName('param_get')?.key !== 'cmd/param_get') throw new Error('bad command helper');
if (commandByName('firmware_prepare')?.key !== 'cmd/firmware_prepare') throw new Error('bad firmware command helper');
if (topicByKey('fw')?.id !== 34) throw new Error('bad firmware progress helper');
class MemoryWritable {
  constructor() { this.parts = []; this.offset = 0n; }
  async write(data) { this.parts.push(new Uint8Array(data)); this.offset += BigInt(data.byteLength); }
  position() { return this.offset; }
  bytes() {
    const output = new Uint8Array(Number(this.offset));
    let offset = 0;
    for (const part of this.parts) { output.set(part, offset); offset += part.byteLength; }
    return output;
  }
}
const sink = new MemoryWritable();
const writer = new Writer({
  writable: sink,
  library: 'synapse-fbs-javascript-test/1',
  sessionId: '0123456789abcdef0123456789abcdef',
  source: 'test-javascript',
  timeBasis: TimeBasis.MONOTONIC_BOOT,
});
await writer.start();
const odometry = topicByName('Odometry');
const channel = await writer.addTopic(odometry, 'test/odom');
await writer.writeFixed(channel, 2000n, 1000n, new Uint8Array(odometry.payloadSize));
await writer.finish();
const bytes = sink.bytes();
writeFileSync(__MCAP_PATH__, bytes);
const reader = new Reader();
reader.append(bytes);
const records = [];
for (let record; (record = reader.nextRecord());) records.push(record);
reader.assertValid();
const schema = records.find((record) => record.type === 'Schema');
const message = records.find((record) => record.type === 'Message');
if (schema?.name !== 'synapse.topic.Odometry' || schema.encoding !== 'flatbuffer') throw new Error('bad MCAP schema');
if (message?.logTime !== 2000n || message.publishTime !== 1000n) throw new Error('bad MCAP message');
const historicalSink = new MemoryWritable();
const historicalWriter = new container.McapWriter({
  writable: historicalSink,
  useStatistics: false,
  useSummaryOffsets: false,
  useChunks: false,
  repeatSchemas: false,
  repeatChannels: false,
  useAttachmentIndex: false,
  useMetadataIndex: false,
  useMessageIndex: false,
  useChunkIndex: false,
});
await historicalWriter.start({ profile: 'synapse/1', library: 'historical-test' });
await historicalWriter.addMetadata({ name: mcapMetadataName, metadata: new Map([
  [mcapSchemaSetHashKey, '0'.repeat(32)],
  [mcapSessionIdKey, 'fedcba9876543210fedcba9876543210'],
  [mcapSourceKey, 'historical-test'],
  [mcapTimeBasisKey, TimeBasis.MONOTONIC_BOOT],
]) });
await historicalWriter.end();
const historicalReader = new Reader();
historicalReader.append(historicalSink.bytes());
while (historicalReader.nextRecord()) {}
historicalReader.assertValid();
console.log('synapse-fbs js package ok');
"#
    .replace(
        "__MCAP_PATH__",
        &source_string_literal(&mcap_path.display().to_string()),
    );

    run(Command::new(&node)
        .current_dir(package_root)
        .arg("--input-type=module")
        .arg("-e")
        .arg(script))?;

    let type_smoke = package_root.join("mcap-type-smoke.ts");
    write_file(
        &type_smoke,
        r#"import type { IWritable } from '@mcap/core';
import { Reader, TimeBasis, Writer, type TopicChannel } from './mcap.js';

declare const writable: IWritable;
const writer = new Writer({
  writable,
  library: 'type-test',
  sessionId: '0123456789abcdef0123456789abcdef',
  source: 'type-test',
  timeBasis: TimeBasis.MONOTONIC_BOOT,
});
const channel: Promise<TopicChannel> = writer.addTopic('Odometry', 'test/odom');
const reader = new Reader();
reader.append(new Uint8Array());
void channel;
"#,
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
) -> Result<()> {
    println!("building generated C and C++ archives");

    let artifacts = root.join("target/xtask/artifacts");
    let workdir = root.join("target/xtask/artifacts-work");
    reset_dir(&artifacts)?;
    reset_dir(&workdir)?;
    let templates = Templates::new(root)?;
    let docs = parse_schema_docs(root)?;
    validate_schema_docs(&docs)?;
    validate_protocol(&docs)?;
    let topics = topic_entries(&docs)?;

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
    generate_reflection_schemas(root, flatc, &cpp_root.join("bfbs"))?;

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
    write_c_topic_catalogs(&templates, &cpp_root, &docs, &topics)?;
    write_cpp_mcap_topics(&templates, &cpp_root, &docs, &topics)?;
    write_cpp_bfbs_assets(&cpp_root, &topics)?;
    write_schema_hashes(root, &cpp_root.join("schema.sha256"))?;
    write_bfbs_hashes(&cpp_root, &cpp_root.join("bfbs.sha256"))?;
    let cpp_mcap_bfbs_source_paths = files_with_extension(&cpp_root.join("src/bfbs"), "cpp")?
        .into_iter()
        .map(|source| {
            let name = source
                .file_name()
                .and_then(|value| value.to_str())
                .expect("generated BFBS source must have a UTF-8 name");
            format!("  \"${{_SYNAPSE_FBS_ROOT}}/src/bfbs/{name}\"")
        })
        .collect::<Vec<_>>()
        .join("\n");
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
            &cpp_mcap_bfbs_source_paths,
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
    write_c_bfbs_assets(&c_root, &topics)?;

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
    write_c_topic_catalogs(&templates, &c_root, &docs, &topics)?;
    write_c_mcap_topics(&templates, &c_root, &docs, &topics)?;
    write_c_topic_print(
        &templates,
        &docs,
        &topics,
        &c_root.join("include/synapse/topic_print.h"),
        &c_root.join("src/topic_print.c"),
    )?;
    write_schema_hashes(root, &c_root.join("schema.sha256"))?;
    write_bfbs_hashes(&c_root, &c_root.join("bfbs.sha256"))?;
    let runtime_source_paths = runtime_source_names(&c_root.join("src/flatcc-runtime"))?
        .into_iter()
        .map(|source| format!("  \"${{_SYNAPSE_FBS_ROOT}}/src/flatcc-runtime/{source}\""))
        .collect::<Vec<_>>()
        .join("\n");
    let mcap_bfbs_source_paths = files_with_extension(&c_root.join("src/bfbs"), "c")?
        .into_iter()
        .map(|source| {
            let name = source
                .file_name()
                .and_then(|value| value.to_str())
                .expect("generated BFBS source must have a UTF-8 name");
            format!("  \"${{_SYNAPSE_FBS_ROOT}}/src/bfbs/{name}\"")
        })
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
            &mcap_bfbs_source_paths,
        ),
    )?;
    write_tar_gz(
        &workdir,
        &artifacts,
        "synapse_fbs-c",
        "synapse_fbs-c.tar.gz",
    )?;

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
    fs::copy(root.join("MCAP.md"), archive_root.join("MCAP.md"))?;
    copy_dir_all(&root.join("fbs"), &archive_root.join("fbs"))?;
    Ok(())
}

fn write_c_bfbs_assets(package_root: &Path, topics: &[TopicEntry]) -> Result<()> {
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
        let mut source = String::from("#include <stddef.h>\n#include <stdint.h>\n\n");
        source.push_str(&format!("const uint8_t synapse_bfbs_{stem}[] = {{\n"));
        for chunk in bytes.chunks(12) {
            source.push_str("    ");
            for byte in chunk {
                source.push_str(&format!("0x{byte:02x}, "));
            }
            source.push('\n');
        }
        source.push_str("};\n");
        source.push_str(&format!(
            "const size_t synapse_bfbs_{stem}_size = sizeof(synapse_bfbs_{stem});\n"
        ));
        write_file(&output_dir.join(format!("{stem}.c")), &source)?;
    }
    Ok(())
}

fn write_cpp_bfbs_assets(package_root: &Path, topics: &[TopicEntry]) -> Result<()> {
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
        let mut source = String::from(
            "#include <synapse/mcap_topics.hpp>\n\nnamespace synapse {\nnamespace mcap {\n\n",
        );
        source.push_str(&format!(
            "extern const std::byte synapse_bfbs_{stem}[] = {{\n"
        ));
        for chunk in bytes.chunks(12) {
            source.push_str("  ");
            for byte in chunk {
                source.push_str(&format!("std::byte{{0x{byte:02x}}}, "));
            }
            source.push('\n');
        }
        source.push_str("};\n");
        source.push_str(&format!(
            "extern const std::size_t synapse_bfbs_{stem}_size = sizeof(synapse_bfbs_{stem});\n\n"
        ));
        source.push_str("}  // namespace mcap\n}  // namespace synapse\n");
        write_file(&output_dir.join(format!("{stem}.cpp")), &source)?;
    }
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
    root_type: Option<String>,
    file_identifier: Option<String>,
}

#[derive(Clone, Debug)]
struct SchemaEntityDoc {
    kind: SchemaEntityKind,
    name: String,
    namespace: String,
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

type EntityLinkMap = BTreeMap<String, String>;
type RootWrapperMap = BTreeMap<String, Vec<String>>;

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
#[derive(Clone, Debug)]
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
    schema_set_hash_literal: String,
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
    mcap_profile_literal: String,
    mcap_schema_encoding_literal: String,
    mcap_message_encoding_literal: String,
    mcap_metadata_name_literal: String,
    mcap_schema_set_hash_key_literal: String,
    mcap_session_id_key_literal: String,
    mcap_source_key_literal: String,
    mcap_time_basis_key_literal: String,
    mcap_time_basis_monotonic_boot_literal: String,
    mcap_time_basis_unix_epoch_literal: String,
    mcap_time_basis_correlated_literal: String,
    mcap_topic_id_key_literal: String,
    cmd_key_prefix: &'static str,
    meta_key_prefix: &'static str,
    liveliness_key_prefix: &'static str,
    cmd_key_prefix_literal: String,
    meta_key_prefix_literal: String,
    liveliness_key_prefix_literal: String,
    mcap_schemas: Vec<McapSchemaTemplateEntry>,
    topics: Vec<TopicTemplateEntry>,
    commands: Vec<CommandTemplateEntry>,
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
    name_literal: String,
    key_literal: String,
    root_table_literal: String,
    payload_type_c: String,
    payload_type_python: String,
    payload_type_rust: String,
    payload_size_c: String,
    payload_size_python: String,
    payload_size_rust: String,
    root_table_rust_path: String,
    payload_type_rust_path: String,
    root_table_qualified_literal: String,
    payload_type_qualified_literal: String,
    schema_file_literal: String,
    mcap_schema_name_literal: String,
    mcap_schema_file_literal: String,
    wire_type_literal: String,
    schema_hash_literal: String,
    fixed_layout_python: &'static str,
    fixed_layout_c: &'static str,
    multi_instance_python: &'static str,
    multi_instance_c: &'static str,
    scope_literal: String,
    encoding_literal: String,
    description_literal: String,
}

#[derive(Clone, Debug, Serialize)]
struct CommandTemplateEntry {
    id: u16,
    name: String,
    key: String,
    request_type: String,
    reply_type: String,
    request_schema_hash: String,
    reply_schema_hash: String,
    request_encoding: &'static str,
    request_size: Option<usize>,
    reply_encoding: &'static str,
    reply_size: Option<usize>,
    description: String,
    name_literal: String,
    key_literal: String,
    request_type_literal: String,
    reply_type_literal: String,
    request_schema_hash_literal: String,
    reply_schema_hash_literal: String,
    request_encoding_literal: String,
    request_size_c: String,
    request_size_python: String,
    request_size_rust: String,
    reply_encoding_literal: String,
    reply_size_c: String,
    reply_size_python: String,
    reply_size_rust: String,
    description_literal: String,
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

fn generate_docs_site(root: &Path, tools: &Tools, version: &str, out_dir: &Path) -> Result<()> {
    let version = normalize_docs_version(version);
    let version_dir_name = docs_dir_name(&version);
    let version_dir = out_dir.join(&version_dir_name);
    let book_dir = root
        .join("target/xtask/docs-mdbook")
        .join(&version_dir_name);

    println!(
        "generating schema docs for {version} into {}",
        version_dir.display()
    );

    ensure_mdbook(&tools.mdbook_version)?;
    let docs = parse_schema_docs(root)?;
    validate_schema_docs(&docs)?;
    validate_protocol(&docs)?;

    fs::create_dir_all(out_dir)?;
    remove_legacy_docs(out_dir)?;
    let versions = docs_versions(out_dir, &version_dir_name)?;
    reset_dir(&book_dir)?;
    write_mdbook_source(
        root,
        &docs,
        &version,
        &version_dir_name,
        &versions,
        &book_dir,
    )?;
    reset_dir(&version_dir)?;
    run(Command::new("mdbook")
        .arg("build")
        .arg(&book_dir)
        .arg("--dest-dir")
        .arg(&version_dir))?;
    copy_dir_all(&root.join("fbs"), &version_dir.join("fbs"))?;
    write_file(&out_dir.join(".nojekyll"), "")?;
    remove_file_if_exists(&out_dir.join("style.css"))?;
    write_docs_root_index(out_dir, &version_dir_name)?;
    refresh_docs_version_selectors(out_dir, &version_dir_name)?;
    write_docs_version_redirect_aliases(out_dir, &version_dir_name)?;

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
    let mut root_type = None;
    let mut file_identifier = None;
    let mut current: Option<SchemaEntityDoc> = None;
    let mut pending_comments = Vec::new();

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        if let Some(comment) = trimmed.strip_prefix("//") {
            pending_comments.push(comment.trim().to_string());
            continue;
        }

        let (code, trailing_comment) = trimmed
            .split_once("//")
            .map(|(before, comment)| (before.trim(), Some(comment.trim().to_string())))
            .unwrap_or((trimmed, None));
        if code.is_empty() {
            continue;
        }

        if let Some(entity) = current.as_mut() {
            if code.starts_with('}') {
                entities.push(current.take().expect("entity exists"));
                pending_comments.clear();
                continue;
            }

            if let Some(comment) = trailing_comment
                && !comment.is_empty()
            {
                pending_comments.push(comment);
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

        if let Some(rest) = code.strip_prefix("root_type ") {
            let declared = rest.trim_end_matches(';').trim();
            root_type = Some(if declared.contains('.') || namespace.is_empty() {
                declared.to_string()
            } else {
                format!("{namespace}.{declared}")
            });
            pending_comments.clear();
            continue;
        }

        if let Some(rest) = code.strip_prefix("file_identifier ") {
            file_identifier = Some(
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
                namespace: namespace.clone(),
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
        root_type,
        file_identifier,
    })
}

fn validate_schema_docs(docs: &SchemaDoc) -> Result<()> {
    validate_canonicalizable(docs)?;
    let mut missing = Vec::new();

    for file in &docs.files {
        for entity in &file.entities {
            if entity.comments.is_empty() {
                missing.push(format!(
                    "{}: missing comment for {} {}",
                    file.name,
                    entity.kind.as_str(),
                    entity.name
                ));
            }

            for member in &entity.members {
                if member.comments.is_empty() {
                    missing.push(format!(
                        "{}: missing comment for {} {} member {}",
                        file.name,
                        entity.kind.as_str(),
                        entity.name,
                        member.name
                    ));
                }
            }
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        fail(format!(
            "schema documentation is incomplete:\n{}",
            missing.join("\n")
        ))
    }
}

/// The wire-schema hash is only as strong as the canonical form's coverage,
/// so every schema construct must be one the canonicalizer models. The only
/// FlatBuffers attribute it models is `(bit_flags)` on enums (captured via
/// the enum value type); any other attribute could change wire layout or
/// semantics without changing the hash, and is rejected here.
fn validate_canonicalizable(docs: &SchemaDoc) -> Result<()> {
    let mut problems = Vec::new();
    let has_parens = |text: &str| text.contains('(') || text.contains(')');

    for file in &docs.files {
        for entity in &file.entities {
            if has_parens(&entity.name) {
                problems.push(format!(
                    "{}: {} declaration {} carries an attribute the schema canonicalizer does not model",
                    file.name,
                    entity.kind.as_str(),
                    entity.name
                ));
            }
            if let Some(value_type) = entity.value_type.as_deref()
                && has_parens(value_type)
                && !(entity.kind == SchemaEntityKind::Enum
                    && value_type.split_once('(').is_some_and(|(base, attribute)| {
                        !base.trim().is_empty() && attribute.trim() == "bit_flags)"
                    }))
            {
                problems.push(format!(
                    "{}: {} {} value type '{value_type}' carries an attribute the schema canonicalizer does not model (only enum (bit_flags) is supported)",
                    file.name,
                    entity.kind.as_str(),
                    entity.name
                ));
            }
            for member in &entity.members {
                let attributed = has_parens(&member.name)
                    || member.type_name.as_deref().is_some_and(has_parens)
                    || member.value.as_deref().is_some_and(has_parens);
                if attributed {
                    problems.push(format!(
                        "{}: {} {} member {} carries an attribute the schema canonicalizer does not model",
                        file.name,
                        entity.kind.as_str(),
                        entity.name,
                        member.name
                    ));
                }
            }
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        fail(format!(
            "schema uses constructs outside the hashed wire contract; extend wire_schema_hash before allowing them:\n{}",
            problems.join("\n")
        ))
    }
}

/// Check TOPIC_KEYS is a valid, exhaustive map for the TopicId enum: one
/// well-formed, unique, non-reserved key per topic and no stale entries.
fn validate_topic_keys(topic_enum: &SchemaEntityDoc) -> Result<()> {
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

fn topic_entries(docs: &SchemaDoc) -> Result<Vec<TopicEntry>> {
    let topic_enum = docs
        .files
        .iter()
        .flat_map(|file| &file.entities)
        .find(|entity| entity.kind == SchemaEntityKind::Enum && entity.name == "TopicId")
        .ok_or_else(|| io::Error::other("TopicId enum not found"))?;
    validate_topic_keys(topic_enum)?;

    let mut topics = Vec::new();
    let mut layouts = BTreeMap::new();
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
            find_schema_entity(docs, &member.name).ok_or_else(|| {
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
            .and_then(|payload| find_schema_entity(docs, payload));
        let payload_type_namespace = payload_entity.map(|(_, entity)| entity.namespace.clone());
        let fixed_layout =
            payload_entity.is_some_and(|(_, entity)| entity.kind == SchemaEntityKind::Struct);
        let payload_size = if fixed_layout {
            let payload = payload_type.as_deref().expect("payload type exists");
            let mut visiting = BTreeSet::new();
            Some(struct_layout(docs, payload, &mut layouts, &mut visiting)?.0)
        } else {
            None
        };
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
        let schema_hash = wire_schema_hash(docs, wire_entity)?;
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
            description: comments_text(&member.comments),
        });
    }

    Ok(topics)
}

/// Hash the exact transitive FlatBuffers contract reachable from one wire
/// type. Comments, source files, and unrelated declarations are deliberately
/// excluded so documentation and unrelated message changes do not invalidate
/// compatible values.
fn wire_schema_hash(docs: &SchemaDoc, root: &SchemaEntityDoc) -> Result<String> {
    let mut reachable = BTreeMap::<String, &SchemaEntityDoc>::new();
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        let qualified = qualified_name(&entity.namespace, &entity.name);
        if reachable.insert(qualified, entity).is_some() {
            continue;
        }
        if let Some(value_type) = entity.value_type.as_deref()
            && let Some(referenced) = resolve_schema_type(docs, &entity.namespace, value_type)
        {
            pending.push(referenced);
        }
        for member in &entity.members {
            if let Some(type_name) = member.type_name.as_deref()
                && let Some(referenced) = resolve_schema_type(docs, &entity.namespace, type_name)
            {
                pending.push(referenced);
            }
        }
    }

    let mut canonical = String::from("synapse-flatbuffers-wire-schema-v1\n");
    for (qualified, entity) in reachable {
        canonical.push_str("entity\t");
        canonical.push_str(entity.kind.as_str());
        canonical.push('\t');
        canonical.push_str(&qualified);
        canonical.push('\t');
        canonical.push_str(entity.value_type.as_deref().unwrap_or(""));
        canonical.push('\n');
        for member in &entity.members {
            canonical.push_str("member\t");
            canonical.push_str(&member.name);
            canonical.push('\t');
            canonical.push_str(member.type_name.as_deref().unwrap_or(""));
            canonical.push('\t');
            if let Some(type_name) = member.type_name.as_deref()
                && let Some(target) = resolve_schema_type(docs, &entity.namespace, type_name)
            {
                canonical.push_str(&qualified_name(&target.namespace, &target.name));
            }
            canonical.push('\t');
            canonical.push_str(member.value.as_deref().unwrap_or(""));
            canonical.push('\n');
        }
    }
    let digest = Sha256::digest(canonical.as_bytes());
    Ok(digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn resolve_schema_type<'a>(
    docs: &'a SchemaDoc,
    namespace: &str,
    type_name: &str,
) -> Option<&'a SchemaEntityDoc> {
    let atom = type_name
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(':')
        .next()
        .unwrap_or(type_name)
        .trim();
    if scalar_layout(atom).is_some() || matches!(atom, "string") {
        return None;
    }
    let qualified = if atom.contains('.') || namespace.is_empty() {
        atom.to_string()
    } else {
        format!("{namespace}.{atom}")
    };
    docs.files
        .iter()
        .flat_map(|file| &file.entities)
        .find(|entity| qualified_name(&entity.namespace, &entity.name) == qualified)
        .or_else(|| {
            docs.files
                .iter()
                .flat_map(|file| &file.entities)
                .find(|entity| entity.name == type_lookup_name(atom))
        })
}

fn command_payload_metadata(
    docs: &SchemaDoc,
    type_name: &str,
    layouts: &mut BTreeMap<String, (usize, usize)>,
) -> Result<CommandPayloadMetadata> {
    let lookup = type_lookup_name(type_name);
    let Some((_, entity)) = find_schema_entity(docs, &lookup) else {
        return fail(format!(
            "command type {type_name} does not resolve to a schema entity"
        ));
    };
    match entity.kind {
        SchemaEntityKind::Struct => {
            let mut visiting = BTreeSet::new();
            let size = struct_layout(docs, &lookup, layouts, &mut visiting)?.0;
            Ok(CommandPayloadMetadata {
                encoding: "struct",
                size: Some(size),
            })
        }
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

/// Resolve the COMMANDS table against the parsed schemas, computing each
/// request and reply type's payload metadata and transitive schema hash.
fn command_entries(docs: &SchemaDoc) -> Result<Vec<CommandEntry>> {
    let mut layouts = BTreeMap::new();
    let mut commands = Vec::new();
    for (id, name, request_type, reply_type, description) in COMMANDS {
        let request_meta = command_payload_metadata(docs, request_type, &mut layouts)?;
        let reply_meta = command_payload_metadata(docs, reply_type, &mut layouts)?;
        commands.push(CommandEntry {
            id: *id,
            name: (*name).to_string(),
            key: format!("{CMD_KEY_PREFIX}/{name}"),
            request_type: (*request_type).to_string(),
            request_schema_hash: command_schema_hash(docs, request_type)?,
            request_encoding: request_meta.encoding,
            request_size: request_meta.size,
            reply_type: (*reply_type).to_string(),
            reply_schema_hash: command_schema_hash(docs, reply_type)?,
            reply_encoding: reply_meta.encoding,
            reply_size: reply_meta.size,
            description: (*description).to_string(),
        });
    }
    Ok(commands)
}

fn command_schema_hash(docs: &SchemaDoc, type_name: &str) -> Result<String> {
    let Some((_, entity)) = find_schema_entity(docs, &type_lookup_name(type_name)) else {
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
    wire_schema_hash(docs, entity)
}

fn round_up(value: usize, align: usize) -> usize {
    value.div_ceil(align) * align
}

fn scalar_layout(type_name: &str) -> Option<(usize, usize)> {
    let size = match type_name {
        "bool" | "byte" | "int8" | "ubyte" | "uint8" => 1,
        "short" | "int16" | "ushort" | "uint16" => 2,
        "int" | "int32" | "uint" | "uint32" | "float" | "float32" => 4,
        "long" | "int64" | "ulong" | "uint64" | "double" | "float64" => 8,
        _ => return None,
    };
    Some((size, size))
}

fn enum_base_type(value_type: &str) -> &str {
    value_type.split_whitespace().next().unwrap_or(value_type)
}

/// Size and alignment of a schema struct or enum per FlatBuffers layout
/// rules: fields in declaration order, each aligned to its natural
/// alignment, total size padded to the struct alignment.
fn struct_layout(
    docs: &SchemaDoc,
    name: &str,
    layouts: &mut BTreeMap<String, (usize, usize)>,
    visiting: &mut BTreeSet<String>,
) -> Result<(usize, usize)> {
    let lookup = type_lookup_name(name);
    if let Some(layout) = layouts.get(&lookup) {
        return Ok(*layout);
    }
    if let Some(layout) = scalar_layout(&lookup) {
        return Ok(layout);
    }
    if !visiting.insert(lookup.clone()) {
        return fail(format!("cyclic struct definition involving {lookup}"));
    }

    let Some((_, entity)) = find_schema_entity(docs, &lookup) else {
        visiting.remove(&lookup);
        return fail(format!(
            "cannot compute layout for unknown fixed-layout type {lookup}"
        ));
    };
    let layout = match entity.kind {
        SchemaEntityKind::Enum => {
            let base = entity
                .value_type
                .as_deref()
                .map(enum_base_type)
                .unwrap_or_default();
            match scalar_layout(base) {
                Some(layout) => layout,
                None => {
                    visiting.remove(&lookup);
                    return fail(format!("enum {lookup} has unsupported base type '{base}'"));
                }
            }
        }
        SchemaEntityKind::Struct => {
            let mut offset = 0usize;
            let mut align = 1usize;
            for member in &entity.members {
                let Some(type_name) = member.type_name.as_deref() else {
                    visiting.remove(&lookup);
                    return fail(format!(
                        "struct {lookup} member {} is missing a type",
                        member.name
                    ));
                };
                let (size, member_align) = struct_layout(docs, type_name, layouts, visiting)?;
                offset = round_up(offset, member_align) + size;
                align = align.max(member_align);
            }
            (round_up(offset, align), align)
        }
        _ => {
            visiting.remove(&lookup);
            return fail(format!(
                "{lookup} is a {}, not a fixed-layout struct or enum",
                entity.kind.as_str()
            ));
        }
    };

    visiting.remove(&lookup);
    layouts.insert(lookup, layout);
    Ok(layout)
}

#[derive(Clone, Debug, Serialize)]
struct FieldDescTemplateEntry {
    name_literal: String,
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

/// Flatten a fixed-layout payload into scalar field descriptors. Nested
/// struct members become dotted names ("attitude.w"); enum members use
/// their base scalar kind. Offsets follow the same FlatBuffers layout
/// rules as struct_layout.
fn collect_field_descs(
    docs: &SchemaDoc,
    type_name: &str,
    prefix: &str,
    base_offset: usize,
    layouts: &mut BTreeMap<String, (usize, usize)>,
    out: &mut Vec<FieldDescTemplateEntry>,
) -> Result<()> {
    let lookup = type_lookup_name(type_name);
    if let Some(kind) = scalar_field_kind(&lookup) {
        out.push(FieldDescTemplateEntry {
            name_literal: source_string_literal(prefix),
            offset: base_offset,
            kind,
        });
        return Ok(());
    }

    let Some((_, entity)) = find_schema_entity(docs, &lookup) else {
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
                name_literal: source_string_literal(prefix),
                offset: base_offset,
                kind,
            });
        }
        SchemaEntityKind::Struct => {
            let mut member_offset = 0usize;
            for member in &entity.members {
                let Some(member_type) = member.type_name.as_deref() else {
                    return fail(format!(
                        "struct {lookup} member {} is missing a type",
                        member.name
                    ));
                };
                let mut visiting = BTreeSet::new();
                let (size, align) = struct_layout(docs, member_type, layouts, &mut visiting)?;
                member_offset = round_up(member_offset, align);
                let name = if prefix.is_empty() {
                    member.name.clone()
                } else {
                    format!("{prefix}.{}", member.name)
                };
                collect_field_descs(
                    docs,
                    member_type,
                    &name,
                    base_offset + member_offset,
                    layouts,
                    out,
                )?;
                member_offset += size;
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

fn topic_print_context(docs: &SchemaDoc, topics: &[TopicEntry]) -> Result<Value> {
    let mut layouts = BTreeMap::new();
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
        collect_field_descs(docs, payload, "", 0, &mut layouts, &mut fields)?;
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
    docs: &SchemaDoc,
    topics: &[TopicEntry],
    header_path: &Path,
    source_path: &Path,
) -> Result<()> {
    let context = topic_print_context(docs, topics)?;
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

/// Protocol-level consistency checks beyond per-entity documentation:
/// TopicId contiguity, TopicId/union agreement, command type resolution, and
/// the unit-suffix lint for quantitative fields.
fn validate_protocol(docs: &SchemaDoc) -> Result<()> {
    let mut problems = Vec::new();

    let topic_enum = docs
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

    match docs
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
            if find_schema_entity(docs, type_name).is_none() {
                problems.push(format!(
                    "command {name} references unknown type {type_name}"
                ));
            }
        }
    }

    // CmdId in fbs/transfer.fbs must mirror the COMMANDS table.
    match docs
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
                .map(|member| {
                    (
                        member.value.clone().unwrap_or_default(),
                        snake_case(&member.name),
                    )
                })
                .collect::<Vec<_>>();
            let command_entries = COMMANDS
                .iter()
                .map(|(id, name, _, _, _)| (id.to_string(), (*name).to_string()))
                .collect::<Vec<_>>();
            if enum_entries != command_entries {
                problems.push(format!(
                    "CmdId enum does not mirror the xtask COMMANDS table.\n  CmdId:    {enum_entries:?}\n  COMMANDS: {command_entries:?}"
                ));
            }
        }
        None => problems.push("CmdId enum not found in fbs/transfer.fbs".to_string()),
    }

    let mut enum_names = BTreeSet::new();
    for file in &docs.files {
        for entity in &file.entities {
            if entity.kind == SchemaEntityKind::Enum {
                enum_names.insert(entity.name.clone());
            }
        }
    }

    for file in &docs.files {
        for entity in &file.entities {
            if entity.namespace == "synapse.types" {
                continue;
            }
            if !matches!(
                entity.kind,
                SchemaEntityKind::Struct | SchemaEntityKind::Table
            ) {
                continue;
            }
            for member in &entity.members {
                let Some(type_name) = member.type_name.as_deref() else {
                    continue;
                };
                if type_name == "bool" {
                    continue;
                }
                let lookup = type_lookup_name(type_name);
                if scalar_layout(&lookup).is_none() || enum_names.contains(&lookup) {
                    continue;
                }
                if unit_scale_note(&member.name).is_some() || lint_allowlisted(&member.name) {
                    continue;
                }
                problems.push(format!(
                    "{}: {} {}.{} has no unit suffix; add one or extend the lint allowlist",
                    file.name,
                    entity.kind.as_str(),
                    entity.name,
                    member.name
                ));
            }
        }
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

/// Quantitative-looking field names that intentionally carry no unit suffix.
fn lint_allowlisted(name: &str) -> bool {
    const EXACT: &[&str] = &[
        "id",
        "buttons",
        "total",
        "residual",
        "color",
        "thrust",
        "flight_mode",
        "vehicle_type",
        "system_state",
        "mission_mode",
        "source",
        "port",
        "estimator_type",
        "float_value",
        "int_value",
        "errors_comm",
        "active_axes",
        "satellites_used",
        "satellites_visible",
        "result_detail",
        "target_system",
        "target_component",
        "seq",
        "sequence",
        "offset",
        "limit",
        "sensors_present",
        "sensors_enabled",
        "sensors_health",
        "sensors_present_ext",
        "sensors_enabled_ext",
        "sensors_health_ext",
        "max_image_size",
        "max_chunk_size",
        "received_bytes",
        "total_bytes",
        "received_chunks",
        "total_chunks",
        "image_size",
        "requested_chunk_size",
        "accepted_chunk_size",
        "chunk_index",
        "data",
        "signature",
    ];
    if EXACT.contains(&name) {
        return true;
    }
    if name.ends_with("_id")
        || name.ends_with("_count")
        || name.ends_with("_counter")
        || name.ends_with("_seq")
        || name.ends_with("_number")
        || name.ends_with("_version")
        || name.ends_with("_crc32")
    {
        return true;
    }
    for prefix in ["arg", "control", "output"] {
        if let Some(rest) = name.strip_prefix(prefix)
            && !rest.is_empty()
            && rest.bytes().all(|byte| byte.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

fn find_schema_entity<'a>(
    docs: &'a SchemaDoc,
    name: &str,
) -> Option<(&'a SchemaFileDoc, &'a SchemaEntityDoc)> {
    let lookup = type_lookup_name(name);
    docs.files.iter().find_map(|file| {
        file.entities
            .iter()
            .find(|entity| entity.name == lookup)
            .map(|entity| (file, entity))
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
    } else if lower.ends_with("_cv") {
        "centi-volts; volts = value / 100"
    } else if lower.ends_with("_ca") {
        "centi-amps; amps = value / 100"
    } else if lower.ends_with("_da") {
        "deci-amps; amps = value / 10"
    } else if lower.ends_with("_dam") {
        "decameters; meters = value * 10"
    } else if lower.ends_with("_mah") {
        "milliamp-hours"
    } else if lower.ends_with("_hj") {
        "hecto-joules; joules = value * 100"
    } else if lower.ends_with("_ratio") {
        "dimensionless ratio"
    } else if lower.ends_with("_pct") {
        "percent"
    } else if lower.ends_with("_hpa") {
        "hectopascals"
    } else if lower.ends_with("_tesla") {
        "tesla"
    } else if lower.ends_with("_deg") {
        "degrees"
    } else if lower.ends_with("_c") {
        "degrees Celsius"
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

#[derive(Clone, Debug)]
struct DocVersion {
    dir: String,
    label: String,
    current: bool,
}

fn ensure_mdbook(expected_version: &str) -> Result<()> {
    let actual = match output(Command::new("mdbook").arg("--version")) {
        Ok(value) => value,
        Err(err) => {
            return fail(format!(
                "mdbook {expected_version} is required to generate docs. Install it with: cargo install mdbook --version {expected_version} --locked\n{err}"
            ));
        }
    };
    let expected = format!("mdbook v{expected_version}");
    if actual.trim() == expected {
        Ok(())
    } else {
        fail(format!(
            "unexpected mdbook version '{}', expected '{}'",
            actual.trim(),
            expected
        ))
    }
}

fn docs_versions(out_dir: &Path, current_version_dir: &str) -> Result<Vec<DocVersion>> {
    let mut dirs = BTreeSet::new();
    dirs.insert(current_version_dir.to_string());

    if out_dir.is_dir() {
        for entry in fs::read_dir(out_dir)? {
            let path = entry?.path();
            if path.is_dir()
                && path.join("index.html").is_file()
                && let Some(name) = path.file_name().and_then(|name| name.to_str())
            {
                dirs.insert(name.to_string());
            }
        }
    }

    let mut dirs = dirs.into_iter().collect::<Vec<_>>();
    dirs.sort_by(|left, right| compare_doc_version_dirs(left, right));
    Ok(dirs
        .into_iter()
        .map(|dir| DocVersion {
            current: dir == current_version_dir,
            label: doc_version_label(&dir),
            dir,
        })
        .collect())
}

fn compare_doc_version_dirs(left: &str, right: &str) -> Ordering {
    match (left == "main", right == "main") {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }

    match (doc_version_key(left), doc_version_key(right)) {
        (Some(left_key), Some(right_key)) => right_key.cmp(&left_key).then_with(|| right.cmp(left)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => right.cmp(left),
    }
}

fn doc_version_key(value: &str) -> Option<Vec<u64>> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }

    let mut key = Vec::new();
    for part in parts {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        key.push(part.parse().ok()?);
    }
    Some(key)
}

fn doc_version_label(dir: &str) -> String {
    if dir == "main" {
        "main (development)".to_string()
    } else {
        dir.to_string()
    }
}

fn remove_legacy_docs(out_dir: &Path) -> Result<()> {
    for dir in LEGACY_DOC_DIRS {
        remove_dir_if_exists(&out_dir.join(dir))?;
    }
    Ok(())
}

fn write_mdbook_source(
    root: &Path,
    docs: &SchemaDoc,
    version: &str,
    version_dir_name: &str,
    versions: &[DocVersion],
    book_dir: &Path,
) -> Result<()> {
    let src_dir = book_dir.join("src");
    let entity_links = entity_link_map(docs);
    let root_wrappers = root_wrapper_map(docs);
    write_file(&book_dir.join("book.toml"), &render_book_toml(version))?;
    write_file(&book_dir.join("theme/synapse.css"), MDBOOK_CSS)?;
    write_file(
        &book_dir.join("theme/version-selector.js"),
        &render_version_selector_js(versions, version_dir_name),
    )?;
    write_file(&src_dir.join("SUMMARY.md"), &render_book_summary(docs))?;
    write_file(
        &src_dir.join("index.md"),
        &render_book_index(docs, version, version_dir_name),
    )?;
    copy_dir_all(&root.join("fbs"), &src_dir.join("fbs"))?;

    for file in &docs.files {
        let file_slug = schema_file_slug(file);
        let file_page = src_dir.join("schemas").join(format!("{file_slug}.md"));
        write_file(
            &file_page,
            &render_schema_file_page(docs, file, &entity_links),
        )?;

        for entity in &file.entities {
            let entity_page = src_dir
                .join("schemas")
                .join(&file_slug)
                .join(format!("{}.md", entity_slug(entity)));
            write_file(
                &entity_page,
                &render_entity_page(file, entity, &entity_links, &root_wrappers),
            )?;
        }
    }

    Ok(())
}

fn render_book_toml(version: &str) -> String {
    format!(
        r#"[book]
title = "Synapse FlatBuffers {version}"
description = "Versioned FlatBuffers message schema documentation for Synapse."
src = "src"

[output.html]
default-theme = "rust"
preferred-dark-theme = "navy"
git-repository-url = "https://github.com/CogniPilot/synapse_fbs"
additional-css = ["theme/synapse.css"]
additional-js = ["theme/version-selector.js"]
"#,
        version = escape_toml_basic(version)
    )
}

fn render_book_summary(docs: &SchemaDoc) -> String {
    let mut md = String::new();
    md.push_str("# Summary\n\n");
    md.push_str("[Introduction](index.md)\n\n");
    md.push_str("# Schemas\n\n");

    for file in &docs.files {
        let file_slug = schema_file_slug(file);
        md.push_str(&format!(
            "- [{}](schemas/{file_slug}.md)\n",
            markdown_link_text(&file.name)
        ));
        for entity in &file.entities {
            if thin_root_wrapper_payload(entity).is_some() {
                continue;
            }
            md.push_str(&format!(
                "  - [`{}` {}](schemas/{file_slug}/{}.md)\n",
                entity.kind.as_str(),
                markdown_link_text(&entity.name),
                entity_slug(entity)
            ));
        }
    }

    md
}

fn render_book_index(docs: &SchemaDoc, version: &str, version_dir_name: &str) -> String {
    let mut md = String::new();
    md.push_str(&format!(
        "# Synapse FlatBuffers `{}`\n\n",
        markdown_text(version)
    ));
    md.push_str("Generated from the FlatBuffers schemas in `fbs/`. Fixed memory layout is the default for runtime protocol payloads so chip-to-chip shared-memory transports can use the payload layout directly. Coordinate frames follow [ROS REP-0103](https://www.ros.org/reps/rep-0103.html): local/world vectors use ENU and body vectors use FLU.\n\n");
    md.push_str("## Motivation\n\n");
    md.push_str("Synapse messages are designed for vehicles that exchange state, sensor, and control data in real time. The schema should be efficient enough for shared-memory message passing between chips, compact enough for constrained over-the-air links, and still straightforward to use from ordinary application code.\n\n");
    md.push_str("- **Fixed memory layout first:** runtime payloads prefer FlatBuffers `struct` definitions so producers and consumers can share predictable native layouts without allocation when the transport and language allow it.\n");
    md.push_str("- **ROS-compatible conventions:** frames and units follow ROS REP-0103 by default: ENU for local/world vectors, FLU for body vectors, and SI units unless a field name explicitly declares a scaled integer representation.\n");
    md.push_str("- **Bit-conscious transport:** scaled integer fields preserve needed fidelity without spending unnecessary bytes. Airborne systems pay for latency, range, and reliability, so schemas should avoid waste on high-rate or over-the-air messages.\n");
    md.push_str("- **Native web and cross-platform use:** release artifacts for npm, Python, Rust, C, and C++ let browser tools, cloud services, embedded firmware, and developer scripts consume the same schema source.\n\n");
    md.push_str("## Transport Boundaries\n\n");
    md.push_str("Most deployments should publish typed topic payloads directly over transports such as Zenoh, UDP, or TCP and rely on those transports or links for framing, integrity checks, and optional security. The optional `Frame` envelope exists for links that need an explicit Synapse byte-stream container, especially serial-style transports where message delimiting, sequence tracking, and future opt-in integrity or authentication metadata belong at the frame boundary.\n\n");
    md.push_str("Checksums, authentication tags, or encryption should not be hardcoded into every topic payload. When needed, they should be transport-envelope features so fixed-layout payloads remain compact, inspectable, and reusable across shared memory, local middleware, native web tooling, and constrained radio links.\n\n");
    md.push_str("## Zenoh Use\n\n");
    md.push_str("Synapse is intended to be straightforward to use with Zenoh. The canonical encoding publishes each fixed-layout topic's bare payload struct bytes on a stable key expression: the key identifies the stream and the type, so the same bytes serve shared memory, Zenoh values, radio frames, and log messages with zero re-serialization. Little-endian byte order is a protocol requirement. Variable-size topics and generic bridges use the thin FlatBuffers root tables instead; the catalog `encoding` field records which applies.\n\n");
    md.push_str("Several parts of the schema support this model:\n\n");
    md.push_str("- **Fixed-layout payload structs:** every runtime topic payload is a struct with a documented byte size, so consumers can decode by overlay without FlatBuffers machinery.\n");
    md.push_str("- **Generated topic catalog:** release artifacts include `TopicId`, canonical Zenoh key, root table, payload struct and size, scope, encoding, command request/reply encoding, and helper lookups so applications do not hand-maintain routing tables.\n");
    md.push_str("- **Stable topic identifiers:** `TopicId` is available for bridges, logs, serial frames, or compact routing tables, while Zenoh deployments can use key expressions as the primary discriminator.\n");
    md.push_str("- **No transport checksums in payloads:** Zenoh, UDP, TCP, and link layers can provide their own integrity behavior, so Synapse payloads stay portable across middleware and shared memory.\n");
    md.push_str("- **Schema assets in every release:** npm, Python, Rust, C, and C++ artifacts carry generated bindings or schema assets so Zenoh tools, web dashboards, firmware bridges, and scripts can decode the same messages.\n\n");
    md.push_str("Canonical keys use `[<namespace>/]<key>[/<instance>]` with short curated topic keys, for example `cub1/odom`, `qualisys/cub1/external_pose`, or `cub1/imu/0`. Multi-instance sensor topics append an instance segment so subscribers can select one sensor without decoding payloads. Deployments prepend vehicle, swarm, or site namespaces from configuration, never hardcoded in firmware; the mandatory per-value contract metadata, not the key, is the compatibility signal. The package helpers parse namespaced keys and look up topics by `TopicId`, name, or key. Commands and transfers are Zenoh queryables under the reserved `cmd/` segment; `meta/` and `live/` are reserved for schema metadata and liveliness.\n\n");
    md.push_str("Lockstep simulation is modeled as pub/sub state, not query/reply RPC. A simulator publishes `LockstepTick` on `tick`, and each participant publishes `LockstepStatus` on `tick_status/<id>`. Strict lockstep waits for a matching `run_id` and completed status sequence before sending the next tick. Use liveliness tokens under `live/...` for endpoint presence; the status topic is the protocol acknowledgement, not discovery.\n\n");
    md.push_str("## Topic Catalog\n\n");
    md.push_str("The generated topic catalog is included as `topics.json` in schema-asset archives and as language helpers where the package has a public API. It records `TopicId`, canonical key expression, FlatBuffers root table, fixed-layout payload type and size, schema file, topic encoding, command request/reply encoding, and descriptions from schema comments.\n\n");
    md.push_str("Use the catalog when writing Zenoh publishers/subscribers, serial frame routers, log readers, gateways, and ROS bridge nodes. That keeps topic routing synchronized with the schema instead of duplicating key strings and numeric IDs in application code.\n\n");
    md.push_str("## ROS and FlatROS\n\n");
    md.push_str("ROS messages are local integration types, not the Synapse over-the-air format. They are useful for visualization, autonomy stacks, simulation, rosbag tooling, and operator workflows, but they should not replace compact Synapse FlatBuffers payloads on constrained vehicle links.\n\n");
    md.push_str("ROS 2 integration should happen at the edge through bridge nodes that translate selected Synapse topics into ROS concepts only where ROS tooling needs them. The planned flatros2 path is a generated ROS workspace or release archive that consumes the Synapse schemas and topic catalog, depends on `flatros2`, and provides adapter nodes without making ROS message definitions the protocol source of truth.\n\n");
    md.push_str("## Layout Rules\n\n");
    md.push_str("Telemetry, state, command, and control samples should use FlatBuffers `struct` definitions. Use `table`, `string`, or vector fields only for thin root wrappers, transport unions, log records, metadata, text, or naturally variable-size data.\n\n");
    md.push_str("## Unit And Scale Rules\n\n");
    md.push_str("Fields encode units and frames in their names. Local/world vectors use `_enu_`; body vectors use `_flu_`. Global coordinates use `_deg_e7`, altitudes use `_mm`, speeds commonly use `_cm_s` or `_mm_s`, temperatures use `_cdeg` or `_c`, currents use `_da`, pack voltages use `_cv`, magnetic field uses `_tesla`, and normalized manual-control axes use `_milli`. The scale column in each entity page is generated from those suffixes, and schema validation fails when a quantitative field has no recognized suffix.\n\n");
    md.push_str("## Schema Files\n\n");
    for file in &docs.files {
        md.push_str(&format!(
            "- [{}](schemas/{}.md)\n",
            markdown_link_text(&file.name),
            schema_file_slug(file)
        ));
    }
    md.push('\n');
    md.push_str(&format!(
        "Version path: `{}`. Generated by `cargo run --locked --manifest-path xtask/Cargo.toml -- docs`.\n",
        markdown_text(version_dir_name)
    ));
    md
}

fn render_schema_file_page(
    docs: &SchemaDoc,
    file: &SchemaFileDoc,
    entity_links: &EntityLinkMap,
) -> String {
    let mut md = String::new();
    let current_page = schema_file_page_path(file);
    md.push_str(&format!("# `{}`\n\n", markdown_text(&file.name)));
    if !file.namespace.is_empty() {
        md.push_str(&format!(
            "**Namespace:** `{}`\n\n",
            markdown_text(&file.namespace)
        ));
    }
    if !file.includes.is_empty() {
        md.push_str("**Includes:** ");
        for (index, include) in file.includes.iter().enumerate() {
            if index > 0 {
                md.push_str(", ");
            }
            md.push_str(&render_include_ref(include, docs, &current_page));
        }
        md.push_str("\n\n");
    }
    md.push_str(&format!(
        "[Source schema](../fbs/{})\n\n",
        source_file_name(file)
    ));

    if file.entities.is_empty() {
        md.push_str("This schema file does not define public entities.\n");
        return md;
    }

    md.push_str("| Kind | Name | Description |\n");
    md.push_str("| --- | --- | --- |\n");
    for entity in &file.entities {
        md.push_str(&format!(
            "| `{}` | {} | {} |\n",
            entity.kind.as_str(),
            markdown_table_cell(&render_entity_name_ref(entity, entity_links, &current_page)),
            markdown_table_cell(&comments_text(&entity.comments))
        ));
    }
    md
}

fn render_entity_page(
    file: &SchemaFileDoc,
    entity: &SchemaEntityDoc,
    entity_links: &EntityLinkMap,
    root_wrappers: &RootWrapperMap,
) -> String {
    let mut md = String::new();
    let current_page = entity_page_path(file, entity);
    md.push_str(&format!(
        "# `{}` {}\n\n",
        entity.kind.as_str(),
        markdown_text(&entity.name)
    ));
    md.push_str(&format!(
        "[{}](../{}.md) / [Source schema](../../fbs/{})\n\n",
        markdown_link_text(&file.name),
        schema_file_slug(file),
        source_file_name(file)
    ));
    if let Some(value_type) = &entity.value_type {
        md.push_str(&format!(
            "**Backing type:** `{}`\n\n",
            markdown_text(value_type)
        ));
    }
    md.push_str(&format!("{}\n\n", comments_text(&entity.comments)));
    if let Some(payload) = thin_root_wrapper_payload(entity) {
        md.push_str(&format!(
            "**Payload:** {}\n\n",
            render_type_ref(payload, entity_links, &current_page)
        ));
    } else if let Some(wrappers) = root_wrappers.get(&entity.name)
        && !wrappers.is_empty()
    {
        let links = wrappers
            .iter()
            .map(|wrapper| code_span(wrapper))
            .collect::<Vec<_>>()
            .join(", ");
        md.push_str(&format!("**FlatBuffers root table:** {links}\n\n"));
    }
    render_member_table_md(&mut md, entity, entity_links, &current_page);
    md
}

fn render_member_table_md(
    md: &mut String,
    entity: &SchemaEntityDoc,
    entity_links: &EntityLinkMap,
    current_page: &str,
) {
    let has_value = entity.members.iter().any(|member| member.value.is_some());
    match entity.kind {
        SchemaEntityKind::Struct | SchemaEntityKind::Table => {
            md.push_str("| Name | Type |");
            if has_value {
                md.push_str(" Default |");
            }
            md.push_str(" Unit / Scale | Notes |\n");
            md.push_str("| --- | --- |");
            if has_value {
                md.push_str(" --- |");
            }
            md.push_str(" --- | --- |\n");
            for member in &entity.members {
                md.push_str(&format!(
                    "| {} | {} |",
                    markdown_table_cell(&code_span(&member.name)),
                    markdown_table_cell(
                        &member
                            .type_name
                            .as_ref()
                            .map(|value| render_type_ref(value, entity_links, current_page))
                            .unwrap_or_default()
                    )
                ));
                if has_value {
                    md.push_str(&format!(
                        " {} |",
                        markdown_table_cell(
                            &member
                                .value
                                .as_ref()
                                .map(|value| code_span(value))
                                .unwrap_or_default()
                        )
                    ));
                }
                md.push_str(&format!(
                    " {} | {} |\n",
                    markdown_table_cell(member.unit_scale.as_deref().unwrap_or_default()),
                    markdown_table_cell(&comments_text(&member.comments))
                ));
            }
        }
        SchemaEntityKind::Enum => {
            md.push_str("| Name | Value | Notes |\n");
            md.push_str("| --- | --- | --- |\n");
            for member in &entity.members {
                md.push_str(&format!(
                    "| {} | {} | {} |\n",
                    markdown_table_cell(&code_span(&member.name)),
                    markdown_table_cell(
                        &member
                            .value
                            .as_ref()
                            .map(|value| code_span(value))
                            .unwrap_or_default()
                    ),
                    markdown_table_cell(&comments_text(&member.comments))
                ));
            }
        }
        SchemaEntityKind::Union => {
            md.push_str("| Name | Type | Notes |\n");
            md.push_str("| --- | --- | --- |\n");
            for member in &entity.members {
                md.push_str(&format!(
                    "| {} | {} | {} |\n",
                    markdown_table_cell(&render_type_ref(&member.name, entity_links, current_page)),
                    markdown_table_cell(
                        &member
                            .type_name
                            .as_ref()
                            .map(|value| render_type_ref(value, entity_links, current_page))
                            .unwrap_or_default()
                    ),
                    markdown_table_cell(&comments_text(&member.comments))
                ));
            }
        }
    }
}

fn entity_link_map(docs: &SchemaDoc) -> EntityLinkMap {
    let mut direct_links = BTreeMap::new();
    for file in &docs.files {
        for entity in &file.entities {
            let path = entity_page_path(file, entity);
            direct_links.insert(entity.name.clone(), path.clone());
            if !file.namespace.is_empty() {
                direct_links.insert(format!("{}.{}", file.namespace, entity.name), path);
            }
        }
    }

    let mut links = BTreeMap::new();
    for file in &docs.files {
        for entity in &file.entities {
            let path = thin_root_wrapper_payload(entity)
                .and_then(|payload| direct_links.get(&type_lookup_name(payload)))
                .cloned()
                .unwrap_or_else(|| entity_page_path(file, entity));
            links.insert(entity.name.clone(), path.clone());
            if !file.namespace.is_empty() {
                links.insert(format!("{}.{}", file.namespace, entity.name), path);
            }
        }
    }
    links
}

fn root_wrapper_map(docs: &SchemaDoc) -> RootWrapperMap {
    let mut wrappers = BTreeMap::new();
    for file in &docs.files {
        for entity in &file.entities {
            if let Some(payload) = thin_root_wrapper_payload(entity) {
                wrappers
                    .entry(type_lookup_name(payload))
                    .or_insert_with(Vec::new)
                    .push(entity.name.clone());
            }
        }
    }
    wrappers
}

fn render_entity_name_ref(
    entity: &SchemaEntityDoc,
    entity_links: &EntityLinkMap,
    current_page: &str,
) -> String {
    if let Some(payload) = thin_root_wrapper_payload(entity) {
        return format!(
            "{} -> {}",
            code_span(&entity.name),
            render_type_ref(payload, entity_links, current_page)
        );
    }

    render_type_ref(&entity.name, entity_links, current_page)
}

fn thin_root_wrapper_payload(entity: &SchemaEntityDoc) -> Option<&str> {
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

fn render_include_ref(include: &str, docs: &SchemaDoc, current_page: &str) -> String {
    let Some(target) = docs.files.iter().find(|file| {
        source_file_name(file) == include
            || file.name == include
            || file.name == format!("fbs/{include}")
    }) else {
        return code_span(include);
    };

    format!(
        "[{}]({})",
        code_span(include),
        relative_md_link(current_page, &schema_file_page_path(target))
    )
}

fn render_type_ref(type_name: &str, entity_links: &EntityLinkMap, current_page: &str) -> String {
    let type_name = type_name.trim();
    if type_name.is_empty() {
        return String::new();
    }

    if let Some(inner) = type_name
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        let inner = inner.trim();
        if type_link_target(inner, entity_links).is_some() {
            return format!(
                "<code>[</code>{}<code>]</code>",
                render_type_atom(inner, entity_links, current_page)
            );
        }
        return code_span(type_name);
    }

    render_type_atom(type_name, entity_links, current_page)
}

fn render_type_atom(type_name: &str, entity_links: &EntityLinkMap, current_page: &str) -> String {
    let Some(target) = type_link_target(type_name, entity_links) else {
        return code_span(type_name);
    };

    format!(
        "[{}]({})",
        code_span(type_name),
        relative_md_link(current_page, target)
    )
}

fn type_link_target<'a>(type_name: &str, entity_links: &'a EntityLinkMap) -> Option<&'a String> {
    entity_links
        .get(type_name)
        .or_else(|| entity_links.get(&type_lookup_name(type_name)))
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

fn schema_file_page_path(file: &SchemaFileDoc) -> String {
    format!("schemas/{}.md", schema_file_slug(file))
}

fn entity_page_path(file: &SchemaFileDoc, entity: &SchemaEntityDoc) -> String {
    format!(
        "schemas/{}/{}.md",
        schema_file_slug(file),
        entity_slug(entity)
    )
}

fn relative_md_link(from_file: &str, to_file: &str) -> String {
    let from_dir = from_file
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .unwrap_or_default();
    let from_parts = split_relative_path(from_dir);
    let to_parts = split_relative_path(to_file);
    let mut common = 0;
    while common < from_parts.len()
        && common < to_parts.len()
        && from_parts[common] == to_parts[common]
    {
        common += 1;
    }

    let mut parts = Vec::new();
    for _ in common..from_parts.len() {
        parts.push("..".to_string());
    }
    for part in &to_parts[common..] {
        parts.push((*part).to_string());
    }

    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

fn split_relative_path(path: &str) -> Vec<&str> {
    path.split('/').filter(|part| !part.is_empty()).collect()
}

fn render_version_selector_js(versions: &[DocVersion], current_version_dir: &str) -> String {
    let mut js = String::new();
    js.push_str("(function () {\n");
    js.push_str("  const versions = [\n");
    for version in versions {
        js.push_str("    { dir: ");
        js.push_str(&js_string(&version.dir));
        js.push_str(", label: ");
        js.push_str(&js_string(&version.label));
        js.push_str(" },\n");
    }
    js.push_str("  ];\n");
    js.push_str("  const current = ");
    js.push_str(&js_string(current_version_dir));
    js.push_str(";\n");
    js.push_str(
        r#"  function docsBaseUrl() {
    const script = document.currentScript || document.querySelector('script[src*="version-selector"]');
    if (!script) {
      return new URL('../', window.location.href);
    }
    const scriptUrl = new URL(script.getAttribute('src'), window.location.href);
    return new URL('../../', scriptUrl);
  }

  function targetUrl(dir) {
    return new URL(dir.replace(/\/+$/, '') + '/', docsBaseUrl()).href;
  }

  function buildSelect() {
    const select = document.createElement('select');
    select.className = 'synapse-version-select';
    select.setAttribute('aria-label', 'Schema documentation version');
    for (const version of versions) {
      const option = document.createElement('option');
      option.value = version.dir;
      option.textContent = version.label;
      option.selected = version.dir === current;
      select.appendChild(option);
    }
    select.addEventListener('change', () => {
      window.location.href = targetUrl(select.value);
    });
    return select;
  }

  function mountMenu() {
    const menu = document.getElementById('mdbook-menu-bar');
    if (!menu || menu.querySelector('.synapse-version-menu')) {
      return;
    }
    const target = menu.querySelector('.right-buttons') || menu;
    const wrapper = document.createElement('div');
    wrapper.className = 'synapse-version-menu';
    const label = document.createElement('label');
    label.textContent = 'Docs';
    wrapper.appendChild(label);
    wrapper.appendChild(buildSelect());
    target.insertBefore(wrapper, target.firstChild);
  }

  function mount() {
    mountMenu();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', mount);
  } else {
    mount();
  }
})();
"#,
    );
    js
}

fn write_docs_root_index(out_dir: &Path, current_version_dir: &str) -> Result<()> {
    let versions = docs_versions(out_dir, current_version_dir)?;
    let redirect_dir = versions
        .iter()
        .find(|version| version.dir == "main")
        .or_else(|| versions.iter().find(|version| version.current))
        .map(|version| version.dir.as_str())
        .unwrap_or(current_version_dir);
    let redirect_href = format!("{redirect_dir}/");

    let mut html = String::new();
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str(&format!(
        "<meta http-equiv=\"refresh\" content=\"0; url={}\">",
        escape_attr(&redirect_href)
    ));
    html.push_str(&format!(
        "<link rel=\"canonical\" href=\"{}\">",
        escape_attr(&redirect_href)
    ));
    html.push_str("<title>Synapse FlatBuffers docs</title><style>");
    html.push_str(ROOT_DOCS_CSS);
    html.push_str("</style></head><body><main><section class=\"panel\"><p class=\"eyebrow\">synapse_fbs</p><h1>Synapse FlatBuffers</h1>");
    html.push_str(&format!(
        "<p>Redirecting to <a href=\"{}\">{}</a>.</p>",
        escape_attr(&redirect_href),
        escape_html(&redirect_href)
    ));
    html.push_str("</section></main><script>");
    html.push_str("window.location.replace(");
    html.push_str(&js_string(&redirect_href));
    html.push_str(");");
    html.push_str("</script></body></html>");
    write_file(&out_dir.join("index.html"), &html)
}

fn refresh_docs_version_selectors(out_dir: &Path, current_version_dir: &str) -> Result<()> {
    let versions = docs_versions(out_dir, current_version_dir)?;
    for version in &versions {
        let theme_dir = out_dir.join(&version.dir).join("theme");
        if !theme_dir.is_dir() {
            continue;
        }
        let js = render_version_selector_js(&versions, &version.dir);
        for entry in fs::read_dir(&theme_dir)? {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.starts_with("version-selector") && name.ends_with(".js") {
                write_file(&path, &js)?;
            }
        }
    }
    Ok(())
}

fn write_docs_version_redirect_aliases(out_dir: &Path, current_version_dir: &str) -> Result<()> {
    let versions = docs_versions(out_dir, current_version_dir)?;
    for source in &versions {
        for target in &versions {
            if source.dir == target.dir {
                continue;
            }
            let href = format!("../../{}/", target.dir);
            let html = render_redirect_page(&href);
            write_file(
                &out_dir
                    .join(&source.dir)
                    .join(&target.dir)
                    .join("index.html"),
                &html,
            )?;
        }
    }
    Ok(())
}

fn render_redirect_page(href: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta http-equiv=\"refresh\" content=\"0; url={href_attr}\"><link rel=\"canonical\" href=\"{href_attr}\"><title>Redirecting</title></head><body><p>Redirecting to <a href=\"{href_attr}\">{href_text}</a>.</p><script>window.location.replace({href_js});</script></body></html>",
        href_attr = escape_attr(href),
        href_text = escape_html(href),
        href_js = js_string(href)
    )
}

fn schema_file_slug(file: &SchemaFileDoc) -> String {
    docs_dir_name(
        file.name
            .strip_prefix("fbs/")
            .unwrap_or(&file.name)
            .trim_end_matches(".fbs"),
    )
}

fn entity_slug(entity: &SchemaEntityDoc) -> String {
    docs_dir_name(&entity.name)
}

fn source_file_name(file: &SchemaFileDoc) -> &str {
    file.name.strip_prefix("fbs/").unwrap_or(&file.name)
}

fn comments_text(comments: &[String]) -> String {
    comments.join(" ")
}

fn code_span(value: &str) -> String {
    format!("`{}`", value.replace('`', "\\`"))
}

fn markdown_link_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn markdown_text(value: &str) -> String {
    value.replace('\\', "\\\\").replace('`', "\\`")
}

fn markdown_table_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', " ")
        .trim()
        .to_string()
}

fn escape_toml_basic(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn js_string(value: &str) -> String {
    let mut escaped = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '<' => escaped.push_str("\\u003c"),
            '>' => escaped.push_str("\\u003e"),
            '&' => escaped.push_str("\\u0026"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

fn source_string_literal(value: &str) -> String {
    let mut escaped = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\0"),
            ch if ch.is_control() => escaped.push_str(&format!("\\x{:02x}", ch as u32)),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

fn snake_case(value: &str) -> String {
    let mut result = String::new();
    let mut previous_was_separator = true;

    for (index, ch) in value.chars().enumerate() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase()
                && index > 0
                && !previous_was_separator
                && value
                    .chars()
                    .nth(index + 1)
                    .is_some_and(|next| next.is_ascii_lowercase())
            {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator {
            result.push('_');
            previous_was_separator = true;
        }
    }

    result.trim_matches('_').to_string()
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

const MDBOOK_CSS: &str = r#".synapse-version-menu {
  align-items: center;
  display: flex;
  gap: 0.4rem;
  height: var(--menu-bar-height);
  padding: 0 0.55rem;
}

.synapse-version-menu label {
  color: var(--fg);
  font-size: 0.75rem;
  font-weight: 600;
}

.synapse-version-select {
  background: var(--bg);
  border: 1px solid var(--table-border-color);
  border-radius: 4px;
  color: var(--fg);
  font: inherit;
  height: 1.9rem;
  max-width: 15rem;
  min-width: 8.5rem;
  padding: 0 0.45rem;
}

.content table {
  font-size: 0.9em;
}

.content table code {
  white-space: nowrap;
}

@media (max-width: 700px) {
  .synapse-version-menu {
    gap: 0.25rem;
    padding: 0 0.25rem;
  }

  .synapse-version-menu label {
    display: none;
  }

  .synapse-version-select {
    max-width: 8.5rem;
    min-width: 7rem;
  }
}
"#;

const ROOT_DOCS_CSS: &str = r#":root {
  color-scheme: light;
  --bg: #f5f7fa;
  --panel: #ffffff;
  --text: #27313f;
  --muted: #647184;
  --border: #d7dee8;
  --accent: #0f766e;
  --code: #eef2f7;
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
  max-width: 760px;
  margin: 0 auto;
  padding: 48px 20px 64px;
}

.panel {
  margin-top: 20px;
  padding: 24px;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 6px;
}

.eyebrow {
  margin: 0 0 8px;
  color: var(--muted);
  font-size: 0.8rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

h1,
h2 {
  line-height: 1.2;
}

h1 {
  margin: 0;
}

a,
.current {
  color: var(--accent);
}

.version-picker {
  align-items: center;
  display: flex;
  flex-wrap: wrap;
  gap: 0.6rem;
  margin-top: 1.2rem;
  font-weight: 600;
}

select {
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text);
  font: inherit;
  min-width: 14rem;
  padding: 0.4rem 0.5rem;
}

ul {
  padding-left: 1.2rem;
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
