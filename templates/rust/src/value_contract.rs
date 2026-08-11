//! Required Zenoh value metadata for Synapse topic payloads.
//!
//! Synapse keys describe semantics and ownership. The Zenoh value encoding is
//! the authoritative wire contract: consumers must validate it before
//! decoding payload bytes.

use std::fmt;

use crate::topic_catalog::{self, TopicInfo};

pub const FLATBUFFER_MEDIA_TYPE: &str = "application/x-flatbuffers";
pub const STRUCT_MEDIA_TYPE: &str = "application/x-synapse-struct";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueContract {
    pub media_type: &'static str,
    pub wire_type: &'static str,
    pub schema_hash: &'static str,
}

impl ValueContract {
    /// Canonical Zenoh encoding. Field order and spelling are part of the
    /// contract so every producer emits one unambiguous representation.
    pub fn encoding(&self) -> String {
        format!(
            "{};type={};schema=sha256-128:{}",
            self.media_type, self.wire_type, self.schema_hash
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractError(String);

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ContractError {}

pub fn contract_for_topic(topic: &'static TopicInfo) -> ValueContract {
    ValueContract {
        media_type: if topic.fixed_layout {
            STRUCT_MEDIA_TYPE
        } else {
            FLATBUFFER_MEDIA_TYPE
        },
        wire_type: topic.wire_type,
        schema_hash: topic.schema_hash,
    }
}

pub fn encoding_for_topic(topic: &'static TopicInfo) -> String {
    contract_for_topic(topic).encoding()
}

/// Resolve and strictly validate a Synapse topic from a Zenoh value encoding.
/// Missing metadata, non-canonical field ordering, unknown fields, and schema
/// fingerprint mismatches are all rejected.
pub fn topic_for_encoding(encoding: &str) -> Result<&'static TopicInfo, ContractError> {
    let wire_type = encoding
        .split(';')
        .find_map(|field| field.strip_prefix("type="))
        .ok_or_else(|| ContractError("Synapse value encoding is missing type metadata".into()))?;
    let topic = topic_catalog::TOPICS
        .iter()
        .find(|topic| topic.wire_type == wire_type)
        .ok_or_else(|| ContractError(format!("unknown Synapse wire type {wire_type}")))?;
    let expected = encoding_for_topic(topic);

    if encoding != expected {
        return Err(ContractError(format!(
            "incompatible value contract for {}: expected {expected}, received {encoding}",
            topic.name
        )));
    }

    Ok(topic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_and_variable_topics_have_canonical_contracts() {
        let health = topic_catalog::topic_by_name("VehicleHealth").unwrap();
        let health_encoding = encoding_for_topic(health);
        assert_eq!(
            health_encoding,
            format!(
                "application/x-synapse-struct;type={};schema=sha256-128:{}",
                health.wire_type, health.schema_hash
            )
        );
        assert_eq!(topic_for_encoding(&health_encoding).unwrap(), health);

        let text = topic_catalog::topic_by_name("TextStatus").unwrap();
        let text_encoding = encoding_for_topic(text);
        assert!(text_encoding.starts_with("application/x-flatbuffers;"));
        assert_eq!(topic_for_encoding(&text_encoding).unwrap(), text);
    }

    #[test]
    fn absent_and_mismatched_metadata_are_rejected() {
        assert!(topic_for_encoding("zenoh/bytes").is_err());
        assert!(
            topic_for_encoding(
                "application/x-synapse-struct;type=synapse.topic.UnknownData;schema=sha256-128:00000000000000000000000000000000"
            )
            .unwrap_err()
            .to_string()
            .contains("unknown Synapse wire type")
        );

        let topic = topic_catalog::topic_by_name("VehicleHealth").unwrap();
        let encoding = encoding_for_topic(topic);
        assert!(
            topic_for_encoding(&encoding.replace("schema=sha256-128:", "schema=sha256-128:deadbeef"))
                .is_err()
        );
        assert!(topic_for_encoding(&format!("{encoding};extra=not-allowed")).is_err());
    }
}
