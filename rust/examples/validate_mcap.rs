#[cfg(not(feature = "mcap"))]
compile_error!("enable the synapse_fbs `mcap` feature");

use std::{env, fs};

use synapse_fbs::{
    mcap::container::{read::LinearReader, records::Record},
    schemas::schema_by_name,
    topic::Odometry,
    topic_catalog::{
        MCAP_MESSAGE_ENCODING, MCAP_PROFILE, MCAP_SCHEMA_ENCODING, MCAP_SCHEMA_SET_HASH_KEY,
        MCAP_TOPIC_ID_KEY, SCHEMA_SET_HASH,
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).ok_or("expected an MCAP path")?;
    let bytes = fs::read(path)?;
    let records = LinearReader::new(&bytes)?.collect::<Result<Vec<_>, _>>()?;
    assert!(matches!(
        &records[0],
        Record::Header(header) if header.profile == MCAP_PROFILE
    ));
    assert!(matches!(
        &records[1],
        Record::Metadata(metadata)
            if metadata.name == "synapse"
                && metadata.metadata[MCAP_SCHEMA_SET_HASH_KEY] == SCHEMA_SET_HASH
    ));
    assert!(matches!(
        &records[2],
        Record::Schema { header, data }
            if header.id == 1
                && header.name == "synapse.topic.Odometry"
                && header.encoding == MCAP_SCHEMA_ENCODING
                && data.as_ref() == schema_by_name("state").unwrap().bfbs
    ));
    assert!(matches!(
        &records[3],
        Record::Channel(channel)
            if channel.id == 0 && channel.schema_id == 1
                && channel.topic == "test/odom"
                && channel.message_encoding == MCAP_MESSAGE_ENCODING
                && channel.metadata[MCAP_TOPIC_ID_KEY] == "40"
    ));
    assert!(matches!(
        &records[4],
        Record::Channel(channel)
            if channel.id == 1 && channel.schema_id == 1
                && channel.topic == "test/odom/1"
                && channel.message_encoding == MCAP_MESSAGE_ENCODING
                && channel.metadata[MCAP_TOPIC_ID_KEY] == "40"
    ));
    assert!(matches!(
        &records[5],
        Record::Message { header, data }
            if header.channel_id == 0 && header.sequence == 0
                && header.log_time == 2000 && header.publish_time == 1000
                && flatbuffers::root::<Odometry<'_>>(data.as_ref()).is_ok()
    ));
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
