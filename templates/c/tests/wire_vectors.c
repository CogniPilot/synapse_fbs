#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <synapse/wire.h>

#define DATAGRAM_SIZE 76U
#define FLOW_PAYLOAD_SIZE 32U

_Static_assert(SYNAPSE_WIRE_REJECTION_NONE == 0, "none code");
_Static_assert(SYNAPSE_WIRE_REJECTION_TRUNCATED_HEADER == 1,
               "truncated_header code");
_Static_assert(SYNAPSE_WIRE_REJECTION_BAD_MAGIC == 2, "bad_magic code");
_Static_assert(SYNAPSE_WIRE_REJECTION_BAD_VERSION == 3, "bad_version code");
_Static_assert(SYNAPSE_WIRE_REJECTION_RESERVED_FLAGS == 4,
               "reserved_flags code");
_Static_assert(SYNAPSE_WIRE_REJECTION_LENGTH == 5, "length code");
_Static_assert(SYNAPSE_WIRE_REJECTION_CARRIER_POLICY == 6,
               "carrier_policy code");
_Static_assert(SYNAPSE_WIRE_REJECTION_BINDING == 7, "binding code");
_Static_assert(SYNAPSE_WIRE_REJECTION_TOPIC == 8, "topic code");
_Static_assert(SYNAPSE_WIRE_REJECTION_SCHEMA == 9, "schema code");
_Static_assert(SYNAPSE_WIRE_REJECTION_SOURCE == 10, "source code");
_Static_assert(SYNAPSE_WIRE_REJECTION_SESSION == 11, "session code");
_Static_assert(SYNAPSE_WIRE_REJECTION_FLAG_SEMANTICS == 12,
               "flag_semantics code");
_Static_assert(SYNAPSE_WIRE_REJECTION_PAYLOAD_STRUCTURE == 13,
               "payload_structure code");
_Static_assert(SYNAPSE_WIRE_REJECTION_SEQUENCE_DUPLICATE == 14,
               "sequence_duplicate code");
_Static_assert(SYNAPSE_WIRE_REJECTION_SEQUENCE_OUT_OF_WINDOW == 15,
               "sequence_out_of_window code");
_Static_assert(SYNAPSE_WIRE_REJECTION_SEQUENCE_AMBIGUOUS == 16,
               "sequence_ambiguous code");
_Static_assert(SYNAPSE_WIRE_REJECTION_SEQUENCE_STALE_OR_BACKWARD == 17,
               "sequence_stale_or_backward code");
_Static_assert(SYNAPSE_WIRE_REJECTION_AGE == 18, "age code");
_Static_assert(SYNAPSE_WIRE_REJECTION_MALFORMED_HEALTH_PREFIX == 19,
               "malformed_health_prefix code");
_Static_assert(SYNAPSE_WIRE_REJECTION_PEER_COMPATIBILITY == 20,
               "peer_compatibility code");

static uint32_t read_u32_le(const uint8_t bytes[4]) {
  return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8U) |
         ((uint32_t)bytes[2] << 16U) | ((uint32_t)bytes[3] << 24U);
}

static uint64_t read_u64_le(const uint8_t bytes[8]) {
  uint64_t value = 0U;
  size_t index;

  for (index = 0U; index < 8U; ++index) {
    value |= (uint64_t)bytes[index] << (index * 8U);
  }
  return value;
}

static void vector_path(char *path, size_t capacity, const char *directory,
                        const char *name) {
  const int written = snprintf(path, capacity, "%s/%s", directory, name);
  assert(written > 0);
  assert((size_t)written < capacity);
}

static void load_vector_path(const char *path,
                             uint8_t datagram[DATAGRAM_SIZE]) {
  FILE *input = fopen(path, "rb");

  assert(input != NULL);
  assert(fread(datagram, 1U, DATAGRAM_SIZE, input) == DATAGRAM_SIZE);
  assert(fgetc(input) == EOF);
  assert(fclose(input) == 0);
}

static void load_vector(const char *directory, const char *name,
                        uint8_t datagram[DATAGRAM_SIZE]) {
  char path[4096];

  vector_path(path, sizeof(path), directory, name);
  load_vector_path(path, datagram);
}

static synapse_wire_validation_policy_t flow_policy(void) {
  const synapse_wire_validation_policy_t policy = {
      .topic_id = 10U,
      .schema_set_id = UINT64_C(0x232721f0ee5b6c32),
      .source_node_id = UINT32_C(0x00000011),
      .source_session_id = UINT64_C(0x1122334455667788),
      .payload_size = FLOW_PAYLOAD_SIZE,
      .allowed_flags_mask = SYNAPSE_WIRE_FLAG_CAPTURE_TIME_GPTP_SYNCED,
      .sequence_window = 1024U,
      .resync_run_length = 2U,
      .maximum_sample_age_ns = UINT64_C(10000000),
      .future_skew_allowance_ns = 0U,
      .freshness_enabled = true,
  };
  return policy;
}

static synapse_wire_observations_t observations(void) {
  const synapse_wire_observations_t value = {
      .carrier_policy_valid = true,
      .binding_valid = true,
      .header_flag_semantics_valid = true,
      .payload_structure_valid = true,
      .payload_flag_semantics_valid = true,
      .receiver_gptp_synchronized = true,
      .receive_gptp_ns = UINT64_C(72623859791382856),
      .receive_monotonic_ns = UINT64_C(1000000000),
  };
  return value;
}

static synapse_wire_receiver_state_t receiver_state(uint32_t sequence) {
  const synapse_wire_receiver_state_t state = {
      .last_accepted_sequence = sequence,
      .has_last_accepted_sequence = true,
  };
  return state;
}

static synapse_wire_validation_result_t
validate_vector(const uint8_t datagram[DATAGRAM_SIZE],
                const synapse_wire_validation_policy_t *policy,
                const synapse_wire_observations_t *receive,
                const synapse_wire_receiver_state_t *state) {
  synapse_wire_validation_result_t result;

  assert(synapse_wire_validate(datagram, DATAGRAM_SIZE, policy, receive, state,
                               &result) == SYNAPSE_WIRE_STATUS_OK);
  return result;
}

static void expect_vector(const char *directory, const char *name,
                          synapse_wire_rejection_t expected,
                          synapse_wire_validation_policy_t policy,
                          synapse_wire_observations_t receive,
                          synapse_wire_receiver_state_t state) {
  synapse_wire_validation_result_t result;
  const synapse_wire_receiver_state_t before = state;
  uint8_t datagram[DATAGRAM_SIZE];

  load_vector(directory, name, datagram);
  result = validate_vector(datagram, &policy, &receive, &state);
  if (result.rejection != expected) {
    fprintf(stderr, "%s: expected %s (%u), received %s (%u)\n", name,
            synapse_wire_rejection_name(expected), (unsigned int)expected,
            synapse_wire_rejection_name(result.rejection),
            (unsigned int)result.rejection);
  }
  assert(result.rejection == expected);
  assert(memcmp(&state, &before, sizeof(state)) == 0);
}

static void test_positive(const char *directory) {
  const synapse_wire_validation_policy_t policy = flow_policy();
  const synapse_wire_observations_t receive = observations();
  synapse_wire_receiver_state_t state = receiver_state(UINT32_C(0x1020303f));
  synapse_wire_validation_result_t result;
  const synapse_wire_receiver_state_t before = state;
  uint8_t datagram[DATAGRAM_SIZE];
  uint8_t encoded[DATAGRAM_SIZE];
  size_t encoded_size = 0U;

  load_vector(directory, "positive.datagram.bin", datagram);
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_NONE);
  assert(result.deliver);
  assert(result.state_transition_valid);
  assert(result.sequence_class == SYNAPSE_WIRE_SEQUENCE_NEXT);
  assert(result.sequence_gap == 0U);
  assert(result.freshness_remaining_ns == UINT64_C(9000000));
  assert(result.freshness_expiry_monotonic_ns == UINT64_C(1009000000));
  assert(memcmp(&state, &before, sizeof(state)) == 0);

  assert(result.datagram.header.magic == SYNAPSE_WIRE_V1_MAGIC);
  assert(result.datagram.header.wire_protocol_version ==
         SYNAPSE_WIRE_V1_VERSION);
  assert(result.datagram.header.topic_id == 10U);
  assert(result.datagram.header.source_node_id == UINT32_C(0x11));
  assert(result.datagram.header.sequence == UINT32_C(0x10203040));
  assert(result.datagram.header.capture_timestamp_ns ==
         UINT64_C(0x0102030405060708));
  assert(result.datagram.header.schema_set_id == UINT64_C(0x232721f0ee5b6c32));
  assert(result.datagram.header.payload_length == FLOW_PAYLOAD_SIZE);
  assert(result.datagram.header.flags ==
         SYNAPSE_WIRE_FLAG_CAPTURE_TIME_GPTP_SYNCED);
  assert(result.datagram.header.source_session_id ==
         UINT64_C(0x1122334455667788));
  assert(result.datagram.payload_size == FLOW_PAYLOAD_SIZE);
  assert(read_u64_le(result.datagram.payload) == UINT64_C(0x0102030405060708));
  assert(read_u32_le(result.datagram.payload + 8U) == UINT32_C(0x3f800000));
  assert(read_u32_le(result.datagram.payload + 12U) == UINT32_C(0xc0000000));

  assert(synapse_wire_encode(encoded, sizeof(encoded), &result.datagram.header,
                             result.datagram.payload,
                             result.datagram.payload_size,
                             &encoded_size) == SYNAPSE_WIRE_STATUS_OK);
  assert(encoded_size == DATAGRAM_SIZE);
  assert(memcmp(encoded, datagram, DATAGRAM_SIZE) == 0);

  assert(synapse_wire_commit(&state, &result) == SYNAPSE_WIRE_STATUS_OK);
  assert(state.has_last_accepted_sequence);
  assert(state.last_accepted_sequence == UINT32_C(0x10203040));
  assert(!state.has_resync_candidate);
}

static void test_vector_outcomes(const char *directory) {
  synapse_wire_validation_policy_t policy = flow_policy();
  synapse_wire_observations_t receive = observations();
  synapse_wire_receiver_state_t state = receiver_state(UINT32_C(0x1020303f));

  expect_vector(directory, "positive.datagram.bin", SYNAPSE_WIRE_REJECTION_NONE,
                policy, receive, state);
  expect_vector(directory, "bad_version.datagram.bin",
                SYNAPSE_WIRE_REJECTION_BAD_VERSION, policy, receive, state);
  expect_vector(directory, "reserved_flags.datagram.bin",
                SYNAPSE_WIRE_REJECTION_RESERVED_FLAGS, policy, receive, state);
  expect_vector(directory, "bad_length.datagram.bin",
                SYNAPSE_WIRE_REJECTION_LENGTH, policy, receive, state);
  expect_vector(directory, "bad_schema.datagram.bin",
                SYNAPSE_WIRE_REJECTION_SCHEMA, policy, receive, state);
  expect_vector(directory, "bad_source.datagram.bin",
                SYNAPSE_WIRE_REJECTION_SOURCE, policy, receive, state);
  expect_vector(directory, "bad_session.datagram.bin",
                SYNAPSE_WIRE_REJECTION_SESSION, policy, receive, state);

  state = receiver_state(UINT32_C(0x10203040));
  expect_vector(directory, "bad_sequence.datagram.bin",
                SYNAPSE_WIRE_REJECTION_SEQUENCE_DUPLICATE, policy, receive,
                state);

  state = receiver_state(UINT32_C(0x1020303f));
  receive.receive_gptp_ns = UINT64_C(72623859800382857);
  expect_vector(directory, "bad_age.datagram.bin", SYNAPSE_WIRE_REJECTION_AGE,
                policy, receive, state);
}

static void test_external_positive(const char *path) {
  synapse_wire_validation_policy_t policy = flow_policy();
  const synapse_wire_observations_t receive = observations();
  const synapse_wire_receiver_state_t state =
      receiver_state(UINT32_C(0x1020303f));
  synapse_wire_validation_result_t result;
  synapse_wire_datagram_view_t view;
  synapse_wire_rejection_t rejection;
  uint8_t datagram[DATAGRAM_SIZE];

  load_vector_path(path, datagram);
  assert(synapse_wire_decode(datagram, sizeof(datagram), &view, &rejection) ==
         SYNAPSE_WIRE_STATUS_OK);
  assert(rejection == SYNAPSE_WIRE_REJECTION_NONE);
  policy.source_node_id = view.header.source_node_id;
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_NONE);
  assert(result.deliver);
}

static void test_primary_order_and_sequence(const char *directory) {
  synapse_wire_validation_policy_t policy = flow_policy();
  synapse_wire_observations_t receive = observations();
  synapse_wire_receiver_state_t state = receiver_state(UINT32_C(0x1020303f));
  synapse_wire_validation_result_t result;
  uint8_t datagram[DATAGRAM_SIZE];
  uint8_t resync_datagram[DATAGRAM_SIZE];
  synapse_wire_datagram_view_t view;
  synapse_wire_rejection_t rejection;
  size_t resync_datagram_size;

  load_vector(directory, "positive.datagram.bin", datagram);

  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_NONE);

  result = (synapse_wire_validation_result_t){0};
  assert(synapse_wire_validate(datagram, SYNAPSE_WIRE_V1_HEADER_SIZE - 1U,
                               &policy, &receive, &state,
                               &result) == SYNAPSE_WIRE_STATUS_OK);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_TRUNCATED_HEADER);

  datagram[0] ^= 1U;
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_BAD_MAGIC);
  datagram[0] ^= 1U;

  receive.carrier_policy_valid = false;
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_CARRIER_POLICY);
  receive = observations();
  receive.binding_valid = false;
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_BINDING);
  receive = observations();

  policy.topic_id = 11U;
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_TOPIC);
  policy = flow_policy();

  receive.header_flag_semantics_valid = false;
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_FLAG_SEMANTICS);
  receive = observations();

  receive.payload_structure_valid = false;
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_PAYLOAD_STRUCTURE);
  receive = observations();

  receive.payload_flag_semantics_valid = false;
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_FLAG_SEMANTICS);
  receive = observations();

  state = receiver_state(UINT32_C(0x10203040));
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_SEQUENCE_DUPLICATE);

  state = receiver_state(UINT32_C(0x90203040));
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_SEQUENCE_AMBIGUOUS);

  state = receiver_state(UINT32_C(0x10203041));
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_SEQUENCE_STALE_OR_BACKWARD);

  state = receiver_state(0U);
  result = validate_vector(datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_SEQUENCE_OUT_OF_WINDOW);
  assert(!result.deliver);
  assert(result.state_transition_valid);
  assert(result.next_state.has_resync_candidate);
  assert(result.next_state.resync_candidate_sequence == UINT32_C(0x10203040));
  assert(result.next_state.resync_candidate_count == 1U);

  assert(synapse_wire_commit(&state, &result) == SYNAPSE_WIRE_STATUS_OK);
  assert(state.has_resync_candidate);
  assert(synapse_wire_decode(datagram, sizeof(datagram), &view, &rejection) ==
         SYNAPSE_WIRE_STATUS_OK);
  assert(rejection == SYNAPSE_WIRE_REJECTION_NONE);
  view.header.sequence++;
  assert(synapse_wire_encode(resync_datagram, sizeof(resync_datagram),
                             &view.header, view.payload, view.payload_size,
                             &resync_datagram_size) == SYNAPSE_WIRE_STATUS_OK);
  assert(resync_datagram_size == DATAGRAM_SIZE);
  result = validate_vector(resync_datagram, &policy, &receive, &state);
  assert(result.rejection == SYNAPSE_WIRE_REJECTION_NONE);
  assert(result.deliver);
  assert(result.sequence_class == SYNAPSE_WIRE_SEQUENCE_RESYNCHRONIZED);
  assert(result.state_transition_valid);
  assert(synapse_wire_commit(&state, &result) == SYNAPSE_WIRE_STATUS_OK);
  assert(state.last_accepted_sequence == UINT32_C(0x10203041));
  assert(!state.has_resync_candidate);
  assert(state.resync_candidate_count == 0U);
}

static void test_api_errors(const char *directory) {
  synapse_wire_validation_policy_t policy = flow_policy();
  const synapse_wire_observations_t receive = observations();
  synapse_wire_receiver_state_t state = {0};
  synapse_wire_validation_result_t result;
  synapse_wire_datagram_view_t view;
  synapse_wire_rejection_t rejection;
  uint8_t datagram[DATAGRAM_SIZE];
  uint8_t encoded[DATAGRAM_SIZE];
  size_t encoded_size;

  load_vector(directory, "positive.datagram.bin", datagram);
  assert(synapse_wire_decode(datagram, sizeof(datagram), &view, &rejection) ==
         SYNAPSE_WIRE_STATUS_OK);
  assert(rejection == SYNAPSE_WIRE_REJECTION_NONE);
  assert(synapse_wire_encode(encoded, sizeof(encoded) - 1U, &view.header,
                             view.payload, view.payload_size, &encoded_size) ==
         SYNAPSE_WIRE_STATUS_BUFFER_TOO_SMALL);

  policy.sequence_window = 0U;
  assert(!synapse_wire_policy_is_valid(&policy));
  assert(synapse_wire_validate(datagram, sizeof(datagram), &policy, &receive,
                               &state,
                               &result) == SYNAPSE_WIRE_STATUS_INVALID_POLICY);
  policy = flow_policy();

  state.has_resync_candidate = true;
  state.resync_candidate_count = 1U;
  assert(!synapse_wire_state_is_valid(&state, &policy));
  assert(synapse_wire_validate(datagram, sizeof(datagram), &policy, &receive,
                               &state,
                               &result) == SYNAPSE_WIRE_STATUS_INVALID_STATE);
}

int main(int argc, char **argv) {
  assert(argc == 2 || argc == 3);
  assert(SYNAPSE_WIRE_V1_HEADER_SIZE == 44U);
  assert(SYNAPSE_WIRE_V1_MAX_PAYLOAD_SIZE == 1408U);

  test_positive(argv[1]);
  test_vector_outcomes(argv[1]);
  test_primary_order_and_sequence(argv[1]);
  test_api_errors(argv[1]);
  if (argc == 3) {
    test_external_positive(argv[2]);
  }

  puts("synapse_wire v1 C vectors passed");
  return 0;
}
