#[derive(Clone, Debug)]
struct CompiledSchema {
    files: Vec<SchemaFile>,
}

#[derive(Clone, Debug)]
struct SchemaFile {
    name: String,
    schema_hash: String,
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
    wire_type: String,
    schema_hash: String,
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
    request_schema_hash: String,
    request_encoding: &'static str,
    request_size: Option<usize>,
    reply_type: String,
    reply_schema_hash: String,
    reply_encoding: &'static str,
    reply_size: Option<usize>,
    description: String,
}

#[derive(Clone, Debug, Serialize)]
struct TopicCatalogContext {
    version: u8,
    schema_set_hash: String,
    mcap_profile: &'static str,
    mcap_schema_encoding: &'static str,
    mcap_message_encoding: &'static str,
    mcap_metadata_name: &'static str,
    mcap_schema_set_hash_key: &'static str,
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
    mcap_schema_name: String,
    mcap_schema_file: String,
    mcap_schema_symbol: String,
    wire_type: String,
    schema_hash: String,
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
        let schema_hash = sha256_hex(&path)?[..32].to_string();
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
            schema_hash,
            entities,
            root_type: schema.root_table().map(|root| root.name().to_string()),
            file_identifier: schema
                .file_ident()
                .filter(|identifier| !identifier.is_empty())
                .map(str::to_string),
        });
    }

    Ok(CompiledSchema { files })
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
    let base = if type_.base_type() == BaseType::Vector {
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

    Ok(if type_.base_type() == BaseType::Vector {
        format!("[{atom}]")
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
        let schema_hash = schema_file.schema_hash.clone();
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
            wire_type,
            schema_hash,
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
        commands.push(CommandEntry {
            id: *id,
            name: (*name).to_string(),
            key: format!("{CMD_KEY_PREFIX}/{name}"),
            request_type: (*request_type).to_string(),
            request_schema_hash: command_schema_hash(schema, request_type)?,
            request_encoding: request_meta.encoding,
            request_size: request_meta.size,
            reply_type: (*reply_type).to_string(),
            reply_schema_hash: command_schema_hash(schema, reply_type)?,
            reply_encoding: reply_meta.encoding,
            reply_size: reply_meta.size,
            description: (*description).to_string(),
        });
    }
    Ok(commands)
}

fn command_schema_hash(schema: &CompiledSchema, type_name: &str) -> Result<String> {
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
    Ok(schema_file.schema_hash.clone())
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

/// Protocol-level consistency checks over FlatCC reflection: TopicId
/// contiguity, TopicId/union agreement, and command type resolution.
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

    if problems.is_empty() {
        Ok(())
    } else {
        fail(format!(
            "protocol validation failed:\n{}",
            problems.join("\n")
        ))
    }
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
