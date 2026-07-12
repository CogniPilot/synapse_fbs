#pragma once

#ifndef MCAP_COMPRESSION_NO_LZ4
#define MCAP_COMPRESSION_NO_LZ4
#endif
#ifndef MCAP_COMPRESSION_NO_ZSTD
#define MCAP_COMPRESSION_NO_ZSTD
#endif
#include <mcap/mcap.hpp>
#include <synapse/mcap_topics.hpp>
#include <synapse/topic_catalog.h>

#include <cstdint>
#include <ostream>
#include <string>
#include <string_view>
#include <vector>

namespace synapse {
namespace mcap {

enum class TimeBasis {
  MonotonicBoot,
  UnixEpoch,
  Correlated,
};

struct Schema {
  ::mcap::Schema value;
  Topic topic;
};

struct Channel {
  ::mcap::Channel value;
  Topic topic;
  std::uint32_t next_sequence = 0;
};

// Host writer backed by the official MCAP C++ implementation. It configures
// the same uncompressed, unchunked, index-less profile as the embedded writer.
class Writer {
 public:
  ::mcap::Status open(std::ostream& output, std::string_view library,
                      std::string_view session_id, std::string_view source,
                      TimeBasis time_basis = TimeBasis::MonotonicBoot);
  ::mcap::Status open(std::string_view filename, std::string_view library,
                      std::string_view session_id, std::string_view source,
                      TimeBasis time_basis = TimeBasis::MonotonicBoot);

  void addSchema(const Topic& topic, Schema& schema);
  void addChannel(const Schema& schema, std::string_view channel_topic,
                  Channel& channel);
  void addTopic(const Topic& topic, std::string_view channel_topic,
                Channel& channel);

  ::mcap::Status write(Channel& channel, std::uint64_t log_time_ns,
                       std::uint64_t publish_time_ns, const std::byte* data,
                       std::size_t size);
  ::mcap::Status writeFixed(Channel& channel, std::uint64_t log_time_ns,
                            std::uint64_t publish_time_ns,
                            const std::byte* payload, std::size_t size);

  void close();
  ::mcap::McapWriter& container() noexcept { return writer_; }
  static bool validSessionId(std::string_view value);

 private:
  ::mcap::Status writeMetadata(std::string_view session_id,
                               std::string_view source, TimeBasis time_basis);
  static const char* timeBasisName(TimeBasis time_basis);

  ::mcap::McapWriter writer_;
  std::vector<std::byte> fixed_buffer_;
};

// Readers use the official ::mcap::McapReader directly. This check makes the
// file-level profile validation explicit before consuming messages.
::mcap::Status validateProfile(const ::mcap::McapReader& reader);

}  // namespace mcap
}  // namespace synapse
