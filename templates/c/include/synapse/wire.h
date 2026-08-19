#ifndef SYNAPSE_WIRE_H
#define SYNAPSE_WIRE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SYNAPSE_WIRE_V1_MAGIC UINT32_C(0x53594e57)
#define SYNAPSE_WIRE_V1_VERSION 1U
#define SYNAPSE_WIRE_V1_HEADER_SIZE 44U
#define SYNAPSE_WIRE_V1_MAX_PAYLOAD_SIZE 1408U
#define SYNAPSE_WIRE_V1_DEFINED_FLAGS_MASK UINT16_C(0x0007)
#define SYNAPSE_WIRE_V1_RESERVED_FLAGS_MASK UINT16_C(0xfff8)

#define SYNAPSE_WIRE_FLAG_CAPTURE_TIME_GPTP_SYNCED UINT16_C(0x0001)
#define SYNAPSE_WIRE_FLAG_FINAL_PART UINT16_C(0x0002)
#define SYNAPSE_WIRE_FLAG_GATE_ACTIVE_AT_CAPTURE UINT16_C(0x0004)

typedef enum synapse_wire_status {
  SYNAPSE_WIRE_STATUS_OK = 0,
  SYNAPSE_WIRE_STATUS_INVALID_ARGUMENT = -1,
  SYNAPSE_WIRE_STATUS_BUFFER_TOO_SMALL = -2,
  SYNAPSE_WIRE_STATUS_INVALID_HEADER = -3,
  SYNAPSE_WIRE_STATUS_INVALID_POLICY = -4,
  SYNAPSE_WIRE_STATUS_INVALID_STATE = -5,
} synapse_wire_status_t;

/*
 * Stable synapse_wire v1 primary rejection values. These values are
 * telemetry and conformance identifiers, not implementation-local errors.
 */
typedef enum synapse_wire_rejection {
  SYNAPSE_WIRE_REJECTION_NONE = 0,
  SYNAPSE_WIRE_REJECTION_TRUNCATED_HEADER = 1,
  SYNAPSE_WIRE_REJECTION_BAD_MAGIC = 2,
  SYNAPSE_WIRE_REJECTION_BAD_VERSION = 3,
  SYNAPSE_WIRE_REJECTION_RESERVED_FLAGS = 4,
  SYNAPSE_WIRE_REJECTION_LENGTH = 5,
  SYNAPSE_WIRE_REJECTION_CARRIER_POLICY = 6,
  SYNAPSE_WIRE_REJECTION_BINDING = 7,
  SYNAPSE_WIRE_REJECTION_TOPIC = 8,
  SYNAPSE_WIRE_REJECTION_SCHEMA = 9,
  SYNAPSE_WIRE_REJECTION_SOURCE = 10,
  SYNAPSE_WIRE_REJECTION_SESSION = 11,
  SYNAPSE_WIRE_REJECTION_FLAG_SEMANTICS = 12,
  SYNAPSE_WIRE_REJECTION_PAYLOAD_STRUCTURE = 13,
  SYNAPSE_WIRE_REJECTION_SEQUENCE_DUPLICATE = 14,
  SYNAPSE_WIRE_REJECTION_SEQUENCE_OUT_OF_WINDOW = 15,
  SYNAPSE_WIRE_REJECTION_SEQUENCE_AMBIGUOUS = 16,
  SYNAPSE_WIRE_REJECTION_SEQUENCE_STALE_OR_BACKWARD = 17,
  SYNAPSE_WIRE_REJECTION_AGE = 18,
  SYNAPSE_WIRE_REJECTION_MALFORMED_HEALTH_PREFIX = 19,
  SYNAPSE_WIRE_REJECTION_PEER_COMPATIBILITY = 20,
} synapse_wire_rejection_t;

typedef enum synapse_wire_sequence_class {
  SYNAPSE_WIRE_SEQUENCE_UNINITIALIZED = 0,
  SYNAPSE_WIRE_SEQUENCE_NEXT,
  SYNAPSE_WIRE_SEQUENCE_FORWARD_GAP,
  SYNAPSE_WIRE_SEQUENCE_DUPLICATE,
  SYNAPSE_WIRE_SEQUENCE_OUT_OF_WINDOW,
  SYNAPSE_WIRE_SEQUENCE_AMBIGUOUS,
  SYNAPSE_WIRE_SEQUENCE_STALE_OR_BACKWARD,
  SYNAPSE_WIRE_SEQUENCE_RESYNCHRONIZED,
} synapse_wire_sequence_class_t;

typedef struct synapse_wire_header {
  uint32_t magic;
  uint16_t wire_protocol_version;
  uint16_t topic_id;
  uint32_t source_node_id;
  uint32_t sequence;
  uint64_t capture_timestamp_ns;
  uint64_t schema_set_id;
  uint16_t payload_length;
  uint16_t flags;
  uint64_t source_session_id;
} synapse_wire_header_t;

typedef struct synapse_wire_datagram_view {
  synapse_wire_header_t header;
  const uint8_t *payload;
  size_t payload_size;
} synapse_wire_datagram_view_t;

/*
 * Stream-specific values are supplied by generated deployment bindings. The
 * codec never embeds endpoint, topic, timing, or sequence policy values.
 */
typedef struct synapse_wire_validation_policy {
  uint16_t topic_id;
  uint64_t schema_set_id;
  uint32_t source_node_id;
  uint64_t source_session_id;
  uint16_t payload_size;
  uint16_t allowed_flags_mask;
  uint32_t sequence_window;
  uint32_t resync_run_length;
  uint64_t maximum_sample_age_ns;
  uint64_t future_skew_allowance_ns;
  bool freshness_enabled;
} synapse_wire_validation_policy_t;

/*
 * Results of checks owned by the network and payload adapters, plus receive
 * timestamps owned by the platform clock adapter. Set a validity field true
 * when its check passed or is not applicable to the selected stream.
 */
typedef struct synapse_wire_observations {
  bool carrier_policy_valid;
  bool binding_valid;
  bool header_flag_semantics_valid;
  bool payload_structure_valid;
  bool payload_flag_semantics_valid;
  bool receiver_gptp_synchronized;
  uint64_t receive_gptp_ns;
  uint64_t receive_monotonic_ns;
} synapse_wire_observations_t;

/* Caller-owned, per-source-session, per-binding sequence state. */
typedef struct synapse_wire_receiver_state {
  uint32_t last_accepted_sequence;
  uint32_t resync_candidate_sequence;
  uint32_t resync_candidate_count;
  bool has_last_accepted_sequence;
  bool has_resync_candidate;
} synapse_wire_receiver_state_t;

/*
 * Validation never mutates receiver state. state_transition_valid identifies
 * a fully evaluated transition that the caller may commit atomically.
 */
typedef struct synapse_wire_validation_result {
  synapse_wire_rejection_t rejection;
  synapse_wire_sequence_class_t sequence_class;
  synapse_wire_datagram_view_t datagram;
  synapse_wire_receiver_state_t next_state;
  uint32_t sequence_gap;
  uint64_t freshness_remaining_ns;
  uint64_t freshness_expiry_monotonic_ns;
  bool deliver;
  bool state_transition_valid;
} synapse_wire_validation_result_t;

/* Encode one complete UDP payload into caller-owned storage. */
synapse_wire_status_t
synapse_wire_encode(uint8_t *output, size_t output_capacity,
                    const synapse_wire_header_t *header, const void *payload,
                    size_t payload_size, size_t *output_size);

/* Decode and validate common framing stages 1 through 5. */
synapse_wire_status_t synapse_wire_decode(const void *datagram,
                                          size_t datagram_size,
                                          synapse_wire_datagram_view_t *view,
                                          synapse_wire_rejection_t *rejection);

/* Validate one ordinary-data datagram without mutating receiver state. */
synapse_wire_status_t
synapse_wire_validate(const void *datagram, size_t datagram_size,
                      const synapse_wire_validation_policy_t *policy,
                      const synapse_wire_observations_t *observations,
                      const synapse_wire_receiver_state_t *state,
                      synapse_wire_validation_result_t *result);

/* Commit a transition returned by synapse_wire_validate(). */
synapse_wire_status_t
synapse_wire_commit(synapse_wire_receiver_state_t *state,
                    const synapse_wire_validation_result_t *result);

bool synapse_wire_policy_is_valid(
    const synapse_wire_validation_policy_t *policy);
bool synapse_wire_state_is_valid(
    const synapse_wire_receiver_state_t *state,
    const synapse_wire_validation_policy_t *policy);
const char *synapse_wire_rejection_name(synapse_wire_rejection_t rejection);

#ifdef __cplusplus
}
#endif

#endif
