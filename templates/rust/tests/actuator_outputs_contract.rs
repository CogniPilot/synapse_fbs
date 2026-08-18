use synapse_fbs::actuator_outputs_contract::*;

const ARM_STATE_DISARMED: u8 = 0;
const ARM_STATE_ARMED: u8 = 1;
const SOURCE_NO_COMMAND: u8 = 0;
const SOURCE_CONTROL_ALLOCATION: u8 = 1;
const SOURCE_ACTUATOR_TEST: u8 = 2;
const NEGATIVE_ZERO_BITS: u32 = 0x8000_0000;

const TROPIC_POSITIVE: &[u8; ACTUATOR_OUTPUTS_PAYLOAD_SIZE] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/test-vectors/actuator_outputs/tropic-positive.bin"
));

fn tropic_profile() -> ActuatorOutputsProfile {
    ActuatorOutputsProfile::new(0x0000_000f, 0)
}

fn payload(arm_state: u8, command_source: u8) -> [u8; ACTUATOR_OUTPUTS_PAYLOAD_SIZE] {
    let mut payload = [0_u8; ACTUATOR_OUTPUTS_PAYLOAD_SIZE];
    payload[ACTUATOR_OUTPUTS_TIMESTAMP_OFFSET..ACTUATOR_OUTPUTS_TIMESTAMP_OFFSET + 8]
        .copy_from_slice(&0x0102_0304_0506_0708_u64.to_le_bytes());
    payload[ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET..ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET + 4]
        .copy_from_slice(&0x0000_000f_u32.to_le_bytes());
    payload[ACTUATOR_OUTPUTS_ARM_STATE_OFFSET] = arm_state;
    payload[ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET] = command_source;
    payload[ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET] = 1;
    payload
}

fn set_output(payload: &mut [u8], slot: usize, value: f32) {
    let offset = ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET + slot * 4;
    payload[offset..offset + 4].copy_from_slice(&value.to_bits().to_le_bytes());
}

#[test]
fn layout_constants_match_the_accepted_candidate_payload() {
    assert_eq!(ACTUATOR_OUTPUTS_TIMESTAMP_OFFSET, 0);
    assert_eq!(ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET, 8);
    assert_eq!(ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET, 12);
    assert_eq!(
        ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET + (ACTUATOR_OUTPUT_COUNT - 1) * 4,
        136
    );
    assert_eq!(ACTUATOR_OUTPUTS_ARM_STATE_OFFSET, 140);
    assert_eq!(ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET, 141);
    assert_eq!(ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET, 142);
    assert_eq!(ACTUATOR_OUTPUTS_PADDING_OFFSET, 143);
    assert_eq!(ACTUATOR_OUTPUTS_PAYLOAD_SIZE, 144);
}

#[test]
fn materialized_tropic_positive_vector_is_accepted() {
    assert_eq!(&TROPIC_POSITIVE[0..8], &[8, 7, 6, 5, 4, 3, 2, 1]);
    assert_eq!(&TROPIC_POSITIVE[8..12], &[15, 0, 0, 0]);
    assert_eq!(&TROPIC_POSITIVE[12..16], &[0, 0, 128, 62]);
    assert_eq!(&TROPIC_POSITIVE[16..20], &[0, 0, 0, 63]);
    assert_eq!(&TROPIC_POSITIVE[20..24], &[0, 0, 64, 63]);
    assert_eq!(&TROPIC_POSITIVE[24..28], &[0, 0, 128, 63]);
    assert_eq!(&TROPIC_POSITIVE[140..144], &[1, 1, 1, 0]);
    assert_eq!(
        validate_actuator_outputs_payload(TROPIC_POSITIVE, &tropic_profile(), false,),
        Ok(())
    );
}

#[test]
fn disarmed_sources_and_authorized_test_follow_the_matrix() {
    let safe = payload(ARM_STATE_DISARMED, SOURCE_NO_COMMAND);
    assert_eq!(
        validate_actuator_outputs_payload(&safe, &tropic_profile(), false),
        Ok(())
    );

    let inhibited = payload(ARM_STATE_DISARMED, SOURCE_CONTROL_ALLOCATION);
    assert_eq!(
        validate_actuator_outputs_payload(&inhibited, &tropic_profile(), false),
        Ok(())
    );

    let mut nonzero_profile = tropic_profile();
    nonzero_profile.disarmed_values[0] = 0.25;
    let mut nonzero_safe = payload(ARM_STATE_DISARMED, SOURCE_NO_COMMAND);
    set_output(&mut nonzero_safe, 0, 0.25);
    assert_eq!(
        validate_actuator_outputs_payload(&nonzero_safe, &nonzero_profile, false),
        Ok(())
    );
    let mut nonzero_inhibited =
        payload(ARM_STATE_DISARMED, SOURCE_CONTROL_ALLOCATION);
    set_output(&mut nonzero_inhibited, 0, 0.25);
    assert_eq!(
        validate_actuator_outputs_payload(
            &nonzero_inhibited,
            &nonzero_profile,
            false,
        ),
        Ok(())
    );

    let mut reversible_profile = tropic_profile();
    reversible_profile.reversible_mask = 1;
    reversible_profile.disarmed_values[0] = -0.25;
    let mut reversible_safe = payload(ARM_STATE_DISARMED, SOURCE_NO_COMMAND);
    set_output(&mut reversible_safe, 0, -0.25);
    assert_eq!(
        validate_actuator_outputs_payload(&reversible_safe, &reversible_profile, false,),
        Ok(())
    );

    let mut test = payload(ARM_STATE_DISARMED, SOURCE_ACTUATOR_TEST);
    set_output(&mut test, 0, 0.5);
    assert_eq!(
        validate_actuator_outputs_payload(&test, &tropic_profile(), true),
        Ok(())
    );
    assert_eq!(
        validate_actuator_outputs_payload(&test, &tropic_profile(), false),
        Err(ActuatorOutputsError::ActuatorTestNotAuthorized)
    );
}

#[test]
fn malformed_layout_mask_enums_and_padding_are_rejected() {
    let canonical = payload(ARM_STATE_DISARMED, SOURCE_NO_COMMAND);
    assert!(matches!(
        validate_actuator_outputs_payload(&canonical[..143], &tropic_profile(), false),
        Err(ActuatorOutputsError::WrongLength { .. })
    ));
    let mut trailing = canonical.to_vec();
    trailing.push(0);
    assert!(matches!(
        validate_actuator_outputs_payload(&trailing, &tropic_profile(), false),
        Err(ActuatorOutputsError::WrongLength { .. })
    ));

    let mut mutated = canonical;
    mutated[ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET] = 3;
    assert!(matches!(
        validate_actuator_outputs_payload(&mutated, &tropic_profile(), false),
        Err(ActuatorOutputsError::ActiveMaskMismatch { .. })
    ));

    let mut mutated = canonical;
    mutated[ACTUATOR_OUTPUTS_ARM_STATE_OFFSET] = 2;
    assert_eq!(
        validate_actuator_outputs_payload(&mutated, &tropic_profile(), false),
        Err(ActuatorOutputsError::UnknownArmState { value: 2 })
    );

    let mut mutated = canonical;
    mutated[ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET] = 3;
    assert_eq!(
        validate_actuator_outputs_payload(&mutated, &tropic_profile(), false),
        Err(ActuatorOutputsError::UnknownCommandSource { value: 3 })
    );

    let mut mutated = canonical;
    mutated[ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET] = 3;
    assert_eq!(
        validate_actuator_outputs_payload(&mutated, &tropic_profile(), false),
        Err(ActuatorOutputsError::UnknownTimeStatus { value: 3 })
    );

    let mut mutated = canonical;
    mutated[ACTUATOR_OUTPUTS_PADDING_OFFSET] = 1;
    assert_eq!(
        validate_actuator_outputs_payload(&mutated, &tropic_profile(), false),
        Err(ActuatorOutputsError::NonzeroPadding { value: 1 })
    );
}

#[test]
fn invalid_float_encodings_and_ranges_are_rejected() {
    let cases = [
        (
            f32::NEG_INFINITY,
            ActuatorOutputsError::NonFiniteOutput { slot: 0 },
        ),
        (f32::NAN, ActuatorOutputsError::NonFiniteOutput { slot: 0 }),
        (
            f32::from_bits(NEGATIVE_ZERO_BITS),
            ActuatorOutputsError::NegativeZeroOutput { slot: 0 },
        ),
        (
            -0.25,
            ActuatorOutputsError::OutputOutOfRange {
                slot: 0,
                reversible: false,
            },
        ),
        (
            1.25,
            ActuatorOutputsError::OutputOutOfRange {
                slot: 0,
                reversible: false,
            },
        ),
    ];
    for (value, expected) in cases {
        let mut payload = payload(ARM_STATE_ARMED, SOURCE_CONTROL_ALLOCATION);
        set_output(&mut payload, 0, value);
        assert_eq!(
            validate_actuator_outputs_payload(&payload, &tropic_profile(), false),
            Err(expected)
        );
    }

    let mut inactive = payload(ARM_STATE_ARMED, SOURCE_CONTROL_ALLOCATION);
    set_output(&mut inactive, 4, 0.25);
    assert_eq!(
        validate_actuator_outputs_payload(&inactive, &tropic_profile(), false),
        Err(ActuatorOutputsError::InactiveOutputNotZero { slot: 4 })
    );

    let reversible_profile = ActuatorOutputsProfile::new(1, 1);
    let mut reversible = [0_u8; ACTUATOR_OUTPUTS_PAYLOAD_SIZE];
    reversible[ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET..ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET + 4]
        .copy_from_slice(&1_u32.to_le_bytes());
    reversible[ACTUATOR_OUTPUTS_ARM_STATE_OFFSET] = ARM_STATE_ARMED;
    reversible[ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET] = SOURCE_CONTROL_ALLOCATION;
    set_output(&mut reversible, 0, -1.0);
    assert_eq!(
        validate_actuator_outputs_payload(&reversible, &reversible_profile, false),
        Ok(())
    );
}

#[test]
fn disarmed_nonzero_values_and_invalid_arm_pairs_are_rejected() {
    let mut inhibited = payload(ARM_STATE_DISARMED, SOURCE_CONTROL_ALLOCATION);
    set_output(&mut inhibited, 0, 0.25);
    assert_eq!(
        validate_actuator_outputs_payload(&inhibited, &tropic_profile(), false),
        Err(ActuatorOutputsError::DisarmedOutputMismatch { slot: 0 })
    );

    for source in [SOURCE_NO_COMMAND, SOURCE_ACTUATOR_TEST] {
        let invalid = payload(ARM_STATE_ARMED, source);
        assert_eq!(
            validate_actuator_outputs_payload(&invalid, &tropic_profile(), true),
            Err(ActuatorOutputsError::InvalidArmSource {
                arm_state: ARM_STATE_ARMED,
                command_source: source,
            })
        );
    }
}

#[test]
fn invalid_profiles_are_rejected() {
    let invalid_mask = ActuatorOutputsProfile::new(1, 2);
    assert!(matches!(
        validate_actuator_outputs_profile(&invalid_mask),
        Err(ActuatorOutputsError::ReversibleMaskOutsideLogicalMask { .. })
    ));

    let mut negative_zero = tropic_profile();
    negative_zero.disarmed_values[0] = -0.0;
    assert_eq!(
        validate_actuator_outputs_profile(&negative_zero),
        Err(ActuatorOutputsError::NegativeZeroProfileDisarmedValue { slot: 0 })
    );

    let mut nonfinite = tropic_profile();
    nonfinite.disarmed_values[0] = f32::INFINITY;
    assert_eq!(
        validate_actuator_outputs_profile(&nonfinite),
        Err(ActuatorOutputsError::NonFiniteProfileDisarmedValue { slot: 0 })
    );

    let mut out_of_range = tropic_profile();
    out_of_range.disarmed_values[0] = -0.25;
    assert_eq!(
        validate_actuator_outputs_profile(&out_of_range),
        Err(ActuatorOutputsError::ProfileDisarmedValueOutOfRange {
            slot: 0,
            reversible: false,
        })
    );

    let mut inactive_nonzero = tropic_profile();
    inactive_nonzero.disarmed_values[4] = 0.25;
    assert_eq!(
        validate_actuator_outputs_profile(&inactive_nonzero),
        Err(ActuatorOutputsError::InactiveProfileDisarmedValueNotZero { slot: 4 })
    );
}
