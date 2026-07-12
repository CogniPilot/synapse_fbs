//! Optional host-side implementation of the frozen `synapse/1` MCAP profile.
//!
//! Enable the crate's `mcap` feature. The embedded C writer in the C release
//! archive follows the same profile and is tested against this implementation.

use std::{collections::BTreeMap, io::Seek, io::Write};

use ::mcap::records::{MessageHeader, Metadata};

use crate::{
    schemas::schema_by_name,
    topic_catalog::{
        MCAP_MESSAGE_ENCODING, MCAP_METADATA_NAME, MCAP_PROFILE, MCAP_SCHEMA_ENCODING,
        MCAP_SCHEMA_SET_HASH_KEY, MCAP_SESSION_ID_KEY, MCAP_SOURCE_KEY, MCAP_TIME_BASIS_CORRELATED,
        MCAP_TIME_BASIS_KEY, MCAP_TIME_BASIS_MONOTONIC_BOOT, MCAP_TIME_BASIS_UNIX_EPOCH,
        MCAP_TOPIC_ID_KEY, SCHEMA_SET_HASH, TopicInfo,
    },
};

/// The timestamp basis recorded in required `synapse/1` metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeBasis {
    MonotonicBoot,
    UnixEpoch,
    Correlated,
}

impl TimeBasis {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MonotonicBoot => MCAP_TIME_BASIS_MONOTONIC_BOOT,
            Self::UnixEpoch => MCAP_TIME_BASIS_UNIX_EPOCH,
            Self::Correlated => MCAP_TIME_BASIS_CORRELATED,
        }
    }
}

/// A registered channel and its caller-owned sequence state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TopicChannel {
    id: u16,
    topic_id: u16,
    next_sequence: u32,
    payload_size: Option<usize>,
    fixed_layout: bool,
}

impl TopicChannel {
    pub const fn id(&self) -> u16 {
        self.id
    }
}

#[derive(Debug)]
pub enum Error {
    Container(::mcap::McapError),
    EmptyLibrary,
    EmptySource,
    InvalidSessionId,
    MissingSchema(&'static str),
    NotFixedLayout(&'static str),
    PayloadSize { expected: usize, actual: usize },
    ChannelTopicMismatch,
    PayloadTooLarge,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Container(error) => error.fmt(formatter),
            Self::EmptyLibrary => formatter.write_str("MCAP library identifier is empty"),
            Self::EmptySource => formatter.write_str("Synapse MCAP source is empty"),
            Self::InvalidSessionId => formatter
                .write_str("Synapse MCAP session id must be 32 lowercase hexadecimal characters"),
            Self::MissingSchema(file) => {
                write!(formatter, "embedded BFBS is missing for {file}")
            }
            Self::NotFixedLayout(topic) => write!(formatter, "{topic} is not fixed-layout"),
            Self::PayloadSize { expected, actual } => {
                write!(
                    formatter,
                    "fixed payload is {actual} bytes, expected {expected}"
                )
            }
            Self::ChannelTopicMismatch => {
                formatter.write_str("fixed payload type does not match the MCAP channel")
            }
            Self::PayloadTooLarge => formatter.write_str("fixed payload is too large to wrap"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Container(error) => Some(error),
            _ => None,
        }
    }
}

impl From<::mcap::McapError> for Error {
    fn from(error: ::mcap::McapError) -> Self {
        Self::Container(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Implemented by every generated fixed-layout Synapse topic struct.
pub trait FixedMessage {
    fn topic() -> &'static TopicInfo;
    fn as_payload_bytes(&self) -> &[u8];
}

/// Uncompressed, unchunked, index-less host writer for `synapse/1` logs.
pub struct Writer<W: Write + Seek> {
    inner: ::mcap::Writer<W>,
    fixed_buffer: Vec<u8>,
}

impl<W: Write + Seek> Writer<W> {
    pub fn new(
        output: W,
        library: &str,
        session_id: &str,
        source: &str,
        time_basis: TimeBasis,
    ) -> Result<Self> {
        if library.is_empty() {
            return Err(Error::EmptyLibrary);
        }
        if source.is_empty() {
            return Err(Error::EmptySource);
        }
        if !is_lowercase_hex_128(session_id) {
            return Err(Error::InvalidSessionId);
        }

        let options = ::mcap::WriteOptions::new()
            .profile(MCAP_PROFILE)
            .library(library)
            .compression(None)
            .use_chunks(false)
            .emit_summary_records(false)
            .emit_summary_offsets(false)
            .emit_message_indexes(false)
            .calculate_data_section_crc(false)
            .calculate_summary_section_crc(false);
        let mut inner = options.create(output)?;
        let metadata = BTreeMap::from([
            (
                MCAP_SCHEMA_SET_HASH_KEY.to_owned(),
                SCHEMA_SET_HASH.to_owned(),
            ),
            (MCAP_SESSION_ID_KEY.to_owned(), session_id.to_owned()),
            (MCAP_SOURCE_KEY.to_owned(), source.to_owned()),
            (
                MCAP_TIME_BASIS_KEY.to_owned(),
                time_basis.as_str().to_owned(),
            ),
        ]);
        inner.write_metadata(&Metadata {
            name: MCAP_METADATA_NAME.to_owned(),
            metadata,
        })?;
        Ok(Self {
            inner,
            fixed_buffer: Vec::new(),
        })
    }

    /// Register one selected topic before writing its first message.
    pub fn add_topic(
        &mut self,
        topic: &'static TopicInfo,
        channel_topic: &str,
    ) -> Result<TopicChannel> {
        let schema =
            schema_by_name(topic.schema_file).ok_or(Error::MissingSchema(topic.schema_file))?;
        let schema_id =
            self.inner
                .add_schema(topic.mcap_schema_name, MCAP_SCHEMA_ENCODING, schema.bfbs)?;
        let metadata = BTreeMap::from([(MCAP_TOPIC_ID_KEY.to_owned(), topic.id.to_string())]);
        let id =
            self.inner
                .add_channel(schema_id, channel_topic, MCAP_MESSAGE_ENCODING, &metadata)?;
        Ok(TopicChannel {
            id,
            topic_id: topic.id,
            next_sequence: 0,
            payload_size: topic.payload_size,
            fixed_layout: topic.fixed_layout,
        })
    }

    pub fn add_fixed_topic<T: FixedMessage>(
        &mut self,
        channel_topic: &str,
    ) -> Result<TopicChannel> {
        let topic = T::topic();
        if !topic.fixed_layout {
            return Err(Error::NotFixedLayout(topic.name));
        }
        self.add_topic(topic, channel_topic)
    }

    /// Write an already table-wrapped topic payload.
    pub fn write(
        &mut self,
        channel: &mut TopicChannel,
        log_time_ns: u64,
        publish_time_ns: u64,
        data: &[u8],
    ) -> Result<()> {
        let sequence = channel.next_sequence;
        channel.next_sequence = channel.next_sequence.wrapping_add(1);
        self.inner.write_to_known_channel(
            &MessageHeader {
                channel_id: channel.id,
                sequence,
                log_time: log_time_ns,
                publish_time: publish_time_ns,
            },
            data,
        )?;
        Ok(())
    }

    /// Wrap a generated fixed-layout struct and write it as its existing root
    /// table. The host scratch vector is retained and reused across messages.
    pub fn write_fixed<T: FixedMessage>(
        &mut self,
        channel: &mut TopicChannel,
        log_time_ns: u64,
        publish_time_ns: u64,
        payload: &T,
    ) -> Result<()> {
        if channel.topic_id != T::topic().id {
            return Err(Error::ChannelTopicMismatch);
        }
        if !channel.fixed_layout {
            return Err(Error::NotFixedLayout(T::topic().name));
        }
        let bytes = payload.as_payload_bytes();
        let expected = channel.payload_size.unwrap_or(0);
        if bytes.len() != expected {
            return Err(Error::PayloadSize {
                expected,
                actual: bytes.len(),
            });
        }
        wrap_fixed_payload(&mut self.fixed_buffer, bytes)?;
        let sequence = channel.next_sequence;
        channel.next_sequence = channel.next_sequence.wrapping_add(1);
        self.inner.write_to_known_channel(
            &MessageHeader {
                channel_id: channel.id,
                sequence,
                log_time: log_time_ns,
                publish_time: publish_time_ns,
            },
            &self.fixed_buffer,
        )?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.inner.flush()?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<W> {
        self.inner.finish()?;
        Ok(self.inner.into_inner())
    }
}

fn is_lowercase_hex_128(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn wrap_fixed_payload(output: &mut Vec<u8>, payload: &[u8]) -> Result<()> {
    let object_size = payload.len().checked_add(4).ok_or(Error::PayloadTooLarge)?;
    if object_size > u16::MAX as usize || payload.len() > u32::MAX as usize - 14 {
        return Err(Error::PayloadTooLarge);
    }
    output.clear();
    output.reserve(payload.len() + 14);
    output.extend_from_slice(&4_u32.to_le_bytes());
    output.extend_from_slice(&(-(object_size as i32)).to_le_bytes());
    output.extend_from_slice(payload);
    output.extend_from_slice(&6_u16.to_le_bytes());
    output.extend_from_slice(&(object_size as u16).to_le_bytes());
    output.extend_from_slice(&4_u16.to_le_bytes());
    Ok(())
}

/// The conformant upstream container implementation remains available for
/// dynamic reading, recovery, and advanced host-only operations.
pub use ::mcap as container;
