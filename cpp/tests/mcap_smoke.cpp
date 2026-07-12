#include <synapse/mcap.hpp>

#include <cstddef>
#include <cstdint>
#include <iostream>
#include <vector>

int main(int argc, char** argv) {
  if (argc != 2) return 2;

  const auto topic = synapse::mcap::OdometryTopic();
  synapse::mcap::Writer writer;
  auto status = writer.open(argv[1], "synapse-fbs-cpp-test/1",
                            "0123456789abcdef0123456789abcdef", "test-cpp");
  if (!status.ok()) return 3;

  synapse::mcap::Channel channel;
  writer.addTopic(topic, "test/odom", channel);
  std::vector<std::byte> payload(topic.payload_size);
  status = writer.writeFixed(channel, 2000, 1000, payload.data(), payload.size());
  if (!status.ok()) return 4;
  writer.close();

  ::mcap::McapReader reader;
  status = reader.open(argv[1]);
  if (!status.ok()) return 5;
  status = synapse::mcap::validateProfile(reader);
  if (!status.ok()) return 6;
  std::size_t messages = 0;
  for (const auto& message : reader.readMessages()) {
    if (message.channel->topic != "test/odom" ||
        message.channel->messageEncoding != "flatbuffer" ||
        !message.schema || message.schema->name != "synapse.topic.Odometry" ||
        message.schema->encoding != "flatbuffer" ||
        message.message.logTime != 2000 || message.message.publishTime != 1000) {
      return 7;
    }
    ++messages;
  }
  reader.close();
  return messages == 1 ? 0 : 8;
}
