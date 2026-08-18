#ifndef SYNAPSE_ACTUATOR_OUTPUTS_CONTRACT_H
#define SYNAPSE_ACTUATOR_OUTPUTS_CONTRACT_H

#include <stdbool.h>
#include <float.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SYNAPSE_ACTUATOR_OUTPUT_COUNT 32U
#define SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE 144U
#define SYNAPSE_ACTUATOR_OUTPUTS_TIMESTAMP_OFFSET 0U
#define SYNAPSE_ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET 8U
#define SYNAPSE_ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET 12U
#define SYNAPSE_ACTUATOR_OUTPUTS_ARM_STATE_OFFSET 140U
#define SYNAPSE_ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET 141U
#define SYNAPSE_ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET 142U
#define SYNAPSE_ACTUATOR_OUTPUTS_PADDING_OFFSET 143U
#define SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT UINT8_MAX

#if defined(__cplusplus)
static_assert(sizeof(float) == 4, "ActuatorOutputs requires a 32-bit float");
static_assert(FLT_RADIX == 2, "ActuatorOutputs requires binary floating point");
static_assert(FLT_MANT_DIG == 24, "ActuatorOutputs requires IEEE-754 binary32 precision");
static_assert(FLT_MAX_EXP == 128, "ActuatorOutputs requires IEEE-754 binary32 range");
#else
_Static_assert(sizeof(float) == 4, "ActuatorOutputs requires a 32-bit float");
_Static_assert(FLT_RADIX == 2, "ActuatorOutputs requires binary floating point");
_Static_assert(FLT_MANT_DIG == 24, "ActuatorOutputs requires IEEE-754 binary32 precision");
_Static_assert(FLT_MAX_EXP == 128, "ActuatorOutputs requires IEEE-754 binary32 range");
#endif

typedef struct synapse_actuator_outputs_profile {
    uint32_t logical_slot_mask;
    uint32_t reversible_mask;
    float disarmed_values[SYNAPSE_ACTUATOR_OUTPUT_COUNT];
} synapse_actuator_outputs_profile_t;

typedef enum synapse_actuator_outputs_result_code {
    SYNAPSE_ACTUATOR_OUTPUTS_OK = 0,
    SYNAPSE_ACTUATOR_OUTPUTS_NULL_ARGUMENT,
    SYNAPSE_ACTUATOR_OUTPUTS_WRONG_LENGTH,
    SYNAPSE_ACTUATOR_OUTPUTS_REVERSIBLE_MASK_OUTSIDE_LOGICAL_MASK,
    SYNAPSE_ACTUATOR_OUTPUTS_INACTIVE_PROFILE_DISARMED_VALUE_NOT_ZERO,
    SYNAPSE_ACTUATOR_OUTPUTS_NONFINITE_PROFILE_DISARMED_VALUE,
    SYNAPSE_ACTUATOR_OUTPUTS_NEGATIVE_ZERO_PROFILE_DISARMED_VALUE,
    SYNAPSE_ACTUATOR_OUTPUTS_PROFILE_DISARMED_VALUE_OUT_OF_RANGE,
    SYNAPSE_ACTUATOR_OUTPUTS_ACTIVE_MASK_MISMATCH,
    SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_ARM_STATE,
    SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_COMMAND_SOURCE,
    SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_TIME_STATUS,
    SYNAPSE_ACTUATOR_OUTPUTS_NONZERO_PADDING,
    SYNAPSE_ACTUATOR_OUTPUTS_NONFINITE_OUTPUT,
    SYNAPSE_ACTUATOR_OUTPUTS_NEGATIVE_ZERO_OUTPUT,
    SYNAPSE_ACTUATOR_OUTPUTS_INACTIVE_OUTPUT_NOT_ZERO,
    SYNAPSE_ACTUATOR_OUTPUTS_OUTPUT_OUT_OF_RANGE,
    SYNAPSE_ACTUATOR_OUTPUTS_INVALID_ARM_SOURCE,
    SYNAPSE_ACTUATOR_OUTPUTS_ACTUATOR_TEST_NOT_AUTHORIZED,
    SYNAPSE_ACTUATOR_OUTPUTS_DISARMED_OUTPUT_MISMATCH
} synapse_actuator_outputs_result_code_t;

typedef struct synapse_actuator_outputs_result {
    synapse_actuator_outputs_result_code_t code;
    uint8_t slot;
} synapse_actuator_outputs_result_t;

enum {
    SYNAPSE_ACTUATOR_ARM_STATE_DISARMED = 0,
    SYNAPSE_ACTUATOR_ARM_STATE_ARMED = 1,
    SYNAPSE_ACTUATOR_OUTPUT_SOURCE_NO_COMMAND = 0,
    SYNAPSE_ACTUATOR_OUTPUT_SOURCE_CONTROL_ALLOCATION = 1,
    SYNAPSE_ACTUATOR_OUTPUT_SOURCE_ACTUATOR_TEST = 2,
    SYNAPSE_ACTUATOR_TIME_STATUS_MAX = 2
};

static inline synapse_actuator_outputs_result_t
synapse_actuator_outputs_result(synapse_actuator_outputs_result_code_t code,
                                uint8_t slot) {
    synapse_actuator_outputs_result_t result = {code, slot};
    return result;
}

static inline uint32_t
synapse_actuator_outputs_read_u32_le(const uint8_t *bytes) {
    return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8U) |
           ((uint32_t)bytes[2] << 16U) | ((uint32_t)bytes[3] << 24U);
}

static inline uint32_t synapse_actuator_outputs_f32_bits(float value) {
    uint32_t bits = 0U;
    memcpy(&bits, &value, sizeof(bits));
    return bits;
}

static inline float synapse_actuator_outputs_f32_from_bits(uint32_t bits) {
    float value = 0.0F;
    memcpy(&value, &bits, sizeof(value));
    return value;
}

static inline uint32_t
synapse_actuator_outputs_output_bits(const uint8_t *payload, size_t slot) {
    const size_t offset =
        SYNAPSE_ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET + slot * 4U;
    return synapse_actuator_outputs_read_u32_le(payload + offset);
}

/*
 * Validate hardware-profile facts needed by the payload validator. The
 * reversible mask must be a subset of the logical-slot mapping mask. Active
 * disarmed values must be finite and in the declared slot range. Inactive
 * disarmed values, and any active disarmed value that is zero, use canonical
 * positive 0.0f.
 */
static inline synapse_actuator_outputs_result_t
synapse_actuator_outputs_validate_profile(
    const synapse_actuator_outputs_profile_t *profile) {
    size_t slot;

    if (profile == NULL) {
        return synapse_actuator_outputs_result(
            SYNAPSE_ACTUATOR_OUTPUTS_NULL_ARGUMENT,
            SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
    }
    if ((profile->reversible_mask & ~profile->logical_slot_mask) != 0U) {
        return synapse_actuator_outputs_result(
            SYNAPSE_ACTUATOR_OUTPUTS_REVERSIBLE_MASK_OUTSIDE_LOGICAL_MASK,
            SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
    }
    for (slot = 0U; slot < SYNAPSE_ACTUATOR_OUTPUT_COUNT; ++slot) {
        const uint32_t bit = UINT32_C(1) << slot;
        const uint32_t value_bits = synapse_actuator_outputs_f32_bits(
            profile->disarmed_values[slot]);
        bool reversible;
        float value;

        if ((profile->logical_slot_mask & bit) == 0U) {
            if (value_bits != 0U) {
                return synapse_actuator_outputs_result(
                    SYNAPSE_ACTUATOR_OUTPUTS_INACTIVE_PROFILE_DISARMED_VALUE_NOT_ZERO,
                    (uint8_t)slot);
            }
            continue;
        }
        if (value_bits == UINT32_C(0x80000000)) {
            return synapse_actuator_outputs_result(
                SYNAPSE_ACTUATOR_OUTPUTS_NEGATIVE_ZERO_PROFILE_DISARMED_VALUE,
                (uint8_t)slot);
        }
        if ((value_bits & UINT32_C(0x7f800000)) ==
            UINT32_C(0x7f800000)) {
            return synapse_actuator_outputs_result(
                SYNAPSE_ACTUATOR_OUTPUTS_NONFINITE_PROFILE_DISARMED_VALUE,
                (uint8_t)slot);
        }

        reversible = (profile->reversible_mask & bit) != 0U;
        value = profile->disarmed_values[slot];
        if ((reversible && (value < -1.0F || value > 1.0F)) ||
            (!reversible && (value < 0.0F || value > 1.0F))) {
            return synapse_actuator_outputs_result(
                SYNAPSE_ACTUATOR_OUTPUTS_PROFILE_DISARMED_VALUE_OUT_OF_RANGE,
                (uint8_t)slot);
        }
    }
    return synapse_actuator_outputs_result(
        SYNAPSE_ACTUATOR_OUTPUTS_OK, SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
}

/*
 * Validate one bare ActuatorOutputsData payload.
 *
 * actuator_test_authorized must come from trusted local state. A true value
 * permits bounded ActuatorTest output that differs from the profile disarmed
 * values while arm_state is Disarmed. No other Disarmed source may differ from
 * those values. Network input must never set or imply this authorization.
 */
static inline synapse_actuator_outputs_result_t
synapse_actuator_outputs_validate(
    const void *payload_data, size_t payload_size,
    const synapse_actuator_outputs_profile_t *profile,
    bool actuator_test_authorized) {
    const uint8_t *payload = (const uint8_t *)payload_data;
    synapse_actuator_outputs_result_t profile_result;
    uint32_t active_mask;
    uint8_t arm_state;
    uint8_t command_source;
    size_t slot;
    bool require_disarmed_values;

    if (payload == NULL || profile == NULL) {
        return synapse_actuator_outputs_result(
            SYNAPSE_ACTUATOR_OUTPUTS_NULL_ARGUMENT,
            SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
    }
    if (payload_size != SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE) {
        return synapse_actuator_outputs_result(
            SYNAPSE_ACTUATOR_OUTPUTS_WRONG_LENGTH,
            SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
    }

    profile_result = synapse_actuator_outputs_validate_profile(profile);
    if (profile_result.code != SYNAPSE_ACTUATOR_OUTPUTS_OK) {
        return profile_result;
    }

    active_mask = synapse_actuator_outputs_read_u32_le(
        payload + SYNAPSE_ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET);
    if (active_mask != profile->logical_slot_mask) {
        return synapse_actuator_outputs_result(
            SYNAPSE_ACTUATOR_OUTPUTS_ACTIVE_MASK_MISMATCH,
            SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
    }

    arm_state = payload[SYNAPSE_ACTUATOR_OUTPUTS_ARM_STATE_OFFSET];
    if (arm_state > SYNAPSE_ACTUATOR_ARM_STATE_ARMED) {
        return synapse_actuator_outputs_result(
            SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_ARM_STATE,
            SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
    }
    command_source =
        payload[SYNAPSE_ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET];
    if (command_source >
        SYNAPSE_ACTUATOR_OUTPUT_SOURCE_ACTUATOR_TEST) {
        return synapse_actuator_outputs_result(
            SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_COMMAND_SOURCE,
            SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
    }
    if (payload[SYNAPSE_ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET] >
        SYNAPSE_ACTUATOR_TIME_STATUS_MAX) {
        return synapse_actuator_outputs_result(
            SYNAPSE_ACTUATOR_OUTPUTS_UNKNOWN_TIME_STATUS,
            SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
    }
    if (payload[SYNAPSE_ACTUATOR_OUTPUTS_PADDING_OFFSET] != 0U) {
        return synapse_actuator_outputs_result(
            SYNAPSE_ACTUATOR_OUTPUTS_NONZERO_PADDING,
            SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
    }

    for (slot = 0U; slot < SYNAPSE_ACTUATOR_OUTPUT_COUNT; ++slot) {
        const uint32_t bit = UINT32_C(1) << slot;
        const uint32_t value_bits =
            synapse_actuator_outputs_output_bits(payload, slot);
        bool reversible;
        float value;

        if (value_bits == UINT32_C(0x80000000)) {
            return synapse_actuator_outputs_result(
                SYNAPSE_ACTUATOR_OUTPUTS_NEGATIVE_ZERO_OUTPUT,
                (uint8_t)slot);
        }
        if ((value_bits & UINT32_C(0x7f800000)) ==
            UINT32_C(0x7f800000)) {
            return synapse_actuator_outputs_result(
                SYNAPSE_ACTUATOR_OUTPUTS_NONFINITE_OUTPUT,
                (uint8_t)slot);
        }
        if ((active_mask & bit) == 0U) {
            if (value_bits != 0U) {
                return synapse_actuator_outputs_result(
                    SYNAPSE_ACTUATOR_OUTPUTS_INACTIVE_OUTPUT_NOT_ZERO,
                    (uint8_t)slot);
            }
            continue;
        }

        reversible = (profile->reversible_mask & bit) != 0U;
        value = synapse_actuator_outputs_f32_from_bits(value_bits);
        if ((reversible && (value < -1.0F || value > 1.0F)) ||
            (!reversible && (value < 0.0F || value > 1.0F))) {
            return synapse_actuator_outputs_result(
                SYNAPSE_ACTUATOR_OUTPUTS_OUTPUT_OUT_OF_RANGE,
                (uint8_t)slot);
        }
    }

    if (arm_state == SYNAPSE_ACTUATOR_ARM_STATE_DISARMED &&
        (command_source == SYNAPSE_ACTUATOR_OUTPUT_SOURCE_NO_COMMAND ||
         command_source ==
             SYNAPSE_ACTUATOR_OUTPUT_SOURCE_CONTROL_ALLOCATION)) {
        require_disarmed_values = true;
    } else if (arm_state == SYNAPSE_ACTUATOR_ARM_STATE_DISARMED &&
               command_source ==
                   SYNAPSE_ACTUATOR_OUTPUT_SOURCE_ACTUATOR_TEST) {
        if (!actuator_test_authorized) {
            return synapse_actuator_outputs_result(
                SYNAPSE_ACTUATOR_OUTPUTS_ACTUATOR_TEST_NOT_AUTHORIZED,
                SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
        }
        require_disarmed_values = false;
    } else if (arm_state == SYNAPSE_ACTUATOR_ARM_STATE_ARMED &&
               command_source ==
                   SYNAPSE_ACTUATOR_OUTPUT_SOURCE_CONTROL_ALLOCATION) {
        require_disarmed_values = false;
    } else {
        return synapse_actuator_outputs_result(
            SYNAPSE_ACTUATOR_OUTPUTS_INVALID_ARM_SOURCE,
            SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
    }

    if (require_disarmed_values) {
        for (slot = 0U; slot < SYNAPSE_ACTUATOR_OUTPUT_COUNT; ++slot) {
            if (synapse_actuator_outputs_output_bits(payload, slot) !=
                synapse_actuator_outputs_f32_bits(
                    profile->disarmed_values[slot])) {
                return synapse_actuator_outputs_result(
                    SYNAPSE_ACTUATOR_OUTPUTS_DISARMED_OUTPUT_MISMATCH,
                    (uint8_t)slot);
            }
        }
    }

    return synapse_actuator_outputs_result(
        SYNAPSE_ACTUATOR_OUTPUTS_OK, SYNAPSE_ACTUATOR_OUTPUTS_NO_SLOT);
}

#ifdef __cplusplus
}
#endif

#endif
