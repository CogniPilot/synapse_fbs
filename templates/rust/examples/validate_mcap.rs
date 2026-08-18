#[cfg(not(feature = "mcap"))]
compile_error!("enable the synapse_fbs `mcap` feature");

use std::{env, fs};

use flatbuffers_reflection::reflection::{self, BaseType};
use synapse_fbs::{
    mcap::{
        container::{read::LinearReader, records::Record},
        schema_metadata_matches_installed_contract,
    },
    schemas::schema_by_name,
    topic::ActuatorOutputs,
    topic_catalog::{
        MCAP_MESSAGE_ENCODING, MCAP_PROFILE, MCAP_SCHEMA_ENCODING, MCAP_TOPIC_ID_KEY, topic_by_id,
    },
};

const LOG_TIME_NS: u64 = 0x1112_1314_1516_1718;
const PUBLISH_TIME_NS: u64 = 0x0102_0304_0506_0708;
const TROPIC_POSITIVE: &[u8; 144] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/test-vectors/actuator_outputs/tropic-positive.bin"
));

fn reflected_fixed_payload<'a>(schema_data: &[u8], wrapped: &'a [u8]) -> &'a [u8] {
    let topic = topic_by_id(47).unwrap();
    let schema = reflection::root_as_schema(schema_data).unwrap();
    let root_object = schema
        .objects()
        .iter()
        .find(|object| object.name() == topic.mcap_schema_name)
        .expect("Schema.name is absent from the embedded BFBS");
    let data_field = root_object
        .fields()
        .iter()
        .find(|field| field.name() == "data")
        .expect("selected root wrapper has no data field");
    assert_eq!(data_field.type_().base_type(), BaseType::Obj);
    let payload_object = schema.objects().get(data_field.type_().index() as usize);
    assert!(payload_object.is_struct());
    assert_eq!(payload_object.name(), topic.wire_type);
    assert_eq!(payload_object.bytesize(), 144);

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
    let payload_offset = u16::from_le_bytes(
        wrapped[vtable_position + data_field.offset() as usize
            ..vtable_position + data_field.offset() as usize + 2]
            .try_into()
            .unwrap(),
    ) as usize;
    let payload_position = root_position + payload_offset;
    &wrapped[payload_position..payload_position + payload_object.bytesize() as usize]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).ok_or("expected an MCAP path")?;
    let bytes = fs::read(path)?;
    let records = LinearReader::new(&bytes)?.collect::<Result<Vec<_>, _>>()?;
    let topic = topic_by_id(47).unwrap();
    assert_eq!(topic.key, "act_out");
    assert_eq!(topic.schema_hash.len(), 64);
    assert!(matches!(
        &records[0],
        Record::Header(header) if header.profile == MCAP_PROFILE
    ));
    assert!(matches!(
        &records[1],
        Record::Metadata(metadata)
            if metadata.name == "synapse"
                && schema_metadata_matches_installed_contract(&metadata.metadata)
    ));
    let schema = schema_by_name("control").unwrap();
    let schema_id = records
        .iter()
        .find_map(|record| match record {
            Record::Schema { header, data }
                if header.name == topic.mcap_schema_name
                    && header.encoding == MCAP_SCHEMA_ENCODING
                    && data.as_ref() == schema.bfbs =>
            {
                Some(header.id)
            }
            _ => None,
        })
        .expect("missing canonical ActuatorOutputs BFBS schema");
    let channel_id = records
        .iter()
        .find_map(|record| match record {
            Record::Channel(channel)
                if channel.schema_id == schema_id
                    && channel.topic == "act_out"
                    && channel.message_encoding == MCAP_MESSAGE_ENCODING
                    && channel.metadata[MCAP_TOPIC_ID_KEY] == "47" =>
            {
                Some(channel.id)
            }
            _ => None,
        })
        .expect("missing canonical ActuatorOutputs channel");
    let message_data = records
        .iter()
        .find_map(|record| match record {
            Record::Message { header, data } if header.channel_id == channel_id => {
                assert_eq!(header.sequence, 0);
                assert_eq!(header.log_time, LOG_TIME_NS);
                assert_eq!(header.publish_time, PUBLISH_TIME_NS);
                Some(data.as_ref())
            }
            _ => None,
        })
        .expect("missing ActuatorOutputs message");
    assert_eq!(
        reflected_fixed_payload(schema.bfbs, message_data),
        TROPIC_POSITIVE
    );
    let decoded = flatbuffers::root::<ActuatorOutputs<'_>>(message_data)?;
    assert_eq!(decoded.data().unwrap().0.as_slice(), TROPIC_POSITIVE);
    assert!(
        records
            .iter()
            .any(|record| matches!(record, Record::DataEnd(_)))
    );
    assert!(
        records
            .iter()
            .any(|record| matches!(record, Record::Footer(_)))
    );
    Ok(())
}
