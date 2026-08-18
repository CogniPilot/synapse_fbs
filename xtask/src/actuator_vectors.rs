const ACTUATOR_VECTOR_DIR: &str = "test-vectors/actuator_outputs";
const ACTUATOR_VECTOR_MANIFEST: &str = "negative-vectors.json";
const TROPIC_POSITIVE_VECTOR: &str = "tropic-positive.bin";
const ACTUATOR_VECTOR_FORMAT: &str =
    "synapse.actuator-outputs-negative-vectors.v1";

#[derive(serde::Deserialize)]
struct ActuatorVectorManifest {
    format: String,
    base: ActuatorVectorBase,
    profile: ActuatorVectorProfile,
    payload_cases: Vec<ActuatorPayloadCase>,
    profile_cases: Vec<ActuatorProfileCase>,
}

#[derive(serde::Deserialize)]
struct ActuatorVectorBase {
    file: String,
    size: usize,
    sha256: String,
}

#[derive(serde::Deserialize)]
struct ActuatorVectorProfile {
    name: String,
    logical_slot_mask: String,
    reversible_mask: String,
    disarmed_values: String,
}

#[derive(serde::Deserialize)]
struct ActuatorPayloadCase {
    name: String,
    operation: ActuatorPayloadOperation,
    expected: String,
}

#[derive(serde::Deserialize)]
struct ActuatorPayloadOperation {
    kind: String,
    size: Option<usize>,
    offset: Option<usize>,
    bytes_hex: Option<String>,
    writes: Option<Vec<ActuatorPayloadWrite>>,
}

#[derive(serde::Deserialize)]
struct ActuatorPayloadWrite {
    offset: usize,
    bytes_hex: String,
}

#[derive(serde::Deserialize)]
struct ActuatorProfileCase {
    name: String,
    field: String,
    value: String,
    expected: String,
}

fn validate_actuator_output_vectors(root: &Path) -> Result<()> {
    use actuator_outputs_contract::{
        ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET,
        ACTUATOR_OUTPUTS_ARM_STATE_OFFSET,
        ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET,
        ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET,
        ACTUATOR_OUTPUTS_PAYLOAD_SIZE,
        ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET, ActuatorOutputsProfile,
        validate_actuator_outputs_payload, validate_actuator_outputs_profile,
    };

    let vector_dir = root.join(ACTUATOR_VECTOR_DIR);
    let vector_path = vector_dir.join(TROPIC_POSITIVE_VECTOR);
    let vector = fs::read(&vector_path)?;
    let expected_vector = expected_tropic_positive_vector();
    if vector.as_slice() != expected_vector {
        return fail(format!(
            "{} does not match the deterministic accepted-candidate projection",
            vector_path.display()
        ));
    }

    let manifest_path = vector_dir.join(ACTUATOR_VECTOR_MANIFEST);
    let manifest: ActuatorVectorManifest =
        serde_json::from_slice(&fs::read(&manifest_path)?).map_err(|error| {
            io::Error::other(format!("invalid {}: {error}", manifest_path.display()))
        })?;
    if manifest.format != ACTUATOR_VECTOR_FORMAT {
        return fail(format!(
            "{} has format '{}', expected '{}'",
            manifest_path.display(),
            manifest.format,
            ACTUATOR_VECTOR_FORMAT
        ));
    }
    if manifest.base.file != TROPIC_POSITIVE_VECTOR
        || manifest.base.size != ACTUATOR_OUTPUTS_PAYLOAD_SIZE
        || manifest.base.size != vector.len()
    {
        return fail(format!(
            "{} base metadata does not identify the 144-byte TROPIC vector",
            manifest_path.display()
        ));
    }
    let vector_sha256 = sha256_bytes_hex(&vector);
    if manifest.base.sha256 != vector_sha256 {
        return fail(format!(
            "{} records base SHA-256 {}, actual {}",
            manifest_path.display(),
            manifest.base.sha256,
            vector_sha256
        ));
    }
    if manifest.profile.name != "tropic"
        || manifest.profile.logical_slot_mask != "0x0000000f"
        || manifest.profile.reversible_mask != "0x00000000"
        || manifest.profile.disarmed_values != "32xf32-le:00000000"
    {
        return fail(format!(
            "{} does not record the accepted TROPIC actuator profile",
            manifest_path.display()
        ));
    }

    let profile = ActuatorOutputsProfile::new(0x0000_000f, 0);
    validate_actuator_outputs_payload(&vector, &profile, false).map_err(|error| {
        io::Error::other(format!(
            "{} fails its accepted profile: {error}",
            vector_path.display()
        ))
    })?;

    if manifest.payload_cases.is_empty() || manifest.profile_cases.is_empty() {
        return fail(format!(
            "{} must contain payload and profile negative cases",
            manifest_path.display()
        ));
    }

    let mut names = BTreeSet::new();
    for case in &manifest.payload_cases {
        if !names.insert(case.name.as_str()) {
            return fail(format!(
                "{} repeats case name '{}'",
                manifest_path.display(),
                case.name
            ));
        }
        let payload = apply_actuator_payload_operation(&vector, &case.operation)?;
        let error = validate_actuator_outputs_payload(&payload, &profile, false)
            .expect_err("negative actuator payload case unexpectedly passed");
        let actual = actuator_outputs_error_name(&error);
        if actual != case.expected {
            return fail(format!(
                "{} case '{}' expected {}, received {}",
                manifest_path.display(),
                case.name,
                case.expected,
                actual
            ));
        }
    }

    for case in &manifest.profile_cases {
        if !names.insert(case.name.as_str()) {
            return fail(format!(
                "{} repeats case name '{}'",
                manifest_path.display(),
                case.name
            ));
        }
        let mut mutated = profile;
        apply_actuator_profile_mutation(&mut mutated, case)?;
        let error = validate_actuator_outputs_profile(&mutated)
            .expect_err("negative actuator profile case unexpectedly passed");
        let actual = actuator_outputs_error_name(&error);
        if actual != case.expected {
            return fail(format!(
                "{} case '{}' expected {}, received {}",
                manifest_path.display(),
                case.name,
                case.expected,
                actual
            ));
        }
    }

    if vector[ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET..ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET + 4]
        != 0x0000_000f_u32.to_le_bytes()
        || vector[ACTUATOR_OUTPUTS_ARM_STATE_OFFSET] != 1
        || vector[ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET] != 1
        || vector[ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET] != 1
        || vector[ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET..ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET + 4]
            != 0.25_f32.to_bits().to_le_bytes()
    {
        return fail("TROPIC positive-vector field projection is inconsistent");
    }

    Ok(())
}

fn expected_tropic_positive_vector(
) -> [u8; actuator_outputs_contract::ACTUATOR_OUTPUTS_PAYLOAD_SIZE] {
    use actuator_outputs_contract::{
        ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET,
        ACTUATOR_OUTPUTS_ARM_STATE_OFFSET,
        ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET,
        ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET,
        ACTUATOR_OUTPUTS_PAYLOAD_SIZE,
        ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET,
        ACTUATOR_OUTPUTS_TIMESTAMP_OFFSET,
    };

    let mut payload = [0_u8; ACTUATOR_OUTPUTS_PAYLOAD_SIZE];
    payload[ACTUATOR_OUTPUTS_TIMESTAMP_OFFSET..ACTUATOR_OUTPUTS_TIMESTAMP_OFFSET + 8]
        .copy_from_slice(&0x0102_0304_0506_0708_u64.to_le_bytes());
    payload[ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET..ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET + 4]
        .copy_from_slice(&0x0000_000f_u32.to_le_bytes());
    for (slot, value) in [0.25_f32, 0.5, 0.75, 1.0].into_iter().enumerate() {
        let offset = ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET + slot * 4;
        payload[offset..offset + 4].copy_from_slice(&value.to_bits().to_le_bytes());
    }
    payload[ACTUATOR_OUTPUTS_ARM_STATE_OFFSET] = 1;
    payload[ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET] = 1;
    payload[ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET] = 1;
    payload
}

fn apply_actuator_payload_operation(
    base: &[u8],
    operation: &ActuatorPayloadOperation,
) -> Result<Vec<u8>> {
    let mut payload = base.to_vec();
    match operation.kind.as_str() {
        "resize" => {
            let size = operation.size.ok_or_else(|| {
                io::Error::other("actuator resize operation is missing size")
            })?;
            payload.resize(size, 0);
        }
        "append" => {
            let bytes = decode_hex(
                operation
                    .bytes_hex
                    .as_deref()
                    .ok_or_else(|| io::Error::other("append operation is missing bytes_hex"))?,
            )?;
            payload.extend_from_slice(&bytes);
        }
        "write" => {
            apply_actuator_payload_write(
                &mut payload,
                operation.offset.ok_or_else(|| {
                    io::Error::other("write operation is missing offset")
                })?,
                operation.bytes_hex.as_deref().ok_or_else(|| {
                    io::Error::other("write operation is missing bytes_hex")
                })?,
            )?;
        }
        "writes" => {
            let writes = operation.writes.as_deref().ok_or_else(|| {
                io::Error::other("writes operation is missing writes")
            })?;
            for write in writes {
                apply_actuator_payload_write(
                    &mut payload,
                    write.offset,
                    &write.bytes_hex,
                )?;
            }
        }
        kind => return fail(format!("unknown actuator payload operation '{kind}'")),
    }
    Ok(payload)
}

fn apply_actuator_payload_write(
    payload: &mut [u8],
    offset: usize,
    bytes_hex: &str,
) -> Result<()> {
    let bytes = decode_hex(bytes_hex)?;
    let end = offset
        .checked_add(bytes.len())
        .ok_or_else(|| io::Error::other("actuator write offset overflow"))?;
    let payload_len = payload.len();
    let destination = payload.get_mut(offset..end).ok_or_else(|| {
        io::Error::other(format!(
            "actuator write {offset}..{end} exceeds {payload_len} bytes"
        ))
    })?;
    destination.copy_from_slice(&bytes);
    Ok(())
}

fn apply_actuator_profile_mutation(
    profile: &mut actuator_outputs_contract::ActuatorOutputsProfile,
    case: &ActuatorProfileCase,
) -> Result<()> {
    if case.field == "reversible_mask" {
        profile.reversible_mask = parse_hex_u32(&case.value)?;
        return Ok(());
    }

    let slot_text = case
        .field
        .strip_prefix("disarmed_values[")
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| {
            io::Error::other(format!(
                "unknown actuator profile field '{}'",
                case.field
            ))
        })?;
    let slot = slot_text.parse::<usize>().map_err(|error| {
        io::Error::other(format!(
            "invalid actuator profile slot '{slot_text}': {error}"
        ))
    })?;
    let bytes = decode_hex(
        case.value
            .strip_prefix("f32-le:")
            .ok_or_else(|| io::Error::other("profile float is missing f32-le prefix"))?,
    )?;
    let bits = u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| io::Error::other("profile float must be exactly four bytes"))?,
    );
    let value = profile.disarmed_values.get_mut(slot).ok_or_else(|| {
        io::Error::other(format!("actuator profile slot {slot} is out of range"))
    })?;
    *value = f32::from_bits(bits);
    Ok(())
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return fail(format!("hex value '{value}' has odd length"));
    }
    (0..value.len())
        .step_by(2)
        .map(|offset| {
            u8::from_str_radix(&value[offset..offset + 2], 16)
                .map_err(|error| io::Error::other(format!("invalid hex value '{value}': {error}")).into())
        })
        .collect()
}

fn parse_hex_u32(value: &str) -> Result<u32> {
    u32::from_str_radix(
        value
            .strip_prefix("0x")
            .ok_or_else(|| io::Error::other("u32 hex value is missing 0x prefix"))?,
        16,
    )
    .map_err(|error| io::Error::other(format!("invalid u32 hex value '{value}': {error}")).into())
}

fn exercise_c_actuator_output_manifest(
    c_root: &Path,
    validator: &Path,
) -> Result<()> {
    use actuator_outputs_contract::{
        ACTUATOR_OUTPUT_COUNT, ActuatorOutputsProfile,
    };

    let vector_dir = c_root.join(ACTUATOR_VECTOR_DIR);
    let manifest_path = vector_dir.join(ACTUATOR_VECTOR_MANIFEST);
    let manifest: ActuatorVectorManifest =
        serde_json::from_slice(&fs::read(&manifest_path)?).map_err(|error| {
            io::Error::other(format!("invalid {}: {error}", manifest_path.display()))
        })?;
    if manifest.format != ACTUATOR_VECTOR_FORMAT {
        return fail(format!(
            "{} has format '{}', expected '{}'",
            manifest_path.display(),
            manifest.format,
            ACTUATOR_VECTOR_FORMAT
        ));
    }

    let base_path = vector_dir.join(&manifest.base.file);
    let base = fs::read(&base_path)?;
    if base.len() != manifest.base.size || sha256_bytes_hex(&base) != manifest.base.sha256 {
        return fail(format!(
            "{} does not match the size and SHA-256 recorded in {}",
            base_path.display(),
            manifest_path.display()
        ));
    }

    let payload_path = c_root.join("actuator_outputs_payload_case.bin");
    let profile = ActuatorOutputsProfile::new(0x0000_000f, 0);
    for case in &manifest.payload_cases {
        println!("C actuator payload case: {}", case.name);
        let payload = apply_actuator_payload_operation(&base, &case.operation)?;
        fs::write(&payload_path, payload)?;
        run(Command::new(validator)
            .arg("--payload-case")
            .arg(&payload_path)
            .arg(&case.expected))?;
    }
    remove_file_if_exists(&payload_path)?;

    let profile_path = c_root.join("actuator_outputs_profile_case.bin");
    for case in &manifest.profile_cases {
        println!("C actuator profile case: {}", case.name);
        let mut mutated = profile;
        apply_actuator_profile_mutation(&mut mutated, case)?;
        let mut bytes = Vec::with_capacity(8 + ACTUATOR_OUTPUT_COUNT * 4);
        bytes.extend_from_slice(&mutated.logical_slot_mask.to_le_bytes());
        bytes.extend_from_slice(&mutated.reversible_mask.to_le_bytes());
        for value in mutated.disarmed_values {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        if bytes.len() != 8 + ACTUATOR_OUTPUT_COUNT * 4 {
            return fail("serialized actuator test profile has the wrong size");
        }
        fs::write(&profile_path, bytes)?;
        run(Command::new(validator)
            .arg("--profile-case")
            .arg(&profile_path)
            .arg(&case.expected))?;
    }
    remove_file_if_exists(&profile_path)?;

    Ok(())
}

fn sha256_bytes_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn actuator_outputs_error_name(
    error: &actuator_outputs_contract::ActuatorOutputsError,
) -> &'static str {
    use actuator_outputs_contract::ActuatorOutputsError;

    match error {
        ActuatorOutputsError::WrongLength { .. } => "wrong_length",
        ActuatorOutputsError::ReversibleMaskOutsideLogicalMask { .. } => {
            "reversible_mask_outside_logical_mask"
        }
        ActuatorOutputsError::InactiveProfileDisarmedValueNotZero { .. } => {
            "inactive_profile_disarmed_value_not_zero"
        }
        ActuatorOutputsError::NonFiniteProfileDisarmedValue { .. } => {
            "nonfinite_profile_disarmed_value"
        }
        ActuatorOutputsError::NegativeZeroProfileDisarmedValue { .. } => {
            "negative_zero_profile_disarmed_value"
        }
        ActuatorOutputsError::ProfileDisarmedValueOutOfRange { .. } => {
            "profile_disarmed_value_out_of_range"
        }
        ActuatorOutputsError::ActiveMaskMismatch { .. } => {
            "active_mask_mismatch"
        }
        ActuatorOutputsError::UnknownArmState { .. } => "unknown_arm_state",
        ActuatorOutputsError::UnknownCommandSource { .. } => {
            "unknown_command_source"
        }
        ActuatorOutputsError::UnknownTimeStatus { .. } => "unknown_time_status",
        ActuatorOutputsError::NonzeroPadding { .. } => "nonzero_padding",
        ActuatorOutputsError::NonFiniteOutput { .. } => "nonfinite_output",
        ActuatorOutputsError::NegativeZeroOutput { .. } => {
            "negative_zero_output"
        }
        ActuatorOutputsError::InactiveOutputNotZero { .. } => {
            "inactive_output_not_zero"
        }
        ActuatorOutputsError::OutputOutOfRange { .. } => "output_out_of_range",
        ActuatorOutputsError::InvalidArmSource { .. } => "invalid_arm_source",
        ActuatorOutputsError::ActuatorTestNotAuthorized => {
            "actuator_test_not_authorized"
        }
        ActuatorOutputsError::DisarmedOutputMismatch { .. } => {
            "disarmed_output_mismatch"
        }
    }
}

#[cfg(test)]
mod actuator_vector_tests {
    use super::*;

    #[test]
    fn materialized_vectors_match_projection_and_manifest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask manifest directory has repository parent");
        validate_actuator_output_vectors(root).unwrap();
    }
}
