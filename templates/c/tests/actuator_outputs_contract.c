#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <synapse/actuator_outputs_contract.h>

static void put_u32_le(uint8_t *bytes, uint32_t value) {
    bytes[0] = (uint8_t)value;
    bytes[1] = (uint8_t)(value >> 8U);
    bytes[2] = (uint8_t)(value >> 16U);
    bytes[3] = (uint8_t)(value >> 24U);
}

static void put_u64_le(uint8_t *bytes, uint64_t value) {
    size_t index;
    for (index = 0U; index < 8U; ++index) {
        bytes[index] = (uint8_t)(value >> (index * 8U));
    }
}

static void put_f32_le(uint8_t *payload, size_t slot, float value) {
    const size_t offset =
        SYNAPSE_ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET + slot * 4U;
    put_u32_le(payload + offset,
               synapse_actuator_outputs_f32_bits(value));
}

static void initialize_payload(uint8_t *payload, uint8_t arm_state,
                               uint8_t command_source) {
    memset(payload, 0, SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE);
    put_u64_le(payload + SYNAPSE_ACTUATOR_OUTPUTS_TIMESTAMP_OFFSET,
               UINT64_C(0x0102030405060708));
    put_u32_le(payload + SYNAPSE_ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET,
               UINT32_C(0x0000000f));
    payload[SYNAPSE_ACTUATOR_OUTPUTS_ARM_STATE_OFFSET] = arm_state;
    payload[SYNAPSE_ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET] =
        command_source;
    payload[SYNAPSE_ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET] = 1U;
}

static synapse_actuator_outputs_result_t validate(
    const uint8_t *payload,
    const synapse_actuator_outputs_profile_t *profile,
    bool test_authorized) {
    return synapse_actuator_outputs_validate(
        payload, SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE, profile,
        test_authorized);
}

static size_t load_bytes(const char *path, uint8_t *bytes,
                         size_t capacity) {
    FILE *stream = fopen(path, "rb");
    size_t size;

    assert(stream != NULL);
    size = fread(bytes, 1U, capacity, stream);
    assert(ferror(stream) == 0);
    assert(fgetc(stream) == EOF);
    assert(fclose(stream) == 0);
    return size;
}

static void load_vector(const char *path, uint8_t *payload) {
    assert(load_bytes(path, payload,
                      SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE) ==
           SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE);
}

static const char *result_code_name(
    synapse_actuator_outputs_result_code_t code) {
    switch (code) {
    case SYNAPSE_ACTUATOR_OUTPUTS_OK:
        return "ok";
    case SYNAPSE_ACTUATOR_OUTPUTS_NULL_ARGUMENT:
        return "null_argument";
    case SYNAPSE_ACTUATOR_OUTPUTS_WRONG_LENGTH:
        return "wrong_length";
    case SYNAPSE_ACTUATOR_OUTPUTS_REVERSIBLE_MASK_OUTSIDE_LOGICAL_MASK:
        return "reversible_mask_outside_logical_mask";
    case SYNAPSE_ACTUATOR_OUTPUTS_INACTIVE_PROFILE_DISARMED_VALUE_NOT_ZERO:
        return "inactive_profile_disarmed_value_not_zero";
    case SYNAPSE_ACTUATOR_OUTPUTS_NONFINITE_PROFILE_DISARMED_VALUE:
        return "nonfinite_profile_disarmed_value";
    case SYNAPSE_ACTUATOR_OUTPUTS_NEGATIVE_ZERO_PROFILE_DISARMED_VALUE:
        return "negative_zero_profile_disarmed_value";
    case SYNAPSE_ACTUATOR_OUTPUTS_PROFILE_DISARMED_VALUE_OUT_OF_RANGE:
        return "profile_disarmed_value_out_of_range";
    case SYNAPSE_ACTUATOR_OUTPUTS_ACTIVE_MASK_MISMATCH:
        return "active_mask_mismatch";
    case SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_ARM_STATE:
        return "unknown_arm_state";
    case SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_COMMAND_SOURCE:
        return "unknown_command_source";
    case SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_TIME_STATUS:
        return "unknown_time_status";
    case SYNAPSE_ACTUATOR_OUTPUTS_NONZERO_PADDING:
        return "nonzero_padding";
    case SYNAPSE_ACTUATOR_OUTPUTS_NONFINITE_OUTPUT:
        return "nonfinite_output";
    case SYNAPSE_ACTUATOR_OUTPUTS_NEGATIVE_ZERO_OUTPUT:
        return "negative_zero_output";
    case SYNAPSE_ACTUATOR_OUTPUTS_INACTIVE_OUTPUT_NOT_ZERO:
        return "inactive_output_not_zero";
    case SYNAPSE_ACTUATOR_OUTPUTS_OUTPUT_OUT_OF_RANGE:
        return "output_out_of_range";
    case SYNAPSE_ACTUATOR_OUTPUTS_INVALID_ARM_SOURCE:
        return "invalid_arm_source";
    case SYNAPSE_ACTUATOR_OUTPUTS_ACTUATOR_TEST_NOT_AUTHORIZED:
        return "actuator_test_not_authorized";
    case SYNAPSE_ACTUATOR_OUTPUTS_DISARMED_OUTPUT_MISMATCH:
        return "disarmed_output_mismatch";
    default:
        return "unknown_result_code";
    }
}

static int assert_expected_result(
    synapse_actuator_outputs_result_t result, const char *expected) {
    const char *actual = result_code_name(result.code);
    if (strcmp(actual, expected) != 0) {
        fprintf(stderr, "expected %s, received %s at slot %u\n",
                expected, actual, (unsigned int)result.slot);
        return 2;
    }
    return 0;
}

static synapse_actuator_outputs_profile_t tropic_profile(void) {
    synapse_actuator_outputs_profile_t profile = {0};
    profile.logical_slot_mask = UINT32_C(0x0000000f);
    return profile;
}

static int run_payload_case(const char *path, const char *expected) {
    synapse_actuator_outputs_profile_t profile = tropic_profile();
    uint8_t payload[256];
    const size_t size = load_bytes(path, payload, sizeof(payload));
    const synapse_actuator_outputs_result_t result =
        synapse_actuator_outputs_validate(payload, size, &profile, false);
    return assert_expected_result(result, expected);
}

static int run_profile_case(const char *path, const char *expected) {
    enum {
        TEST_PROFILE_SIZE = 8U + SYNAPSE_ACTUATOR_OUTPUT_COUNT * 4U
    };
    synapse_actuator_outputs_profile_t profile = {0};
    uint8_t bytes[TEST_PROFILE_SIZE];
    synapse_actuator_outputs_result_t result;
    size_t slot;

    assert(load_bytes(path, bytes, sizeof(bytes)) == sizeof(bytes));
    profile.logical_slot_mask =
        synapse_actuator_outputs_read_u32_le(bytes);
    profile.reversible_mask =
        synapse_actuator_outputs_read_u32_le(bytes + 4U);
    for (slot = 0U; slot < SYNAPSE_ACTUATOR_OUTPUT_COUNT; ++slot) {
        const uint32_t bits = synapse_actuator_outputs_read_u32_le(
            bytes + 8U + slot * 4U);
        profile.disarmed_values[slot] =
            synapse_actuator_outputs_f32_from_bits(bits);
    }
    result = synapse_actuator_outputs_validate_profile(&profile);
    return assert_expected_result(result, expected);
}

int main(int argc, char **argv) {
    synapse_actuator_outputs_profile_t profile = tropic_profile();
    uint8_t payload[SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE];
    uint8_t trailing_payload[SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE + 1U];
    synapse_actuator_outputs_result_t result;

    assert(SYNAPSE_ACTUATOR_OUTPUT_COUNT == 32U);
    assert(SYNAPSE_ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET + 31U * 4U ==
           136U);
    assert(SYNAPSE_ACTUATOR_OUTPUTS_ARM_STATE_OFFSET == 140U);
    assert(SYNAPSE_ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET == 141U);
    assert(SYNAPSE_ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET == 142U);
    assert(SYNAPSE_ACTUATOR_OUTPUTS_PADDING_OFFSET == 143U);
    assert(SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE == 144U);

    if (argc == 4 && strcmp(argv[1], "--payload-case") == 0) {
        return run_payload_case(argv[2], argv[3]);
    }
    if (argc == 4 && strcmp(argv[1], "--profile-case") == 0) {
        return run_profile_case(argv[2], argv[3]);
    }

    assert(argc == 2);
    load_vector(argv[1], payload);
    assert(memcmp(payload, (const uint8_t[]){8, 7, 6, 5, 4, 3, 2, 1},
                  8U) == 0);
    assert(memcmp(payload + 8U, (const uint8_t[]){15, 0, 0, 0},
                  4U) == 0);
    assert(memcmp(payload + 12U, (const uint8_t[]){0, 0, 128, 62},
                  4U) == 0);
    assert(memcmp(payload + 140U, (const uint8_t[]){1, 1, 1, 0},
                  4U) == 0);
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_OK);

    initialize_payload(
        payload, SYNAPSE_ACTUATOR_ARM_STATE_DISARMED,
        SYNAPSE_ACTUATOR_OUTPUT_SOURCE_NO_COMMAND);
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_OK);

    profile.disarmed_values[0] = 0.25F;
    put_f32_le(payload, 0U, 0.25F);
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_OK);
    initialize_payload(
        payload, SYNAPSE_ACTUATOR_ARM_STATE_DISARMED,
        SYNAPSE_ACTUATOR_OUTPUT_SOURCE_CONTROL_ALLOCATION);
    put_f32_le(payload, 0U, 0.25F);
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_OK);
    profile.disarmed_values[0] = 0.0F;

    profile.reversible_mask = 1U;
    profile.disarmed_values[0] = -0.25F;
    put_f32_le(payload, 0U, -0.25F);
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_OK);
    profile.reversible_mask = 0U;
    profile.disarmed_values[0] = 0.0F;

    initialize_payload(
        payload, SYNAPSE_ACTUATOR_ARM_STATE_DISARMED,
        SYNAPSE_ACTUATOR_OUTPUT_SOURCE_CONTROL_ALLOCATION);
    put_f32_le(payload, 0U, 0.25F);
    result = validate(payload, &profile, false);
    assert(result.code ==
           SYNAPSE_ACTUATOR_OUTPUTS_DISARMED_OUTPUT_MISMATCH);
    assert(result.slot == 0U);

    initialize_payload(
        payload, SYNAPSE_ACTUATOR_ARM_STATE_DISARMED,
        SYNAPSE_ACTUATOR_OUTPUT_SOURCE_ACTUATOR_TEST);
    put_f32_le(payload, 0U, 0.5F);
    assert(validate(payload, &profile, true).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_OK);
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_ACTUATOR_TEST_NOT_AUTHORIZED);

    initialize_payload(
        payload, SYNAPSE_ACTUATOR_ARM_STATE_DISARMED,
        SYNAPSE_ACTUATOR_OUTPUT_SOURCE_NO_COMMAND);
    assert(synapse_actuator_outputs_validate(
               payload, SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE - 1U,
               &profile, false)
               .code == SYNAPSE_ACTUATOR_OUTPUTS_WRONG_LENGTH);
    memcpy(trailing_payload, payload, SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE);
    trailing_payload[SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE] = 0U;
    assert(synapse_actuator_outputs_validate(
               trailing_payload, sizeof(trailing_payload), &profile, false)
               .code == SYNAPSE_ACTUATOR_OUTPUTS_WRONG_LENGTH);
    payload[SYNAPSE_ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET] = 3U;
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_ACTIVE_MASK_MISMATCH);

    initialize_payload(
        payload, SYNAPSE_ACTUATOR_ARM_STATE_DISARMED,
        SYNAPSE_ACTUATOR_OUTPUT_SOURCE_NO_COMMAND);
    payload[SYNAPSE_ACTUATOR_OUTPUTS_ARM_STATE_OFFSET] = 2U;
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_ARM_STATE);
    payload[SYNAPSE_ACTUATOR_OUTPUTS_ARM_STATE_OFFSET] =
        SYNAPSE_ACTUATOR_ARM_STATE_DISARMED;
    payload[SYNAPSE_ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET] = 3U;
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_COMMAND_SOURCE);
    payload[SYNAPSE_ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET] =
        SYNAPSE_ACTUATOR_OUTPUT_SOURCE_NO_COMMAND;
    payload[SYNAPSE_ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET] = 3U;
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_TIME_STATUS);
    payload[SYNAPSE_ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET] = 0U;
    payload[SYNAPSE_ACTUATOR_OUTPUTS_PADDING_OFFSET] = 1U;
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_NONZERO_PADDING);

    initialize_payload(
        payload, SYNAPSE_ACTUATOR_ARM_STATE_ARMED,
        SYNAPSE_ACTUATOR_OUTPUT_SOURCE_CONTROL_ALLOCATION);
    put_u32_le(payload + SYNAPSE_ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET,
               UINT32_C(0x80000000));
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_NEGATIVE_ZERO_OUTPUT);
    put_u32_le(payload + SYNAPSE_ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET,
               UINT32_C(0x7f800000));
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_NONFINITE_OUTPUT);
    put_u32_le(payload + SYNAPSE_ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET,
               UINT32_C(0x7fc00000));
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_NONFINITE_OUTPUT);
    put_f32_le(payload, 0U, -0.25F);
    assert(validate(payload, &profile, false).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_OUTPUT_OUT_OF_RANGE);
    put_f32_le(payload, 0U, 0.0F);
    put_f32_le(payload, 4U, 0.25F);
    result = validate(payload, &profile, false);
    assert(result.code ==
           SYNAPSE_ACTUATOR_OUTPUTS_INACTIVE_OUTPUT_NOT_ZERO);
    assert(result.slot == 4U);

    profile.reversible_mask = UINT32_C(0x00000010);
    assert(synapse_actuator_outputs_validate_profile(&profile).code ==
           SYNAPSE_ACTUATOR_OUTPUTS_REVERSIBLE_MASK_OUTSIDE_LOGICAL_MASK);
    profile.reversible_mask = 0U;
    profile.disarmed_values[0] = -0.0F;
    result = synapse_actuator_outputs_validate_profile(&profile);
    assert(result.code ==
           SYNAPSE_ACTUATOR_OUTPUTS_NEGATIVE_ZERO_PROFILE_DISARMED_VALUE);
    assert(result.slot == 0U);

    profile.disarmed_values[0] = synapse_actuator_outputs_f32_from_bits(
        UINT32_C(0x7f800000));
    result = synapse_actuator_outputs_validate_profile(&profile);
    assert(result.code ==
           SYNAPSE_ACTUATOR_OUTPUTS_NONFINITE_PROFILE_DISARMED_VALUE);
    assert(result.slot == 0U);

    profile.disarmed_values[0] = -0.25F;
    result = synapse_actuator_outputs_validate_profile(&profile);
    assert(result.code ==
           SYNAPSE_ACTUATOR_OUTPUTS_PROFILE_DISARMED_VALUE_OUT_OF_RANGE);
    assert(result.slot == 0U);

    profile.disarmed_values[0] = 0.0F;
    profile.disarmed_values[4] = 0.25F;
    result = synapse_actuator_outputs_validate_profile(&profile);
    assert(result.code ==
           SYNAPSE_ACTUATOR_OUTPUTS_INACTIVE_PROFILE_DISARMED_VALUE_NOT_ZERO);
    assert(result.slot == 4U);

    return 0;
}
