#ifndef MCAP_COMPRESSION_NO_LZ4
#define MCAP_COMPRESSION_NO_LZ4
#endif
#ifndef MCAP_COMPRESSION_NO_ZSTD
#define MCAP_COMPRESSION_NO_ZSTD
#endif
#define MCAP_IMPLEMENTATION
#include <synapse/mcap.hpp>

#include <algorithm>
#include <cstring>
#include <stdexcept>
#include <unordered_map>

namespace synapse {
namespace mcap {

namespace {

::mcap::McapWriterOptions writerOptions(std::string_view library) {
  ::mcap::McapWriterOptions options(SYNAPSE_MCAP_PROFILE);
  options.library = std::string(library);
  options.noChunkCRC = true;
  options.enableDataCRC = false;
  options.noSummaryCRC = true;
  options.noChunking = true;
  options.noMessageIndex = true;
  options.noSummary = true;
  options.compression = ::mcap::Compression::None;
  options.noRepeatedSchemas = true;
  options.noRepeatedChannels = true;
  options.noAttachmentIndex = true;
  options.noMetadataIndex = true;
  options.noChunkIndex = true;
  options.noStatistics = true;
  options.noSummaryOffsets = true;
  return options;
}

void validateOpenArguments(std::string_view library,
                           std::string_view session_id,
                           std::string_view source) {
  if (library.empty()) throw std::invalid_argument("MCAP library identifier is empty");
  if (source.empty()) throw std::invalid_argument("Synapse MCAP source is empty");
  if (!Writer::validSessionId(session_id)) {
    throw std::invalid_argument(
        "Synapse MCAP session id must be 32 lowercase hexadecimal characters");
  }
}

}  // namespace

bool Writer::validSessionId(std::string_view value) {
  return value.size() == 32 &&
         std::all_of(value.begin(), value.end(), [](char value) {
           return (value >= '0' && value <= '9') ||
                  (value >= 'a' && value <= 'f');
         });
}

const char* Writer::timeBasisName(TimeBasis time_basis) {
  switch (time_basis) {
    case TimeBasis::MonotonicBoot:
      return SYNAPSE_MCAP_TIME_BASIS_MONOTONIC_BOOT;
    case TimeBasis::UnixEpoch:
      return SYNAPSE_MCAP_TIME_BASIS_UNIX_EPOCH;
    case TimeBasis::Correlated:
      return SYNAPSE_MCAP_TIME_BASIS_CORRELATED;
  }
  throw std::invalid_argument("unsupported Synapse MCAP time basis");
}

::mcap::Status Writer::open(std::ostream& output, std::string_view library,
                            std::string_view session_id,
                            std::string_view source, TimeBasis time_basis) {
  validateOpenArguments(library, session_id, source);
  writer_.open(output, writerOptions(library));
  return writeMetadata(session_id, source, time_basis);
}

::mcap::Status Writer::open(std::string_view filename,
                            std::string_view library,
                            std::string_view session_id,
                            std::string_view source, TimeBasis time_basis) {
  validateOpenArguments(library, session_id, source);
  auto status = writer_.open(filename, writerOptions(library));
  if (!status.ok()) return status;
  return writeMetadata(session_id, source, time_basis);
}

::mcap::Status Writer::writeMetadata(std::string_view session_id,
                                     std::string_view source,
                                     TimeBasis time_basis) {
  ::mcap::Metadata metadata;
  metadata.name = SYNAPSE_MCAP_METADATA_NAME;
  metadata.metadata = {
      {SYNAPSE_MCAP_SCHEMA_SET_HASH_KEY, SYNAPSE_SCHEMA_SET_HASH},
      {SYNAPSE_MCAP_SESSION_ID_KEY, std::string(session_id)},
      {SYNAPSE_MCAP_SOURCE_KEY, std::string(source)},
      {SYNAPSE_MCAP_TIME_BASIS_KEY, timeBasisName(time_basis)},
  };
  return writer_.write(metadata);
}

void Writer::addSchema(const Topic& topic, Schema& schema) {
  schema.topic = topic;
  schema.value.name = std::string(topic.name);
  schema.value.encoding = SYNAPSE_MCAP_SCHEMA_ENCODING;
  schema.value.data.assign(topic.schema_data, topic.schema_data + topic.schema_size);
  writer_.addSchema(schema.value);
}

void Writer::addChannel(const Schema& schema, std::string_view channel_topic,
                        Channel& channel) {
  channel.topic = schema.topic;
  channel.next_sequence = 0;
  channel.value = ::mcap::Channel(
      channel_topic, SYNAPSE_MCAP_MESSAGE_ENCODING, schema.value.id,
      {{SYNAPSE_MCAP_TOPIC_ID_KEY, std::to_string(schema.topic.id)}});
  writer_.addChannel(channel.value);
}

void Writer::addTopic(const Topic& topic, std::string_view channel_topic,
                      Channel& channel) {
  Schema schema;
  addSchema(topic, schema);
  addChannel(schema, channel_topic, channel);
}

::mcap::Status Writer::write(Channel& channel, std::uint64_t log_time_ns,
                             std::uint64_t publish_time_ns,
                             const std::byte* data, std::size_t size) {
  ::mcap::Message message;
  message.channelId = channel.value.id;
  message.sequence = channel.next_sequence++;
  message.logTime = log_time_ns;
  message.publishTime = publish_time_ns;
  message.data = data;
  message.dataSize = size;
  return writer_.write(message);
}

::mcap::Status Writer::writeFixed(Channel& channel,
                                  std::uint64_t log_time_ns,
                                  std::uint64_t publish_time_ns,
                                  const std::byte* payload, std::size_t size) {
  if (!channel.topic.fixed_layout) {
    throw std::invalid_argument(std::string(channel.topic.name) +
                                " is not fixed-layout");
  }
  if (size != channel.topic.payload_size) {
    throw std::invalid_argument("fixed payload size does not match topic catalog");
  }
  const std::size_t object_size = size + 4;
  if (object_size > UINT16_MAX || size > UINT32_MAX - 14) {
    throw std::length_error("fixed payload is too large to wrap");
  }
  fixed_buffer_.resize(size + 14);
  const std::uint32_t root = 4;
  const std::int32_t vtable_offset = -static_cast<std::int32_t>(object_size);
  const std::uint16_t vtable_size = 6;
  const std::uint16_t object_size_u16 = static_cast<std::uint16_t>(object_size);
  const std::uint16_t field_offset = 4;
  std::memcpy(fixed_buffer_.data(), &root, sizeof(root));
  std::memcpy(fixed_buffer_.data() + 4, &vtable_offset, sizeof(vtable_offset));
  std::memcpy(fixed_buffer_.data() + 8, payload, size);
  std::memcpy(fixed_buffer_.data() + 8 + size, &vtable_size, sizeof(vtable_size));
  std::memcpy(fixed_buffer_.data() + 10 + size, &object_size_u16,
              sizeof(object_size_u16));
  std::memcpy(fixed_buffer_.data() + 12 + size, &field_offset,
              sizeof(field_offset));
  return write(channel, log_time_ns, publish_time_ns, fixed_buffer_.data(),
               fixed_buffer_.size());
}

void Writer::close() { writer_.close(); }

::mcap::Status validateProfile(const ::mcap::McapReader& reader) {
  if (!reader.header().has_value()) {
    return {::mcap::StatusCode::InvalidFile, "missing MCAP header"};
  }
  if (reader.header()->profile != SYNAPSE_MCAP_PROFILE) {
    return {::mcap::StatusCode::InvalidFile,
            "unsupported MCAP profile: " + reader.header()->profile};
  }
  return {};
}

}  // namespace mcap
}  // namespace synapse
