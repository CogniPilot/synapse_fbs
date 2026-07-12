"""First-class support for the frozen ``synapse/1`` MCAP profile.

Install with ``synapse-fbs[mcap]``. Container encoding and decoding are
delegated to the upstream :mod:`mcap` package; this module only applies the
Synapse profile, topic catalog, embedded BFBS, and fixed-struct wrapper.
"""

from dataclasses import dataclass
from enum import Enum
from importlib import resources
from typing import BinaryIO, Iterator, Union

from mcap.reader import make_reader
from mcap.writer import CompressionType, IndexType, Writer as ContainerWriter

from . import topic_catalog


class TimeBasis(str, Enum):
    MONOTONIC_BOOT = topic_catalog.MCAP_TIME_BASIS_MONOTONIC_BOOT
    UNIX_EPOCH = topic_catalog.MCAP_TIME_BASIS_UNIX_EPOCH
    CORRELATED = topic_catalog.MCAP_TIME_BASIS_CORRELATED


@dataclass
class TopicChannel:
    """A registered channel with per-channel sequence state."""

    id: int
    topic: topic_catalog.TopicInfo
    next_sequence: int = 0


def _valid_session_id(value: str) -> bool:
    return len(value) == 32 and all(char in "0123456789abcdef" for char in value)


def _topic(value: Union[str, topic_catalog.TopicInfo]) -> topic_catalog.TopicInfo:
    if isinstance(value, topic_catalog.TopicInfo):
        return value
    result = topic_catalog.topic_by_name(value)
    if result is None:
        raise KeyError(f"unknown Synapse topic: {value}")
    return result


def schema_data(topic: Union[str, topic_catalog.TopicInfo]) -> bytes:
    """Return the exact BFBS required by a topic's MCAP Schema record."""

    info = _topic(topic)
    relative = info.mcap_schema_file.removeprefix("bfbs/")
    return resources.files("synapse").joinpath("bfbs", relative).read_bytes()


def wrap_fixed_payload(payload: Union[bytes, bytearray, memoryview]) -> bytes:
    """Wrap fixed struct bytes in their existing one-field FlatBuffer root."""

    data = bytes(payload)
    object_size = len(data) + 4
    if object_size > 0xFFFF or len(data) > 0xFFFFFFFF - 14:
        raise ValueError("fixed payload is too large to wrap")
    return b"".join(
        (
            (4).to_bytes(4, "little"),
            (-object_size).to_bytes(4, "little", signed=True),
            data,
            (6).to_bytes(2, "little"),
            object_size.to_bytes(2, "little"),
            (4).to_bytes(2, "little"),
        )
    )


class Writer:
    """Uncompressed, unchunked, index-less ``synapse/1`` writer."""

    def __init__(
        self,
        output: BinaryIO,
        library: str,
        session_id: str,
        source: str,
        time_basis: TimeBasis = TimeBasis.MONOTONIC_BOOT,
    ) -> None:
        if not library:
            raise ValueError("MCAP library identifier is empty")
        if not source:
            raise ValueError("Synapse MCAP source is empty")
        if not _valid_session_id(session_id):
            raise ValueError(
                "Synapse MCAP session id must be 32 lowercase hexadecimal characters"
            )
        self._writer = ContainerWriter(
            output,
            compression=CompressionType.NONE,
            index_types=IndexType.NONE,
            repeat_channels=False,
            repeat_schemas=False,
            use_chunking=False,
            use_statistics=False,
            use_summary_offsets=False,
            enable_crcs=False,
            enable_data_crcs=False,
        )
        self._writer.start(profile=topic_catalog.MCAP_PROFILE, library=library)
        self._writer.add_metadata(
            topic_catalog.MCAP_METADATA_NAME,
            {
                topic_catalog.MCAP_SCHEMA_SET_HASH_KEY: topic_catalog.SCHEMA_SET_HASH,
                topic_catalog.MCAP_SESSION_ID_KEY: session_id,
                topic_catalog.MCAP_SOURCE_KEY: source,
                topic_catalog.MCAP_TIME_BASIS_KEY: TimeBasis(time_basis).value,
            },
        )

    def add_topic(
        self,
        topic: Union[str, topic_catalog.TopicInfo],
        channel_topic: str,
    ) -> TopicChannel:
        info = _topic(topic)
        schema_id = self._writer.register_schema(
            name=info.mcap_schema_name,
            encoding=topic_catalog.MCAP_SCHEMA_ENCODING,
            data=schema_data(info),
        )
        channel_id = self._writer.register_channel(
            topic=channel_topic,
            message_encoding=topic_catalog.MCAP_MESSAGE_ENCODING,
            schema_id=schema_id,
            metadata={topic_catalog.MCAP_TOPIC_ID_KEY: str(info.id)},
        )
        return TopicChannel(channel_id, info)

    def write(
        self,
        channel: TopicChannel,
        log_time_ns: int,
        publish_time_ns: int,
        data: Union[bytes, bytearray, memoryview],
    ) -> None:
        self._writer.add_message(
            channel_id=channel.id,
            sequence=channel.next_sequence,
            log_time=log_time_ns,
            publish_time=publish_time_ns,
            data=bytes(data),
        )
        channel.next_sequence = (channel.next_sequence + 1) & 0xFFFFFFFF

    def write_fixed(
        self,
        channel: TopicChannel,
        log_time_ns: int,
        publish_time_ns: int,
        payload: Union[bytes, bytearray, memoryview],
    ) -> None:
        if not channel.topic.fixed_layout:
            raise ValueError(f"{channel.topic.name} is not fixed-layout")
        if channel.topic.payload_size != len(payload):
            raise ValueError(
                f"fixed payload is {len(payload)} bytes, "
                f"expected {channel.topic.payload_size}"
            )
        self.write(
            channel,
            log_time_ns,
            publish_time_ns,
            wrap_fixed_payload(payload),
        )

    def finish(self) -> None:
        self._writer.finish()


class Reader:
    """Validated ``synapse/1`` reader backed by the upstream MCAP reader."""

    def __init__(self, stream: BinaryIO) -> None:
        self._reader = make_reader(stream)
        header = self._reader.get_header()
        if header.profile != topic_catalog.MCAP_PROFILE:
            raise ValueError(f"unsupported MCAP profile: {header.profile!r}")
        metadata = next(
            (
                item
                for item in self._reader.iter_metadata()
                if item.name == topic_catalog.MCAP_METADATA_NAME
            ),
            None,
        )
        if metadata is None:
            raise ValueError("missing required Synapse MCAP metadata")
        schema_set_hash = metadata.metadata.get(topic_catalog.MCAP_SCHEMA_SET_HASH_KEY, "")
        if len(schema_set_hash) != 32 or any(
            char not in "0123456789abcdef" for char in schema_set_hash
        ):
            raise ValueError("invalid Synapse MCAP schema-set hash")
        if not _valid_session_id(
            metadata.metadata.get(topic_catalog.MCAP_SESSION_ID_KEY, "")
        ):
            raise ValueError("invalid Synapse MCAP session id")
        if not metadata.metadata.get(topic_catalog.MCAP_SOURCE_KEY):
            raise ValueError("missing Synapse MCAP source")
        if metadata.metadata.get(topic_catalog.MCAP_TIME_BASIS_KEY) not in {
            basis.value for basis in TimeBasis
        }:
            raise ValueError("invalid Synapse MCAP time basis")
        self.metadata = metadata.metadata

    def messages(self, **kwargs: object) -> Iterator[object]:
        """Yield upstream ``(schema, channel, message)`` tuples."""

        yield from self._reader.iter_messages(**kwargs)


__all__ = [
    "Reader",
    "TimeBasis",
    "TopicChannel",
    "Writer",
    "schema_data",
    "wrap_fixed_payload",
]
