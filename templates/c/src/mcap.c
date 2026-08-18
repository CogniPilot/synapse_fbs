#include <synapse/mcap.h>

#include <limits.h>
#include <stdbool.h>
#include <string.h>

#include <synapse/topic_catalog.h>

enum {
    MCAP_OP_HEADER = 0x01,
    MCAP_OP_FOOTER = 0x02,
    MCAP_OP_SCHEMA = 0x03,
    MCAP_OP_CHANNEL = 0x04,
    MCAP_OP_MESSAGE = 0x05,
    MCAP_OP_METADATA = 0x0c,
    MCAP_OP_DATA_END = 0x0f,
};

static const uint8_t mcap_magic[] = {0x89, 'M', 'C', 'A', 'P', '0', '\r', '\n'};

static int fail(synapse_mcap_writer_t *writer, int error) {
    if (writer != NULL && writer->error == SYNAPSE_MCAP_OK) {
        writer->error = error;
    }
    return error;
}

static bool checked_add_u64(uint64_t *value, uint64_t addend) {
    if (UINT64_MAX - *value < addend) {
        return false;
    }
    *value += addend;
    return true;
}

static bool string_size(const char *value, uint32_t *size) {
    size_t length;
    if (value == NULL) {
        return false;
    }
    length = strlen(value);
    if (length > UINT32_MAX) {
        return false;
    }
    *size = (uint32_t)length;
    return true;
}

static void put_u16_le(uint8_t out[2], uint16_t value) {
    out[0] = (uint8_t)value;
    out[1] = (uint8_t)(value >> 8);
}

static void put_u32_le(uint8_t out[4], uint32_t value) {
    out[0] = (uint8_t)value;
    out[1] = (uint8_t)(value >> 8);
    out[2] = (uint8_t)(value >> 16);
    out[3] = (uint8_t)(value >> 24);
}

static void put_u64_le(uint8_t out[8], uint64_t value) {
    for (unsigned int i = 0; i < 8; ++i) {
        out[i] = (uint8_t)(value >> (8U * i));
    }
}

static int flush_buffer(synapse_mcap_writer_t *writer) {
    if (writer->error != SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    if (writer->output_size != 0U) {
        if (writer->sink.write(writer->sink.context, writer->output_buffer,
                               writer->output_size) != 0) {
            return fail(writer, SYNAPSE_MCAP_ERROR_SINK);
        }
        writer->output_size = 0U;
    }
    return SYNAPSE_MCAP_OK;
}

static int emit(synapse_mcap_writer_t *writer, const void *data, size_t size) {
    const uint8_t *input = (const uint8_t *)data;
    if (writer->error != SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    if (size != 0U && data == NULL) {
        return fail(writer, SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT);
    }
    if (writer->output_capacity == 0U) {
        if (size != 0U &&
            writer->sink.write(writer->sink.context, input, size) != 0) {
            return fail(writer, SYNAPSE_MCAP_ERROR_SINK);
        }
        return SYNAPSE_MCAP_OK;
    }

    while (size != 0U) {
        size_t available = writer->output_capacity - writer->output_size;
        size_t copied = size < available ? size : available;
        memcpy(writer->output_buffer + writer->output_size, input, copied);
        writer->output_size += copied;
        input += copied;
        size -= copied;
        if (writer->output_size == writer->output_capacity &&
            flush_buffer(writer) != SYNAPSE_MCAP_OK) {
            return writer->error;
        }
    }
    return SYNAPSE_MCAP_OK;
}

static int emit_u16(synapse_mcap_writer_t *writer, uint16_t value) {
    uint8_t bytes[2];
    put_u16_le(bytes, value);
    return emit(writer, bytes, sizeof(bytes));
}

static int emit_u32(synapse_mcap_writer_t *writer, uint32_t value) {
    uint8_t bytes[4];
    put_u32_le(bytes, value);
    return emit(writer, bytes, sizeof(bytes));
}

static int emit_u64(synapse_mcap_writer_t *writer, uint64_t value) {
    uint8_t bytes[8];
    put_u64_le(bytes, value);
    return emit(writer, bytes, sizeof(bytes));
}

static int emit_record_header(synapse_mcap_writer_t *writer, uint8_t opcode,
                              uint64_t length) {
    return emit(writer, &opcode, 1U) == SYNAPSE_MCAP_OK
               ? emit_u64(writer, length)
               : writer->error;
}

static int emit_string(synapse_mcap_writer_t *writer, const char *value,
                       uint32_t size) {
    return emit_u32(writer, size) == SYNAPSE_MCAP_OK
               ? emit(writer, value, size)
               : writer->error;
}

static bool lowercase_hex_128(const char *value) {
    if (value == NULL || strlen(value) != 32U) {
        return false;
    }
    for (size_t i = 0; i < 32U; ++i) {
        char c = value[i];
        if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'))) {
            return false;
        }
    }
    return true;
}

static const char *time_basis_string(synapse_mcap_time_basis_t basis) {
    switch (basis) {
    case SYNAPSE_MCAP_TIME_MONOTONIC_BOOT:
        return SYNAPSE_MCAP_TIME_BASIS_MONOTONIC_BOOT;
    case SYNAPSE_MCAP_TIME_UNIX_EPOCH:
        return SYNAPSE_MCAP_TIME_BASIS_UNIX_EPOCH;
    case SYNAPSE_MCAP_TIME_CORRELATED:
        return SYNAPSE_MCAP_TIME_BASIS_CORRELATED;
    default:
        return NULL;
    }
}

static int emit_map_pair(synapse_mcap_writer_t *writer, const char *key,
                         const char *value) {
    uint32_t key_size;
    uint32_t value_size;
    if (!string_size(key, &key_size) || !string_size(value, &value_size)) {
        return fail(writer, SYNAPSE_MCAP_ERROR_OVERFLOW);
    }
    if (emit_string(writer, key, key_size) != SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    return emit_string(writer, value, value_size);
}

static bool map_pair_size(const char *key, const char *value, uint64_t *size) {
    uint32_t key_size;
    uint32_t value_size;
    return string_size(key, &key_size) && string_size(value, &value_size) &&
           checked_add_u64(size, 4U + key_size) &&
           checked_add_u64(size, 4U + value_size);
}

static int write_header(synapse_mcap_writer_t *writer, const char *library) {
    uint32_t profile_size;
    uint32_t library_size;
    uint64_t body_size = 0;
    if (!string_size(SYNAPSE_MCAP_PROFILE, &profile_size) ||
        !string_size(library, &library_size) || library_size == 0U ||
        !checked_add_u64(&body_size, 4U + profile_size) ||
        !checked_add_u64(&body_size, 4U + library_size)) {
        return fail(writer, SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT);
    }
    if (emit_record_header(writer, MCAP_OP_HEADER, body_size) !=
            SYNAPSE_MCAP_OK ||
        emit_string(writer, SYNAPSE_MCAP_PROFILE, profile_size) !=
            SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    return emit_string(writer, library, library_size);
}

static int write_metadata(synapse_mcap_writer_t *writer,
                          const char *session_id, const char *source,
                          const char *time_basis) {
    uint32_t name_size;
    uint32_t map_size;
    uint64_t map_size_64 = 0;
    uint64_t body_size;
    if (!string_size(SYNAPSE_MCAP_METADATA_NAME, &name_size) ||
        !map_pair_size(SYNAPSE_MCAP_SCHEMA_SET_HASH_KEY,
                       SYNAPSE_LEGACY_SCHEMA_SET_HASH_128, &map_size_64) ||
        !map_pair_size(SYNAPSE_MCAP_SCHEMA_SET_IDENTITY_KEY,
                       SYNAPSE_SCHEMA_SET_IDENTITY, &map_size_64) ||
        !map_pair_size(SYNAPSE_MCAP_SCHEMA_PACKAGE_CONTRACT_IDENTITY_KEY,
                       SYNAPSE_SCHEMA_PACKAGE_CONTRACT_IDENTITY,
                       &map_size_64) ||
        !map_pair_size(SYNAPSE_MCAP_SESSION_ID_KEY, session_id,
                       &map_size_64) ||
        !map_pair_size(SYNAPSE_MCAP_SOURCE_KEY, source, &map_size_64) ||
        !map_pair_size(SYNAPSE_MCAP_TIME_BASIS_KEY, time_basis,
                       &map_size_64) ||
        map_size_64 > UINT32_MAX) {
        return fail(writer, SYNAPSE_MCAP_ERROR_OVERFLOW);
    }
    map_size = (uint32_t)map_size_64;
    body_size = 4U + name_size + 4U + map_size;
    if (emit_record_header(writer, MCAP_OP_METADATA, body_size) !=
            SYNAPSE_MCAP_OK ||
        emit_string(writer, SYNAPSE_MCAP_METADATA_NAME, name_size) !=
            SYNAPSE_MCAP_OK ||
        emit_u32(writer, map_size) != SYNAPSE_MCAP_OK ||
        emit_map_pair(writer, SYNAPSE_MCAP_SCHEMA_SET_HASH_KEY,
                      SYNAPSE_LEGACY_SCHEMA_SET_HASH_128) != SYNAPSE_MCAP_OK ||
        emit_map_pair(writer, SYNAPSE_MCAP_SCHEMA_SET_IDENTITY_KEY,
                      SYNAPSE_SCHEMA_SET_IDENTITY) != SYNAPSE_MCAP_OK ||
        emit_map_pair(writer,
                      SYNAPSE_MCAP_SCHEMA_PACKAGE_CONTRACT_IDENTITY_KEY,
                      SYNAPSE_SCHEMA_PACKAGE_CONTRACT_IDENTITY) !=
            SYNAPSE_MCAP_OK ||
        emit_map_pair(writer, SYNAPSE_MCAP_SESSION_ID_KEY, session_id) !=
            SYNAPSE_MCAP_OK ||
        emit_map_pair(writer, SYNAPSE_MCAP_SOURCE_KEY, source) !=
            SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    return emit_map_pair(writer, SYNAPSE_MCAP_TIME_BASIS_KEY, time_basis);
}

int synapse_mcap_open(synapse_mcap_writer_t *writer,
                      synapse_mcap_sink_t sink, uint8_t *output_buffer,
                      size_t output_capacity, const char *library,
                      const char *session_id, const char *source,
                      synapse_mcap_time_basis_t time_basis) {
    const char *basis = time_basis_string(time_basis);
    if (writer == NULL || sink.write == NULL || library == NULL ||
        library[0] == '\0' ||
        source == NULL || source[0] == '\0' || basis == NULL ||
        !lowercase_hex_128(session_id) ||
        (output_capacity != 0U && output_buffer == NULL)) {
        return SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT;
    }
    memset(writer, 0, sizeof(*writer));
    writer->sink = sink;
    writer->output_buffer = output_buffer;
    writer->output_capacity = output_capacity;
    writer->next_schema_id = 1U;
    writer->next_channel_id = 0U;
    writer->opened = 1U;
    if (emit(writer, mcap_magic, sizeof(mcap_magic)) != SYNAPSE_MCAP_OK ||
        write_header(writer, library) != SYNAPSE_MCAP_OK ||
        write_metadata(writer, session_id, source, basis) != SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    return SYNAPSE_MCAP_OK;
}

static size_t u16_decimal(char out[5], uint16_t value) {
    char reverse[5];
    size_t size = 0;
    do {
        reverse[size++] = (char)('0' + value % 10U);
        value = (uint16_t)(value / 10U);
    } while (value != 0U);
    for (size_t i = 0; i < size; ++i) {
        out[i] = reverse[size - i - 1U];
    }
    return size;
}

int synapse_mcap_add_schema(synapse_mcap_writer_t *writer,
                            const synapse_mcap_topic_t *topic,
                            synapse_mcap_schema_t *schema) {
    uint32_t schema_name_size;
    uint32_t schema_encoding_size;
    uint64_t body_size;
    uint16_t schema_id;

    if (writer == NULL || topic == NULL || schema == NULL ||
        topic->schema_name == NULL || topic->schema_data == NULL ||
        topic->schema_size == 0U || topic->schema_size > UINT32_MAX) {
        return writer == NULL ? SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT
                              : fail(writer, SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT);
    }
    if (!writer->opened || writer->closed || writer->messages_started) {
        return fail(writer, SYNAPSE_MCAP_ERROR_STATE);
    }
    if (writer->error != SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    if (writer->next_schema_id == 0U || writer->next_schema_id > UINT16_MAX) {
        return fail(writer, SYNAPSE_MCAP_ERROR_TOO_MANY_SCHEMAS);
    }
    if (!string_size(topic->schema_name, &schema_name_size) ||
        !string_size(SYNAPSE_MCAP_SCHEMA_ENCODING, &schema_encoding_size)) {
        return fail(writer, SYNAPSE_MCAP_ERROR_OVERFLOW);
    }

    schema_id = (uint16_t)writer->next_schema_id++;
    body_size = 2U + 4U + schema_name_size + 4U + schema_encoding_size + 4U;
    if (!checked_add_u64(&body_size, topic->schema_size) ||
        emit_record_header(writer, MCAP_OP_SCHEMA, body_size) !=
            SYNAPSE_MCAP_OK ||
        emit_u16(writer, schema_id) != SYNAPSE_MCAP_OK ||
        emit_string(writer, topic->schema_name, schema_name_size) !=
            SYNAPSE_MCAP_OK ||
        emit_string(writer, SYNAPSE_MCAP_SCHEMA_ENCODING,
                    schema_encoding_size) != SYNAPSE_MCAP_OK ||
        emit_u32(writer, (uint32_t)topic->schema_size) != SYNAPSE_MCAP_OK ||
        emit(writer, topic->schema_data, topic->schema_size) !=
            SYNAPSE_MCAP_OK) {
        return writer->error;
    }

    schema->id = schema_id;
    schema->topic_id = topic->topic_id;
    schema->payload_size = topic->payload_size;
    schema->fixed_layout = topic->fixed_layout;
    return SYNAPSE_MCAP_OK;
}

int synapse_mcap_add_channel(synapse_mcap_writer_t *writer,
                             const synapse_mcap_schema_t *schema,
                             const char *channel_topic,
                             synapse_mcap_channel_t *channel) {
    uint32_t channel_topic_size;
    uint32_t message_encoding_size;
    uint32_t topic_id_key_size;
    char topic_id[5];
    size_t topic_id_size;
    uint64_t body_size;
    uint64_t map_size;
    uint16_t channel_id;

    if (writer == NULL || schema == NULL || schema->id == 0U ||
        channel == NULL || channel_topic == NULL || channel_topic[0] == '\0') {
        return writer == NULL ? SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT
                              : fail(writer, SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT);
    }
    if (!writer->opened || writer->closed || writer->messages_started) {
        return fail(writer, SYNAPSE_MCAP_ERROR_STATE);
    }
    if (writer->error != SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    if (writer->next_channel_id > UINT16_MAX) {
        return fail(writer, SYNAPSE_MCAP_ERROR_TOO_MANY_CHANNELS);
    }
    if (!string_size(channel_topic, &channel_topic_size) ||
        !string_size(SYNAPSE_MCAP_MESSAGE_ENCODING,
                     &message_encoding_size) ||
        !string_size(SYNAPSE_MCAP_TOPIC_ID_KEY, &topic_id_key_size)) {
        return fail(writer, SYNAPSE_MCAP_ERROR_OVERFLOW);
    }

    channel_id = (uint16_t)writer->next_channel_id++;
    topic_id_size = u16_decimal(topic_id, schema->topic_id);
    map_size = 4U + topic_id_key_size + 4U + topic_id_size;
    body_size = 2U + 2U + 4U + channel_topic_size + 4U +
                message_encoding_size + 4U + map_size;
    if (emit_record_header(writer, MCAP_OP_CHANNEL, body_size) !=
            SYNAPSE_MCAP_OK ||
        emit_u16(writer, channel_id) != SYNAPSE_MCAP_OK ||
        emit_u16(writer, schema->id) != SYNAPSE_MCAP_OK ||
        emit_string(writer, channel_topic, channel_topic_size) !=
            SYNAPSE_MCAP_OK ||
        emit_string(writer, SYNAPSE_MCAP_MESSAGE_ENCODING,
                    message_encoding_size) != SYNAPSE_MCAP_OK ||
        emit_u32(writer, (uint32_t)map_size) != SYNAPSE_MCAP_OK ||
        emit_string(writer, SYNAPSE_MCAP_TOPIC_ID_KEY, topic_id_key_size) !=
            SYNAPSE_MCAP_OK ||
        emit_u32(writer, (uint32_t)topic_id_size) != SYNAPSE_MCAP_OK ||
        emit(writer, topic_id, topic_id_size) != SYNAPSE_MCAP_OK) {
        return writer->error;
    }

    channel->id = channel_id;
    channel->topic_id = schema->topic_id;
    channel->next_sequence = 0U;
    channel->payload_size = schema->payload_size;
    channel->fixed_layout = schema->fixed_layout;
    return SYNAPSE_MCAP_OK;
}

int synapse_mcap_add_topic(synapse_mcap_writer_t *writer,
                           const synapse_mcap_topic_t *topic,
                           const char *channel_topic,
                           synapse_mcap_channel_t *channel) {
    synapse_mcap_schema_t schema;
    int result = synapse_mcap_add_schema(writer, topic, &schema);
    if (result != SYNAPSE_MCAP_OK) {
        return result;
    }
    return synapse_mcap_add_channel(writer, &schema, channel_topic, channel);
}

static int start_message(synapse_mcap_writer_t *writer,
                         synapse_mcap_channel_t *channel, uint64_t log_time_ns,
                         uint64_t publish_time_ns, size_t data_size) {
    uint64_t body_size = 22U;
    uint32_t sequence;
    if (!checked_add_u64(&body_size, data_size)) {
        return fail(writer, SYNAPSE_MCAP_ERROR_OVERFLOW);
    }
    sequence = channel->next_sequence++;
    writer->messages_started = 1U;
    if (emit_record_header(writer, MCAP_OP_MESSAGE, body_size) !=
            SYNAPSE_MCAP_OK ||
        emit_u16(writer, channel->id) != SYNAPSE_MCAP_OK ||
        emit_u32(writer, sequence) != SYNAPSE_MCAP_OK ||
        emit_u64(writer, log_time_ns) != SYNAPSE_MCAP_OK ||
        emit_u64(writer, publish_time_ns) != SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    return SYNAPSE_MCAP_OK;
}

int synapse_mcap_write(synapse_mcap_writer_t *writer,
                       synapse_mcap_channel_t *channel, uint64_t log_time_ns,
                       uint64_t publish_time_ns, const void *data,
                       size_t size) {
    if (writer == NULL || channel == NULL || (size != 0U && data == NULL)) {
        return writer == NULL ? SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT
                              : fail(writer, SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT);
    }
    if (!writer->opened || writer->closed) {
        return fail(writer, SYNAPSE_MCAP_ERROR_STATE);
    }
    if (start_message(writer, channel, log_time_ns, publish_time_ns, size) !=
        SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    return emit(writer, data, size);
}

int synapse_mcap_write_fixed(synapse_mcap_writer_t *writer,
                             synapse_mcap_channel_t *channel,
                             uint64_t log_time_ns, uint64_t publish_time_ns,
                             const void *payload, size_t payload_size) {
    uint8_t table_offset[4];
    uint8_t vtable_offset[4];
    uint8_t vtable[6];
    size_t wrapped_size;
    uint32_t signed_vtable_offset;
    if (writer == NULL || channel == NULL || payload == NULL) {
        return writer == NULL ? SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT
                              : fail(writer, SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT);
    }
    if (!writer->opened || writer->closed) {
        return fail(writer, SYNAPSE_MCAP_ERROR_STATE);
    }
    if (!channel->fixed_layout || payload_size != channel->payload_size) {
        return fail(writer, SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT);
    }
    if (payload_size > UINT16_MAX - 4U || payload_size > UINT32_MAX - 14U) {
        return fail(writer, SYNAPSE_MCAP_ERROR_OVERFLOW);
    }
    wrapped_size = payload_size + 14U;
    if (start_message(writer, channel, log_time_ns, publish_time_ns,
                      wrapped_size) != SYNAPSE_MCAP_OK) {
        return writer->error;
    }

    put_u32_le(table_offset, 4U);
    signed_vtable_offset = (uint32_t)(0U - (uint32_t)(payload_size + 4U));
    put_u32_le(vtable_offset, signed_vtable_offset);
    put_u16_le(vtable, 6U);
    put_u16_le(vtable + 2, (uint16_t)(payload_size + 4U));
    put_u16_le(vtable + 4, 4U);
    if (emit(writer, table_offset, sizeof(table_offset)) != SYNAPSE_MCAP_OK ||
        emit(writer, vtable_offset, sizeof(vtable_offset)) != SYNAPSE_MCAP_OK ||
        emit(writer, payload, payload_size) != SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    return emit(writer, vtable, sizeof(vtable));
}

int synapse_mcap_flush(synapse_mcap_writer_t *writer) {
    if (writer == NULL) {
        return SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT;
    }
    if (flush_buffer(writer) != SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    if (writer->sink.flush != NULL &&
        writer->sink.flush(writer->sink.context) != 0) {
        return fail(writer, SYNAPSE_MCAP_ERROR_SINK);
    }
    return SYNAPSE_MCAP_OK;
}

int synapse_mcap_close(synapse_mcap_writer_t *writer) {
    static const uint8_t footer[20] = {0};
    if (writer == NULL) {
        return SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT;
    }
    if (!writer->opened || writer->closed) {
        return fail(writer, SYNAPSE_MCAP_ERROR_STATE);
    }
    if (writer->error != SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    if (emit_record_header(writer, MCAP_OP_DATA_END, 4U) !=
            SYNAPSE_MCAP_OK ||
        emit_u32(writer, 0U) != SYNAPSE_MCAP_OK ||
        emit_record_header(writer, MCAP_OP_FOOTER, sizeof(footer)) !=
            SYNAPSE_MCAP_OK ||
        emit(writer, footer, sizeof(footer)) != SYNAPSE_MCAP_OK ||
        emit(writer, mcap_magic, sizeof(mcap_magic)) != SYNAPSE_MCAP_OK) {
        return writer->error;
    }
    writer->closed = 1U;
    return synapse_mcap_flush(writer);
}

int synapse_mcap_error(const synapse_mcap_writer_t *writer) {
    return writer == NULL ? SYNAPSE_MCAP_ERROR_INVALID_ARGUMENT : writer->error;
}
