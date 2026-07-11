// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="index.html">Introduction</a></span></li><li class="chapter-item expanded "><li class="part-title">Schemas</li></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/all.html"><strong aria-hidden="true">1.</strong> fbs/all.fbs</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control.html"><strong aria-hidden="true">2.</strong> fbs/control.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/ManualControlAxes.html"><strong aria-hidden="true">2.1.</strong> enum ManualControlAxes</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/ManualControlFlags.html"><strong aria-hidden="true">2.2.</strong> enum ManualControlFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/AttitudeCommandMask.html"><strong aria-hidden="true">2.3.</strong> enum AttitudeCommandMask</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/RateCommandMask.html"><strong aria-hidden="true">2.4.</strong> enum RateCommandMask</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/LocalPositionCommandMask.html"><strong aria-hidden="true">2.5.</strong> enum LocalPositionCommandMask</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/RadioControlData.html"><strong aria-hidden="true">2.6.</strong> struct RadioControlData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/ManualControlData.html"><strong aria-hidden="true">2.7.</strong> struct ManualControlData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/AttitudeCommandData.html"><strong aria-hidden="true">2.8.</strong> struct AttitudeCommandData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/RateCommandData.html"><strong aria-hidden="true">2.9.</strong> struct RateCommandData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/LocalPositionCommandData.html"><strong aria-hidden="true">2.10.</strong> struct LocalPositionCommandData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/ActuatorCommandData.html"><strong aria-hidden="true">2.11.</strong> struct ActuatorCommandData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/ActuatorFeedbackData.html"><strong aria-hidden="true">2.12.</strong> struct ActuatorFeedbackData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/PwmSignalOutputsData.html"><strong aria-hidden="true">2.13.</strong> struct PwmSignalOutputsData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/control/ControlLoopMetricsData.html"><strong aria-hidden="true">2.14.</strong> struct ControlLoopMetricsData</a></span></li></ol><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware.html"><strong aria-hidden="true">3.</strong> fbs/firmware.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareUpdateState.html"><strong aria-hidden="true">3.1.</strong> enum FirmwareUpdateState</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareMaintenanceFlags.html"><strong aria-hidden="true">3.2.</strong> enum FirmwareMaintenanceFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareInfoRequest.html"><strong aria-hidden="true">3.3.</strong> table FirmwareInfoRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareInfoReply.html"><strong aria-hidden="true">3.4.</strong> table FirmwareInfoReply</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareStatusRequest.html"><strong aria-hidden="true">3.5.</strong> table FirmwareStatusRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareStatusReply.html"><strong aria-hidden="true">3.6.</strong> table FirmwareStatusReply</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwarePrepareRequest.html"><strong aria-hidden="true">3.7.</strong> table FirmwarePrepareRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwarePrepareReply.html"><strong aria-hidden="true">3.8.</strong> table FirmwarePrepareReply</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareChunkRequest.html"><strong aria-hidden="true">3.9.</strong> table FirmwareChunkRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareChunkReply.html"><strong aria-hidden="true">3.10.</strong> table FirmwareChunkReply</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareCommitRequest.html"><strong aria-hidden="true">3.11.</strong> table FirmwareCommitRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareCommitReply.html"><strong aria-hidden="true">3.12.</strong> table FirmwareCommitReply</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareAbortRequest.html"><strong aria-hidden="true">3.13.</strong> table FirmwareAbortRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareAbortReply.html"><strong aria-hidden="true">3.14.</strong> table FirmwareAbortReply</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/firmware/FirmwareProgress.html"><strong aria-hidden="true">3.15.</strong> table FirmwareProgress</a></span></li></ol><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/mocap.html"><strong aria-hidden="true">4.</strong> fbs/mocap.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/mocap/MocapRawComponent.html"><strong aria-hidden="true">4.1.</strong> enum MocapRawComponent</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/mocap/MocapRawFlags.html"><strong aria-hidden="true">4.2.</strong> enum MocapRawFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/mocap/MocapMarkerData.html"><strong aria-hidden="true">4.3.</strong> struct MocapMarkerData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/mocap/MocapRigidBodyData.html"><strong aria-hidden="true">4.4.</strong> struct MocapRigidBodyData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/mocap/MocapRigidBodyPoseData.html"><strong aria-hidden="true">4.5.</strong> struct MocapRigidBodyPoseData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/mocap/MocapFrame.html"><strong aria-hidden="true">4.6.</strong> table MocapFrame</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/mocap/MocapPoseFrame.html"><strong aria-hidden="true">4.7.</strong> table MocapPoseFrame</a></span></li></ol><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/optical_flow.html"><strong aria-hidden="true">5.</strong> fbs/optical_flow.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/optical_flow/OpticalFlowData.html"><strong aria-hidden="true">5.1.</strong> struct OpticalFlowData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/optical_flow/OpticalFlowVelocityData.html"><strong aria-hidden="true">5.2.</strong> struct OpticalFlowVelocityData</a></span></li></ol><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sensors.html"><strong aria-hidden="true">6.</strong> fbs/sensors.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sensors/InertialFieldFlags.html"><strong aria-hidden="true">6.1.</strong> enum InertialFieldFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sensors/AirDataFlags.html"><strong aria-hidden="true">6.2.</strong> enum AirDataFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sensors/GnssFixFlags.html"><strong aria-hidden="true">6.3.</strong> enum GnssFixFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sensors/PowerFaultFlags.html"><strong aria-hidden="true">6.4.</strong> enum PowerFaultFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sensors/InertialSampleData.html"><strong aria-hidden="true">6.5.</strong> struct InertialSampleData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sensors/AirDataData.html"><strong aria-hidden="true">6.6.</strong> struct AirDataData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sensors/BatteryCellVoltages16.html"><strong aria-hidden="true">6.7.</strong> struct BatteryCellVoltages16</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sensors/PowerStatusData.html"><strong aria-hidden="true">6.8.</strong> struct PowerStatusData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sensors/GnssFixData.html"><strong aria-hidden="true">6.9.</strong> struct GnssFixData</a></span></li></ol><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sim.html"><strong aria-hidden="true">7.</strong> fbs/sim.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sim/LockstepTickFlags.html"><strong aria-hidden="true">7.1.</strong> enum LockstepTickFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sim/LockstepStatusState.html"><strong aria-hidden="true">7.2.</strong> enum LockstepStatusState</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sim/LockstepTickData.html"><strong aria-hidden="true">7.3.</strong> struct LockstepTickData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/sim/LockstepStatusData.html"><strong aria-hidden="true">7.4.</strong> struct LockstepStatusData</a></span></li></ol><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state.html"><strong aria-hidden="true">8.</strong> fbs/state.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/SensorComponentFlags.html"><strong aria-hidden="true">8.1.</strong> enum SensorComponentFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/VehicleHealthFlags.html"><strong aria-hidden="true">8.2.</strong> enum VehicleHealthFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/AttitudeEstimateFlags.html"><strong aria-hidden="true">8.3.</strong> enum AttitudeEstimateFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/LocalPositionFlags.html"><strong aria-hidden="true">8.4.</strong> enum LocalPositionFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/ExternalOdometryFlags.html"><strong aria-hidden="true">8.5.</strong> enum ExternalOdometryFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/ExternalOdometryStatus.html"><strong aria-hidden="true">8.6.</strong> enum ExternalOdometryStatus</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/OdometryFlags.html"><strong aria-hidden="true">8.7.</strong> enum OdometryFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/OdometryStatus.html"><strong aria-hidden="true">8.8.</strong> enum OdometryStatus</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/GlobalPositionFlags.html"><strong aria-hidden="true">8.9.</strong> enum GlobalPositionFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/HomeReferenceFlags.html"><strong aria-hidden="true">8.10.</strong> enum HomeReferenceFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/VehicleHealthData.html"><strong aria-hidden="true">8.11.</strong> struct VehicleHealthData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/TimeReferenceData.html"><strong aria-hidden="true">8.12.</strong> struct TimeReferenceData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/AttitudeEstimateData.html"><strong aria-hidden="true">8.13.</strong> struct AttitudeEstimateData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/LocalPositionEstimateData.html"><strong aria-hidden="true">8.14.</strong> struct LocalPositionEstimateData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/GlobalPositionEstimateData.html"><strong aria-hidden="true">8.15.</strong> struct GlobalPositionEstimateData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/OdometryEstimateData.html"><strong aria-hidden="true">8.16.</strong> struct OdometryEstimateData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/RawPoseData.html"><strong aria-hidden="true">8.17.</strong> struct RawPoseData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/PoseData.html"><strong aria-hidden="true">8.18.</strong> struct PoseData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/PoseWithCovarianceData.html"><strong aria-hidden="true">8.19.</strong> struct PoseWithCovarianceData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/TwistData.html"><strong aria-hidden="true">8.20.</strong> struct TwistData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/TwistWithCovarianceData.html"><strong aria-hidden="true">8.21.</strong> struct TwistWithCovarianceData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/OdometryData.html"><strong aria-hidden="true">8.22.</strong> struct OdometryData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/OdometryWithCovarianceData.html"><strong aria-hidden="true">8.23.</strong> struct OdometryWithCovarianceData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/ExternalOdometryData.html"><strong aria-hidden="true">8.24.</strong> struct ExternalOdometryData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/ExternalOdometryCovarianceData.html"><strong aria-hidden="true">8.25.</strong> struct ExternalOdometryCovarianceData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/EstimatorHealthData.html"><strong aria-hidden="true">8.26.</strong> struct EstimatorHealthData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/MissionProgressData.html"><strong aria-hidden="true">8.27.</strong> struct MissionProgressData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/NavigationTargetData.html"><strong aria-hidden="true">8.28.</strong> struct NavigationTargetData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/state/HomeReferenceData.html"><strong aria-hidden="true">8.29.</strong> struct HomeReferenceData</a></span></li></ol><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/telemetry.html"><strong aria-hidden="true">9.</strong> fbs/telemetry.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/telemetry/GcsStatusFlags.html"><strong aria-hidden="true">9.1.</strong> enum GcsStatusFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/telemetry/GcsStatusData.html"><strong aria-hidden="true">9.2.</strong> struct GcsStatusData</a></span></li></ol><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/trajectory.html"><strong aria-hidden="true">10.</strong> fbs/trajectory.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/trajectory/TrajectoryType.html"><strong aria-hidden="true">10.1.</strong> enum TrajectoryType</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/trajectory/TrajectoryDegree.html"><strong aria-hidden="true">10.2.</strong> enum TrajectoryDegree</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/trajectory/TrajectorySegmentFlags.html"><strong aria-hidden="true">10.3.</strong> enum TrajectorySegmentFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/trajectory/TrajectorySegmentData.html"><strong aria-hidden="true">10.4.</strong> struct TrajectorySegmentData</a></span></li></ol><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer.html"><strong aria-hidden="true">11.</strong> fbs/transfer.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/CmdId.html"><strong aria-hidden="true">11.1.</strong> enum CmdId</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/ParamKind.html"><strong aria-hidden="true">11.2.</strong> enum ParamKind</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/TransferFlags.html"><strong aria-hidden="true">11.3.</strong> enum TransferFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/MissionItemKind.html"><strong aria-hidden="true">11.4.</strong> enum MissionItemKind</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/MissionItemFlags.html"><strong aria-hidden="true">11.5.</strong> enum MissionItemFlags</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/ParamValue.html"><strong aria-hidden="true">11.6.</strong> table ParamValue</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/ParamGetRequest.html"><strong aria-hidden="true">11.7.</strong> table ParamGetRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/ParamGetReply.html"><strong aria-hidden="true">11.8.</strong> table ParamGetReply</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/ParamSetRequest.html"><strong aria-hidden="true">11.9.</strong> table ParamSetRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/ParamSetReply.html"><strong aria-hidden="true">11.10.</strong> table ParamSetReply</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/MissionItemData.html"><strong aria-hidden="true">11.11.</strong> struct MissionItemData</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/MissionGetRequest.html"><strong aria-hidden="true">11.12.</strong> table MissionGetRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/MissionGetReply.html"><strong aria-hidden="true">11.13.</strong> table MissionGetReply</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/MissionSetRequest.html"><strong aria-hidden="true">11.14.</strong> table MissionSetRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/MissionSetReply.html"><strong aria-hidden="true">11.15.</strong> table MissionSetReply</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/TrajectoryGetRequest.html"><strong aria-hidden="true">11.16.</strong> table TrajectoryGetRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/TrajectoryGetReply.html"><strong aria-hidden="true">11.17.</strong> table TrajectoryGetReply</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/TrajectorySetRequest.html"><strong aria-hidden="true">11.18.</strong> table TrajectorySetRequest</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transfer/TrajectorySetReply.html"><strong aria-hidden="true">11.19.</strong> table TrajectorySetReply</a></span></li></ol><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transport.html"><strong aria-hidden="true">12.</strong> fbs/transport.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transport/TextStatus.html"><strong aria-hidden="true">12.1.</strong> table TextStatus</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transport/SynapseMessage.html"><strong aria-hidden="true">12.2.</strong> union SynapseMessage</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transport/FrameHeader.html"><strong aria-hidden="true">12.3.</strong> struct FrameHeader</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/transport/Frame.html"><strong aria-hidden="true">12.4.</strong> table Frame</a></span></li></ol><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types.html"><strong aria-hidden="true">13.</strong> fbs/types.fbs</a></span><ol class="section"><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/Vec2f.html"><strong aria-hidden="true">13.1.</strong> struct Vec2f</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/Vec3f.html"><strong aria-hidden="true">13.2.</strong> struct Vec3f</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/Quaternionf.html"><strong aria-hidden="true">13.3.</strong> struct Quaternionf</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/RotationMatrix3f.html"><strong aria-hidden="true">13.4.</strong> struct RotationMatrix3f</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/RateTriplet.html"><strong aria-hidden="true">13.5.</strong> struct RateTriplet</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/Posef.html"><strong aria-hidden="true">13.6.</strong> struct Posef</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/Twistf.html"><strong aria-hidden="true">13.7.</strong> struct Twistf</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/CovarianceUpperTriangle21f.html"><strong aria-hidden="true">13.8.</strong> struct CovarianceUpperTriangle21f</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/CovarianceUpperTriangle78f.html"><strong aria-hidden="true">13.9.</strong> struct CovarianceUpperTriangle78f</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/Severity.html"><strong aria-hidden="true">13.10.</strong> enum Severity</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/CommandResultCode.html"><strong aria-hidden="true">13.11.</strong> enum CommandResultCode</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/GnssFixType.html"><strong aria-hidden="true">13.12.</strong> enum GnssFixType</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/BatteryChargeState.html"><strong aria-hidden="true">13.13.</strong> enum BatteryChargeState</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/BatteryFunction.html"><strong aria-hidden="true">13.14.</strong> enum BatteryFunction</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/BatteryType.html"><strong aria-hidden="true">13.15.</strong> enum BatteryType</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/MissionState.html"><strong aria-hidden="true">13.16.</strong> enum MissionState</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/GeoAltitudeFrame.html"><strong aria-hidden="true">13.17.</strong> enum GeoAltitudeFrame</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/LocalFrame.html"><strong aria-hidden="true">13.18.</strong> enum LocalFrame</a></span></li><li class="chapter-item expanded "><span class="chapter-link-wrapper"><a href="schemas/types/TopicId.html"><strong aria-hidden="true">13.19.</strong> enum TopicId</a></span></li></ol></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString().split('#')[0].split('?')[0];
        if (current_page.endsWith('/')) {
            current_page += 'index.html';
        }
        const links = Array.prototype.slice.call(this.querySelectorAll('a'));
        const l = links.length;
        for (let i = 0; i < l; ++i) {
            const link = links[i];
            const href = link.getAttribute('href');
            if (href && !href.startsWith('#') && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The 'index' page is supposed to alias the first chapter in the book.
            // Check both with and without the '.html' suffix to be robust against pretty URLs
            if (link.href.replace(/\.html$/, '') === current_page.replace(/\.html$/, '')
                || i === 0
                && path_to_root === ''
                && current_page.endsWith('/index.html')) {
                link.classList.add('active');
                let parent = link.parentElement;
                while (parent) {
                    if (parent.tagName === 'LI' && parent.classList.contains('chapter-item')) {
                        parent.classList.add('expanded');
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', e => {
            if (e.target.tagName === 'A') {
                const clientRect = e.target.getBoundingClientRect();
                const sidebarRect = this.getBoundingClientRect();
                sessionStorage.setItem('sidebar-scroll-offset', clientRect.top - sidebarRect.top);
            }
        }, { passive: true });
        const sidebarScrollOffset = sessionStorage.getItem('sidebar-scroll-offset');
        sessionStorage.removeItem('sidebar-scroll-offset');
        if (sidebarScrollOffset !== null) {
            // preserve sidebar scroll position when navigating via links within sidebar
            const activeSection = this.querySelector('.active');
            if (activeSection) {
                const clientRect = activeSection.getBoundingClientRect();
                const sidebarRect = this.getBoundingClientRect();
                const currentOffset = clientRect.top - sidebarRect.top;
                this.scrollTop += currentOffset - parseFloat(sidebarScrollOffset);
            }
        } else {
            // scroll sidebar to current active section when navigating via
            // 'next/previous chapter' buttons
            const activeSection = document.querySelector('#mdbook-sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        const sidebarAnchorToggles = document.querySelectorAll('.chapter-fold-toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(el => {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define('mdbook-sidebar-scrollbox', MDBookSidebarScrollbox);


// ---------------------------------------------------------------------------
// Support for dynamically adding headers to the sidebar.

(function() {
    // This is used to detect which direction the page has scrolled since the
    // last scroll event.
    let lastKnownScrollPosition = 0;
    // This is the threshold in px from the top of the screen where it will
    // consider a header the "current" header when scrolling down.
    const defaultDownThreshold = 150;
    // Same as defaultDownThreshold, except when scrolling up.
    const defaultUpThreshold = 300;
    // The threshold is a virtual horizontal line on the screen where it
    // considers the "current" header to be above the line. The threshold is
    // modified dynamically to handle headers that are near the bottom of the
    // screen, and to slightly offset the behavior when scrolling up vs down.
    let threshold = defaultDownThreshold;
    // This is used to disable updates while scrolling. This is needed when
    // clicking the header in the sidebar, which triggers a scroll event. It
    // is somewhat finicky to detect when the scroll has finished, so this
    // uses a relatively dumb system of disabling scroll updates for a short
    // time after the click.
    let disableScroll = false;
    // Array of header elements on the page.
    let headers;
    // Array of li elements that are initially collapsed headers in the sidebar.
    // I'm not sure why eslint seems to have a false positive here.
    // eslint-disable-next-line prefer-const
    let headerToggles = [];
    // This is a debugging tool for the threshold which you can enable in the console.
    let thresholdDebug = false;

    // Updates the threshold based on the scroll position.
    function updateThreshold() {
        const scrollTop = window.pageYOffset || document.documentElement.scrollTop;
        const windowHeight = window.innerHeight;
        const documentHeight = document.documentElement.scrollHeight;

        // The number of pixels below the viewport, at most documentHeight.
        // This is used to push the threshold down to the bottom of the page
        // as the user scrolls towards the bottom.
        const pixelsBelow = Math.max(0, documentHeight - (scrollTop + windowHeight));
        // The number of pixels above the viewport, at least defaultDownThreshold.
        // Similar to pixelsBelow, this is used to push the threshold back towards
        // the top when reaching the top of the page.
        const pixelsAbove = Math.max(0, defaultDownThreshold - scrollTop);
        // How much the threshold should be offset once it gets close to the
        // bottom of the page.
        const bottomAdd = Math.max(0, windowHeight - pixelsBelow - defaultDownThreshold);
        let adjustedBottomAdd = bottomAdd;

        // Adjusts bottomAdd for a small document. The calculation above
        // assumes the document is at least twice the windowheight in size. If
        // it is less than that, then bottomAdd needs to be shrunk
        // proportional to the difference in size.
        if (documentHeight < windowHeight * 2) {
            const maxPixelsBelow = documentHeight - windowHeight;
            const t = 1 - pixelsBelow / Math.max(1, maxPixelsBelow);
            const clamp = Math.max(0, Math.min(1, t));
            adjustedBottomAdd *= clamp;
        }

        let scrollingDown = true;
        if (scrollTop < lastKnownScrollPosition) {
            scrollingDown = false;
        }

        if (scrollingDown) {
            // When scrolling down, move the threshold up towards the default
            // downwards threshold position. If near the bottom of the page,
            // adjustedBottomAdd will offset the threshold towards the bottom
            // of the page.
            const amountScrolledDown = scrollTop - lastKnownScrollPosition;
            const adjustedDefault = defaultDownThreshold + adjustedBottomAdd;
            threshold = Math.max(adjustedDefault, threshold - amountScrolledDown);
        } else {
            // When scrolling up, move the threshold down towards the default
            // upwards threshold position. If near the bottom of the page,
            // quickly transition the threshold back up where it normally
            // belongs.
            const amountScrolledUp = lastKnownScrollPosition - scrollTop;
            const adjustedDefault = defaultUpThreshold - pixelsAbove
                + Math.max(0, adjustedBottomAdd - defaultDownThreshold);
            threshold = Math.min(adjustedDefault, threshold + amountScrolledUp);
        }

        if (documentHeight <= windowHeight) {
            threshold = 0;
        }

        if (thresholdDebug) {
            const id = 'mdbook-threshold-debug-data';
            let data = document.getElementById(id);
            if (data === null) {
                data = document.createElement('div');
                data.id = id;
                data.style.cssText = `
                    position: fixed;
                    top: 50px;
                    right: 10px;
                    background-color: 0xeeeeee;
                    z-index: 9999;
                    pointer-events: none;
                `;
                document.body.appendChild(data);
            }
            data.innerHTML = `
                <table>
                  <tr><td>documentHeight</td><td>${documentHeight.toFixed(1)}</td></tr>
                  <tr><td>windowHeight</td><td>${windowHeight.toFixed(1)}</td></tr>
                  <tr><td>scrollTop</td><td>${scrollTop.toFixed(1)}</td></tr>
                  <tr><td>pixelsAbove</td><td>${pixelsAbove.toFixed(1)}</td></tr>
                  <tr><td>pixelsBelow</td><td>${pixelsBelow.toFixed(1)}</td></tr>
                  <tr><td>bottomAdd</td><td>${bottomAdd.toFixed(1)}</td></tr>
                  <tr><td>adjustedBottomAdd</td><td>${adjustedBottomAdd.toFixed(1)}</td></tr>
                  <tr><td>scrollingDown</td><td>${scrollingDown}</td></tr>
                  <tr><td>threshold</td><td>${threshold.toFixed(1)}</td></tr>
                </table>
            `;
            drawDebugLine();
        }

        lastKnownScrollPosition = scrollTop;
    }

    function drawDebugLine() {
        if (!document.body) {
            return;
        }
        const id = 'mdbook-threshold-debug-line';
        const existingLine = document.getElementById(id);
        if (existingLine) {
            existingLine.remove();
        }
        const line = document.createElement('div');
        line.id = id;
        line.style.cssText = `
            position: fixed;
            top: ${threshold}px;
            left: 0;
            width: 100vw;
            height: 2px;
            background-color: red;
            z-index: 9999;
            pointer-events: none;
        `;
        document.body.appendChild(line);
    }

    function mdbookEnableThresholdDebug() {
        thresholdDebug = true;
        updateThreshold();
        drawDebugLine();
    }

    window.mdbookEnableThresholdDebug = mdbookEnableThresholdDebug;

    // Updates which headers in the sidebar should be expanded. If the current
    // header is inside a collapsed group, then it, and all its parents should
    // be expanded.
    function updateHeaderExpanded(currentA) {
        // Add expanded to all header-item li ancestors.
        let current = currentA.parentElement;
        while (current) {
            if (current.tagName === 'LI' && current.classList.contains('header-item')) {
                current.classList.add('expanded');
            }
            current = current.parentElement;
        }
    }

    // Updates which header is marked as the "current" header in the sidebar.
    // This is done with a virtual Y threshold, where headers at or below
    // that line will be considered the current one.
    function updateCurrentHeader() {
        if (!headers || !headers.length) {
            return;
        }

        // Reset the classes, which will be rebuilt below.
        const els = document.getElementsByClassName('current-header');
        for (const el of els) {
            el.classList.remove('current-header');
        }
        for (const toggle of headerToggles) {
            toggle.classList.remove('expanded');
        }

        // Find the last header that is above the threshold.
        let lastHeader = null;
        for (const header of headers) {
            const rect = header.getBoundingClientRect();
            if (rect.top <= threshold) {
                lastHeader = header;
            } else {
                break;
            }
        }
        if (lastHeader === null) {
            lastHeader = headers[0];
            const rect = lastHeader.getBoundingClientRect();
            const windowHeight = window.innerHeight;
            if (rect.top >= windowHeight) {
                return;
            }
        }

        // Get the anchor in the summary.
        const href = '#' + lastHeader.id;
        const a = [...document.querySelectorAll('.header-in-summary')]
            .find(element => element.getAttribute('href') === href);
        if (!a) {
            return;
        }

        a.classList.add('current-header');

        updateHeaderExpanded(a);
    }

    // Updates which header is "current" based on the threshold line.
    function reloadCurrentHeader() {
        if (disableScroll) {
            return;
        }
        updateThreshold();
        updateCurrentHeader();
    }


    // When clicking on a header in the sidebar, this adjusts the threshold so
    // that it is located next to the header. This is so that header becomes
    // "current".
    function headerThresholdClick(event) {
        // See disableScroll description why this is done.
        disableScroll = true;
        setTimeout(() => {
            disableScroll = false;
        }, 100);
        // requestAnimationFrame is used to delay the update of the "current"
        // header until after the scroll is done, and the header is in the new
        // position.
        requestAnimationFrame(() => {
            requestAnimationFrame(() => {
                // Closest is needed because if it has child elements like <code>.
                const a = event.target.closest('a');
                const href = a.getAttribute('href');
                const targetId = href.substring(1);
                const targetElement = document.getElementById(targetId);
                if (targetElement) {
                    threshold = targetElement.getBoundingClientRect().bottom;
                    updateCurrentHeader();
                }
            });
        });
    }

    // Takes the nodes from the given head and copies them over to the
    // destination, along with some filtering.
    function filterHeader(source, dest) {
        const clone = source.cloneNode(true);
        clone.querySelectorAll('mark').forEach(mark => {
            mark.replaceWith(...mark.childNodes);
        });
        dest.append(...clone.childNodes);
    }

    // Scans page for headers and adds them to the sidebar.
    document.addEventListener('DOMContentLoaded', function() {
        const activeSection = document.querySelector('#mdbook-sidebar .active');
        if (activeSection === null) {
            return;
        }

        const main = document.getElementsByTagName('main')[0];
        headers = Array.from(main.querySelectorAll('h2, h3, h4, h5, h6'))
            .filter(h => h.id !== '' && h.children.length && h.children[0].tagName === 'A');

        if (headers.length === 0) {
            return;
        }

        // Build a tree of headers in the sidebar.

        const stack = [];

        const firstLevel = parseInt(headers[0].tagName.charAt(1));
        for (let i = 1; i < firstLevel; i++) {
            const ol = document.createElement('ol');
            ol.classList.add('section');
            if (stack.length > 0) {
                stack[stack.length - 1].ol.appendChild(ol);
            }
            stack.push({level: i + 1, ol: ol});
        }

        // The level where it will start folding deeply nested headers.
        const foldLevel = 3;

        for (let i = 0; i < headers.length; i++) {
            const header = headers[i];
            const level = parseInt(header.tagName.charAt(1));

            const currentLevel = stack[stack.length - 1].level;
            if (level > currentLevel) {
                // Begin nesting to this level.
                for (let nextLevel = currentLevel + 1; nextLevel <= level; nextLevel++) {
                    const ol = document.createElement('ol');
                    ol.classList.add('section');
                    const last = stack[stack.length - 1];
                    const lastChild = last.ol.lastChild;
                    // Handle the case where jumping more than one nesting
                    // level, which doesn't have a list item to place this new
                    // list inside of.
                    if (lastChild) {
                        lastChild.appendChild(ol);
                    } else {
                        last.ol.appendChild(ol);
                    }
                    stack.push({level: nextLevel, ol: ol});
                }
            } else if (level < currentLevel) {
                while (stack.length > 1 && stack[stack.length - 1].level > level) {
                    stack.pop();
                }
            }

            const li = document.createElement('li');
            li.classList.add('header-item');
            li.classList.add('expanded');
            if (level < foldLevel) {
                li.classList.add('expanded');
            }
            const span = document.createElement('span');
            span.classList.add('chapter-link-wrapper');
            const a = document.createElement('a');
            span.appendChild(a);
            a.href = '#' + header.id;
            a.classList.add('header-in-summary');
            filterHeader(header.children[0], a);
            a.addEventListener('click', headerThresholdClick);
            const nextHeader = headers[i + 1];
            if (nextHeader !== undefined) {
                const nextLevel = parseInt(nextHeader.tagName.charAt(1));
                if (nextLevel > level && level >= foldLevel) {
                    const toggle = document.createElement('a');
                    toggle.classList.add('chapter-fold-toggle');
                    toggle.classList.add('header-toggle');
                    toggle.addEventListener('click', () => {
                        li.classList.toggle('expanded');
                    });
                    const toggleDiv = document.createElement('div');
                    toggleDiv.textContent = '❱';
                    toggle.appendChild(toggleDiv);
                    span.appendChild(toggle);
                    headerToggles.push(li);
                }
            }
            li.appendChild(span);

            const currentParent = stack[stack.length - 1];
            currentParent.ol.appendChild(li);
        }

        const onThisPage = document.createElement('div');
        onThisPage.classList.add('on-this-page');
        onThisPage.append(stack[0].ol);
        const activeItemSpan = activeSection.parentElement;
        activeItemSpan.after(onThisPage);
    });

    document.addEventListener('DOMContentLoaded', reloadCurrentHeader);
    document.addEventListener('scroll', reloadCurrentHeader, { passive: true });
})();

