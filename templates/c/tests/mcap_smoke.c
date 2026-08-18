#include <assert.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include <synapse/actuator_outputs_contract.h>
#include <synapse/mcap.h>
#include <synapse/mcap_topics.h>
#include <synapse/topic_catalog.h>

#define LOG_TIME_NS UINT64_C(0x1112131415161718)
#define PUBLISH_TIME_NS UINT64_C(0x0102030405060708)
#define WRAPPED_PAYLOAD_SIZE (SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE + 14U)

typedef struct memory_sink {
    uint8_t data[131072];
    size_t size;
} memory_sink_t;

typedef struct byte_cursor {
    const uint8_t *data;
    size_t size;
    size_t position;
} byte_cursor_t;

typedef struct parsed_message {
    const uint8_t *data;
    size_t size;
} parsed_message_t;

static int memory_write(void *context, const uint8_t *data, size_t size) {
    memory_sink_t *sink = context;
    if (size > sizeof(sink->data) - sink->size) {
        return -1;
    }
    memcpy(sink->data + sink->size, data, size);
    sink->size += size;
    return 0;
}

static void cursor_require(const byte_cursor_t *cursor, size_t size) {
    assert(cursor->position <= cursor->size);
    assert(size <= cursor->size - cursor->position);
}

static uint8_t cursor_u8(byte_cursor_t *cursor) {
    cursor_require(cursor, 1U);
    return cursor->data[cursor->position++];
}

static uint16_t cursor_u16(byte_cursor_t *cursor) {
    uint16_t value;
    cursor_require(cursor, 2U);
    value = (uint16_t)cursor->data[cursor->position] |
            ((uint16_t)cursor->data[cursor->position + 1U] << 8U);
    cursor->position += 2U;
    return value;
}

static uint32_t cursor_u32(byte_cursor_t *cursor) {
    uint32_t value;
    cursor_require(cursor, 4U);
    value = (uint32_t)cursor->data[cursor->position] |
            ((uint32_t)cursor->data[cursor->position + 1U] << 8U) |
            ((uint32_t)cursor->data[cursor->position + 2U] << 16U) |
            ((uint32_t)cursor->data[cursor->position + 3U] << 24U);
    cursor->position += 4U;
    return value;
}

static uint64_t cursor_u64(byte_cursor_t *cursor) {
    uint64_t value = 0U;
    unsigned int shift;
    cursor_require(cursor, 8U);
    for (shift = 0U; shift < 64U; shift += 8U) {
        value |= (uint64_t)cursor->data[cursor->position++] << shift;
    }
    return value;
}

static byte_cursor_t cursor_take(byte_cursor_t *cursor, size_t size) {
    byte_cursor_t taken;
    cursor_require(cursor, size);
    taken.data = cursor->data + cursor->position;
    taken.size = size;
    taken.position = 0U;
    cursor->position += size;
    return taken;
}

static void cursor_string(byte_cursor_t *cursor, const char *expected) {
    const size_t expected_size = strlen(expected);
    const uint32_t actual_size = cursor_u32(cursor);
    assert(actual_size == expected_size);
    cursor_require(cursor, actual_size);
    assert(memcmp(cursor->data + cursor->position, expected, actual_size) == 0);
    cursor->position += actual_size;
}

static byte_cursor_t cursor_record(byte_cursor_t *cursor, uint8_t opcode) {
    uint64_t size;
    assert(cursor_u8(cursor) == opcode);
    size = cursor_u64(cursor);
    assert(size <= SIZE_MAX);
    return cursor_take(cursor, (size_t)size);
}

static void cursor_done(const byte_cursor_t *cursor) {
    assert(cursor->position == cursor->size);
}

static bool is_lowercase_hex(const char *value, size_t expected_size) {
    size_t index;
    if (strlen(value) != expected_size) {
        return false;
    }
    for (index = 0U; index < expected_size; ++index) {
        const char byte = value[index];
        if (!((byte >= '0' && byte <= '9') || (byte >= 'a' && byte <= 'f'))) {
            return false;
        }
    }
    return true;
}

static void load_payload(const char *path, uint8_t *payload) {
    FILE *input = fopen(path, "rb");
    assert(input != NULL);
    assert(fread(payload, 1U, SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE, input) ==
           SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE);
    assert(fgetc(input) == EOF);
    assert(fclose(input) == 0);
}

static void record_actuator(memory_sink_t *sink, const void *data, size_t size,
                            bool fixed_layout) {
    uint8_t output_buffer[257];
    synapse_mcap_writer_t writer;
    synapse_mcap_channel_t channel;
    synapse_mcap_topic_t topic = SYNAPSE_MCAP_TOPIC_ActuatorOutputs;

    assert(synapse_mcap_open(
               &writer,
               (synapse_mcap_sink_t){.write = memory_write,
                                     .flush = NULL,
                                     .context = sink},
               output_buffer, sizeof(output_buffer), "synapse-fbs-c-test/1",
               "0123456789abcdef0123456789abcdef", "test-vehicle",
               SYNAPSE_MCAP_TIME_MONOTONIC_BOOT) == SYNAPSE_MCAP_OK);
    assert(synapse_mcap_add_topic(&writer, &topic, "act_out", &channel) ==
           SYNAPSE_MCAP_OK);
    assert(channel.id == 0U && channel.topic_id == 47U &&
           channel.next_sequence == 0U && channel.payload_size == 144U &&
           channel.fixed_layout == 1U);
    if (fixed_layout) {
        assert(synapse_mcap_write_fixed(&writer, &channel, LOG_TIME_NS,
                                        PUBLISH_TIME_NS, data, size) ==
               SYNAPSE_MCAP_OK);
    } else {
        assert(synapse_mcap_write(&writer, &channel, LOG_TIME_NS,
                                  PUBLISH_TIME_NS, data, size) ==
               SYNAPSE_MCAP_OK);
    }
    assert(channel.next_sequence == 1U);
    assert(synapse_mcap_close(&writer) == SYNAPSE_MCAP_OK);
}

static parsed_message_t validate_recording(const memory_sink_t *sink,
                                           const uint8_t *payload) {
    static const uint8_t magic[] = {0x89, 'M', 'C', 'A', 'P', '0', '\r', '\n'};
    byte_cursor_t file = {sink->data, sink->size, 0U};
    byte_cursor_t record;
    byte_cursor_t map;
    parsed_message_t message;
    const synapse_topic_info_t *topic_info = synapse_topic_by_id(47U);
    synapse_mcap_topic_t mcap_topic = SYNAPSE_MCAP_TOPIC_ActuatorOutputs;

    assert(topic_info != NULL && topic_info->id == 47U &&
           strcmp(topic_info->name, "ActuatorOutputs") == 0 &&
           strcmp(topic_info->key, "act_out") == 0 &&
           strcmp(topic_info->mcap_schema_name,
                  "synapse.topic.ActuatorOutputs") == 0 &&
           strcmp(topic_info->wire_type,
                  "synapse.topic.ActuatorOutputsData") == 0 &&
           topic_info->payload_size == 144U && topic_info->fixed_layout);
    assert(is_lowercase_hex(topic_info->schema_hash, 64U));
    assert(is_lowercase_hex(SYNAPSE_SCHEMA_SET_IDENTITY, 64U));
    assert(is_lowercase_hex(SYNAPSE_SCHEMA_PACKAGE_CONTRACT_IDENTITY, 64U));
    assert(is_lowercase_hex(SYNAPSE_LEGACY_SCHEMA_SET_HASH_128, 32U));
    assert(mcap_topic.topic_id == 47U && mcap_topic.payload_size == 144U &&
           mcap_topic.fixed_layout == 1U &&
           strcmp(mcap_topic.schema_name,
                  "synapse.topic.ActuatorOutputs") == 0 &&
           mcap_topic.schema_data == synapse_bfbs_control &&
           mcap_topic.schema_size == synapse_bfbs_control_size);

    cursor_require(&file, sizeof(magic));
    assert(memcmp(file.data, magic, sizeof(magic)) == 0);
    file.position += sizeof(magic);

    record = cursor_record(&file, 0x01U);
    cursor_string(&record, SYNAPSE_MCAP_PROFILE);
    cursor_string(&record, "synapse-fbs-c-test/1");
    cursor_done(&record);

    record = cursor_record(&file, 0x0cU);
    cursor_string(&record, SYNAPSE_MCAP_METADATA_NAME);
    map = cursor_take(&record, cursor_u32(&record));
    cursor_string(&map, SYNAPSE_MCAP_SCHEMA_SET_HASH_KEY);
    cursor_string(&map, SYNAPSE_LEGACY_SCHEMA_SET_HASH_128);
    cursor_string(&map, SYNAPSE_MCAP_SCHEMA_SET_IDENTITY_KEY);
    cursor_string(&map, SYNAPSE_SCHEMA_SET_IDENTITY);
    cursor_string(&map, SYNAPSE_MCAP_SCHEMA_PACKAGE_CONTRACT_IDENTITY_KEY);
    cursor_string(&map, SYNAPSE_SCHEMA_PACKAGE_CONTRACT_IDENTITY);
    cursor_string(&map, SYNAPSE_MCAP_SESSION_ID_KEY);
    cursor_string(&map, "0123456789abcdef0123456789abcdef");
    cursor_string(&map, SYNAPSE_MCAP_SOURCE_KEY);
    cursor_string(&map, "test-vehicle");
    cursor_string(&map, SYNAPSE_MCAP_TIME_BASIS_KEY);
    cursor_string(&map, SYNAPSE_MCAP_TIME_BASIS_MONOTONIC_BOOT);
    cursor_done(&map);
    cursor_done(&record);

    record = cursor_record(&file, 0x03U);
    assert(cursor_u16(&record) == 1U);
    cursor_string(&record, "synapse.topic.ActuatorOutputs");
    cursor_string(&record, SYNAPSE_MCAP_SCHEMA_ENCODING);
    assert(cursor_u32(&record) == synapse_bfbs_control_size);
    cursor_require(&record, synapse_bfbs_control_size);
    assert(memcmp(record.data + record.position, synapse_bfbs_control,
                  synapse_bfbs_control_size) == 0);
    record.position += synapse_bfbs_control_size;
    cursor_done(&record);

    record = cursor_record(&file, 0x04U);
    assert(cursor_u16(&record) == 0U);
    assert(cursor_u16(&record) == 1U);
    cursor_string(&record, "act_out");
    cursor_string(&record, SYNAPSE_MCAP_MESSAGE_ENCODING);
    map = cursor_take(&record, cursor_u32(&record));
    cursor_string(&map, SYNAPSE_MCAP_TOPIC_ID_KEY);
    cursor_string(&map, "47");
    cursor_done(&map);
    cursor_done(&record);

    record = cursor_record(&file, 0x05U);
    assert(cursor_u16(&record) == 0U);
    assert(cursor_u32(&record) == 0U);
    assert(cursor_u64(&record) == LOG_TIME_NS);
    assert(cursor_u64(&record) == PUBLISH_TIME_NS);
    message.data = record.data + record.position;
    message.size = record.size - record.position;
    record.position = record.size;
    cursor_done(&record);

    assert(message.size == WRAPPED_PAYLOAD_SIZE);
    record = (byte_cursor_t){message.data, message.size, 0U};
    assert(cursor_u32(&record) == 4U);
    assert(cursor_u32(&record) == UINT32_C(0xffffff6c));
    assert(memcmp(record.data + record.position, payload,
                  SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE) == 0);
    record.position += SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE;
    assert(cursor_u16(&record) == 6U);
    assert(cursor_u16(&record) == 148U);
    assert(cursor_u16(&record) == 4U);
    cursor_done(&record);

    record = cursor_record(&file, 0x0fU);
    assert(cursor_u32(&record) == 0U);
    cursor_done(&record);
    record = cursor_record(&file, 0x02U);
    assert(cursor_u64(&record) == 0U);
    assert(cursor_u64(&record) == 0U);
    assert(cursor_u32(&record) == 0U);
    cursor_done(&record);
    cursor_require(&file, sizeof(magic));
    assert(memcmp(file.data + file.position, magic, sizeof(magic)) == 0);
    file.position += sizeof(magic);
    cursor_done(&file);
    return message;
}

int main(int argc, char **argv) {
    static memory_sink_t recording;
    static memory_sink_t replay;
    uint8_t payload[SYNAPSE_ACTUATOR_OUTPUTS_PAYLOAD_SIZE];
    synapse_actuator_outputs_profile_t profile = {
        .logical_slot_mask = UINT32_C(0x0000000f),
        .reversible_mask = 0U,
        .disarmed_values = {0.0F},
    };
    parsed_message_t message;
    byte_cursor_t payload_cursor;

    assert(argc == 2 || argc == 3);
    load_payload(argv[1], payload);
    payload_cursor = (byte_cursor_t){payload, sizeof(payload), 0U};
    assert(cursor_u64(&payload_cursor) == PUBLISH_TIME_NS);
    assert(synapse_actuator_outputs_validate(payload, sizeof(payload), &profile,
                                             false)
               .code == SYNAPSE_ACTUATOR_OUTPUTS_OK);
    record_actuator(&recording, payload, sizeof(payload), true);
    message = validate_recording(&recording, payload);

    record_actuator(&replay, message.data, message.size, false);
    assert(replay.size == recording.size);
    assert(memcmp(replay.data, recording.data, recording.size) == 0);

    if (argc == 3) {
        FILE *output = fopen(argv[2], "wb");
        assert(output != NULL);
        assert(fwrite(recording.data, 1U, recording.size, output) ==
               recording.size);
        assert(fclose(output) == 0);
    }
    return 0;
}
