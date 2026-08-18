//! Validation for the fixed-layout ActuatorOutputsData wire payload.
//!
//! The schema fixes logical slot meaning and byte layout. The selected hardware
//! profile supplies the active logical-slot mask, the reversible-slot subset,
//! and all thirty-two declared disarmed values. Local authorization for ActuatorTest is an input from trusted local
//! state and must never be derived from this network payload.

use core::fmt;

pub const ACTUATOR_OUTPUT_COUNT: usize = 32;
pub const ACTUATOR_OUTPUTS_PAYLOAD_SIZE: usize = 144;
pub const ACTUATOR_OUTPUTS_TIMESTAMP_OFFSET: usize = 0;
pub const ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET: usize = 8;
pub const ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET: usize = 12;
pub const ACTUATOR_OUTPUTS_ARM_STATE_OFFSET: usize = 140;
pub const ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET: usize = 141;
pub const ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET: usize = 142;
pub const ACTUATOR_OUTPUTS_PADDING_OFFSET: usize = 143;

const ARM_STATE_DISARMED: u8 = 0;
const ARM_STATE_ARMED: u8 = 1;
const SOURCE_NO_COMMAND: u8 = 0;
const SOURCE_CONTROL_ALLOCATION: u8 = 1;
const SOURCE_ACTUATOR_TEST: u8 = 2;
const TIME_STATUS_MAX: u8 = 2;
const NEGATIVE_ZERO_BITS: u32 = 0x8000_0000;
const EXPONENT_MASK: u32 = 0x7f80_0000;

/// Hardware-profile facts needed to validate one actuator-output payload.
///
/// logical_slot_mask identifies which logical indices have physical mappings.
/// reversible_mask is a subset of logical_slot_mask. Each active disarmed value
/// must be finite and in the slot's declared range. Inactive disarmed values
/// are positive zero, and any active disarmed value that is zero is also
/// canonical positive zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActuatorOutputsProfile {
    pub logical_slot_mask: u32,
    pub reversible_mask: u32,
    pub disarmed_values: [f32; ACTUATOR_OUTPUT_COUNT],
}

impl ActuatorOutputsProfile {
    pub const fn new(logical_slot_mask: u32, reversible_mask: u32) -> Self {
        Self {
            logical_slot_mask,
            reversible_mask,
            disarmed_values: [0.0; ACTUATOR_OUTPUT_COUNT],
        }
    }
}

/// A reason an actuator-output profile or payload is not canonical.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActuatorOutputsError {
    WrongLength {
        expected: usize,
        actual: usize,
    },
    ReversibleMaskOutsideLogicalMask {
        logical_slot_mask: u32,
        reversible_mask: u32,
    },
    InactiveProfileDisarmedValueNotZero {
        slot: usize,
    },
    NonFiniteProfileDisarmedValue {
        slot: usize,
    },
    NegativeZeroProfileDisarmedValue {
        slot: usize,
    },
    ProfileDisarmedValueOutOfRange {
        slot: usize,
        reversible: bool,
    },
    ActiveMaskMismatch {
        expected: u32,
        actual: u32,
    },
    UnknownArmState {
        value: u8,
    },
    UnknownCommandSource {
        value: u8,
    },
    UnknownTimeStatus {
        value: u8,
    },
    NonzeroPadding {
        value: u8,
    },
    NonFiniteOutput {
        slot: usize,
    },
    NegativeZeroOutput {
        slot: usize,
    },
    InactiveOutputNotZero {
        slot: usize,
    },
    OutputOutOfRange {
        slot: usize,
        reversible: bool,
    },
    InvalidArmSource {
        arm_state: u8,
        command_source: u8,
    },
    ActuatorTestNotAuthorized,
    DisarmedOutputMismatch {
        slot: usize,
    },
}

impl fmt::Display for ActuatorOutputsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::WrongLength { expected, actual } => {
                write!(formatter, "payload is {actual} bytes, expected {expected}")
            }
            Self::ReversibleMaskOutsideLogicalMask {
                logical_slot_mask,
                reversible_mask,
            } => write!(
                formatter,
                "reversible mask {reversible_mask:#010x} is not a subset of logical-slot mask {logical_slot_mask:#010x}"
            ),
            Self::InactiveProfileDisarmedValueNotZero { slot } => write!(
                formatter,
                "inactive profile disarmed value for slot {slot} is not positive zero"
            ),
            Self::NonFiniteProfileDisarmedValue { slot } => {
                write!(
                    formatter,
                    "profile disarmed value for slot {slot} is not finite"
                )
            }
            Self::NegativeZeroProfileDisarmedValue { slot } => {
                write!(
                    formatter,
                    "profile disarmed value for slot {slot} is negative zero"
                )
            }
            Self::ProfileDisarmedValueOutOfRange { slot, reversible } => {
                let range = if reversible { "[-1, 1]" } else { "[0, 1]" };
                write!(
                    formatter,
                    "profile disarmed value for slot {slot} is outside inclusive range {range}"
                )
            }
            Self::ActiveMaskMismatch { expected, actual } => write!(
                formatter,
                "active mask {actual:#010x} does not equal profile mask {expected:#010x}"
            ),
            Self::UnknownArmState { value } => {
                write!(formatter, "unknown actuator arm-state value {value}")
            }
            Self::UnknownCommandSource { value } => {
                write!(formatter, "unknown actuator command-source value {value}")
            }
            Self::UnknownTimeStatus { value } => {
                write!(formatter, "unknown time-status value {value}")
            }
            Self::NonzeroPadding { value } => {
                write!(
                    formatter,
                    "canonical padding byte is {value}, expected zero"
                )
            }
            Self::NonFiniteOutput { slot } => {
                write!(formatter, "logical actuator slot {slot} is not finite")
            }
            Self::NegativeZeroOutput { slot } => {
                write!(formatter, "logical actuator slot {slot} is negative zero")
            }
            Self::InactiveOutputNotZero { slot } => {
                write!(
                    formatter,
                    "inactive logical actuator slot {slot} is not positive zero"
                )
            }
            Self::OutputOutOfRange { slot, reversible } => {
                let range = if reversible { "[-1, 1]" } else { "[0, 1]" };
                write!(
                    formatter,
                    "logical actuator slot {slot} is outside inclusive range {range}"
                )
            }
            Self::InvalidArmSource {
                arm_state,
                command_source,
            } => write!(
                formatter,
                "invalid actuator arm/source pair {arm_state}/{command_source}"
            ),
            Self::ActuatorTestNotAuthorized => {
                formatter.write_str("ActuatorTest is not locally authorized")
            }
            Self::DisarmedOutputMismatch { slot } => write!(
                formatter,
                "disarmed logical actuator slot {slot} does not equal its profile value"
            ),
        }
    }
}

impl std::error::Error for ActuatorOutputsError {}

/// Validate hardware-profile facts used by validate_actuator_outputs_payload.
pub fn validate_actuator_outputs_profile(
    profile: &ActuatorOutputsProfile,
) -> Result<(), ActuatorOutputsError> {
    if profile.reversible_mask & !profile.logical_slot_mask != 0 {
        return Err(ActuatorOutputsError::ReversibleMaskOutsideLogicalMask {
            logical_slot_mask: profile.logical_slot_mask,
            reversible_mask: profile.reversible_mask,
        });
    }

    for (slot, value) in profile.disarmed_values.iter().enumerate() {
        let bit = 1_u32 << slot;
        let value_bits = value.to_bits();
        if profile.logical_slot_mask & bit == 0 {
            if value_bits != 0 {
                return Err(ActuatorOutputsError::InactiveProfileDisarmedValueNotZero { slot });
            }
            continue;
        }
        if value_bits == NEGATIVE_ZERO_BITS {
            return Err(ActuatorOutputsError::NegativeZeroProfileDisarmedValue { slot });
        }
        if value_bits & EXPONENT_MASK == EXPONENT_MASK {
            return Err(ActuatorOutputsError::NonFiniteProfileDisarmedValue { slot });
        }

        let reversible = profile.reversible_mask & bit != 0;
        let in_range = if reversible {
            (-1.0..=1.0).contains(value)
        } else {
            (0.0..=1.0).contains(value)
        };
        if !in_range {
            return Err(ActuatorOutputsError::ProfileDisarmedValueOutOfRange { slot, reversible });
        }
    }

    Ok(())
}

/// Validate one bare 144-byte ActuatorOutputsData payload.
///
/// actuator_test_authorized must reflect trusted local authorization. Passing
/// true permits bounded ActuatorTest values that differ from the profile
/// disarmed values while arm_state remains Disarmed. No other disarmed source
/// may differ from the profile disarmed values.
pub fn validate_actuator_outputs_payload(
    payload: &[u8],
    profile: &ActuatorOutputsProfile,
    actuator_test_authorized: bool,
) -> Result<(), ActuatorOutputsError> {
    if payload.len() != ACTUATOR_OUTPUTS_PAYLOAD_SIZE {
        return Err(ActuatorOutputsError::WrongLength {
            expected: ACTUATOR_OUTPUTS_PAYLOAD_SIZE,
            actual: payload.len(),
        });
    }
    validate_actuator_outputs_profile(profile)?;

    let active_mask = read_u32_le(payload, ACTUATOR_OUTPUTS_ACTIVE_MASK_OFFSET);
    if active_mask != profile.logical_slot_mask {
        return Err(ActuatorOutputsError::ActiveMaskMismatch {
            expected: profile.logical_slot_mask,
            actual: active_mask,
        });
    }

    let arm_state = payload[ACTUATOR_OUTPUTS_ARM_STATE_OFFSET];
    if arm_state > ARM_STATE_ARMED {
        return Err(ActuatorOutputsError::UnknownArmState { value: arm_state });
    }
    let command_source = payload[ACTUATOR_OUTPUTS_COMMAND_SOURCE_OFFSET];
    if command_source > SOURCE_ACTUATOR_TEST {
        return Err(ActuatorOutputsError::UnknownCommandSource {
            value: command_source,
        });
    }
    let time_status = payload[ACTUATOR_OUTPUTS_TIME_STATUS_OFFSET];
    if time_status > TIME_STATUS_MAX {
        return Err(ActuatorOutputsError::UnknownTimeStatus { value: time_status });
    }
    let padding = payload[ACTUATOR_OUTPUTS_PADDING_OFFSET];
    if padding != 0 {
        return Err(ActuatorOutputsError::NonzeroPadding { value: padding });
    }

    for slot in 0..ACTUATOR_OUTPUT_COUNT {
        let bit = 1_u32 << slot;
        let value_bits = output_bits(payload, slot);
        if value_bits == NEGATIVE_ZERO_BITS {
            return Err(ActuatorOutputsError::NegativeZeroOutput { slot });
        }
        if value_bits & EXPONENT_MASK == EXPONENT_MASK {
            return Err(ActuatorOutputsError::NonFiniteOutput { slot });
        }
        if active_mask & bit == 0 {
            if value_bits != 0 {
                return Err(ActuatorOutputsError::InactiveOutputNotZero { slot });
            }
            continue;
        }

        let value = f32::from_bits(value_bits);
        let reversible = profile.reversible_mask & bit != 0;
        let in_range = if reversible {
            (-1.0..=1.0).contains(&value)
        } else {
            (0.0..=1.0).contains(&value)
        };
        if !in_range {
            return Err(ActuatorOutputsError::OutputOutOfRange { slot, reversible });
        }
    }

    let require_disarmed_values = match (arm_state, command_source) {
        (ARM_STATE_DISARMED, SOURCE_NO_COMMAND | SOURCE_CONTROL_ALLOCATION) => true,
        (ARM_STATE_DISARMED, SOURCE_ACTUATOR_TEST) if actuator_test_authorized => false,
        (ARM_STATE_DISARMED, SOURCE_ACTUATOR_TEST) => {
            return Err(ActuatorOutputsError::ActuatorTestNotAuthorized);
        }
        (ARM_STATE_ARMED, SOURCE_CONTROL_ALLOCATION) => false,
        _ => {
            return Err(ActuatorOutputsError::InvalidArmSource {
                arm_state,
                command_source,
            });
        }
    };

    if require_disarmed_values {
        for slot in 0..ACTUATOR_OUTPUT_COUNT {
            if output_bits(payload, slot) != profile.disarmed_values[slot].to_bits() {
                return Err(ActuatorOutputsError::DisarmedOutputMismatch { slot });
            }
        }
    }

    Ok(())
}

fn read_u32_le(payload: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        payload[offset..offset + 4]
            .try_into()
            .expect("validated actuator payload bounds"),
    )
}

fn output_bits(payload: &[u8], slot: usize) -> u32 {
    read_u32_le(payload, ACTUATOR_OUTPUTS_FIRST_OUTPUT_OFFSET + slot * 4)
}
