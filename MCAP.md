# Synapse MCAP Profile 1

MCAP is the officially supported Synapse log format. This document is the
normative `synapse/1` profile. Writers must use these values exactly; an
incompatible change requires a new profile name such as `synapse/2`.

## File contract

- `Header.profile` must be `synapse/1`.
- `Header.library` must be a non-empty UTF-8 string identifying the writer and
  version, conventionally `<name>/<version>`.
- One MCAP Metadata record named `synapse` must precede the first Message.
- The Metadata map must contain:
  - `synapse.schema_set_hash`: the producer release's 32-character lowercase
    schema-set hash;
  - `synapse.session_id`: exactly 32 lowercase hexadecimal characters
    representing a recording-unique 128-bit identifier;
  - `synapse.source`: a non-empty UTF-8 vehicle, simulator, or device identity;
  - `synapse.time_basis`: exactly `monotonic_boot`, `unix_epoch`, or
    `correlated`.
- All MCAP timestamps are unsigned nanoseconds. `monotonic_boot` means that
  timestamps are elapsed time since this producer boot. `unix_epoch` means
  nanoseconds since 1970-01-01T00:00:00Z. `correlated` uses monotonic-boot
  timestamps in Message records and requires `TimeReference` samples that map
  that clock to Unix time.
- `Message.log_time` is the time at which the logger accepted the sample,
  expressed in the selected basis. A topic payload's `timestamp_ns` becomes
  `Message.publish_time = timestamp_ns` without conversion. When the
  sample has no publication timestamp, `publish_time` equals `log_time`.

## Topic schemas and channels

Every selected Synapse topic has one MCAP Schema and one or more MCAP Channels.

- `Schema.name` is the fully qualified existing root table, for example
  `synapse.topic.Odometry`.
- `Schema.encoding` is `flatbuffer`.
- `Schema.data` is the BFBS named by the topic catalog's `mcap_schema_file`,
  for example `bfbs/state.bfbs`. It must contain the object named by
  `Schema.name`. Multiple topic schemas may reuse the same BFBS bytes;
  `Schema.name`, not a BFBS file-level root declaration, selects the exact
  Message root type.
- `Channel.topic` is the canonical, optionally namespaced Synapse key,
  including an instance suffix when required.
- `Channel.message_encoding` is `flatbuffer`.
- Channel metadata contains `synapse.topic_id`, formatted as the unsigned
  decimal `TopicId` value from the producing release.
- Schema and Channel records must occur before the first Message that refers
  to them. Identifiers are file-local MCAP identifiers and need not equal
  `TopicId`.

## Message payloads

`Message.data` is the existing topic root table named by the channel's Schema.
There is no generic Synapse log envelope.

- A live fixed-layout topic struct is copied first, then wrapped in its
  existing root table outside the real-time path.
- A naturally variable-size topic is already root-table encoded and is stored
  unchanged.
- `Message.sequence` starts at zero independently for every Channel and
  increments modulo 2^32 for every written Message. A reader uses discontinuity
  for loss detection, while accounting for the defined wrap.
- Readers must decode with the `Schema.name` and BFBS embedded in that MCAP
  file. They must not substitute generated bindings or BFBS from the reader's
  installed `synapse_fbs` version. Embedded historical BFBS is authoritative.

## Onboard streaming profile

The constant-memory baseline for flight controllers is direct, uncompressed,
unchunked, index-less MCAP:

- write complete Schema, Channel, Metadata, and Message records sequentially;
- retain only a bounded sink buffer and the current root-table builder buffer;
- do not retain the complete log, message indexes, or summary statistics in
  RAM;
- write `DataEnd`, `Footer`, and trailing magic on clean close;
- allow the Summary and Summary Offset sections to be absent;
- recover an interrupted file by scanning complete records and rebuilding an
  optional summary/index on the host.

Chunking and compression are optional extensions. An onboard writer may use
them only with an explicitly bounded chunk buffer; its RAM cost is at least
the configured uncompressed chunk size plus compressor state. They are not
required for `synapse/1` compatibility.

Real-time publishers must never wait for storage. Firmware captures selected
generated topic structs into a bounded queue and a lower-priority logger wraps
and writes them. Queue overflow increments a dropped-sample counter rather
than blocking control execution.

The same index-less MCAP byte stream may be written to SD card, flash, USB, or
a reliable serial byte stream. An unreliable serial transport must provide its
own framing, integrity, retransmission, or loss policy below the MCAP sink; the
log format does not change.

## Compatibility

The `synapse/1` mapping and literal strings above are frozen. Topic schemas may
evolve under the normal Synapse compatibility rules because each file embeds
the exact historical `Schema.name` and BFBS needed for its own messages. A
reader must retain support for MCAP major version 0 and the `synapse/1` profile
indefinitely.
