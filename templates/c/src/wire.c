#include <synapse/wire.h>

#include <limits.h>
#include <string.h>

static uint16_t read_be16(const uint8_t bytes[2]) {
  return ((uint16_t)bytes[0] << 8U) | (uint16_t)bytes[1];
}

static uint32_t read_be32(const uint8_t bytes[4]) {
  return ((uint32_t)bytes[0] << 24U) | ((uint32_t)bytes[1] << 16U) |
         ((uint32_t)bytes[2] << 8U) | (uint32_t)bytes[3];
}

static uint64_t read_be64(const uint8_t bytes[8]) {
  uint64_t value = 0U;
  size_t index;

  for (index = 0U; index < 8U; ++index) {
    value = (value << 8U) | bytes[index];
  }
  return value;
}

static void write_be16(uint8_t bytes[2], uint16_t value) {
  bytes[0] = (uint8_t)(value >> 8U);
  bytes[1] = (uint8_t)value;
}

static void write_be32(uint8_t bytes[4], uint32_t value) {
  bytes[0] = (uint8_t)(value >> 24U);
  bytes[1] = (uint8_t)(value >> 16U);
  bytes[2] = (uint8_t)(value >> 8U);
  bytes[3] = (uint8_t)value;
}

static void write_be64(uint8_t bytes[8], uint64_t value) {
  size_t index;

  for (index = 0U; index < 8U; ++index) {
    bytes[7U - index] = (uint8_t)(value >> (index * 8U));
  }
}

static synapse_wire_rejection_t
decode_header(const uint8_t *bytes, size_t size,
              synapse_wire_datagram_view_t *view) {
  if (size < SYNAPSE_WIRE_V1_HEADER_SIZE) {
    return SYNAPSE_WIRE_REJECTION_TRUNCATED_HEADER;
  }

  view->header.magic = read_be32(bytes);
  view->header.wire_protocol_version = read_be16(bytes + 4U);
  view->header.topic_id = read_be16(bytes + 6U);
  view->header.source_node_id = read_be32(bytes + 8U);
  view->header.sequence = read_be32(bytes + 12U);
  view->header.capture_timestamp_ns = read_be64(bytes + 16U);
  view->header.schema_set_id = read_be64(bytes + 24U);
  view->header.payload_length = read_be16(bytes + 32U);
  view->header.flags = read_be16(bytes + 34U);
  view->header.source_session_id = read_be64(bytes + 36U);

  if (view->header.magic != SYNAPSE_WIRE_V1_MAGIC) {
    return SYNAPSE_WIRE_REJECTION_BAD_MAGIC;
  }
  if (view->header.wire_protocol_version != SYNAPSE_WIRE_V1_VERSION) {
    return SYNAPSE_WIRE_REJECTION_BAD_VERSION;
  }
  if ((view->header.flags & SYNAPSE_WIRE_V1_RESERVED_FLAGS_MASK) != 0U) {
    return SYNAPSE_WIRE_REJECTION_RESERVED_FLAGS;
  }
  if (view->header.payload_length > SYNAPSE_WIRE_V1_MAX_PAYLOAD_SIZE ||
      size - SYNAPSE_WIRE_V1_HEADER_SIZE != view->header.payload_length) {
    return SYNAPSE_WIRE_REJECTION_LENGTH;
  }

  view->payload = bytes + SYNAPSE_WIRE_V1_HEADER_SIZE;
  view->payload_size = view->header.payload_length;
  return SYNAPSE_WIRE_REJECTION_NONE;
}

synapse_wire_status_t
synapse_wire_encode(uint8_t *output, size_t output_capacity,
                    const synapse_wire_header_t *header, const void *payload,
                    size_t payload_size, size_t *output_size) {
  size_t datagram_size;

  if (output == NULL || header == NULL || output_size == NULL ||
      (payload == NULL && payload_size != 0U)) {
    return SYNAPSE_WIRE_STATUS_INVALID_ARGUMENT;
  }
  if (header->magic != SYNAPSE_WIRE_V1_MAGIC ||
      header->wire_protocol_version != SYNAPSE_WIRE_V1_VERSION ||
      (header->flags & SYNAPSE_WIRE_V1_RESERVED_FLAGS_MASK) != 0U ||
      payload_size > SYNAPSE_WIRE_V1_MAX_PAYLOAD_SIZE ||
      payload_size > UINT16_MAX || header->payload_length != payload_size) {
    return SYNAPSE_WIRE_STATUS_INVALID_HEADER;
  }

  datagram_size = SYNAPSE_WIRE_V1_HEADER_SIZE + payload_size;
  if (output_capacity < datagram_size) {
    return SYNAPSE_WIRE_STATUS_BUFFER_TOO_SMALL;
  }

  if (payload_size != 0U) {
    memmove(output + SYNAPSE_WIRE_V1_HEADER_SIZE, payload, payload_size);
  }
  write_be32(output, header->magic);
  write_be16(output + 4U, header->wire_protocol_version);
  write_be16(output + 6U, header->topic_id);
  write_be32(output + 8U, header->source_node_id);
  write_be32(output + 12U, header->sequence);
  write_be64(output + 16U, header->capture_timestamp_ns);
  write_be64(output + 24U, header->schema_set_id);
  write_be16(output + 32U, header->payload_length);
  write_be16(output + 34U, header->flags);
  write_be64(output + 36U, header->source_session_id);
  *output_size = datagram_size;
  return SYNAPSE_WIRE_STATUS_OK;
}

synapse_wire_status_t synapse_wire_decode(const void *datagram,
                                          size_t datagram_size,
                                          synapse_wire_datagram_view_t *view,
                                          synapse_wire_rejection_t *rejection) {
  if (datagram == NULL || view == NULL || rejection == NULL) {
    return SYNAPSE_WIRE_STATUS_INVALID_ARGUMENT;
  }

  memset(view, 0, sizeof(*view));
  *rejection = decode_header((const uint8_t *)datagram, datagram_size, view);
  return SYNAPSE_WIRE_STATUS_OK;
}

bool synapse_wire_policy_is_valid(
    const synapse_wire_validation_policy_t *policy) {
  return policy != NULL && policy->source_session_id != 0U &&
         policy->payload_size <= SYNAPSE_WIRE_V1_MAX_PAYLOAD_SIZE &&
         (policy->allowed_flags_mask & ~SYNAPSE_WIRE_V1_DEFINED_FLAGS_MASK) ==
             0U &&
         policy->sequence_window >= 1U &&
         policy->sequence_window <= UINT32_C(0x7fffffff) &&
         policy->resync_run_length >= 2U;
}

bool synapse_wire_state_is_valid(
    const synapse_wire_receiver_state_t *state,
    const synapse_wire_validation_policy_t *policy) {
  if (state == NULL || !synapse_wire_policy_is_valid(policy)) {
    return false;
  }
  if (!state->has_last_accepted_sequence && state->has_resync_candidate) {
    return false;
  }
  if (!state->has_resync_candidate && (state->resync_candidate_sequence != 0U ||
                                       state->resync_candidate_count != 0U)) {
    return false;
  }
  if (state->has_resync_candidate &&
      (state->resync_candidate_count == 0U ||
       state->resync_candidate_count >= policy->resync_run_length)) {
    return false;
  }
  return true;
}

static bool header_flags_valid(const synapse_wire_header_t *header,
                               const synapse_wire_validation_policy_t *policy) {
  const bool capture_time_synchronized =
      (header->flags & SYNAPSE_WIRE_FLAG_CAPTURE_TIME_GPTP_SYNCED) != 0U;

  if ((header->flags & ~policy->allowed_flags_mask) != 0U) {
    return false;
  }
  return capture_time_synchronized ? header->capture_timestamp_ns != 0U
                                   : header->capture_timestamp_ns == 0U;
}

static synapse_wire_rejection_t
validate_binding(const synapse_wire_datagram_view_t *view,
                 const synapse_wire_validation_policy_t *policy,
                 const synapse_wire_observations_t *observations) {
  if (!observations->carrier_policy_valid) {
    return SYNAPSE_WIRE_REJECTION_CARRIER_POLICY;
  }
  if (!observations->binding_valid) {
    return SYNAPSE_WIRE_REJECTION_BINDING;
  }
  if (view->header.topic_id != policy->topic_id) {
    return SYNAPSE_WIRE_REJECTION_TOPIC;
  }
  if (view->header.schema_set_id != policy->schema_set_id) {
    return SYNAPSE_WIRE_REJECTION_SCHEMA;
  }
  if (view->header.source_node_id != policy->source_node_id) {
    return SYNAPSE_WIRE_REJECTION_SOURCE;
  }
  if (view->header.source_session_id != policy->source_session_id) {
    return SYNAPSE_WIRE_REJECTION_SESSION;
  }
  if (!observations->header_flag_semantics_valid ||
      !header_flags_valid(&view->header, policy)) {
    return SYNAPSE_WIRE_REJECTION_FLAG_SEMANTICS;
  }
  if (view->payload_size != policy->payload_size ||
      !observations->payload_structure_valid) {
    return SYNAPSE_WIRE_REJECTION_PAYLOAD_STRUCTURE;
  }
  if (!observations->payload_flag_semantics_valid) {
    return SYNAPSE_WIRE_REJECTION_FLAG_SEMANTICS;
  }
  return SYNAPSE_WIRE_REJECTION_NONE;
}

static synapse_wire_sequence_class_t
classify_sequence(uint32_t sequence, uint32_t baseline, uint32_t window,
                  synapse_wire_rejection_t *rejection, uint32_t *gap) {
  const uint32_t delta = sequence - baseline;

  *gap = 0U;
  if (delta == 0U) {
    *rejection = SYNAPSE_WIRE_REJECTION_SEQUENCE_DUPLICATE;
    return SYNAPSE_WIRE_SEQUENCE_DUPLICATE;
  }
  if (delta == 1U) {
    *rejection = SYNAPSE_WIRE_REJECTION_NONE;
    return SYNAPSE_WIRE_SEQUENCE_NEXT;
  }
  if (delta <= window) {
    *rejection = SYNAPSE_WIRE_REJECTION_NONE;
    *gap = delta - 1U;
    return SYNAPSE_WIRE_SEQUENCE_FORWARD_GAP;
  }
  if (delta < UINT32_C(0x80000000)) {
    *rejection = SYNAPSE_WIRE_REJECTION_SEQUENCE_OUT_OF_WINDOW;
    return SYNAPSE_WIRE_SEQUENCE_OUT_OF_WINDOW;
  }
  if (delta == UINT32_C(0x80000000)) {
    *rejection = SYNAPSE_WIRE_REJECTION_SEQUENCE_AMBIGUOUS;
    return SYNAPSE_WIRE_SEQUENCE_AMBIGUOUS;
  }
  *rejection = SYNAPSE_WIRE_REJECTION_SEQUENCE_STALE_OR_BACKWARD;
  return SYNAPSE_WIRE_SEQUENCE_STALE_OR_BACKWARD;
}

static void clear_candidate(synapse_wire_receiver_state_t *state) {
  state->has_resync_candidate = false;
  state->resync_candidate_sequence = 0U;
  state->resync_candidate_count = 0U;
}

static synapse_wire_rejection_t
evaluate_sequence(uint32_t sequence,
                  const synapse_wire_validation_policy_t *policy,
                  const synapse_wire_receiver_state_t *state,
                  synapse_wire_validation_result_t *result) {
  synapse_wire_rejection_t rejection;
  synapse_wire_sequence_class_t classification;
  uint32_t gap;

  result->next_state = *state;
  if (!state->has_last_accepted_sequence) {
    result->next_state.has_last_accepted_sequence = true;
    result->next_state.last_accepted_sequence = sequence;
    clear_candidate(&result->next_state);
    result->sequence_class = SYNAPSE_WIRE_SEQUENCE_UNINITIALIZED;
    result->state_transition_valid = true;
    return SYNAPSE_WIRE_REJECTION_NONE;
  }

  classification = classify_sequence(sequence, state->last_accepted_sequence,
                                     policy->sequence_window, &rejection, &gap);
  result->sequence_class = classification;
  result->sequence_gap = gap;

  if (rejection == SYNAPSE_WIRE_REJECTION_NONE) {
    result->next_state.last_accepted_sequence = sequence;
    clear_candidate(&result->next_state);
    result->state_transition_valid = true;
    return rejection;
  }

  if (classification == SYNAPSE_WIRE_SEQUENCE_AMBIGUOUS) {
    if (state->has_resync_candidate) {
      clear_candidate(&result->next_state);
      result->state_transition_valid = true;
    }
    return rejection;
  }

  if (!state->has_resync_candidate) {
    if (classification == SYNAPSE_WIRE_SEQUENCE_OUT_OF_WINDOW) {
      result->next_state.has_resync_candidate = true;
      result->next_state.resync_candidate_sequence = sequence;
      result->next_state.resync_candidate_count = 1U;
      result->state_transition_valid = true;
    }
    return rejection;
  }

  {
    synapse_wire_rejection_t candidate_rejection;
    uint32_t candidate_gap;
    const synapse_wire_sequence_class_t candidate_class = classify_sequence(
        sequence, state->resync_candidate_sequence, policy->sequence_window,
        &candidate_rejection, &candidate_gap);

    (void)candidate_rejection;
    (void)candidate_gap;
    if (candidate_class == SYNAPSE_WIRE_SEQUENCE_NEXT ||
        candidate_class == SYNAPSE_WIRE_SEQUENCE_FORWARD_GAP) {
      const uint32_t next_count = state->resync_candidate_count + 1U;
      if (next_count >= policy->resync_run_length) {
        result->next_state.last_accepted_sequence = sequence;
        clear_candidate(&result->next_state);
        result->sequence_class = SYNAPSE_WIRE_SEQUENCE_RESYNCHRONIZED;
        result->sequence_gap = 0U;
        result->state_transition_valid = true;
        return SYNAPSE_WIRE_REJECTION_NONE;
      }
      result->next_state.resync_candidate_sequence = sequence;
      result->next_state.resync_candidate_count = next_count;
      result->sequence_class = SYNAPSE_WIRE_SEQUENCE_OUT_OF_WINDOW;
      result->sequence_gap = 0U;
      result->state_transition_valid = true;
      return SYNAPSE_WIRE_REJECTION_SEQUENCE_OUT_OF_WINDOW;
    }
    if (candidate_class == SYNAPSE_WIRE_SEQUENCE_OUT_OF_WINDOW) {
      result->next_state.resync_candidate_sequence = sequence;
      result->next_state.resync_candidate_count = 1U;
      result->sequence_class = SYNAPSE_WIRE_SEQUENCE_OUT_OF_WINDOW;
      result->sequence_gap = 0U;
      result->state_transition_valid = true;
      return SYNAPSE_WIRE_REJECTION_SEQUENCE_OUT_OF_WINDOW;
    }

    clear_candidate(&result->next_state);
    result->state_transition_valid = true;
    return rejection;
  }
}

static synapse_wire_rejection_t
evaluate_freshness(const synapse_wire_header_t *header,
                   const synapse_wire_validation_policy_t *policy,
                   const synapse_wire_observations_t *observations,
                   synapse_wire_validation_result_t *result) {
  uint64_t remaining;

  if (!policy->freshness_enabled) {
    return SYNAPSE_WIRE_REJECTION_NONE;
  }

  remaining = policy->maximum_sample_age_ns;
  if (observations->receiver_gptp_synchronized &&
      (header->flags & SYNAPSE_WIRE_FLAG_CAPTURE_TIME_GPTP_SYNCED) != 0U) {
    if (header->capture_timestamp_ns > observations->receive_gptp_ns) {
      const uint64_t future_skew =
          header->capture_timestamp_ns - observations->receive_gptp_ns;
      if (future_skew > policy->future_skew_allowance_ns) {
        return SYNAPSE_WIRE_REJECTION_AGE;
      }
    } else {
      const uint64_t capture_age =
          observations->receive_gptp_ns - header->capture_timestamp_ns;
      if (capture_age > policy->maximum_sample_age_ns) {
        return SYNAPSE_WIRE_REJECTION_AGE;
      }
      remaining -= capture_age;
    }
  }

  if (UINT64_MAX - observations->receive_monotonic_ns < remaining) {
    return SYNAPSE_WIRE_REJECTION_AGE;
  }
  result->freshness_remaining_ns = remaining;
  result->freshness_expiry_monotonic_ns =
      observations->receive_monotonic_ns + remaining;
  return SYNAPSE_WIRE_REJECTION_NONE;
}

synapse_wire_status_t
synapse_wire_validate(const void *datagram, size_t datagram_size,
                      const synapse_wire_validation_policy_t *policy,
                      const synapse_wire_observations_t *observations,
                      const synapse_wire_receiver_state_t *state,
                      synapse_wire_validation_result_t *result) {
  synapse_wire_rejection_t rejection;
  synapse_wire_status_t status;

  if (datagram == NULL || policy == NULL || observations == NULL ||
      state == NULL || result == NULL) {
    return SYNAPSE_WIRE_STATUS_INVALID_ARGUMENT;
  }
  if (!synapse_wire_policy_is_valid(policy)) {
    return SYNAPSE_WIRE_STATUS_INVALID_POLICY;
  }
  if (!synapse_wire_state_is_valid(state, policy)) {
    return SYNAPSE_WIRE_STATUS_INVALID_STATE;
  }

  memset(result, 0, sizeof(*result));
  result->next_state = *state;
  status = synapse_wire_decode(datagram, datagram_size, &result->datagram,
                               &rejection);
  if (status != SYNAPSE_WIRE_STATUS_OK) {
    return status;
  }
  if (rejection == SYNAPSE_WIRE_REJECTION_NONE) {
    rejection = validate_binding(&result->datagram, policy, observations);
  }
  if (rejection != SYNAPSE_WIRE_REJECTION_NONE) {
    result->rejection = rejection;
    return SYNAPSE_WIRE_STATUS_OK;
  }

  rejection = evaluate_sequence(result->datagram.header.sequence, policy, state,
                                result);
  if (rejection != SYNAPSE_WIRE_REJECTION_NONE &&
      !result->state_transition_valid) {
    result->rejection = rejection;
    return SYNAPSE_WIRE_STATUS_OK;
  }

  {
    const synapse_wire_rejection_t freshness_rejection = evaluate_freshness(
        &result->datagram.header, policy, observations, result);
    if (freshness_rejection != SYNAPSE_WIRE_REJECTION_NONE) {
      result->state_transition_valid = false;
      result->next_state = *state;
      result->rejection = rejection == SYNAPSE_WIRE_REJECTION_NONE
                              ? freshness_rejection
                              : rejection;
      return SYNAPSE_WIRE_STATUS_OK;
    }
  }

  result->rejection = rejection;
  result->deliver = rejection == SYNAPSE_WIRE_REJECTION_NONE;
  return SYNAPSE_WIRE_STATUS_OK;
}

synapse_wire_status_t
synapse_wire_commit(synapse_wire_receiver_state_t *state,
                    const synapse_wire_validation_result_t *result) {
  if (state == NULL || result == NULL) {
    return SYNAPSE_WIRE_STATUS_INVALID_ARGUMENT;
  }
  if (!result->state_transition_valid) {
    return SYNAPSE_WIRE_STATUS_INVALID_STATE;
  }
  *state = result->next_state;
  return SYNAPSE_WIRE_STATUS_OK;
}

const char *synapse_wire_rejection_name(synapse_wire_rejection_t rejection) {
  switch (rejection) {
  case SYNAPSE_WIRE_REJECTION_NONE:
    return "none";
  case SYNAPSE_WIRE_REJECTION_TRUNCATED_HEADER:
    return "truncated_header";
  case SYNAPSE_WIRE_REJECTION_BAD_MAGIC:
    return "bad_magic";
  case SYNAPSE_WIRE_REJECTION_BAD_VERSION:
    return "bad_version";
  case SYNAPSE_WIRE_REJECTION_RESERVED_FLAGS:
    return "reserved_flags";
  case SYNAPSE_WIRE_REJECTION_LENGTH:
    return "length";
  case SYNAPSE_WIRE_REJECTION_CARRIER_POLICY:
    return "carrier_policy";
  case SYNAPSE_WIRE_REJECTION_BINDING:
    return "binding";
  case SYNAPSE_WIRE_REJECTION_TOPIC:
    return "topic";
  case SYNAPSE_WIRE_REJECTION_SCHEMA:
    return "schema";
  case SYNAPSE_WIRE_REJECTION_SOURCE:
    return "source";
  case SYNAPSE_WIRE_REJECTION_SESSION:
    return "session";
  case SYNAPSE_WIRE_REJECTION_FLAG_SEMANTICS:
    return "flag_semantics";
  case SYNAPSE_WIRE_REJECTION_PAYLOAD_STRUCTURE:
    return "payload_structure";
  case SYNAPSE_WIRE_REJECTION_SEQUENCE_DUPLICATE:
    return "sequence_duplicate";
  case SYNAPSE_WIRE_REJECTION_SEQUENCE_OUT_OF_WINDOW:
    return "sequence_out_of_window";
  case SYNAPSE_WIRE_REJECTION_SEQUENCE_AMBIGUOUS:
    return "sequence_ambiguous";
  case SYNAPSE_WIRE_REJECTION_SEQUENCE_STALE_OR_BACKWARD:
    return "sequence_stale_or_backward";
  case SYNAPSE_WIRE_REJECTION_AGE:
    return "age";
  case SYNAPSE_WIRE_REJECTION_MALFORMED_HEALTH_PREFIX:
    return "malformed_health_prefix";
  case SYNAPSE_WIRE_REJECTION_PEER_COMPATIBILITY:
    return "peer_compatibility";
  default:
    return "reserved";
  }
}
