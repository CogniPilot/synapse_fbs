#[derive(Clone, Debug)]
struct CompiledSchema {
    files: Vec<SchemaFile>,
    strict_type_hashes: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
struct SchemaFile {
    name: String,
    bfbs_sha256: String,
    legacy_bfbs_hash_128: String,
    entities: Vec<SchemaEntity>,
    root_type: Option<String>,
    file_identifier: Option<String>,
}

#[derive(Clone, Debug)]
struct SchemaEntity {
    kind: SchemaEntityKind,
    name: String,
    namespace: String,
    value_type: Option<String>,
    members: Vec<SchemaMember>,
    byte_size: Option<usize>,
}

#[derive(Clone, Debug)]
struct SchemaMember {
    name: String,
    type_name: Option<String>,
    value: Option<String>,
    offset: Option<usize>,
}

#[derive(Clone, Debug)]
struct TopicEntry {
    id: u16,
    name: String,
    key: String,
    root_table: String,
    root_table_namespace: String,
    payload_type: Option<String>,
    payload_type_namespace: Option<String>,
    payload_size: Option<usize>,
    schema_file: String,
    schema_artifact_sha256: String,
    wire_type: String,
    type_schema_hash: String,
    legacy_schema_file_hash_128: String,
    fixed_layout: bool,
    multi_instance: bool,
    scope: &'static str,
    encoding: &'static str,
    description: String,
}

#[derive(Clone, Copy, Debug)]
struct CommandPayloadMetadata {
    encoding: &'static str,
    size: Option<usize>,
}

/// One queryable command service with its full wire contract: request and
/// reply types each carry the same transitive schema hash topics use, so the
/// compatibility allowlist and the schema-set hash cover command payloads.
#[derive(Clone, Debug, Serialize)]
struct CommandEntry {
    id: u16,
    name: String,
    key: String,
    request_type: String,
    request_type_schema_hash: String,
    legacy_request_schema_file_hash_128: String,
    request_encoding: &'static str,
    request_size: Option<usize>,
    reply_type: String,
    reply_type_schema_hash: String,
    legacy_reply_schema_file_hash_128: String,
    reply_encoding: &'static str,
    reply_size: Option<usize>,
    description: String,
}

#[derive(Clone, Debug, Serialize)]
struct TopicCatalogContext {
    version: u8,
    flatbuffer_value_media_type: &'static str,
    struct_value_media_type: &'static str,
    type_schema_hash_algorithm: &'static str,
    topic_instance_key_grammar: &'static str,
    schema_set_identity: String,
    schema_package_contract_identity: String,
    legacy_schema_set_hash_128: String,
    mcap_profile: &'static str,
    mcap_schema_encoding: &'static str,
    mcap_message_encoding: &'static str,
    mcap_metadata_name: &'static str,
    mcap_schema_set_hash_key: &'static str,
    mcap_schema_set_identity_key: &'static str,
    mcap_schema_package_contract_identity_key: &'static str,
    mcap_session_id_key: &'static str,
    mcap_source_key: &'static str,
    mcap_time_basis_key: &'static str,
    mcap_time_basis_monotonic_boot: &'static str,
    mcap_time_basis_unix_epoch: &'static str,
    mcap_time_basis_correlated: &'static str,
    mcap_topic_id_key: &'static str,
    cmd_key_prefix: &'static str,
    meta_key_prefix: &'static str,
    liveliness_key_prefix: &'static str,
    mcap_schemas: Vec<McapSchemaTemplateEntry>,
    topics: Vec<TopicTemplateEntry>,
    commands: Vec<CommandEntry>,
}

#[derive(Clone, Debug, Serialize)]
struct McapSchemaTemplateEntry {
    symbol: String,
    schema_file: String,
}

#[derive(Clone, Debug, Serialize)]
struct TopicTemplateEntry {
    id: u16,
    name: String,
    key: String,
    root_table: String,
    payload_type: Option<String>,
    payload_size: Option<usize>,
    schema_file: String,
    schema_artifact_sha256: String,
    mcap_schema_name: String,
    mcap_schema_file: String,
    mcap_schema_symbol: String,
    wire_type: String,
    type_schema_hash: String,
    legacy_schema_file_hash_128: String,
    fixed_layout: bool,
    multi_instance: bool,
    scope: &'static str,
    encoding: &'static str,
    description: String,
    root_table_rust_path: String,
    payload_type_rust_path: String,
    root_table_qualified: String,
    payload_type_qualified: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SchemaEntityKind {
    Struct,
    Table,
    Enum,
    Union,
}

impl SchemaEntityKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Struct => "struct",
            Self::Table => "table",
            Self::Enum => "enum",
            Self::Union => "union",
        }
    }
}

fn load_compiled_schema(bfbs_dir: &Path) -> Result<CompiledSchema> {
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();

    // SCHEMAS is dependency ordered. FlatCC emits each top-level schema with
    // all of its includes, so the declarations first seen in each BFBS belong
    // to that source file. xtask only adapts compiler reflection data; it does
    // not read or interpret FBS source.
    for schema_file in SCHEMAS {
        let stem = Path::new(schema_file)
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| io::Error::other(format!("schema path has no stem: {schema_file}")))?;
        let path = bfbs_dir.join(format!("{stem}.bfbs"));
        let bytes = fs::read(&path)?;
        let bfbs_sha256 = sha256_hex(&path)?;
        let legacy_bfbs_hash_128 = bfbs_sha256[..32].to_string();
        let schema = reflection::root_as_schema(&bytes)
            .map_err(|err| io::Error::other(format!("invalid {}: {err}", path.display())))?;
        let mut entities = Vec::new();

        for object in schema.objects() {
            if seen.insert(object.name().to_string()) {
                entities.push(reflected_object(&schema, object)?);
            }
        }
        for reflected_enum in schema.enums() {
            if seen.insert(reflected_enum.name().to_string()) {
                entities.push(reflected_enum_entity(&schema, reflected_enum)?);
            }
        }
        entities.sort_by(|left, right| left.name.cmp(&right.name));

        files.push(SchemaFile {
            name: (*schema_file).to_string(),
            bfbs_sha256,
            legacy_bfbs_hash_128,
            entities,
            root_type: schema.root_table().map(|root| root.name().to_string()),
            file_identifier: schema
                .file_ident()
                .filter(|identifier| !identifier.is_empty())
                .map(str::to_string),
        });
    }

    let strict_type_hashes = strict_type_hashes(bfbs_dir)?;
    Ok(CompiledSchema {
        files,
        strict_type_hashes,
    })
}

fn reflected_object(
    schema: &reflection::Schema<'_>,
    object: reflection::Object<'_>,
) -> Result<SchemaEntity> {
    let (namespace, name) = split_qualified_name(object.name());
    let mut fields = object
        .fields()
        .into_iter()
        .filter(|field| field.type_().base_type() != BaseType::UType)
        .collect::<Vec<_>>();
    if object.is_struct() {
        fields.sort_by_key(|field| field.offset());
    } else {
        fields.sort_by_key(|field| field.id());
    }
    let members = fields
        .into_iter()
        .map(|field| {
            let type_name = reflected_type_name(schema, field.type_(), &namespace)?;
            Ok(SchemaMember {
                name: field.name().to_string(),
                value: reflected_default(schema, field, &type_name),
                type_name: Some(type_name),
                offset: object.is_struct().then(|| usize::from(field.offset())),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(SchemaEntity {
        kind: if object.is_struct() {
            SchemaEntityKind::Struct
        } else {
            SchemaEntityKind::Table
        },
        name,
        namespace,
        value_type: None,
        members,
        byte_size: object
            .is_struct()
            .then(|| usize::try_from(object.bytesize()))
            .transpose()?,
    })
}

fn reflected_enum_entity(
    schema: &reflection::Schema<'_>,
    reflected_enum: reflection::Enum<'_>,
) -> Result<SchemaEntity> {
    let (namespace, name) = split_qualified_name(reflected_enum.name());
    let bit_flags = reflected_enum.attributes().is_some_and(|attributes| {
        attributes
            .into_iter()
            .any(|attribute| attribute.key() == "bit_flags")
    });
    let members = reflected_enum
        .values()
        .into_iter()
        .filter(|value| !(reflected_enum.is_union() && value.name() == "NONE"))
        .map(|value| {
            let type_name = value
                .union_type()
                .filter(|type_| type_.base_type() != BaseType::None)
                .map(|type_| reflected_type_name(schema, type_, &namespace))
                .transpose()?;
            Ok(SchemaMember {
                name: value.name().to_string(),
                type_name,
                value: (!reflected_enum.is_union() && !bit_flags)
                    .then(|| value.value().to_string()),
                offset: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let value_type = if reflected_enum.is_union() {
        None
    } else {
        let base = reflected_scalar_name(reflected_enum.underlying_type().base_type())?;
        Some(if bit_flags {
            format!("{base} (bit_flags)")
        } else {
            base.to_string()
        })
    };

    Ok(SchemaEntity {
        kind: if reflected_enum.is_union() {
            SchemaEntityKind::Union
        } else {
            SchemaEntityKind::Enum
        },
        name,
        namespace,
        value_type,
        members,
        byte_size: None,
    })
}

fn reflected_type_name(
    schema: &reflection::Schema<'_>,
    type_: reflection::Type<'_>,
    namespace: &str,
) -> Result<String> {
    let is_vector = type_.base_type() == BaseType::Vector;
    let is_array = type_.base_type() == BaseType::Array;
    let base = if is_vector || is_array {
        type_.element()
    } else {
        type_.base_type()
    };
    let atom = if type_.index() >= 0 {
        let qualified = if base == BaseType::Obj {
            schema.objects().get(usize::try_from(type_.index())?).name()
        } else {
            schema.enums().get(usize::try_from(type_.index())?).name()
        };
        local_type_name(namespace, qualified)
    } else {
        reflected_scalar_name(base)?.to_string()
    };

    Ok(if is_vector {
        format!("[{atom}]")
    } else if is_array {
        format!("[{atom}:{}]", type_.fixed_length())
    } else {
        atom
    })
}

fn reflected_default(
    schema: &reflection::Schema<'_>,
    field: reflection::Field<'_>,
    type_name: &str,
) -> Option<String> {
    if field.default_integer() == 0 && field.default_real() == 0.0 {
        return None;
    }
    if field.type_().index() >= 0 && field.type_().base_type() != BaseType::Obj {
        let reflected_enum = schema
            .enums()
            .get(usize::try_from(field.type_().index()).ok()?);
        return reflected_enum
            .values()
            .into_iter()
            .find(|value| value.value() == field.default_integer())
            .map(|value| value.name().to_string());
    }
    if matches!(
        field.type_().base_type(),
        BaseType::Float | BaseType::Double
    ) {
        Some(field.default_real().to_string())
    } else if is_scalar_type(type_name) {
        Some(field.default_integer().to_string())
    } else {
        None
    }
}

fn reflected_scalar_name(base: BaseType) -> Result<&'static str> {
    match base {
        BaseType::Bool => Ok("bool"),
        BaseType::Byte => Ok("byte"),
        BaseType::UByte => Ok("ubyte"),
        BaseType::Short => Ok("short"),
        BaseType::UShort => Ok("ushort"),
        BaseType::Int => Ok("int"),
        BaseType::UInt => Ok("uint"),
        BaseType::Long => Ok("long"),
        BaseType::ULong => Ok("ulong"),
        BaseType::Float => Ok("float"),
        BaseType::Double => Ok("double"),
        BaseType::String => Ok("string"),
        _ => fail(format!("unsupported reflected FlatBuffers type {base:?}")),
    }
}

fn split_qualified_name(qualified: &str) -> (String, String) {
    qualified
        .rsplit_once('.')
        .map(|(namespace, name)| (namespace.to_string(), name.to_string()))
        .unwrap_or_else(|| (String::new(), qualified.to_string()))
}

fn local_type_name(namespace: &str, qualified: &str) -> String {
    let (target_namespace, name) = split_qualified_name(qualified);
    if target_namespace == namespace {
        name
    } else {
        qualified.to_string()
    }
}

/// Check TOPIC_KEYS is a valid, exhaustive map for the TopicId enum: one
/// well-formed, unique, non-reserved key per topic and no stale entries.
fn validate_topic_keys(topic_enum: &SchemaEntity) -> Result<()> {
    let mut problems = Vec::new();
    let members: BTreeSet<&str> = topic_enum
        .members
        .iter()
        .map(|member| member.name.as_str())
        .filter(|name| *name != "Unknown")
        .collect();

    let mut seen_names = BTreeSet::new();
    let mut seen_keys = BTreeSet::new();
    for (name, key) in TOPIC_KEYS {
        if !seen_names.insert(*name) {
            problems.push(format!("TOPIC_KEYS lists {name} more than once"));
        }
        if !seen_keys.insert(*key) {
            problems.push(format!("topic key '{key}' is used more than once"));
        }
        if !members.contains(name) {
            problems.push(format!("TOPIC_KEYS entry {name} is not a TopicId member"));
        }
        if RESERVED_KEY_SEGMENTS.contains(key) {
            problems.push(format!("topic key '{key}' is a reserved key segment"));
        }
        let well_formed = key.starts_with(|c: char| c.is_ascii_lowercase())
            && key
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !well_formed {
            problems.push(format!(
                "topic key '{key}' must be lowercase snake_case starting with a letter"
            ));
        }
    }
    for member in &members {
        if !seen_names.contains(member) {
            problems.push(format!("TopicId {member} has no TOPIC_KEYS entry"));
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        fail(format!(
            "topic key table is invalid:\n{}",
            problems.join("\n")
        ))
    }
}

fn topic_key(member: &str) -> Result<&'static str> {
    TOPIC_KEYS
        .iter()
        .find(|(name, _)| *name == member)
        .map(|(_, key)| *key)
        .ok_or_else(|| io::Error::other(format!("TopicId {member} has no TOPIC_KEYS entry")).into())
}

fn topic_entries(schema: &CompiledSchema) -> Result<Vec<TopicEntry>> {
    let topic_enum = schema
        .files
        .iter()
        .flat_map(|file| &file.entities)
        .find(|entity| entity.kind == SchemaEntityKind::Enum && entity.name == "TopicId")
        .ok_or_else(|| io::Error::other("TopicId enum not found"))?;
    validate_topic_keys(topic_enum)?;

    let mut topics = Vec::new();
    for member in &topic_enum.members {
        let Some(value) = &member.value else {
            return fail(format!(
                "TopicId {} is missing an explicit value",
                member.name
            ));
        };
        let id = value.parse::<u16>().map_err(|err| {
            io::Error::other(format!(
                "TopicId {} has invalid value {value}: {err}",
                member.name
            ))
        })?;
        if id == 0 || member.name == "Unknown" {
            continue;
        }

        let (schema_file, root_table) =
            find_schema_entity(schema, &member.name).ok_or_else(|| {
                io::Error::other(format!(
                    "TopicId {} does not match a root table in the schema",
                    member.name
                ))
            })?;
        if root_table.kind != SchemaEntityKind::Table {
            return fail(format!(
                "TopicId {} resolves to {} {}, expected a table",
                member.name,
                root_table.kind.as_str(),
                root_table.name
            ));
        }

        let payload_type = thin_root_wrapper_payload(root_table).map(type_lookup_name);
        let payload_entity = payload_type
            .as_ref()
            .and_then(|payload| find_schema_entity(schema, payload));
        let payload_type_namespace = payload_entity.map(|(_, entity)| entity.namespace.clone());
        let fixed_layout =
            payload_entity.is_some_and(|(_, entity)| entity.kind == SchemaEntityKind::Struct);
        let payload_size = payload_entity.and_then(|(_, entity)| entity.byte_size);
        let multi_instance = fixed_layout
            && payload_entity
                .is_some_and(|(_, entity)| entity.members.iter().any(|member| member.name == "id"));
        let scope = if VEHICLE_SCOPE_TOPICS.contains(&member.name.as_str()) {
            "vehicle"
        } else {
            "any"
        };
        let encoding = if fixed_layout { "struct" } else { "table" };
        let wire_entity = if fixed_layout {
            payload_entity
                .map(|(_, entity)| entity)
                .expect("fixed-layout topic has a payload entity")
        } else {
            root_table
        };
        let wire_type = qualified_name(&wire_entity.namespace, &wire_entity.name);
        let type_schema_hash = schema
            .strict_type_hashes
            .get(&wire_type)
            .cloned()
            .ok_or_else(|| io::Error::other(format!("no strict type identity for {wire_type}")))?;
        let legacy_schema_file_hash_128 = schema_file.legacy_bfbs_hash_128.clone();
        topics.push(TopicEntry {
            id,
            name: member.name.clone(),
            key: topic_key(&member.name)?.to_string(),
            root_table: root_table.name.clone(),
            root_table_namespace: root_table.namespace.clone(),
            payload_type,
            payload_type_namespace,
            payload_size,
            schema_file: schema_file.name.clone(),
            schema_artifact_sha256: schema_file.bfbs_sha256.clone(),
            wire_type,
            type_schema_hash,
            legacy_schema_file_hash_128,
            fixed_layout,
            multi_instance,
            scope,
            encoding,
            description: String::new(),
        });
    }

    Ok(topics)
}

fn command_payload_metadata(
    schema: &CompiledSchema,
    type_name: &str,
) -> Result<CommandPayloadMetadata> {
    let lookup = type_lookup_name(type_name);
    let Some((_, entity)) = find_schema_entity(schema, &lookup) else {
        return fail(format!(
            "command type {type_name} does not resolve to a schema entity"
        ));
    };
    match entity.kind {
        SchemaEntityKind::Struct => Ok(CommandPayloadMetadata {
            encoding: "struct",
            size: entity.byte_size,
        }),
        SchemaEntityKind::Table => Ok(CommandPayloadMetadata {
            encoding: "table",
            size: None,
        }),
        _ => fail(format!(
            "command type {type_name} resolves to {} {}, expected struct or table",
            entity.kind.as_str(),
            entity.name
        )),
    }
}

/// Resolve the COMMANDS table against compiler reflection, computing each
/// request and reply type's payload metadata and transitive schema hash.
fn command_entries(schema: &CompiledSchema) -> Result<Vec<CommandEntry>> {
    let mut commands = Vec::new();
    for (id, name, request_type, reply_type, description) in COMMANDS {
        let request_meta = command_payload_metadata(schema, request_type)?;
        let reply_meta = command_payload_metadata(schema, reply_type)?;
        let (request_type_schema_hash, legacy_request_schema_file_hash_128) =
            command_schema_identities(schema, request_type)?;
        let (reply_type_schema_hash, legacy_reply_schema_file_hash_128) =
            command_schema_identities(schema, reply_type)?;
        commands.push(CommandEntry {
            id: *id,
            name: (*name).to_string(),
            key: format!("{CMD_KEY_PREFIX}/{name}"),
            request_type: (*request_type).to_string(),
            request_type_schema_hash,
            legacy_request_schema_file_hash_128,
            request_encoding: request_meta.encoding,
            request_size: request_meta.size,
            reply_type: (*reply_type).to_string(),
            reply_type_schema_hash,
            legacy_reply_schema_file_hash_128,
            reply_encoding: reply_meta.encoding,
            reply_size: reply_meta.size,
            description: (*description).to_string(),
        });
    }
    Ok(commands)
}

fn command_schema_identities(
    schema: &CompiledSchema,
    type_name: &str,
) -> Result<(String, String)> {
    let Some((schema_file, entity)) = find_schema_entity(schema, &type_lookup_name(type_name))
    else {
        return fail(format!(
            "command type {type_name} does not resolve to a schema entity"
        ));
    };
    let qualified = qualified_name(&entity.namespace, &entity.name);
    if qualified != type_name {
        return fail(format!(
            "command type {type_name} resolves to {qualified}; declare the fully qualified name"
        ));
    }
    let type_schema_hash = schema
        .strict_type_hashes
        .get(&qualified)
        .cloned()
        .ok_or_else(|| io::Error::other(format!("no strict type identity for {qualified}")))?;
    Ok((
        type_schema_hash,
        schema_file.legacy_bfbs_hash_128.clone(),
    ))
}

fn is_scalar_type(type_name: &str) -> bool {
    matches!(
        type_name,
        "bool"
            | "byte"
            | "int8"
            | "ubyte"
            | "uint8"
            | "short"
            | "int16"
            | "ushort"
            | "uint16"
            | "int"
            | "int32"
            | "uint"
            | "uint32"
            | "float"
            | "float32"
            | "long"
            | "int64"
            | "ulong"
            | "uint64"
            | "double"
            | "float64"
    )
}

fn enum_base_type(value_type: &str) -> &str {
    value_type.split_whitespace().next().unwrap_or(value_type)
}

#[derive(Clone, Debug, Serialize)]
struct FieldDescTemplateEntry {
    name: String,
    offset: usize,
    kind: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct TopicFieldsTemplateEntry {
    payload_type: String,
    topic_id: u16,
    payload_size: usize,
    field_count: usize,
    fields: Vec<FieldDescTemplateEntry>,
}

fn scalar_field_kind(type_name: &str) -> Option<&'static str> {
    Some(match type_name {
        "bool" => "SYNAPSE_FIELD_BOOL",
        "byte" | "int8" => "SYNAPSE_FIELD_I8",
        "ubyte" | "uint8" => "SYNAPSE_FIELD_U8",
        "short" | "int16" => "SYNAPSE_FIELD_I16",
        "ushort" | "uint16" => "SYNAPSE_FIELD_U16",
        "int" | "int32" => "SYNAPSE_FIELD_I32",
        "uint" | "uint32" => "SYNAPSE_FIELD_U32",
        "long" | "int64" => "SYNAPSE_FIELD_I64",
        "ulong" | "uint64" => "SYNAPSE_FIELD_U64",
        "float" | "float32" => "SYNAPSE_FIELD_F32",
        "double" | "float64" => "SYNAPSE_FIELD_F64",
        _ => return None,
    })
}

/// Flatten FlatCC-reflected fixed-layout fields into scalar descriptors.
/// Nested struct members become dotted names ("attitude.w").
fn collect_field_descs(
    schema: &CompiledSchema,
    type_name: &str,
    prefix: &str,
    base_offset: usize,
    out: &mut Vec<FieldDescTemplateEntry>,
) -> Result<()> {
    // Fixed-length arrays are opaque to the scalar debug printer: their wire
    // structure is captured by the wire descriptor hash, and the value printer
    // has no per-element scalar kind for them, so they are omitted here.
    if type_name.trim_start().starts_with('[') {
        return Ok(());
    }
    let lookup = type_lookup_name(type_name);
    if let Some(kind) = scalar_field_kind(&lookup) {
        out.push(FieldDescTemplateEntry {
            name: prefix.to_string(),
            offset: base_offset,
            kind,
        });
        return Ok(());
    }

    let Some((_, entity)) = find_schema_entity(schema, &lookup) else {
        return fail(format!(
            "cannot collect field descriptors for unknown type {lookup}"
        ));
    };
    match entity.kind {
        SchemaEntityKind::Enum => {
            let base = entity
                .value_type
                .as_deref()
                .map(enum_base_type)
                .unwrap_or_default();
            let Some(kind) = scalar_field_kind(base) else {
                return fail(format!("enum {lookup} has unsupported base type '{base}'"));
            };
            out.push(FieldDescTemplateEntry {
                name: prefix.to_string(),
                offset: base_offset,
                kind,
            });
        }
        SchemaEntityKind::Struct => {
            for member in &entity.members {
                let Some(member_type) = member.type_name.as_deref() else {
                    return fail(format!(
                        "struct {lookup} member {} is missing a type",
                        member.name
                    ));
                };
                let member_offset = member.offset.ok_or_else(|| {
                    io::Error::other(format!(
                        "FlatCC reflection omitted the offset for {lookup}.{}",
                        member.name
                    ))
                })?;
                let name = if prefix.is_empty() {
                    member.name.clone()
                } else {
                    format!("{prefix}.{}", member.name)
                };
                collect_field_descs(schema, member_type, &name, base_offset + member_offset, out)?;
            }
        }
        _ => {
            return fail(format!(
                "{lookup} is a {}, not a fixed-layout struct or enum",
                entity.kind.as_str()
            ));
        }
    }
    Ok(())
}

fn topic_print_context(schema: &CompiledSchema, topics: &[TopicEntry]) -> Result<Value> {
    let mut structs = Vec::new();
    for topic in topics {
        if !topic.fixed_layout {
            continue;
        }
        let payload = topic
            .payload_type
            .as_deref()
            .expect("fixed-layout topic has a payload type");
        let payload_size = topic
            .payload_size
            .expect("fixed-layout topic has a payload size");
        let mut fields = Vec::new();
        collect_field_descs(schema, payload, "", 0, &mut fields)?;
        structs.push(TopicFieldsTemplateEntry {
            payload_type: type_lookup_name(payload),
            topic_id: topic.id,
            payload_size,
            field_count: fields.len(),
            fields,
        });
    }
    Ok(context! { structs => structs })
}

fn write_c_topic_print(
    templates: &Templates,
    schema: &CompiledSchema,
    topics: &[TopicEntry],
    header_path: &Path,
    source_path: &Path,
) -> Result<()> {
    let context = topic_print_context(schema, topics)?;
    templates.render_to_file(
        "xtask/topic_catalog/topic_print.h.jinja",
        context.clone(),
        header_path,
    )?;
    templates.render_to_file(
        "xtask/topic_catalog/topic_print.c.jinja",
        context,
        source_path,
    )
}

/// Check the accepted candidate byte contract consumed by the C and Rust ActuatorOutputs validators.
/// The final byte is implicit alignment padding, so the 144-byte size and the
/// last reflected field ending at byte 143 together reserve byte 143 as the
/// required zero padding byte.
fn validate_actuator_outputs_layout(schema: &CompiledSchema, problems: &mut Vec<String>) {
    let Some((_, entity)) = find_schema_entity(schema, "synapse.topic.ActuatorOutputsData") else {
        problems.push("ActuatorOutputsData not found".to_string());
        return;
    };
    if entity.kind != SchemaEntityKind::Struct {
        problems.push(format!(
            "ActuatorOutputsData is a {}, expected struct",
            entity.kind.as_str()
        ));
        return;
    }
    if entity.byte_size != Some(144) {
        problems.push(format!(
            "ActuatorOutputsData is {:?} bytes, expected exactly 144",
            entity.byte_size
        ));
    }

    let mut expected = vec![
        ("timestamp_ns".to_string(), "ulong".to_string(), Some(0)),
        ("active_mask".to_string(), "uint".to_string(), Some(8)),
    ];
    for slot in 0..32 {
        expected.push((
            format!("output{slot}"),
            "float".to_string(),
            Some(12 + slot * 4),
        ));
    }
    expected.extend([
        (
            "arm_state".to_string(),
            "ActuatorArmState".to_string(),
            Some(140),
        ),
        (
            "command_source".to_string(),
            "ActuatorOutputSource".to_string(),
            Some(141),
        ),
        (
            "time_status".to_string(),
            "synapse.types.TimeStatus".to_string(),
            Some(142),
        ),
    ]);
    let actual = entity
        .members
        .iter()
        .map(|member| {
            (
                member.name.clone(),
                member.type_name.clone().unwrap_or_default(),
                member.offset,
            )
        })
        .collect::<Vec<_>>();
    if actual != expected {
        problems.push(format!(
            "ActuatorOutputsData fields do not match the accepted candidate byte layout.\n  expected: {expected:?}\n  actual:   {actual:?}"
        ));
    }
}

/// Protocol-level consistency checks over FlatCC reflection: TopicId
/// contiguity, TopicId/union agreement, command type resolution, and the exact
/// ActuatorOutputsData validator layout.
fn validate_protocol(schema: &CompiledSchema) -> Result<()> {
    let mut problems = Vec::new();

    let topic_enum = schema
        .files
        .iter()
        .flat_map(|file| &file.entities)
        .find(|entity| entity.kind == SchemaEntityKind::Enum && entity.name == "TopicId")
        .ok_or_else(|| io::Error::other("TopicId enum not found"))?;
    let topic_names = topic_enum
        .members
        .iter()
        .filter(|member| member.name != "Unknown")
        .map(|member| member.name.as_str())
        .collect::<Vec<_>>();

    for (index, member) in topic_enum
        .members
        .iter()
        .filter(|member| member.name != "Unknown")
        .enumerate()
    {
        let expected = (index + 1).to_string();
        if member.value.as_deref() != Some(expected.as_str()) {
            problems.push(format!(
                "TopicId {} has value {}, expected contiguous value {expected}",
                member.name,
                member.value.as_deref().unwrap_or("<none>")
            ));
        }
    }

    match schema
        .files
        .iter()
        .flat_map(|file| &file.entities)
        .find(|entity| entity.kind == SchemaEntityKind::Union && entity.name == "SynapseMessage")
    {
        Some(union_entity) => {
            let union_names = union_entity
                .members
                .iter()
                .map(|member| member.name.as_str())
                .collect::<Vec<_>>();
            if union_names != topic_names {
                problems.push(format!(
                    "SynapseMessage union does not mirror TopicId.\n  TopicId: {}\n  union:   {}",
                    topic_names.join(", "),
                    union_names.join(", ")
                ));
            }
        }
        None => problems.push("SynapseMessage union not found".to_string()),
    }

    for (_, name, request_type, reply_type, _) in COMMANDS {
        for type_name in [request_type, reply_type] {
            if find_schema_entity(schema, type_name).is_none() {
                problems.push(format!(
                    "command {name} references unknown type {type_name}"
                ));
            }
        }
    }

    // CmdId in fbs/transfer.fbs must mirror the COMMANDS table.
    match schema
        .files
        .iter()
        .flat_map(|file| &file.entities)
        .find(|entity| entity.kind == SchemaEntityKind::Enum && entity.name == "CmdId")
    {
        Some(cmd_enum) => {
            let enum_entries = cmd_enum
                .members
                .iter()
                .filter(|member| member.name != "Unknown")
                .map(|member| member.value.clone().unwrap_or_default())
                .collect::<Vec<_>>();
            let command_entries = COMMANDS
                .iter()
                .map(|(id, _, _, _, _)| id.to_string())
                .collect::<Vec<_>>();
            if enum_entries != command_entries {
                problems.push(format!(
                    "CmdId enum does not mirror the xtask COMMANDS table.\n  CmdId:    {enum_entries:?}\n  COMMANDS: {command_entries:?}"
                ));
            }
        }
        None => problems.push("CmdId enum not found in fbs/transfer.fbs".to_string()),
    }

    validate_actuator_outputs_layout(schema, &mut problems);

    if problems.is_empty() {
        Ok(())
    } else {
        fail(format!(
            "protocol validation failed:\n{}",
            problems.join("\n")
        ))
    }
}

/// Committed wire-compatibility baseline: one structural, name-free descriptor
/// per FlatBuffers type. Renaming a field, enum value, or union arm is wire
/// safe, so names never enter a hash; struct leaf paths are the sole exception,
/// kept inside the struct hash so a same-type field reorder is still caught.
const WIRE_SCHEMA_PATH: &str = "compatibility/wire-schema.toml";

/// Header prepended to the generated baseline so the file explains itself.
const WIRE_SCHEMA_HEADER: &str = "\
# Wire-compatibility baseline for synapse_fbs. Each entry pins the on-wire
# structure of one FlatBuffers type by a name-free structural hash. Regenerate
# with `cargo run --manifest-path xtask/Cargo.toml -- wire-check --update`.
#
# The file is append-only for published types: `wire-check` fails when a
# recorded type is removed or changed in a wire-incompatible way. To evolve a
# type, introduce a new wire type and topic instead of mutating an existing
# one. `--update` records append-compatible changes, but refuses a wire break
# unless the fully qualified name is listed under [allow].break, which it then
# consumes as a single-use bless.
";

type WireDescriptorSet = BTreeMap<String, WireType>;

/// One type's wire descriptor. The `hash` field is the authoritative structural
/// fingerprint; the extra per-kind fields let `breaking_reason` classify a hash
/// difference as append-compatible or breaking without recompiling the schema.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum WireType {
    Struct {
        hash: String,
    },
    Table {
        hash: String,
        fields: Vec<String>,
    },
    Enum {
        hash: String,
        underlying: String,
        bit_flags: bool,
        values: Vec<i64>,
    },
    Union {
        hash: String,
        members: Vec<String>,
    },
}

impl WireType {
    fn kind(&self) -> &'static str {
        match self {
            Self::Struct { .. } => "struct",
            Self::Table { .. } => "table",
            Self::Enum { .. } => "enum",
            Self::Union { .. } => "union",
        }
    }

    fn hash(&self) -> &str {
        match self {
            Self::Struct { hash }
            | Self::Table { hash, .. }
            | Self::Enum { hash, .. }
            | Self::Union { hash, .. } => hash,
        }
    }
}

#[derive(Default, Serialize, serde::Deserialize)]
struct WireBaseline {
    #[serde(default)]
    allow: WireAllow,
    #[serde(default)]
    types: WireDescriptorSet,
}

/// Escape hatch for intentional breaks. Each fully qualified name here lets
/// `--update` record a changed hash once; the entry is consumed on write.
#[derive(Default, Serialize, serde::Deserialize)]
struct WireAllow {
    #[serde(default, rename = "break")]
    break_: Vec<String>,
}

/// One flattened scalar of a fixed-layout struct: a dotted path (to catch a
/// same-type reorder), the absolute byte offset, the scalar base name (an enum
/// leaf collapses to its underlying scalar), and the fixed-array length if any.
struct StructLeaf {
    path: String,
    offset: usize,
    scalar_base: String,
    array_len: Option<u16>,
}

/// Build the per-type wire descriptors from the fully include-expanded
/// `all.bfbs` (the last SCHEMAS entry), so no cross-file dedup is needed. The
/// walk is over raw reflection, not the reduced SchemaEntity, which drops
/// Field.id and BaseType.
fn build_wire_descriptors(bfbs_dir: &Path) -> Result<WireDescriptorSet> {
    let path = bfbs_dir.join("all.bfbs");
    let bytes = fs::read(&path)?;
    let schema = reflection::root_as_schema(&bytes)
        .map_err(|err| io::Error::other(format!("invalid {}: {err}", path.display())))?;
    let mut set = WireDescriptorSet::new();
    for object in schema.objects() {
        let desc = if object.is_struct() {
            wire_struct(&schema, object)?
        } else {
            wire_table(&schema, object)?
        };
        set.insert(object.name().to_string(), desc);
    }
    for reflected_enum in schema.enums() {
        let desc = if reflected_enum.is_union() {
            wire_union(&schema, reflected_enum)?
        } else {
            wire_enum(&schema, reflected_enum)?
        };
        set.insert(reflected_enum.name().to_string(), desc);
    }
    Ok(set)
}

/// Flatten a fixed-layout struct into scalar leaves, recursing into nested
/// structs (and fixed arrays of structs) with an accumulated base offset so a
/// reorder of same-typed fields still moves at least one leaf offset.
fn struct_leaves(
    schema: &reflection::Schema<'_>,
    object: reflection::Object<'_>,
    prefix: &str,
    base_offset: usize,
    out: &mut Vec<StructLeaf>,
) -> Result<()> {
    let mut fields = object
        .fields()
        .into_iter()
        .filter(|field| field.type_().base_type() != BaseType::UType)
        .collect::<Vec<_>>();
    fields.sort_by_key(|field| field.offset());
    for field in fields {
        let type_ = field.type_();
        let offset = base_offset + usize::from(field.offset());
        let path = if prefix.is_empty() {
            field.name().to_string()
        } else {
            format!("{prefix}.{}", field.name())
        };
        match type_.base_type() {
            BaseType::Obj => {
                let nested = schema.objects().get(usize::try_from(type_.index())?);
                struct_leaves(schema, nested, &path, offset, out)?;
            }
            BaseType::Array if type_.element() == BaseType::Obj => {
                let nested = schema.objects().get(usize::try_from(type_.index())?);
                let stride = usize::try_from(nested.bytesize())?;
                for element in 0..type_.fixed_length() {
                    let element_path = format!("{path}[{element}]");
                    struct_leaves(
                        schema,
                        nested,
                        &element_path,
                        offset + usize::from(element) * stride,
                        out,
                    )?;
                }
            }
            BaseType::Array => out.push(StructLeaf {
                path,
                offset,
                scalar_base: reflected_scalar_name(type_.element())?.to_string(),
                array_len: Some(type_.fixed_length()),
            }),
            base => out.push(StructLeaf {
                path,
                offset,
                scalar_base: reflected_scalar_name(base)?.to_string(),
                array_len: None,
            }),
        }
    }
    Ok(())
}

/// Struct descriptor: minalign, bytesize, and the offset-ordered scalar leaves.
/// Any layout change (offset, type, size, alignment, reorder) is breaking.
fn wire_struct(
    schema: &reflection::Schema<'_>,
    object: reflection::Object<'_>,
) -> Result<WireType> {
    let mut leaves = Vec::new();
    struct_leaves(schema, object, "", 0, &mut leaves)?;
    leaves.sort_by(|left, right| {
        left.offset
            .cmp(&right.offset)
            .then_with(|| left.path.cmp(&right.path))
    });

    let mut digest = Sha256::new();
    digest.update(b"synapse-wire-struct-v1\n");
    digest.update(object.minalign().to_string());
    digest.update(b"\t");
    digest.update(object.bytesize().to_string());
    digest.update(b"\n");
    for leaf in &leaves {
        digest.update(leaf.path.as_bytes());
        digest.update(b"\t");
        digest.update(leaf.offset.to_string());
        digest.update(b"\t");
        digest.update(leaf.scalar_base.as_bytes());
        digest.update(b"\t");
        digest.update(match leaf.array_len {
            Some(length) => length.to_string(),
            None => "-".to_string(),
        });
        digest.update(b"\n");
    }
    Ok(WireType::Struct {
        hash: finish_wire_hash(digest),
    })
}

/// Table descriptor: id-ordered field signatures, each name free. The stored
/// signatures let `breaking_reason` tell an appended id from a retyped one.
fn wire_table(
    schema: &reflection::Schema<'_>,
    object: reflection::Object<'_>,
) -> Result<WireType> {
    let mut fields = object
        .fields()
        .into_iter()
        .filter(|field| field.type_().base_type() != BaseType::UType)
        .collect::<Vec<_>>();
    fields.sort_by_key(|field| field.id());

    let mut signatures = Vec::new();
    for field in fields {
        signatures.push(wire_field_signature(schema, field)?);
    }

    let mut digest = Sha256::new();
    digest.update(b"synapse-wire-table-v1\n");
    for signature in &signatures {
        digest.update(signature.as_bytes());
        digest.update(b"\n");
    }
    Ok(WireType::Table {
        hash: finish_wire_hash(digest),
        fields: signatures,
    })
}

/// Render one table field as `"<id> <type_ref>[=<default>][ required][ deprecated]"`.
/// The default is the numeric value, never an enum member name, so renaming an
/// enum value does not perturb the referencing table.
fn wire_field_signature(
    schema: &reflection::Schema<'_>,
    field: reflection::Field<'_>,
) -> Result<String> {
    let type_ref = wire_type_ref(schema, field.type_())?;
    let default = wire_field_default(field)
        .map(|value| format!("={value}"))
        .unwrap_or_default();
    let required = if field.required() { " required" } else { "" };
    let deprecated = if field.deprecated() { " deprecated" } else { "" };
    Ok(format!(
        "{} {type_ref}{default}{required}{deprecated}",
        field.id()
    ))
}

/// Numeric default of a table field, or None when it is the zero default. Enum
/// and integer defaults collapse to the underlying integer; float defaults use
/// the real value.
fn wire_field_default(field: reflection::Field<'_>) -> Option<String> {
    if field.default_integer() == 0 && field.default_real() == 0.0 {
        return None;
    }
    if matches!(
        field.type_().base_type(),
        BaseType::Float | BaseType::Double
    ) {
        Some(field.default_real().to_string())
    } else {
        Some(field.default_integer().to_string())
    }
}

/// Enum descriptor: underlying scalar, bit_flags attribute, and the sorted
/// numeric values. Names are excluded; only the numeric surface is wire visible.
fn wire_enum(
    _schema: &reflection::Schema<'_>,
    reflected_enum: reflection::Enum<'_>,
) -> Result<WireType> {
    let underlying = reflected_scalar_name(reflected_enum.underlying_type().base_type())?.to_string();
    let bit_flags = reflected_enum.attributes().is_some_and(|attributes| {
        attributes
            .into_iter()
            .any(|attribute| attribute.key() == "bit_flags")
    });
    let mut values = reflected_enum
        .values()
        .into_iter()
        .map(|value| value.value())
        .collect::<Vec<_>>();
    values.sort_unstable();

    let mut digest = Sha256::new();
    digest.update(b"synapse-wire-enum-v1\n");
    digest.update(underlying.as_bytes());
    digest.update(if bit_flags { b"\t1\n" } else { b"\t0\n" });
    for value in &values {
        digest.update(value.to_string());
        digest.update(b"\n");
    }
    Ok(WireType::Enum {
        hash: finish_wire_hash(digest),
        underlying,
        bit_flags,
        values,
    })
}

/// Union descriptor: discriminator-ordered `"<disc> <qualified_target>"` arms.
/// A retargeted or removed discriminator is breaking; a new higher one appends.
fn wire_union(
    schema: &reflection::Schema<'_>,
    reflected_enum: reflection::Enum<'_>,
) -> Result<WireType> {
    let mut members = Vec::new();
    for value in reflected_enum.values() {
        let Some(target) = value
            .union_type()
            .filter(|type_| type_.base_type() != BaseType::None)
        else {
            continue;
        };
        members.push((value.value(), wire_type_ref(schema, target)?));
    }
    members.sort_by_key(|(discriminator, _)| *discriminator);
    let members = members
        .into_iter()
        .map(|(discriminator, target)| format!("{discriminator} {target}"))
        .collect::<Vec<_>>();

    let mut digest = Sha256::new();
    digest.update(b"synapse-wire-union-v1\n");
    for member in &members {
        digest.update(member.as_bytes());
        digest.update(b"\n");
    }
    Ok(WireType::Union {
        hash: finish_wire_hash(digest),
        members,
    })
}

/// Fully qualified, name-stable reference for a table field or union arm type.
/// Vectors render as `[elem]`, fixed arrays as `[elem;N]`; an enum keeps its
/// qualified name so its own descriptor pins the underlying representation.
fn wire_type_ref(schema: &reflection::Schema<'_>, type_: reflection::Type<'_>) -> Result<String> {
    let base_type = type_.base_type();
    let element_base = match base_type {
        BaseType::Vector | BaseType::Vector64 | BaseType::Array => type_.element(),
        other => other,
    };
    let atom = if type_.index() >= 0 {
        if element_base == BaseType::Obj {
            schema
                .objects()
                .get(usize::try_from(type_.index())?)
                .name()
                .to_string()
        } else {
            schema
                .enums()
                .get(usize::try_from(type_.index())?)
                .name()
                .to_string()
        }
    } else {
        reflected_scalar_name(element_base)?.to_string()
    };
    Ok(match base_type {
        BaseType::Vector | BaseType::Vector64 => format!("[{atom}]"),
        BaseType::Array => format!("[{atom};{}]", type_.fixed_length()),
        _ => atom,
    })
}

/// 32-hex (128-bit truncated SHA-256) of a per-kind, name-free structure. Same
/// truncation as the schema-set handshake hash, but with a per-kind domain tag.
fn finish_wire_hash(digest: Sha256) -> String {
    let digest = digest.finalize();
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Load the committed baseline, or a friendly instruction when it is absent.
fn parse_wire_baseline(root: &Path) -> Result<WireBaseline> {
    let path = root.join(WIRE_SCHEMA_PATH);
    if !path.is_file() {
        return fail(format!(
            "wire compatibility baseline {} is missing; run xtask wire-check --update to establish the baseline",
            path.display()
        ));
    }
    let content = fs::read_to_string(&path)?;
    toml::from_str(&content)
        .map_err(|err| io::Error::other(format!("invalid {}: {err}", path.display())).into())
}

/// Compare freshly built descriptors against the committed baseline. A removed
/// or wire-incompatibly changed type fails the build; a new type or an
/// append-compatible change only prints an info line. `[allow].break` downgrades
/// a specific breaking change to a warning.
fn wire_check(root: &Path, current: &WireDescriptorSet) -> Result<()> {
    let baseline = parse_wire_baseline(root)?;
    let allow = baseline
        .allow
        .break_
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut problems = Vec::new();

    for (name, old) in &baseline.types {
        let Some(new) = current.get(name) else {
            problems.push(format!(
                "REMOVED: {name}; run xtask wire-check --update after reviewing the removal"
            ));
            continue;
        };
        if old.hash() == new.hash() {
            continue;
        }
        if let Some(reason) = breaking_reason(old, new) {
            if allow.contains(name.as_str()) {
                println!("WARNING: {name} breaking change blessed by [allow].break ({reason})");
            } else {
                problems.push(format!(
                    "BREAKING: {name} changed from {} to {} ({reason}); introduce a new wire type and topic instead",
                    old.hash(),
                    new.hash()
                ));
            }
        }
    }

    for (name, new) in current {
        if !baseline.types.contains_key(name) {
            println!(
                "NEW: {name} ({}); review it, then run xtask wire-check --update",
                new.hash()
            );
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        fail(format!(
            "wire compatibility check failed:\n{}",
            problems.join("\n")
        ))
    }
}

/// Classify a hash difference between two descriptors of the same name. Returns
/// None for an append-only or deprecation-only change, Some(reason) otherwise.
/// A kind change (for example struct becoming a table) is always breaking.
fn breaking_reason(old: &WireType, new: &WireType) -> Option<String> {
    match (old, new) {
        (WireType::Struct { hash: old_hash }, WireType::Struct { hash: new_hash }) => {
            (old_hash != new_hash).then(|| "struct layout changed".to_string())
        }
        (
            WireType::Table {
                fields: old_fields, ..
            },
            WireType::Table {
                fields: new_fields, ..
            },
        ) => table_breaking_reason(old_fields, new_fields),
        (
            WireType::Enum {
                underlying: old_underlying,
                bit_flags: old_bit_flags,
                values: old_values,
                ..
            },
            WireType::Enum {
                underlying: new_underlying,
                bit_flags: new_bit_flags,
                values: new_values,
                ..
            },
        ) => enum_breaking_reason(
            old_underlying,
            *old_bit_flags,
            old_values,
            new_underlying,
            *new_bit_flags,
            new_values,
        ),
        (
            WireType::Union {
                members: old_members,
                ..
            },
            WireType::Union {
                members: new_members,
                ..
            },
        ) => union_breaking_reason(old_members, new_members),
        (old, new) => Some(format!("kind {} -> {}", old.kind(), new.kind())),
    }
}

/// One parsed table field signature, name free. `default` is the numeric value.
struct ParsedTableField {
    type_ref: String,
    default: Option<String>,
    required: bool,
    deprecated: bool,
}

fn parse_table_fields(fields: &[String]) -> BTreeMap<u16, ParsedTableField> {
    let mut parsed = BTreeMap::new();
    for signature in fields {
        if let Some((id, field)) = parse_table_field(signature) {
            parsed.insert(id, field);
        }
    }
    parsed
}

fn parse_table_field(signature: &str) -> Option<(u16, ParsedTableField)> {
    let (id, mut rest) = signature.split_once(' ')?;
    let id = id.parse::<u16>().ok()?;
    let mut deprecated = false;
    let mut required = false;
    if let Some(prefix) = rest.strip_suffix(" deprecated") {
        deprecated = true;
        rest = prefix;
    }
    if let Some(prefix) = rest.strip_suffix(" required") {
        required = true;
        rest = prefix;
    }
    let (type_ref, default) = match rest.split_once('=') {
        Some((type_ref, default)) => (type_ref.to_string(), Some(default.to_string())),
        None => (rest.to_string(), None),
    };
    Some((
        id,
        ParsedTableField {
            type_ref,
            default,
            required,
            deprecated,
        },
    ))
}

fn table_breaking_reason(old_fields: &[String], new_fields: &[String]) -> Option<String> {
    let old = parse_table_fields(old_fields);
    let new = parse_table_fields(new_fields);
    for (id, old_field) in &old {
        let Some(new_field) = new.get(id) else {
            return Some(format!("field id {id} removed"));
        };
        if old_field.deprecated && !new_field.deprecated {
            return Some(format!("field id {id} undeprecated"));
        }
        if old_field.deprecated {
            continue;
        }
        if old_field.type_ref != new_field.type_ref {
            return Some(format!(
                "field id {id} type {} -> {}",
                old_field.type_ref, new_field.type_ref
            ));
        }
        if old_field.default != new_field.default {
            return Some(format!(
                "field id {id} default {} -> {}",
                old_field.default.as_deref().unwrap_or("-"),
                new_field.default.as_deref().unwrap_or("-")
            ));
        }
        if old_field.required != new_field.required {
            return Some(format!(
                "field id {id} required {} -> {}",
                old_field.required, new_field.required
            ));
        }
    }
    None
}

fn enum_breaking_reason(
    old_underlying: &str,
    old_bit_flags: bool,
    old_values: &[i64],
    new_underlying: &str,
    new_bit_flags: bool,
    new_values: &[i64],
) -> Option<String> {
    if old_underlying != new_underlying {
        return Some(format!("underlying {old_underlying} -> {new_underlying}"));
    }
    if old_bit_flags != new_bit_flags {
        return Some(format!("bit_flags {old_bit_flags} -> {new_bit_flags}"));
    }
    let new_set = new_values.iter().copied().collect::<BTreeSet<_>>();
    for value in old_values {
        if !new_set.contains(value) {
            return Some(format!("enum value {value} removed"));
        }
    }
    None
}

fn union_breaking_reason(old_members: &[String], new_members: &[String]) -> Option<String> {
    let old = parse_union_members(old_members);
    let new = parse_union_members(new_members);
    for (discriminator, old_target) in &old {
        match new.get(discriminator) {
            None => return Some(format!("union discriminator {discriminator} removed")),
            Some(new_target) if new_target != old_target => {
                return Some(format!("union discriminator {discriminator} retargeted"));
            }
            _ => {}
        }
    }
    None
}

fn parse_union_members(members: &[String]) -> BTreeMap<i64, String> {
    let mut parsed = BTreeMap::new();
    for member in members {
        if let Some((discriminator, target)) = member.split_once(' ')
            && let Ok(discriminator) = discriminator.parse::<i64>()
        {
            parsed.insert(discriminator, target.to_string());
        }
    }
    parsed
}

fn validate_wire_baseline_update(
    existing: &WireBaseline,
    current: &WireDescriptorSet,
) -> Result<()> {
    let blessed = existing
        .allow
        .break_
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for (name, old) in &existing.types {
        let Some(new) = current.get(name) else {
            return fail(format!(
                "refusing to remove published {name} from the wire compatibility baseline"
            ));
        };
        if old.hash() == new.hash() {
            continue;
        }
        if let Some(reason) = breaking_reason(old, new)
            && !blessed.contains(name.as_str())
        {
            return fail(format!(
                "refusing to record a wire-incompatible change to published {name}: {} -> {} ({reason}). Introduce a new wire type and topic, or add it to [allow].break to bless an intentional break",
                old.hash(),
                new.hash()
            ));
        }
    }
    Ok(())
}

fn remaining_wire_break_blesses(
    existing: &WireBaseline,
    current: &WireDescriptorSet,
) -> Vec<String> {
    existing
        .allow
        .break_
        .iter()
        .filter(|name| match (existing.types.get(*name), current.get(*name)) {
            (Some(old), Some(new)) => {
                old.hash() == new.hash() || breaking_reason(old, new).is_none()
            }
            _ => true,
        })
        .cloned()
        .collect()
}

/// Rewrite the committed baseline from freshly built descriptors. Records
/// new types and append-compatible changes directly. Published types cannot be
/// removed. A wire-incompatible change requires a name under `[allow].break`,
/// which is consumed so a bless is single-use.
fn update_wire_baseline(root: &Path, current: &WireDescriptorSet) -> Result<()> {
    let path = root.join(WIRE_SCHEMA_PATH);
    let mut allow = WireAllow::default();
    if path.is_file() {
        let content = fs::read_to_string(&path)?;
        let existing: WireBaseline = toml::from_str(&content)
            .map_err(|err| io::Error::other(format!("invalid {}: {err}", path.display())))?;
        validate_wire_baseline_update(&existing, current)?;
        // Keep only blesses that were not consumed by this rewrite, so a bless
        // for a genuinely changed type is single-use.
        allow.break_ = remaining_wire_break_blesses(&existing, current);
    }

    let baseline = WireBaseline {
        allow,
        types: current.clone(),
    };
    let body = toml::to_string_pretty(&baseline)?;
    let content = format!("{WIRE_SCHEMA_HEADER}{body}");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)?;
    println!("updated {}", path.display());
    Ok(())
}

fn find_schema_entity<'a>(
    schema: &'a CompiledSchema,
    name: &str,
) -> Option<(&'a SchemaFile, &'a SchemaEntity)> {
    let lookup = type_lookup_name(name);
    schema.files.iter().find_map(|file| {
        file.entities
            .iter()
            .find(|entity| entity.name == lookup)
            .map(|entity| (file, entity))
    })
}

fn thin_root_wrapper_payload(entity: &SchemaEntity) -> Option<&str> {
    if entity.kind != SchemaEntityKind::Table || entity.members.len() != 1 {
        return None;
    }
    let member = &entity.members[0];
    if member.name != "data" {
        return None;
    }
    member
        .type_name
        .as_deref()
        .filter(|type_name| type_lookup_name(type_name).ends_with("Data"))
}

/// Path of a schema entity inside the flatc-generated Rust module tree, for
/// example ("synapse.topic", "VehicleHealthData") -> "synapse::topic::VehicleHealthData".
fn rust_module_path(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", namespace.replace('.', "::"))
    }
}

/// Fully qualified FlatBuffers name, for example "synapse.topic.VehicleHealthData".
fn qualified_name(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{namespace}.{name}")
    }
}

fn type_lookup_name(type_name: &str) -> String {
    type_name
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .rsplit('.')
        .next()
        .unwrap_or(type_name)
        .trim()
        .to_string()
}

/// Pure-logic coverage for the wire-compatibility classifier. These build
/// WireType values directly, so they need no compiled schema or FlatCC.
#[cfg(test)]
mod wire_tests {
    use super::*;

    fn struct_type(hash: &str) -> WireType {
        WireType::Struct {
            hash: hash.to_string(),
        }
    }

    fn table_type(hash: &str, fields: &[&str]) -> WireType {
        WireType::Table {
            hash: hash.to_string(),
            fields: fields.iter().map(|field| field.to_string()).collect(),
        }
    }

    fn enum_type(hash: &str, underlying: &str, bit_flags: bool, values: &[i64]) -> WireType {
        WireType::Enum {
            hash: hash.to_string(),
            underlying: underlying.to_string(),
            bit_flags,
            values: values.to_vec(),
        }
    }

    fn union_type(hash: &str, members: &[&str]) -> WireType {
        WireType::Union {
            hash: hash.to_string(),
            members: members.iter().map(|member| member.to_string()).collect(),
        }
    }

    fn wire_baseline(name: &str, type_: WireType, breaks: &[&str]) -> WireBaseline {
        WireBaseline {
            allow: WireAllow {
                break_: breaks.iter().map(|name| (*name).to_string()).collect(),
            },
            types: BTreeMap::from([(name.to_string(), type_)]),
        }
    }

    fn wire_set(name: &str, type_: WireType) -> WireDescriptorSet {
        BTreeMap::from([(name.to_string(), type_)])
    }

    #[test]
    fn updater_rejects_removing_published_type() {
        let existing = wire_baseline("synapse.msg.A", struct_type("a"), &[]);
        let error = validate_wire_baseline_update(&existing, &BTreeMap::new())
            .unwrap_err()
            .to_string();
        assert!(error.contains("refusing to remove published synapse.msg.A"));
    }

    #[test]
    fn updater_accepts_append_compatible_change() {
        let existing = wire_baseline(
            "synapse.msg.Kind",
            enum_type("a", "ubyte", false, &[0, 1]),
            &[],
        );
        let current = wire_set(
            "synapse.msg.Kind",
            enum_type("b", "ubyte", false, &[0, 1, 2]),
        );
        validate_wire_baseline_update(&existing, &current).unwrap();
    }

    #[test]
    fn updater_rejects_unblessed_break() {
        let existing = wire_baseline("synapse.msg.A", struct_type("a"), &[]);
        let current = wire_set("synapse.msg.A", struct_type("b"));
        let error = validate_wire_baseline_update(&existing, &current)
            .unwrap_err()
            .to_string();
        assert!(error.contains("wire-incompatible change"));
    }

    #[test]
    fn updater_consumes_used_break_bless() {
        let existing = wire_baseline(
            "synapse.msg.A",
            struct_type("a"),
            &["synapse.msg.A"],
        );
        let current = wire_set("synapse.msg.A", struct_type("b"));
        validate_wire_baseline_update(&existing, &current).unwrap();
        assert!(remaining_wire_break_blesses(&existing, &current).is_empty());
    }

    #[test]
    fn struct_hash_change_is_breaking() {
        assert!(breaking_reason(&struct_type("1111"), &struct_type("2222")).is_some());
    }

    #[test]
    fn struct_same_hash_is_compatible() {
        assert!(breaking_reason(&struct_type("1111"), &struct_type("1111")).is_none());
    }

    #[test]
    fn table_append_id_is_compatible() {
        let old = table_type("a", &["1 int", "2 float"]);
        let new = table_type("b", &["1 int", "2 float", "3 synapse.type.Vec3f"]);
        assert!(breaking_reason(&old, &new).is_none());
    }

    #[test]
    fn table_remove_id_is_breaking() {
        let old = table_type("a", &["1 int", "2 float"]);
        let new = table_type("b", &["1 int"]);
        assert!(breaking_reason(&old, &new).is_some());
    }

    #[test]
    fn table_retype_id_is_breaking() {
        let old = table_type("a", &["1 int", "2 float"]);
        let new = table_type("b", &["1 int", "2 uint"]);
        assert!(breaking_reason(&old, &new).is_some());
    }

    #[test]
    fn table_deprecate_id_is_compatible() {
        let old = table_type("a", &["1 int", "2 float"]);
        let new = table_type("b", &["1 int", "2 float deprecated"]);
        assert!(breaking_reason(&old, &new).is_none());
    }

    #[test]
    fn table_undeprecate_id_is_breaking() {
        let old = table_type("a", &["1 int", "2 float deprecated"]);
        let new = table_type("b", &["1 int", "2 float"]);
        assert!(breaking_reason(&old, &new).is_some());
    }

    #[test]
    fn enum_add_value_is_compatible() {
        let old = enum_type("a", "ubyte", false, &[0, 1, 2]);
        let new = enum_type("b", "ubyte", false, &[0, 1, 2, 3]);
        assert!(breaking_reason(&old, &new).is_none());
    }

    #[test]
    fn enum_remove_value_is_breaking() {
        let old = enum_type("a", "ubyte", false, &[0, 1, 2]);
        let new = enum_type("b", "ubyte", false, &[0, 1]);
        assert!(breaking_reason(&old, &new).is_some());
    }

    #[test]
    fn enum_underlying_change_is_breaking() {
        let old = enum_type("a", "ubyte", false, &[0, 1]);
        let new = enum_type("b", "ushort", false, &[0, 1]);
        assert!(breaking_reason(&old, &new).is_some());
    }

    #[test]
    fn enum_bit_flags_change_is_breaking() {
        let old = enum_type("a", "ubyte", false, &[0, 1]);
        let new = enum_type("b", "ubyte", true, &[0, 1]);
        assert!(breaking_reason(&old, &new).is_some());
    }

    #[test]
    fn union_append_higher_discriminator_is_compatible() {
        let old = union_type("a", &["1 synapse.msgs.A", "2 synapse.msgs.B"]);
        let new = union_type("b", &["1 synapse.msgs.A", "2 synapse.msgs.B", "3 synapse.msgs.C"]);
        assert!(breaking_reason(&old, &new).is_none());
    }

    #[test]
    fn union_retarget_discriminator_is_breaking() {
        let old = union_type("a", &["1 synapse.msgs.A", "2 synapse.msgs.B"]);
        let new = union_type("b", &["1 synapse.msgs.A", "2 synapse.msgs.Z"]);
        assert!(breaking_reason(&old, &new).is_some());
    }

    #[test]
    fn union_remove_discriminator_is_breaking() {
        let old = union_type("a", &["1 synapse.msgs.A", "2 synapse.msgs.B"]);
        let new = union_type("b", &["1 synapse.msgs.A"]);
        assert!(breaking_reason(&old, &new).is_some());
    }

    #[test]
    fn kind_mismatch_is_breaking() {
        assert!(breaking_reason(&struct_type("a"), &table_type("b", &["1 int"])).is_some());
    }
}
