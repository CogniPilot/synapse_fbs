const CDR_ENCAPSULATION_HEADER_BYTES: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CdrPrimitive {
    U8,
    I16,
    U16,
    I32,
    U64,
    F32,
}

impl CdrPrimitive {
    const fn size(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::F32 => 4,
            Self::U64 => 8,
        }
    }

    const fn idl_type(self) -> &'static str {
        match self {
            Self::U8 => "uint8",
            Self::I16 => "int16",
            Self::U16 => "uint16",
            Self::I32 => "int32",
            Self::U64 => "uint64",
            Self::F32 => "float",
        }
    }

    const fn c_type(self) -> &'static str {
        match self {
            Self::U8 => "uint8_t",
            Self::I16 => "int16_t",
            Self::U16 => "uint16_t",
            Self::I32 => "int32_t",
            Self::U64 => "uint64_t",
            Self::F32 => "float",
        }
    }

    const fn rust_type(self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U64 => "u64",
            Self::F32 => "f32",
        }
    }

    const fn codec_suffix(self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U64 => "u64",
            Self::F32 => "f32",
        }
    }

    const fn rust_default(self) -> &'static str {
        match self {
            Self::F32 => "0.0",
            _ => "0",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CdrFieldSpec {
    name: &'static str,
    primitive: CdrPrimitive,
    count: usize,
    synapse_type: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct CdrProjectionSpec {
    synapse_topic: &'static str,
    topic_id: u16,
    synapse_schema_hash: &'static str,
    symbol: &'static str,
    ros_message: &'static str,
    ros_topic: &'static str,
    ros_type: &'static str,
    dds_type: &'static str,
    rihs01: &'static str,
    idl_path: &'static str,
    idl_sha256: &'static str,
    expected_body_bytes: usize,
    fields: &'static [CdrFieldSpec],
}

const OPTICAL_FLOW_VELOCITY_CDR_FIELDS: &[CdrFieldSpec] = &[
    CdrFieldSpec {
        name: "timestamp_ns",
        primitive: CdrPrimitive::U64,
        count: 1,
        synapse_type: "ulong",
    },
    CdrFieldSpec {
        name: "velocity_flu_m_s",
        primitive: CdrPrimitive::F32,
        count: 2,
        synapse_type: "synapse.types.Vec2f",
    },
    CdrFieldSpec {
        name: "distance_m",
        primitive: CdrPrimitive::F32,
        count: 1,
        synapse_type: "float",
    },
    CdrFieldSpec {
        name: "roll_rad",
        primitive: CdrPrimitive::F32,
        count: 1,
        synapse_type: "float",
    },
    CdrFieldSpec {
        name: "pitch_rad",
        primitive: CdrPrimitive::F32,
        count: 1,
        synapse_type: "float",
    },
    CdrFieldSpec {
        name: "quality",
        primitive: CdrPrimitive::U8,
        count: 1,
        synapse_type: "ubyte",
    },
    CdrFieldSpec {
        name: "flags",
        primitive: CdrPrimitive::U8,
        count: 1,
        synapse_type: "ubyte",
    },
    CdrFieldSpec {
        name: "time_status",
        primitive: CdrPrimitive::U8,
        count: 1,
        synapse_type: "synapse.types.TimeStatus",
    },
    CdrFieldSpec {
        name: "id",
        primitive: CdrPrimitive::U8,
        count: 1,
        synapse_type: "ubyte",
    },
];

const GNSS_FIX_CDR_FIELDS: &[CdrFieldSpec] = &[
    CdrFieldSpec {
        name: "timestamp_ns",
        primitive: CdrPrimitive::U64,
        count: 1,
        synapse_type: "ulong",
    },
    CdrFieldSpec {
        name: "time_unix_ns",
        primitive: CdrPrimitive::U64,
        count: 1,
        synapse_type: "ulong",
    },
    CdrFieldSpec {
        name: "latitude_deg_e7",
        primitive: CdrPrimitive::I32,
        count: 1,
        synapse_type: "int",
    },
    CdrFieldSpec {
        name: "longitude_deg_e7",
        primitive: CdrPrimitive::I32,
        count: 1,
        synapse_type: "int",
    },
    CdrFieldSpec {
        name: "altitude_msl_mm",
        primitive: CdrPrimitive::I32,
        count: 1,
        synapse_type: "int",
    },
    CdrFieldSpec {
        name: "altitude_ellipsoid_mm",
        primitive: CdrPrimitive::I32,
        count: 1,
        synapse_type: "int",
    },
    CdrFieldSpec {
        name: "horizontal_accuracy_mm",
        primitive: CdrPrimitive::U16,
        count: 1,
        synapse_type: "ushort",
    },
    CdrFieldSpec {
        name: "vertical_accuracy_mm",
        primitive: CdrPrimitive::U16,
        count: 1,
        synapse_type: "ushort",
    },
    CdrFieldSpec {
        name: "velocity_accuracy_mm_s",
        primitive: CdrPrimitive::U16,
        count: 1,
        synapse_type: "ushort",
    },
    CdrFieldSpec {
        name: "yaw_accuracy_cdeg",
        primitive: CdrPrimitive::U16,
        count: 1,
        synapse_type: "ushort",
    },
    CdrFieldSpec {
        name: "hdop_centi",
        primitive: CdrPrimitive::U16,
        count: 1,
        synapse_type: "ushort",
    },
    CdrFieldSpec {
        name: "vdop_centi",
        primitive: CdrPrimitive::U16,
        count: 1,
        synapse_type: "ushort",
    },
    CdrFieldSpec {
        name: "ground_speed_cm_s",
        primitive: CdrPrimitive::U16,
        count: 1,
        synapse_type: "ushort",
    },
    CdrFieldSpec {
        name: "course_over_ground_cdeg",
        primitive: CdrPrimitive::U16,
        count: 1,
        synapse_type: "ushort",
    },
    CdrFieldSpec {
        name: "yaw_cdeg",
        primitive: CdrPrimitive::U16,
        count: 1,
        synapse_type: "ushort",
    },
    CdrFieldSpec {
        name: "velocity_up_cm_s",
        primitive: CdrPrimitive::I16,
        count: 1,
        synapse_type: "short",
    },
    CdrFieldSpec {
        name: "flags",
        primitive: CdrPrimitive::U8,
        count: 1,
        synapse_type: "ubyte",
    },
    CdrFieldSpec {
        name: "fix_type",
        primitive: CdrPrimitive::U8,
        count: 1,
        synapse_type: "synapse.types.GnssFixType",
    },
    CdrFieldSpec {
        name: "satellites_used",
        primitive: CdrPrimitive::U8,
        count: 1,
        synapse_type: "ubyte",
    },
    CdrFieldSpec {
        name: "satellites_visible",
        primitive: CdrPrimitive::U8,
        count: 1,
        synapse_type: "ubyte",
    },
    CdrFieldSpec {
        name: "time_status",
        primitive: CdrPrimitive::U8,
        count: 1,
        synapse_type: "synapse.types.TimeStatus",
    },
    CdrFieldSpec {
        name: "id",
        primitive: CdrPrimitive::U8,
        count: 1,
        synapse_type: "ubyte",
    },
];

const CDR_PROJECTIONS: &[CdrProjectionSpec] = &[
    CdrProjectionSpec {
        synapse_topic: "GnssFix",
        topic_id: 8,
        synapse_schema_hash: "521ee174bede79165f703d63657d51247cb78c8bff6c303e8a4a43cdadc5b421",
        symbol: "gnss_fix",
        ros_message: "GnssFix",
        ros_topic: "/synapse/gnss_fix",
        ros_type: "synapse_msgs/msg/GnssFix",
        dds_type: "synapse_msgs::msg::dds_::GnssFix_",
        rihs01: "RIHS01_ac8d665c1bf6f81796d95bdd6a2285537bbfcb34869ba0e042f8ce24f75d9f0e",
        idl_path: "cdr/idl/synapse_msgs/msg/GnssFix.idl",
        idl_sha256: "d24400931aeb01b33d29f25ac2a846069b58c9656bd6cbcef405ae1d36406208",
        expected_body_bytes: 60,
        fields: GNSS_FIX_CDR_FIELDS,
    },
    CdrProjectionSpec {
        synapse_topic: "OpticalFlowVelocity",
        topic_id: 10,
        synapse_schema_hash: "743ff5b0a1f9f58725a1ee2fd04833a89d0d6f1275061def2fc4582ddcd3a3fe",
        symbol: "optical_flow_velocity",
        ros_message: "OpticalFlowVelocity",
        ros_topic: "/synapse/optical_flow_velocity",
        ros_type: "synapse_msgs/msg/OpticalFlowVelocity",
        dds_type: "synapse_msgs::msg::dds_::OpticalFlowVelocity_",
        rihs01: "RIHS01_8f46bb3da905598105f99e502394842afa66d849de841143565a193074829d09",
        idl_path: "cdr/idl/synapse_msgs/msg/OpticalFlowVelocity.idl",
        idl_sha256: "e4aabe78567ea3a8a118402fd3a2146b118d7b7f1e6e60dbac112afb782494ec",
        expected_body_bytes: 32,
        fields: OPTICAL_FLOW_VELOCITY_CDR_FIELDS,
    },
];

#[derive(Clone, Debug, Serialize)]
struct CdrFieldTemplateEntry {
    name: String,
    idl_type: &'static str,
    c_type: &'static str,
    rust_type: &'static str,
    rust_default: &'static str,
    codec_suffix: &'static str,
    count: usize,
    offset: usize,
}

#[derive(Clone, Debug, Serialize)]
struct CdrProjectionTemplateEntry {
    synapse_topic: String,
    topic_id: u16,
    synapse_schema_hash: String,
    symbol: String,
    ros_message: String,
    ros_topic: String,
    ros_type: String,
    dds_type: String,
    rihs01: String,
    idl_path: String,
    package_idl_path: String,
    idl_sha256: String,
    body_bytes: usize,
    total_bytes: usize,
    trailing_padding: usize,
    needs_index: bool,
    fields: Vec<CdrFieldTemplateEntry>,
}

#[derive(Clone, Debug, Serialize)]
struct CdrProjectionContext {
    version: u8,
    encoding: &'static str,
    scalar_byte_order: &'static str,
    encapsulation_header_bytes: usize,
    projection_set_identity: String,
    projections: Vec<CdrProjectionTemplateEntry>,
}

fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

fn cdr_projection_entry(spec: &CdrProjectionSpec) -> Result<CdrProjectionTemplateEntry> {
    let mut cursor = 0usize;
    let mut fields = Vec::new();
    for field in spec.fields {
        if field.count == 0 {
            return fail(format!(
                "CDR field {}.{} has a zero element count",
                spec.ros_type, field.name
            ));
        }
        cursor = align_up(cursor, field.primitive.size());
        fields.push(CdrFieldTemplateEntry {
            name: field.name.to_string(),
            idl_type: field.primitive.idl_type(),
            c_type: field.primitive.c_type(),
            rust_type: field.primitive.rust_type(),
            rust_default: field.primitive.rust_default(),
            codec_suffix: field.primitive.codec_suffix(),
            count: field.count,
            offset: cursor,
        });
        cursor = cursor
            .checked_add(field.primitive.size().checked_mul(field.count).ok_or_else(|| {
                io::Error::other(format!("CDR field size overflows for {}", field.name))
            })?)
            .ok_or_else(|| io::Error::other(format!("CDR layout overflows for {}", spec.ros_type)))?;
    }
    let field_bytes = cursor;
    cursor = align_up(cursor, 4);
    if cursor != spec.expected_body_bytes {
        return fail(format!(
            "CDR body for {} is {cursor} bytes, expected {}",
            spec.ros_type, spec.expected_body_bytes
        ));
    }
    if !spec.idl_path.starts_with("cdr/") {
        return fail(format!(
            "CDR IDL path is not under cdr/: {}",
            spec.idl_path
        ));
    }
    Ok(CdrProjectionTemplateEntry {
        synapse_topic: spec.synapse_topic.to_string(),
        topic_id: spec.topic_id,
        synapse_schema_hash: spec.synapse_schema_hash.to_string(),
        symbol: spec.symbol.to_string(),
        ros_message: spec.ros_message.to_string(),
        ros_topic: spec.ros_topic.to_string(),
        ros_type: spec.ros_type.to_string(),
        dds_type: spec.dds_type.to_string(),
        rihs01: spec.rihs01.to_string(),
        idl_path: spec.idl_path.to_string(),
        package_idl_path: spec.idl_path.to_string(),
        idl_sha256: spec.idl_sha256.to_string(),
        body_bytes: cursor,
        total_bytes: cursor + CDR_ENCAPSULATION_HEADER_BYTES,
        trailing_padding: cursor - field_bytes,
        needs_index: cursor != field_bytes || spec.fields.iter().any(|field| field.count > 1),
        fields,
    })
}

fn synapse_scalar_cdr_primitive(type_name: &str) -> Option<CdrPrimitive> {
    Some(match type_name {
        "ubyte" | "uint8" => CdrPrimitive::U8,
        "short" | "int16" => CdrPrimitive::I16,
        "ushort" | "uint16" => CdrPrimitive::U16,
        "int" | "int32" => CdrPrimitive::I32,
        "ulong" | "uint64" => CdrPrimitive::U64,
        "float" | "float32" => CdrPrimitive::F32,
        _ => return None,
    })
}

fn collect_synapse_cdr_primitives(
    schema: &CompiledSchema,
    type_name: &str,
    primitives: &mut Vec<CdrPrimitive>,
) -> Result<()> {
    let trimmed = type_name.trim();
    if let Some(inner) = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        let (element_type, count_text) = inner.rsplit_once(':').ok_or_else(|| {
            io::Error::other(format!(
                "unbounded Synapse type {type_name} cannot back a fixed CDR field"
            ))
        })?;
        let count = count_text.parse::<usize>().map_err(|error| {
            io::Error::other(format!(
                "invalid fixed array count in Synapse type {type_name}: {error}"
            ))
        })?;
        for _ in 0..count {
            collect_synapse_cdr_primitives(schema, element_type, primitives)?;
        }
        return Ok(());
    }

    let lookup = type_lookup_name(trimmed);
    if let Some(primitive) = synapse_scalar_cdr_primitive(&lookup) {
        primitives.push(primitive);
        return Ok(());
    }
    let (_, entity) = find_schema_entity(schema, &lookup)
        .ok_or_else(|| io::Error::other(format!("unknown Synapse CDR source type {type_name}")))?;
    match entity.kind {
        SchemaEntityKind::Enum => {
            let base = entity
                .value_type
                .as_deref()
                .map(enum_base_type)
                .unwrap_or_default();
            let primitive = synapse_scalar_cdr_primitive(base).ok_or_else(|| {
                io::Error::other(format!(
                    "Synapse enum {type_name} has unsupported CDR base type {base}"
                ))
            })?;
            primitives.push(primitive);
        }
        SchemaEntityKind::Struct => {
            for member in &entity.members {
                let member_type = member.type_name.as_deref().ok_or_else(|| {
                    io::Error::other(format!(
                        "Synapse struct {type_name} member {} has no reflected type",
                        member.name
                    ))
                })?;
                collect_synapse_cdr_primitives(schema, member_type, primitives)?;
            }
        }
        _ => {
            return fail(format!(
                "Synapse CDR source type {type_name} is a {}, expected a scalar, enum, or fixed-layout struct",
                entity.kind.as_str()
            ));
        }
    }
    Ok(())
}

fn validate_cdr_projection_source(schema: &CompiledSchema, spec: &CdrProjectionSpec) -> Result<()> {
    let topics = topic_entries(schema)?;
    let topic = topics
        .iter()
        .find(|topic| topic.name == spec.synapse_topic)
        .ok_or_else(|| io::Error::other(format!("CDR topic {} is absent", spec.synapse_topic)))?;
    if topic.id != spec.topic_id
        || topic.type_schema_hash != spec.synapse_schema_hash
        || !topic.fixed_layout
    {
        return fail(format!(
            "CDR projection {} requires fixed TopicId {} with source identity {}, found id {} with source identity {} and fixed_layout={}",
            spec.synapse_topic,
            spec.topic_id,
            spec.synapse_schema_hash,
            topic.id,
            topic.type_schema_hash,
            topic.fixed_layout
        ));
    }
    let payload_name = topic.payload_type.as_deref().ok_or_else(|| {
        io::Error::other(format!("CDR projection {} has no payload type", spec.synapse_topic))
    })?;
    let (_, payload) = find_schema_entity(schema, payload_name).ok_or_else(|| {
        io::Error::other(format!("CDR payload type {payload_name} is absent"))
    })?;
    if payload.members.len() != spec.fields.len() {
        return fail(format!(
            "CDR projection {} has {} fields but reflected payload has {}",
            spec.synapse_topic,
            spec.fields.len(),
            payload.members.len()
        ));
    }
    for (member, field) in payload.members.iter().zip(spec.fields) {
        let actual_type = member.type_name.as_deref().unwrap_or_default();
        if member.name != field.name || actual_type != field.synapse_type {
            return fail(format!(
                "CDR projection {} field mismatch: expected {} {}, found {} {}",
                spec.synapse_topic, field.synapse_type, field.name, actual_type, member.name
            ));
        }
        let mut source_primitives = Vec::new();
        collect_synapse_cdr_primitives(schema, actual_type, &mut source_primitives)?;
        if source_primitives.len() != field.count
            || source_primitives
                .iter()
                .any(|primitive| *primitive != field.primitive)
        {
            return fail(format!(
                "CDR projection {} field {} maps Synapse type {} to {} {} elements, but its reflected scalar shape is {:?}",
                spec.synapse_topic,
                field.name,
                actual_type,
                field.primitive.idl_type(),
                field.count,
                source_primitives
            ));
        }
    }
    Ok(())
}

fn cdr_projection_identity(entries: &[CdrProjectionTemplateEntry]) -> String {
    let mut transcript = IdentityTranscript::new("synapse-cdr-projection-set-v1");
    transcript.field("CDRv1");
    transcript.field("little-endian");
    transcript.field(CDR_ENCAPSULATION_HEADER_BYTES.to_string());
    transcript.field(entries.len().to_string());
    for entry in entries {
        for value in [
            entry.synapse_topic.as_str(),
            &entry.topic_id.to_string(),
            entry.synapse_schema_hash.as_str(),
            entry.ros_topic.as_str(),
            entry.ros_type.as_str(),
            entry.dds_type.as_str(),
            entry.rihs01.as_str(),
            entry.idl_path.as_str(),
            entry.idl_sha256.as_str(),
            &entry.body_bytes.to_string(),
            &entry.total_bytes.to_string(),
        ] {
            transcript.field(value);
        }
        transcript.field(entry.fields.len().to_string());
        for field in &entry.fields {
            transcript.field(&field.name);
            transcript.field(field.idl_type);
            transcript.field(field.count.to_string());
            transcript.field(field.offset.to_string());
        }
    }
    transcript.finish()
}

fn cdr_projection_context(root: &Path, schema: &CompiledSchema) -> Result<Value> {
    let mut entries = Vec::new();
    let mut topic_ids = BTreeSet::new();
    let mut ros_types = BTreeSet::new();
    for spec in CDR_PROJECTIONS {
        validate_cdr_projection_source(schema, spec)?;
        if !topic_ids.insert(spec.topic_id) || !ros_types.insert(spec.ros_type) {
            return fail(format!("duplicate CDR projection for {}", spec.ros_type));
        }
        let path = root.join(spec.idl_path);
        let actual_hash = sha256_hex(&path)?;
        if actual_hash != spec.idl_sha256 {
            return fail(format!(
                "CDR IDL hash mismatch for {}: expected {}, found {actual_hash}",
                spec.ros_type, spec.idl_sha256
            ));
        }
        if !spec.rihs01.starts_with("RIHS01_") || spec.rihs01.len() != 71 {
            return fail(format!("invalid RIHS01 identity for {}", spec.ros_type));
        }
        entries.push(cdr_projection_entry(spec)?);
    }
    let projection_set_identity = cdr_projection_identity(&entries);
    Ok(Value::from_serialize(CdrProjectionContext {
        version: 1,
        encoding: "CDRv1",
        scalar_byte_order: "little-endian",
        encapsulation_header_bytes: CDR_ENCAPSULATION_HEADER_BYTES,
        projection_set_identity,
        projections: entries,
    }))
}

fn validate_cdr_idl_sources(
    root: &Path,
    templates: &Templates,
    schema: &CompiledSchema,
) -> Result<()> {
    let context = cdr_projection_context(root, schema)?;
    let serialized = serde_json::to_value(&context)?;
    let projections = serialized["projections"]
        .as_array()
        .ok_or_else(|| io::Error::other("CDR projection context has no projections"))?;
    for projection in projections {
        let rendered = format!(
            "{}\n",
            templates
                .env
                .get_template("xtask/cdr/idl.jinja")?
                .render(Value::from_serialize(projection))?
        );
        let path = projection["idl_path"]
            .as_str()
            .ok_or_else(|| io::Error::other("CDR projection has no IDL path"))?;
        let committed = fs::read_to_string(root.join(path))?;
        if rendered != committed {
            return fail(format!("CDR IDL is not canonical: {path}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod cdr_projection_tests {
    use super::*;

    #[test]
    fn initial_cdr_layouts_match_ros_jazzy_sizes() {
        let gnss = cdr_projection_entry(&CDR_PROJECTIONS[0]).unwrap();
        assert_eq!(gnss.body_bytes, 60);
        assert_eq!(gnss.total_bytes, 64);
        let optical = cdr_projection_entry(&CDR_PROJECTIONS[1]).unwrap();
        assert_eq!(optical.body_bytes, 32);
        assert_eq!(optical.total_bytes, 36);
    }

    #[test]
    fn accepted_optical_flow_authority_is_unchanged() {
        let optical = &CDR_PROJECTIONS[1];
        assert_eq!(
            optical.rihs01,
            "RIHS01_8f46bb3da905598105f99e502394842afa66d849de841143565a193074829d09"
        );
        assert_eq!(
            optical.idl_sha256,
            "e4aabe78567ea3a8a118402fd3a2146b118d7b7f1e6e60dbac112afb782494ec"
        );
        assert_eq!(
            optical.synapse_schema_hash,
            "743ff5b0a1f9f58725a1ee2fd04833a89d0d6f1275061def2fc4582ddcd3a3fe"
        );
    }
}
