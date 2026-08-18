struct IdentityTranscript {
    digest: Sha256,
}

impl IdentityTranscript {
    fn new(domain: &str) -> Self {
        let mut transcript = Self {
            digest: Sha256::new(),
        };
        transcript.field(domain);
        transcript
    }

    fn field(&mut self, value: impl AsRef<[u8]>) {
        let value = value.as_ref();
        self.digest.update((value.len() as u64).to_le_bytes());
        self.digest.update(value);
    }

    fn finish(self) -> String {
        let digest = self.digest.finalize();
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct StrictTypeNode {
    normalized_fields: Vec<String>,
    dependencies: BTreeSet<String>,
}

type StrictTypeGraph = BTreeMap<String, StrictTypeNode>;

/// Build strict type identities from the fully include-expanded BFBS.
/// Documentation and declaration-file paths are deliberately excluded.
fn strict_type_hashes(bfbs_dir: &Path) -> Result<BTreeMap<String, String>> {
    let path = bfbs_dir.join("all.bfbs");
    let bytes = fs::read(&path)?;
    let schema = reflection::root_as_schema(&bytes)
        .map_err(|err| io::Error::other(format!("invalid {}: {err}", path.display())))?;
    let graph = strict_type_graph(&schema)?;
    graph
        .keys()
        .map(|name| Ok((name.clone(), strict_type_hash(&graph, name)?)))
        .collect()
}

fn strict_type_graph(schema: &reflection::Schema<'_>) -> Result<StrictTypeGraph> {
    let mut graph = BTreeMap::new();
    for object in schema.objects() {
        let mut dependencies = BTreeSet::new();
        let mut normalized_fields = vec![
            "object".to_string(),
            object.name().to_string(),
            if object.is_struct() {
                "struct".to_string()
            } else {
                "table".to_string()
            },
            object.minalign().to_string(),
            object.bytesize().to_string(),
        ];
        append_attributes(&mut normalized_fields, object.attributes());

        let mut fields = object.fields().into_iter().collect::<Vec<_>>();
        if object.is_struct() {
            fields.sort_by_key(|field| (field.offset(), field.id(), field.name()));
        } else {
            fields.sort_by_key(|field| (field.id(), field.name()));
        }
        normalized_fields.push(fields.len().to_string());
        for field in fields {
            normalized_fields.push("field".to_string());
            normalized_fields.push(field.name().to_string());
            normalized_fields.push(field.id().to_string());
            normalized_fields.push(field.offset().to_string());
            normalized_fields.push(strict_type_ref(schema, field.type_(), &mut dependencies)?);
            normalized_fields.push(field.default_integer().to_string());
            normalized_fields.push(format!("{:016x}", field.default_real().to_bits()));
            normalized_fields.push(bool_token(field.deprecated()).to_string());
            normalized_fields.push(bool_token(field.required()).to_string());
            normalized_fields.push(bool_token(field.key()).to_string());
            normalized_fields.push(bool_token(field.optional()).to_string());
            normalized_fields.push(field.padding().to_string());
            normalized_fields.push(bool_token(field.offset64()).to_string());
            append_attributes(&mut normalized_fields, field.attributes());
        }
        insert_strict_node(
            &mut graph,
            object.name(),
            StrictTypeNode {
                normalized_fields,
                dependencies,
            },
        )?;
    }

    for reflected_enum in schema.enums() {
        let mut dependencies = BTreeSet::new();
        let mut normalized_fields = vec![
            "enum".to_string(),
            reflected_enum.name().to_string(),
            bool_token(reflected_enum.is_union()).to_string(),
            strict_type_ref(
                schema,
                reflected_enum.underlying_type(),
                &mut dependencies,
            )?,
        ];
        append_attributes(&mut normalized_fields, reflected_enum.attributes());

        let mut values = reflected_enum.values().into_iter().collect::<Vec<_>>();
        values.sort_by_key(|value| (value.value(), value.name()));
        normalized_fields.push(values.len().to_string());
        for value in values {
            normalized_fields.push("value".to_string());
            normalized_fields.push(value.name().to_string());
            normalized_fields.push(value.value().to_string());
            match value.union_type() {
                Some(type_) => normalized_fields.push(strict_type_ref(
                    schema,
                    type_,
                    &mut dependencies,
                )?),
                None => normalized_fields.push("none".to_string()),
            }
            append_attributes(&mut normalized_fields, value.attributes());
        }
        insert_strict_node(
            &mut graph,
            reflected_enum.name(),
            StrictTypeNode {
                normalized_fields,
                dependencies,
            },
        )?;
    }
    Ok(graph)
}

fn insert_strict_node(
    graph: &mut StrictTypeGraph,
    name: &str,
    node: StrictTypeNode,
) -> Result<()> {
    if graph.insert(name.to_string(), node).is_some() {
        fail(format!("duplicate reflected type {name}"))
    } else {
        Ok(())
    }
}

fn bool_token(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

fn append_attributes<'a>(
    normalized: &mut Vec<String>,
    attributes: Option<impl IntoIterator<Item = reflection::KeyValue<'a>>>,
) {
    let Some(attributes) = attributes else {
        normalized.push("0".to_string());
        return;
    };
    let mut attributes = attributes
        .into_iter()
        .map(|attribute| {
            (
                attribute.key().to_string(),
                attribute.value().map(str::to_string),
            )
        })
        .collect::<Vec<_>>();
    attributes.sort();
    normalized.push(attributes.len().to_string());
    for (key, value) in attributes {
        normalized.push(key);
        match value {
            Some(value) => {
                normalized.push("1".to_string());
                normalized.push(value);
            }
            None => {
                normalized.push("0".to_string());
                normalized.push(String::new());
            }
        }
    }
}

fn strict_type_ref(
    schema: &reflection::Schema<'_>,
    type_: reflection::Type<'_>,
    dependencies: &mut BTreeSet<String>,
) -> Result<String> {
    let base = strict_base_type_name(type_.base_type())?;
    let element = strict_base_type_name(type_.element())?;
    let target = if type_.index() >= 0 {
        let referenced_base = match type_.base_type() {
            BaseType::Vector | BaseType::Vector64 | BaseType::Array => type_.element(),
            other => other,
        };
        let name = if referenced_base == BaseType::Obj {
            schema
                .objects()
                .get(usize::try_from(type_.index())?)
                .name()
        } else {
            schema
                .enums()
                .get(usize::try_from(type_.index())?)
                .name()
        };
        dependencies.insert(name.to_string());
        name
    } else {
        "-"
    };
    Ok(format!(
        "base={base};element={element};target={target};fixed_length={};base_size={};element_size={}",
        type_.fixed_length(),
        type_.base_size(),
        type_.element_size()
    ))
}

fn strict_base_type_name(base: BaseType) -> Result<&'static str> {
    base.variant_name()
        .ok_or_else(|| io::Error::other(format!("unknown reflected base type {}", base.0)).into())
}

fn strict_type_hash(graph: &StrictTypeGraph, root: &str) -> Result<String> {
    let mut reachable = BTreeSet::new();
    let mut pending = vec![root.to_string()];
    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        let node = graph
            .get(&name)
            .ok_or_else(|| io::Error::other(format!("strict type graph is missing {name}")))?;
        pending.extend(node.dependencies.iter().cloned());
    }

    let mut transcript = IdentityTranscript::new("synapse-strict-transitive-type-v1");
    transcript.field(root);
    transcript.field(reachable.len().to_string());
    for name in reachable {
        let node = graph
            .get(&name)
            .expect("reachable strict type was checked while walking the graph");
        transcript.field(&name);
        transcript.field(node.normalized_fields.len().to_string());
        for field in &node.normalized_fields {
            transcript.field(field);
        }
    }
    Ok(transcript.finish())
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    fn node(fields: &[&str], dependencies: &[&str]) -> StrictTypeNode {
        StrictTypeNode {
            normalized_fields: fields.iter().map(|value| (*value).to_string()).collect(),
            dependencies: dependencies
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        }
    }

    #[test]
    fn strict_type_hash_is_full_width_deterministic_and_transitive() {
        let graph = BTreeMap::from([
            ("example.Leaf".to_string(), node(&["leaf", "uint"], &[])),
            (
                "example.Root".to_string(),
                node(&["root", "value"], &["example.Leaf"]),
            ),
            ("example.Unused".to_string(), node(&["unused"], &[])),
        ]);
        let hash = strict_type_hash(&graph, "example.Root").unwrap();
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, strict_type_hash(&graph, "example.Root").unwrap());

        let mut changed_dependency = graph.clone();
        changed_dependency
            .get_mut("example.Leaf")
            .unwrap()
            .normalized_fields
            .push("changed".to_string());
        assert_ne!(
            hash,
            strict_type_hash(&changed_dependency, "example.Root").unwrap()
        );

        let mut changed_unrelated = graph;
        changed_unrelated
            .get_mut("example.Unused")
            .unwrap()
            .normalized_fields
            .push("changed".to_string());
        assert_eq!(
            hash,
            strict_type_hash(&changed_unrelated, "example.Root").unwrap()
        );
    }

    #[test]
    fn transcript_boundaries_are_unambiguous() {
        let mut left = IdentityTranscript::new("test");
        left.field("ab");
        left.field("c");
        let mut right = IdentityTranscript::new("test");
        right.field("a");
        right.field("bc");
        assert_ne!(left.finish(), right.finish());
    }

    #[test]
    fn schema_set_identity_includes_global_literals() {
        let empty_topics = [];
        let empty_commands = [];
        let left = schema_set_identity_with_globals(
            &empty_topics,
            &empty_commands,
            &["grammar-a".to_string()],
        );
        let right = schema_set_identity_with_globals(
            &empty_topics,
            &empty_commands,
            &["grammar-b".to_string()],
        );
        assert_ne!(left, right);
    }

    #[test]
    fn schema_set_identity_includes_topic_scope() {
        let vehicle = TopicEntry {
            id: 1,
            name: "Example".to_string(),
            key: "example".to_string(),
            root_table: "Example".to_string(),
            root_table_namespace: "synapse.topic".to_string(),
            payload_type: Some("ExampleData".to_string()),
            payload_type_namespace: Some("synapse.topic".to_string()),
            payload_size: Some(4),
            schema_file: "fbs/example.fbs".to_string(),
            schema_artifact_sha256: "artifact".to_string(),
            wire_type: "synapse.topic.ExampleData".to_string(),
            type_schema_hash: "type".to_string(),
            legacy_schema_file_hash_128: "legacy".to_string(),
            fixed_layout: true,
            multi_instance: false,
            scope: "vehicle",
            encoding: "struct",
            description: "Example topic".to_string(),
        };
        let mut any = vehicle.clone();
        any.scope = "any";
        let globals = [];
        let commands = [];

        assert_ne!(
            schema_set_identity_with_globals(&[vehicle], &commands, &globals),
            schema_set_identity_with_globals(&[any], &commands, &globals)
        );
    }

    #[test]
    fn package_contract_identity_keys_each_appear_once() {
        let literals = schema_package_contract_literals();

        for key in [
            MCAP_SCHEMA_SET_HASH_KEY,
            MCAP_SCHEMA_SET_IDENTITY_KEY,
            MCAP_SCHEMA_PACKAGE_CONTRACT_IDENTITY_KEY,
        ] {
            assert_eq!(
                literals.iter().filter(|literal| **literal == key).count(),
                1,
                "{key} must occur exactly once in the package transcript"
            );
        }
    }
}
fn schema_set_identity(topics: &[TopicEntry], commands: &[CommandEntry]) -> String {
    let global_literals = [
        CATALOG_VERSION.to_string(),
        CMD_KEY_PREFIX.to_string(),
        META_KEY_PREFIX.to_string(),
        LIVELINESS_KEY_PREFIX.to_string(),
        FLATBUFFER_VALUE_MEDIA_TYPE.to_string(),
        STRUCT_VALUE_MEDIA_TYPE.to_string(),
        TYPE_SCHEMA_HASH_ALGORITHM.to_string(),
        TOPIC_INSTANCE_KEY_GRAMMAR.to_string(),
    ];
    schema_set_identity_with_globals(topics, commands, &global_literals)
}

fn schema_set_identity_with_globals(
    topics: &[TopicEntry],
    commands: &[CommandEntry],
    global_literals: &[String],
) -> String {
    let mut transcript = IdentityTranscript::new("synapse-schema-set-v4");
    transcript.field(global_literals.len().to_string());
    for literal in global_literals {
        transcript.field(literal);
    }
    let mut topics = topics.iter().collect::<Vec<_>>();
    topics.sort_by_key(|topic| topic.id);
    transcript.field(topics.len().to_string());
    for topic in topics {
        for field in [
            topic.id.to_string(),
            topic.key.clone(),
            bool_token(topic.multi_instance).to_string(),
            topic.scope.to_string(),
            topic.encoding.to_string(),
            topic.wire_type.clone(),
            topic.type_schema_hash.clone(),
            optional_usize(topic.payload_size),
        ] {
            transcript.field(field);
        }
    }

    let mut commands = commands.iter().collect::<Vec<_>>();
    commands.sort_by_key(|command| command.id);
    transcript.field(commands.len().to_string());
    for command in commands {
        for field in [
            command.id.to_string(),
            command.name.clone(),
            command.key.clone(),
            command.request_encoding.to_string(),
            command.request_type.clone(),
            command.request_type_schema_hash.clone(),
            optional_usize(command.request_size),
            command.reply_encoding.to_string(),
            command.reply_type.clone(),
            command.reply_type_schema_hash.clone(),
            optional_usize(command.reply_size),
        ] {
            transcript.field(field);
        }
    }
    transcript.finish()
}

fn optional_usize(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string())
}

/// Exact pre-v4 128-bit algorithm retained for the frozen synapse/1 metadata
/// key. The inputs remain the legacy per-BFBS 128-bit prefixes.
fn legacy_schema_set_hash_128(topics: &[TopicEntry], commands: &[CommandEntry]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"synapse-schema-set-v3\n");
    let mut sorted_topics = topics.iter().collect::<Vec<_>>();
    sorted_topics.sort_by_key(|topic| topic.id);
    for topic in sorted_topics {
        digest.update(b"topic\t");
        digest.update(topic.id.to_string());
        digest.update(b"\t");
        digest.update(topic.key.as_bytes());
        digest.update(if topic.multi_instance {
            b"\t1\t"
        } else {
            b"\t0\t"
        });
        digest.update(topic.encoding.as_bytes());
        digest.update(b"\t");
        digest.update(topic.wire_type.as_bytes());
        digest.update(b"\t");
        digest.update(topic.legacy_schema_file_hash_128.as_bytes());
        digest.update(b"\n");
    }

    let mut sorted_commands = commands.iter().collect::<Vec<_>>();
    sorted_commands.sort_by_key(|command| command.id);
    for command in sorted_commands {
        digest.update(b"cmd\t");
        digest.update(command.id.to_string());
        for field in [
            command.name.as_str(),
            command.request_encoding,
            command.request_type.as_str(),
            command.legacy_request_schema_file_hash_128.as_str(),
            command.reply_encoding,
            command.reply_type.as_str(),
            command.legacy_reply_schema_file_hash_128.as_str(),
        ] {
            digest.update(b"\t");
            digest.update(field.as_bytes());
        }
        digest.update(b"\n");
    }
    digest.finalize()[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn schema_package_contract_literals() -> [&'static str; 21] {
    [
        CMD_KEY_PREFIX,
        META_KEY_PREFIX,
        LIVELINESS_KEY_PREFIX,
        FLATBUFFER_VALUE_MEDIA_TYPE,
        STRUCT_VALUE_MEDIA_TYPE,
        TYPE_SCHEMA_HASH_ALGORITHM,
        TOPIC_INSTANCE_KEY_GRAMMAR,
        MCAP_PROFILE,
        MCAP_SCHEMA_ENCODING,
        MCAP_MESSAGE_ENCODING,
        MCAP_METADATA_NAME,
        MCAP_SCHEMA_SET_HASH_KEY,
        MCAP_SCHEMA_SET_IDENTITY_KEY,
        MCAP_SCHEMA_PACKAGE_CONTRACT_IDENTITY_KEY,
        MCAP_SESSION_ID_KEY,
        MCAP_SOURCE_KEY,
        MCAP_TIME_BASIS_KEY,
        MCAP_TIME_BASIS_MONOTONIC_BOOT,
        MCAP_TIME_BASIS_UNIX_EPOCH,
        MCAP_TIME_BASIS_CORRELATED,
        MCAP_TOPIC_ID_KEY,
    ]
}

/// Identity of the complete schema package contract authored in this
/// repository. External CDR/RIHS01 projections, HCDF mappings, and deployment
/// manifests are intentionally excluded.
fn schema_package_contract_identity(
    schema: &CompiledSchema,
    topics: &[TopicEntry],
    commands: &[CommandEntry],
    schema_set_identity: &str,
    legacy_schema_set_hash_128: &str,
) -> String {
    let mut transcript = IdentityTranscript::new("synapse-schema-package-contract-v1");
    transcript.field(CATALOG_VERSION.to_string());
    transcript.field(schema_set_identity);
    transcript.field(legacy_schema_set_hash_128);
    for field in schema_package_contract_literals() {
        transcript.field(field);
    }

    let mut files = schema.files.iter().collect::<Vec<_>>();
    files.sort_by(|left, right| left.name.cmp(&right.name));
    transcript.field(files.len().to_string());
    for file in files {
        for field in [
            file.name.as_str(),
            file.bfbs_sha256.as_str(),
            file.root_type.as_deref().unwrap_or("-"),
            file.file_identifier.as_deref().unwrap_or("-"),
        ] {
            transcript.field(field);
        }
    }

    let mut topics = topics.iter().collect::<Vec<_>>();
    topics.sort_by_key(|topic| topic.id);
    transcript.field(topics.len().to_string());
    for topic in topics {
        let root_table = qualified_name(&topic.root_table_namespace, &topic.root_table);
        let payload_type = topic
            .payload_type
            .as_deref()
            .zip(topic.payload_type_namespace.as_deref())
            .map(|(name, namespace)| qualified_name(namespace, name))
            .unwrap_or_else(|| "-".to_string());
        let schema_stem = Path::new(&topic.schema_file)
            .file_stem()
            .and_then(|value| value.to_str())
            .expect("validated topic schema path has a UTF-8 stem");
        for field in [
            topic.id.to_string(),
            topic.name.clone(),
            topic.key.clone(),
            root_table.clone(),
            payload_type,
            optional_usize(topic.payload_size),
            topic.schema_file.clone(),
            topic.schema_artifact_sha256.clone(),
            root_table,
            format!("bfbs/{schema_stem}.bfbs"),
            topic.wire_type.clone(),
            topic.type_schema_hash.clone(),
            topic.legacy_schema_file_hash_128.clone(),
            bool_token(topic.fixed_layout).to_string(),
            bool_token(topic.multi_instance).to_string(),
            topic.scope.to_string(),
            topic.encoding.to_string(),
            topic.description.clone(),
        ] {
            transcript.field(field);
        }
    }

    let mut commands = commands.iter().collect::<Vec<_>>();
    commands.sort_by_key(|command| command.id);
    transcript.field(commands.len().to_string());
    for command in commands {
        for field in [
            command.id.to_string(),
            command.name.clone(),
            command.key.clone(),
            command.request_type.clone(),
            command.request_type_schema_hash.clone(),
            command.legacy_request_schema_file_hash_128.clone(),
            command.request_encoding.to_string(),
            optional_usize(command.request_size),
            command.reply_type.clone(),
            command.reply_type_schema_hash.clone(),
            command.legacy_reply_schema_file_hash_128.clone(),
            command.reply_encoding.to_string(),
            optional_usize(command.reply_size),
            command.description.clone(),
        ] {
            transcript.field(field);
        }
    }
    transcript.finish()
}
