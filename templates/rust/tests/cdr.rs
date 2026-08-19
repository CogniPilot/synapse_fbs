use synapse_fbs::{
    cdr::CdrError,
    cdr_catalog::{
        CDR_PROJECTION_SET_IDENTITY, CDR_PROJECTIONS, GNSS_FIX_CDR_TOTAL_BYTES,
        GnssFixCdr, OPTICAL_FLOW_VELOCITY_CDR_TOTAL_BYTES,
        OpticalFlowVelocityCdr, cdr_projection_by_topic_id,
    },
};

#[test]
fn optical_flow_velocity_matches_the_ros_jazzy_cdr_vector() {
    let expected = [
        0x00, 0x01, 0x00, 0x00, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01,
        0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0xc0, 0x00, 0x00, 0x60, 0x40,
        0x00, 0x00, 0x80, 0xbe, 0x00, 0x00, 0x00, 0x3f, 0xaa, 0x05, 0x01, 0x02,
    ];
    let value = OpticalFlowVelocityCdr {
        timestamp_ns: 0x0102_0304_0506_0708,
        velocity_flu_m_s: [1.0, -2.0],
        distance_m: 3.5,
        roll_rad: -0.25,
        pitch_rad: 0.5,
        quality: 0xaa,
        flags: 0x05,
        time_status: 1,
        id: 2,
    };
    let mut bytes = [0u8; OPTICAL_FLOW_VELOCITY_CDR_TOTAL_BYTES];
    assert_eq!(value.encode(&mut bytes), Ok(expected.len()));
    assert_eq!(bytes, expected);
    assert_eq!(OpticalFlowVelocityCdr::decode(&bytes), Ok(value));

    let mut short = [0u8; OPTICAL_FLOW_VELOCITY_CDR_TOTAL_BYTES - 1];
    assert_eq!(value.encode(&mut short), Err(CdrError::BufferTooSmall));
    let mut trailing = [0u8; OPTICAL_FLOW_VELOCITY_CDR_TOTAL_BYTES + 1];
    trailing[..bytes.len()].copy_from_slice(&bytes);
    assert_eq!(
        OpticalFlowVelocityCdr::decode(&trailing),
        Err(CdrError::TrailingBytes)
    );
}

#[test]
fn gnss_fix_is_fixed_bounded_and_round_trips() {
    let value = GnssFixCdr {
        timestamp_ns: 0x0102_0304_0506_0708,
        time_unix_ns: 0x1112_1314_1516_1718,
        latitude_deg_e7: 425_000_000,
        longitude_deg_e7: -830_000_000,
        altitude_msl_mm: 123_456,
        altitude_ellipsoid_mm: 157_890,
        horizontal_accuracy_mm: 200,
        vertical_accuracy_mm: 300,
        velocity_accuracy_mm_s: 40,
        yaw_accuracy_cdeg: 50,
        hdop_centi: 60,
        vdop_centi: 70,
        ground_speed_cm_s: 800,
        course_over_ground_cdeg: 900,
        yaw_cdeg: 1000,
        velocity_up_cm_s: -110,
        flags: 0x0f,
        fix_type: 3,
        satellites_used: 12,
        satellites_visible: 20,
        time_status: 1,
        id: 0,
    };
    let mut bytes = [0u8; GNSS_FIX_CDR_TOTAL_BYTES];
    assert_eq!(value.encode(&mut bytes), Ok(64));
    assert_eq!(GnssFixCdr::decode(&bytes), Ok(value));
}

#[test]
fn catalog_exposes_only_materialized_projections() {
    assert_eq!(CDR_PROJECTION_SET_IDENTITY.len(), 64);
    assert_eq!(CDR_PROJECTIONS.len(), 2);
    assert_eq!(
        cdr_projection_by_topic_id(8).unwrap().rihs01,
        "RIHS01_ac8d665c1bf6f81796d95bdd6a2285537bbfcb34869ba0e042f8ce24f75d9f0e"
    );
    assert_eq!(
        cdr_projection_by_topic_id(10).unwrap().rihs01,
        "RIHS01_8f46bb3da905598105f99e502394842afa66d849de841143565a193074829d09"
    );
    assert!(cdr_projection_by_topic_id(47).is_none());
}
