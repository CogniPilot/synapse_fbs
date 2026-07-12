#![cfg(feature = "mcap")]

use std::io::Cursor;

use synapse_fbs::{
    mcap::{self, TimeBasis, Writer},
    schemas::schema_by_name,
    topic::{Odometry, OdometryData},
    topic_catalog::{
        MCAP_MESSAGE_ENCODING, MCAP_PROFILE, MCAP_SCHEMA_ENCODING, MCAP_SCHEMA_SET_HASH_KEY,
        MCAP_TOPIC_ID_KEY, SCHEMA_SET_HASH, topic_by_name,
    },
};

#[test]
fn writes_frozen_profile_with_embedded_bfbs() {
    let cursor = Cursor::new(Vec::new());
    let mut writer = Writer::new(
        cursor,
        "synapse-fbs-test/1",
        "0123456789abcdef0123456789abcdef",
        "test-vehicle",
        TimeBasis::MonotonicBoot,
    )
    .unwrap();
    let topic = topic_by_name("Odometry").unwrap();
    let mut channel = writer.add_fixed_topic::<OdometryData>("test/odom").unwrap();
    writer
        .write_fixed(&mut channel, 2_000, 1_000, &OdometryData::default())
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();

    let records = mcap::container::read::LinearReader::new(&bytes)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(matches!(
        &records[0],
        mcap::container::records::Record::Header(header)
            if header.profile == MCAP_PROFILE && header.library == "synapse-fbs-test/1"
    ));
    assert!(matches!(
        &records[1],
        mcap::container::records::Record::Metadata(metadata)
            if metadata.name == "synapse"
                && metadata.metadata[MCAP_SCHEMA_SET_HASH_KEY] == SCHEMA_SET_HASH
    ));
    assert!(matches!(
        &records[2],
        mcap::container::records::Record::Schema { header, data }
            if header.name == topic.mcap_schema_name
                && header.encoding == MCAP_SCHEMA_ENCODING
                && data.as_ref() == schema_by_name("state").unwrap().bfbs
    ));
    assert!(matches!(
        &records[3],
        mcap::container::records::Record::Channel(channel)
            if channel.topic == "test/odom"
                && channel.message_encoding == MCAP_MESSAGE_ENCODING
                && channel.metadata[MCAP_TOPIC_ID_KEY] == topic.id.to_string()
    ));
    assert!(matches!(
        &records[4],
        mcap::container::records::Record::Message { header, data }
            if header.sequence == 0 && header.log_time == 2_000
                && header.publish_time == 1_000
                && flatbuffers::root::<Odometry<'_>>(data.as_ref()).is_ok()
    ));
}
