#ifndef SYNAPSE_MCAP_H
#define SYNAPSE_MCAP_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * A complete-write byte sink. Returning zero means every byte was accepted;
 * any other value permanently fails the writer. The sink may block because it
 * is called only by the storage/logger context, never by a control publisher.
 */
typedef int (*synapse_mcap_sink_write_fn)(void *context,
                                         const uint8_t *data, size_t size);
typedef int (*synapse_mcap_sink_flush_fn)(void *context);

typedef struct synapse_mcap_sink {
    synapse_mcap_sink_write_fn write;
    synapse_mcap_sink_flush_fn flush;
    void *context;
} synapse_mcap_sink_t;

typedef enum synapse_mcap_time_basis {
    SYNAPSE_MCAP_TIME_MONOTONIC_BOOT = 0,
    SYNAPSE_MCAP_TIME_UNIX_EPOCH = 1,
    SYNAPSE_MCAP_TIME_CORRELATED = 2,
} synapse_mcap_time_basis_t;

typedef enum synapse_mcap_result {
    SYNAPSE_MCAP_OK = 0,
    SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT = -1,
    SYNAPSE_MCAP_ERROR_STATE = -2,
    SYNAPSE_MCAP_ERROR_OVERFLOW = -3,
    SYNAPSE_MCAP_ERROR_SINK = -4,
    SYNAPSE_MCAP_ERROR_TOO_MANY_SCHEMAS = -5,
    SYNAPSE_MCAP_ERROR_TOO_MANY_CHANNELS = -6,
} synapse_mcap_result_t;

/** Generated topic contract consumed by synapse_mcap_add_topic(). */
typedef struct synapse_mcap_topic {
    uint16_t topic_id;
    const char *schema_name;
    const uint8_t *schema_data;
    size_t schema_size;
    size_t payload_size;
    uint8_t fixed_layout;
} synapse_mcap_topic_t;

/** Caller-owned registered-schema state, reusable for multiple Channels. */
typedef struct synapse_mcap_schema {
    uint16_t id;
    uint16_t topic_id;
    size_t payload_size;
    uint8_t fixed_layout;
} synapse_mcap_schema_t;

/** Caller-owned per-channel sequence state. */
typedef struct synapse_mcap_channel {
    uint16_t id;
    uint16_t topic_id;
    uint32_t next_sequence;
    size_t payload_size;
    uint8_t fixed_layout;
} synapse_mcap_channel_t;

/**
 * Constant-memory streaming writer state. All memory is caller-owned. The
 * output buffer may be NULL only when output_capacity is zero; a zero-capacity
 * writer forwards every piece directly to the sink.
 */
typedef struct synapse_mcap_writer {
    synapse_mcap_sink_t sink;
    uint8_t *output_buffer;
    size_t output_capacity;
    size_t output_size;
    uint32_t next_schema_id;
    uint32_t next_channel_id;
    int error;
    uint8_t opened;
    uint8_t messages_started;
    uint8_t closed;
} synapse_mcap_writer_t;

/**
 * Start a `synapse/1` file and write its required Header and Metadata records.
 * session_id must be exactly 32 lowercase hexadecimal characters. library and
 * source must be non-empty UTF-8 MCAP strings.
 */
int synapse_mcap_open(synapse_mcap_writer_t *writer,
                      synapse_mcap_sink_t sink, uint8_t *output_buffer,
                      size_t output_capacity, const char *library,
                      const char *session_id, const char *source,
                      synapse_mcap_time_basis_t time_basis);

/** Write one selected topic Schema before the first Message. */
int synapse_mcap_add_schema(synapse_mcap_writer_t *writer,
                            const synapse_mcap_topic_t *topic,
                            synapse_mcap_schema_t *schema);

/**
 * Add a Channel for a registered Schema. Reuse one schema for every instance
 * or namespace of the same topic. channel_topic is the full canonical key.
 */
int synapse_mcap_add_channel(synapse_mcap_writer_t *writer,
                             const synapse_mcap_schema_t *schema,
                             const char *channel_topic,
                             synapse_mcap_channel_t *channel);

/** Convenience function that adds one Schema and one Channel. */
int synapse_mcap_add_topic(synapse_mcap_writer_t *writer,
                           const synapse_mcap_topic_t *topic,
                           const char *channel_topic,
                           synapse_mcap_channel_t *channel);

/** Write an already table-wrapped FlatBuffer payload. */
int synapse_mcap_write(synapse_mcap_writer_t *writer,
                       synapse_mcap_channel_t *channel, uint64_t log_time_ns,
                       uint64_t publish_time_ns, const void *data,
                       size_t size);

/**
 * Wrap and write one generated fixed-layout Synapse struct without allocating
 * or constructing an intermediate FlatBuffer. payload_size must exactly match
 * the generated topic catalog.
 */
int synapse_mcap_write_fixed(synapse_mcap_writer_t *writer,
                             synapse_mcap_channel_t *channel,
                             uint64_t log_time_ns, uint64_t publish_time_ns,
                             const void *payload, size_t payload_size);

/** Push buffered bytes through the sink and invoke its optional flush hook. */
int synapse_mcap_flush(synapse_mcap_writer_t *writer);

/** Write DataEnd, an empty-summary Footer, trailing magic, and flush. */
int synapse_mcap_close(synapse_mcap_writer_t *writer);

/** Return the first sticky writer error, or SYNAPSE_MCAP_OK. */
int synapse_mcap_error(const synapse_mcap_writer_t *writer);

#ifdef __cplusplus
}
#endif

#endif
