#include <assert.h>
#include <stdio.h>
#include <string.h>

#include <synapse/mcap.h>
#include <synapse/mcap_topics.h>
#include <synapse/state_reader.h>

typedef struct memory_sink {
    uint8_t data[65536];
    size_t size;
} memory_sink_t;

static int memory_write(void *context, const uint8_t *data, size_t size) {
    memory_sink_t *sink = context;
    if (size > sizeof(sink->data) - sink->size) {
        return -1;
    }
    memcpy(sink->data + sink->size, data, size);
    sink->size += size;
    return 0;
}

int main(int argc, char **argv) {
    static const uint8_t magic[] = {0x89, 'M', 'C', 'A', 'P', '0', '\r', '\n'};
    synapse_topic_OdometryData_t payload = {0};
    memory_sink_t sink = {0};
    uint8_t output_buffer[257];
    synapse_mcap_writer_t writer;
    synapse_mcap_schema_t schema;
    synapse_mcap_channel_t channel;
    synapse_mcap_channel_t second_channel;
    synapse_mcap_topic_t topic = SYNAPSE_MCAP_TOPIC_Odometry;

    assert(synapse_mcap_open(
               &writer,
               (synapse_mcap_sink_t){.write = memory_write,
                                     .flush = NULL,
                                     .context = &sink},
               output_buffer, sizeof(output_buffer), "synapse-fbs-c-test/1",
               "0123456789abcdef0123456789abcdef", "test-vehicle",
               SYNAPSE_MCAP_TIME_MONOTONIC_BOOT) == SYNAPSE_MCAP_OK);
    assert(synapse_mcap_add_schema(&writer, &topic, &schema) ==
           SYNAPSE_MCAP_OK);
    assert(synapse_mcap_add_channel(&writer, &schema, "test/odom", &channel) ==
           SYNAPSE_MCAP_OK);
    assert(synapse_mcap_add_channel(&writer, &schema, "test/odom/1",
                                    &second_channel) == SYNAPSE_MCAP_OK);
    assert(channel.id == 0 && channel.topic_id == 40 &&
           channel.next_sequence == 0 && channel.payload_size == sizeof(payload));
    assert(second_channel.id == 1 && second_channel.topic_id == 40);
    assert(synapse_mcap_write_fixed(&writer, &channel, 2000, 1000, &payload,
                                    sizeof(payload)) == SYNAPSE_MCAP_OK);
    assert(channel.next_sequence == 1);
    assert(synapse_mcap_close(&writer) == SYNAPSE_MCAP_OK);
    assert(sink.size > synapse_bfbs_state_size);
    assert(memcmp(sink.data, magic, sizeof(magic)) == 0);
    assert(memcmp(sink.data + sink.size - sizeof(magic), magic, sizeof(magic)) ==
           0);

    if (argc == 2) {
        FILE *output = fopen(argv[1], "wb");
        assert(output != NULL);
        assert(fwrite(sink.data, 1, sink.size, output) == sink.size);
        assert(fclose(output) == 0);
    }
    return 0;
}
