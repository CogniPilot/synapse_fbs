#![cfg(feature = "mcap")]

use std::io::Cursor;

use flatbuffers_reflection::reflection::{self, BaseType};
use synapse_fbs::{
    mcap::{self, TimeBasis, Writer},
    schemas::schema_by_name,
    topic::{ActuatorOutputs, ActuatorOutputsData},
    topic_catalog::{
        LEGACY_SCHEMA_SET_HASH_128, MCAP_MESSAGE_ENCODING, MCAP_PROFILE, MCAP_SCHEMA_ENCODING,
        MCAP_SCHEMA_PACKAGE_CONTRACT_IDENTITY_KEY, MCAP_SCHEMA_SET_HASH_KEY,
        MCAP_SCHEMA_SET_IDENTITY_KEY, MCAP_TOPIC_ID_KEY, SCHEMA_PACKAGE_CONTRACT_IDENTITY,
        SCHEMA_SET_IDENTITY, topic_by_id,
    },
};

const LOG_TIME_NS: u64 = 0x1112_1314_1516_1718;
const PUBLISH_TIME_NS: u64 = 0x0102_0304_0506_0708;
const TROPIC_POSITIVE: &[u8; 144] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/test-vectors/actuator_outputs/tropic-positive.bin"
));

fn is_lowercase_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn record_actuator(payload: &ActuatorOutputsData) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = Writer::new(
        cursor,
        "synapse-fbs-test/1",
        "0123456789abcdef0123456789abcdef",
        "test-vehicle",
        TimeBasis::MonotonicBoot,
    )
    .unwrap();
    let mut channel = writer
        .add_fixed_topic::<ActuatorOutputsData>("act_out")
        .unwrap();
    writer
        .write_fixed(&mut channel, LOG_TIME_NS, PUBLISH_TIME_NS, payload)
        .unwrap();
    writer.finish().unwrap().into_inner()
}

fn reflected_fixed_payload<'a>(
    schema_data: &[u8],
    schema_name: &str,
    payload_type_name: &str,
    wrapped: &'a [u8],
) -> &'a [u8] {
    let schema = reflection::root_as_schema(schema_data).unwrap();
    let root_object = schema
        .objects()
        .iter()
        .find(|object| object.name() == schema_name)
        .expect("Schema.name is absent from the embedded BFBS");
    assert!(!root_object.is_struct());
    let data_field = root_object
        .fields()
        .iter()
        .find(|field| field.name() == "data")
        .expect("selected root wrapper has no data field");
    assert_eq!(data_field.type_().base_type(), BaseType::Obj);
    let payload_object = schema.objects().get(data_field.type_().index() as usize);
    assert!(payload_object.is_struct());
    assert_eq!(payload_object.name(), payload_type_name);
    let payload_size = payload_object.bytesize() as usize;
    assert_eq!(payload_size, TROPIC_POSITIVE.len());

    let root_position = u32::from_le_bytes(wrapped[0..4].try_into().unwrap()) as usize;
    let vtable_offset = i32::from_le_bytes(
        wrapped[root_position..root_position + 4]
            .try_into()
            .unwrap(),
    );
    let vtable_position = if vtable_offset >= 0 {
        root_position.checked_sub(vtable_offset as usize).unwrap()
    } else {
        root_position
            .checked_add(vtable_offset.unsigned_abs() as usize)
            .unwrap()
    };
    let vtable_size = u16::from_le_bytes(
        wrapped[vtable_position..vtable_position + 2]
            .try_into()
            .unwrap(),
    ) as usize;
    let object_size = u16::from_le_bytes(
        wrapped[vtable_position + 2..vtable_position + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    let vtable_field_position = vtable_position + data_field.offset() as usize;
    assert!(vtable_field_position + 2 <= vtable_position + vtable_size);
    let payload_offset = u16::from_le_bytes(
        wrapped[vtable_field_position..vtable_field_position + 2]
            .try_into()
            .unwrap(),
    ) as usize;
    assert_ne!(payload_offset, 0);
    assert_eq!(object_size, payload_offset + payload_size);
    let payload_position = root_position + payload_offset;
    &wrapped[payload_position..payload_position + payload_size]
}

#[test]
fn actuator_outputs_root_wrap_record_decode_and_replay() {
    let topic = topic_by_id(47).unwrap();
    assert_eq!(topic.name, "ActuatorOutputs");
    assert_eq!(topic.key, "act_out");
    assert_eq!(topic.mcap_schema_name, "synapse.topic.ActuatorOutputs");
    assert_eq!(topic.wire_type, "synapse.topic.ActuatorOutputsData");
    assert_eq!(topic.payload_size, Some(144));
    assert!(topic.fixed_layout);
    assert!(is_lowercase_hex(topic.schema_hash, 64));
    assert!(is_lowercase_hex(SCHEMA_SET_IDENTITY, 64));
    assert!(is_lowercase_hex(SCHEMA_PACKAGE_CONTRACT_IDENTITY, 64));
    assert!(is_lowercase_hex(LEGACY_SCHEMA_SET_HASH_128, 32));
    assert_eq!(
        u64::from_le_bytes(TROPIC_POSITIVE[0..8].try_into().unwrap()),
        PUBLISH_TIME_NS
    );

    let payload = ActuatorOutputsData(*TROPIC_POSITIVE);
    let bytes = record_actuator(&payload);
    let records = mcap::container::read::LinearReader::new(&bytes)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 7);
    assert!(matches!(
        &records[0],
        mcap::container::records::Record::Header(header)
            if header.profile == MCAP_PROFILE && header.library == "synapse-fbs-test/1"
    ));
    assert!(matches!(
        &records[1],
        mcap::container::records::Record::Metadata(metadata)
            if metadata.name == "synapse"
                && metadata.metadata[MCAP_SCHEMA_SET_HASH_KEY] == LEGACY_SCHEMA_SET_HASH_128
                && metadata.metadata[MCAP_SCHEMA_SET_IDENTITY_KEY] == SCHEMA_SET_IDENTITY
                && metadata.metadata[MCAP_SCHEMA_PACKAGE_CONTRACT_IDENTITY_KEY]
                    == SCHEMA_PACKAGE_CONTRACT_IDENTITY
    ));

    let schema_data = match &records[2] {
        mcap::container::records::Record::Schema { header, data } => {
            assert_eq!(header.name, topic.mcap_schema_name);
            assert_eq!(header.encoding, MCAP_SCHEMA_ENCODING);
            assert_eq!(data.as_ref(), schema_by_name("control").unwrap().bfbs);
            data.as_ref()
        }
        record => panic!("expected Schema record, received {record:?}"),
    };
    assert!(matches!(
        &records[3],
        mcap::container::records::Record::Channel(channel)
            if channel.topic == "act_out"
                && channel.message_encoding == MCAP_MESSAGE_ENCODING
                && channel.metadata[MCAP_TOPIC_ID_KEY] == "47"
    ));
    let message_data = match &records[4] {
        mcap::container::records::Record::Message { header, data } => {
            assert_eq!(header.sequence, 0);
            assert_eq!(header.log_time, LOG_TIME_NS);
            assert_eq!(header.publish_time, PUBLISH_TIME_NS);
            data.as_ref()
        }
        record => panic!("expected Message record, received {record:?}"),
    };

    let mut expected_wrapper = Vec::with_capacity(158);
    expected_wrapper.extend_from_slice(&4_u32.to_le_bytes());
    expected_wrapper.extend_from_slice(&(-148_i32).to_le_bytes());
    expected_wrapper.extend_from_slice(TROPIC_POSITIVE);
    expected_wrapper.extend_from_slice(&6_u16.to_le_bytes());
    expected_wrapper.extend_from_slice(&148_u16.to_le_bytes());
    expected_wrapper.extend_from_slice(&4_u16.to_le_bytes());
    assert_eq!(message_data, expected_wrapper);

    let reflected_payload = reflected_fixed_payload(
        schema_data,
        topic.mcap_schema_name,
        topic.wire_type,
        message_data,
    );
    assert_eq!(reflected_payload, TROPIC_POSITIVE);
    let decoded = flatbuffers::root::<ActuatorOutputs<'_>>(message_data).unwrap();
    let decoded_payload = decoded.data().unwrap();
    assert_eq!(decoded_payload.0.as_slice(), TROPIC_POSITIVE);

    let replay = record_actuator(decoded_payload);
    assert_eq!(replay, bytes);
}
