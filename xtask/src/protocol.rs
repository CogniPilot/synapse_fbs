const SCHEMAS: &[&str] = &[
    "fbs/types.fbs",
    "fbs/sensors.fbs",
    "fbs/inertial_batch.fbs",
    "fbs/rtcm3.fbs",
    "fbs/state.fbs",
    "fbs/control.fbs",
    "fbs/optical_flow.fbs",
    "fbs/argus.fbs",
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
    ("MagneticField", "mag"),
    ("ArgusPointCloud", "argus"),
    ("Rtcm3", "rtcm3"),
    ("InertialBatch", "imu_batch"),
    ("ActuatorOutputs", "act_out"),
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
    "InertialBatch",
    "AirData",
    "OpticalFlow",
    "OpticalFlowVelocity",
    "ArgusPointCloud",
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
    "ActuatorOutputs",
    "ControlLoopMetrics",
    "LockstepTick",
    "LockstepStatus",
];
