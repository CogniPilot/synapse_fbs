//! BFBS-backed semantic IR used by language-neutral generators.

use std::{collections::BTreeMap, fs, io, path::Path};

use flatbuffers_reflection::reflection::{self, BaseType};
use serde::Serialize;

use super::{Result, SchemaDoc, qualified_name};

pub const PROFILE: &str = "synapse-fbs-semantics/1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Schema {
    pub profile: &'static str,
    pub version: String,
    pub source: &'static str,
    pub objects: Vec<Object>,
    pub enums: Vec<Enum>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Object {
    pub name: String,
    pub namespace: String,
    pub short_name: String,
    pub kind: &'static str,
    pub declaration_file: String,
    pub documentation: String,
    pub attributes: BTreeMap<String, String>,
    pub fixed_layout: bool,
    pub byte_size: Option<i32>,
    pub alignment: Option<i32>,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Field {
    pub name: String,
    pub id: u16,
    pub offset: u16,
    pub type_name: String,
    pub type_info: TypeInfo,
    pub synthetic: bool,
    pub documentation: String,
    pub attributes: BTreeMap<String, String>,
    pub unit: Option<String>,
    pub min: Option<String>,
    pub max: Option<String>,
    pub frame: Option<String>,
    pub clock: Option<String>,
    pub scale: Option<String>,
    pub valid_if: Option<String>,
    pub logical_type: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TypeInfo {
    pub base: &'static str,
    pub element: Option<&'static str>,
    pub referenced_type: Option<String>,
    pub fixed_length: Option<u16>,
    pub variable_length: bool,
    pub base_size: u32,
    pub element_size: u32,
    pub scalar_width_bits: Option<u32>,
    pub signed: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Enum {
    pub name: String,
    pub namespace: String,
    pub short_name: String,
    pub declaration_file: String,
    pub documentation: String,
    pub attributes: BTreeMap<String, String>,
    pub underlying_type: String,
    pub bit_flags: bool,
    pub is_union: bool,
    pub values: Vec<EnumValue>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EnumValue {
    pub name: String,
    pub value: i64,
    pub documentation: String,
    pub attributes: BTreeMap<String, String>,
    pub union_type: Option<TypeInfo>,
}

pub fn load(path: &Path, docs: &SchemaDoc, version: &str) -> Result<Schema> {
    let bytes = fs::read(path)?;
    let bfbs = reflection::root_as_schema(&bytes)
        .map_err(|err| io::Error::other(format!("invalid {}: {err}", path.display())))?;
    let object_refs: Vec<_> = bfbs.objects().iter().collect();
    let enum_refs: Vec<_> = bfbs.enums().iter().collect();
    let docs = Documentation::new(docs)?;

    let mut objects = Vec::with_capacity(object_refs.len());
    for object in &object_refs {
        let (namespace, short_name) = split_name(object.name());
        let mut fields: Vec<_> = object.fields().iter().collect();
        fields.sort_by_key(|field| field.id());
        let fields = fields
            .into_iter()
            .map(|field| {
                let attributes = attributes(field.attributes());
                Field {
                    name: field.name().to_string(),
                    id: field.id(),
                    offset: field.offset(),
                    type_name: fbs_type(field.type_(), &object_refs, &enum_refs),
                    type_info: type_info(field.type_(), &object_refs, &enum_refs),
                    synthetic: field.type_().base_type() == BaseType::UType
                        && field.name().ends_with("_type"),
                    documentation: docs.field(object.name(), field.name()),
                    unit: attributes.get("unit").cloned(),
                    min: attributes.get("min").cloned(),
                    max: attributes.get("max").cloned(),
                    frame: attributes.get("frame").cloned(),
                    clock: attributes.get("clock").cloned(),
                    scale: attributes.get("scale").cloned(),
                    valid_if: attributes.get("valid_if").cloned(),
                    logical_type: attributes.get("logical_type").cloned(),
                    attributes,
                }
            })
            .collect::<Vec<_>>();
        let fixed_layout = object.is_struct();
        objects.push(Object {
            name: object.name().to_string(),
            namespace,
            short_name,
            kind: if fixed_layout { "struct" } else { "table" },
            declaration_file: declaration_file(object.declaration_file()),
            documentation: docs.entity(object.name()),
            attributes: attributes(object.attributes()),
            fixed_layout,
            byte_size: fixed_layout.then_some(object.bytesize()),
            alignment: fixed_layout.then_some(object.minalign()),
            fields,
        });
    }

    let mut enums = Vec::with_capacity(enum_refs.len());
    for enumeration in &enum_refs {
        let (namespace, short_name) = split_name(enumeration.name());
        let attrs = attributes(enumeration.attributes());
        let bit_flags = attrs.contains_key("bit_flags");
        let values = enumeration
            .values()
            .iter()
            .filter(|value| !(enumeration.is_union() && value.name() == "NONE"))
            .map(|value| EnumValue {
                name: value.name().to_string(),
                value: value.value(),
                documentation: docs.field(enumeration.name(), value.name()),
                attributes: attributes(value.attributes()),
                union_type: enumeration
                    .is_union()
                    .then(|| value.union_type())
                    .flatten()
                    .map(|ty| type_info(ty, &object_refs, &enum_refs)),
            })
            .collect();
        enums.push(Enum {
            name: enumeration.name().to_string(),
            namespace,
            short_name,
            declaration_file: declaration_file(enumeration.declaration_file()),
            documentation: docs.entity(enumeration.name()),
            attributes: attrs,
            underlying_type: scalar_name(enumeration.underlying_type().base_type()).to_string(),
            bit_flags,
            is_union: enumeration.is_union(),
            values,
        });
    }

    objects.sort_by(|left, right| left.name.cmp(&right.name));
    enums.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(Schema {
        profile: PROFILE,
        version: version.to_string(),
        source: "fbs/all.fbs via flatc BFBS",
        objects,
        enums,
    })
}

/// Runtime BFBS must never carry prose: these exact bytes are embedded in
/// firmware/language packages and copied into MCAP Schema records.
pub fn validate_compact_bfbs(path: &Path) -> Result<()> {
    let bytes = fs::read(path)?;
    let bfbs = reflection::root_as_schema(&bytes)
        .map_err(|err| io::Error::other(format!("invalid {}: {err}", path.display())))?;
    let mut documented = Vec::new();
    for object in bfbs.objects() {
        if has_documentation(object.documentation()) {
            documented.push(object.name().to_string());
        }
        for field in object.fields() {
            if has_documentation(field.documentation()) {
                documented.push(format!("{}.{}", object.name(), field.name()));
            }
        }
    }
    for enumeration in bfbs.enums() {
        if has_documentation(enumeration.documentation()) {
            documented.push(enumeration.name().to_string());
        }
        for value in enumeration.values() {
            if has_documentation(value.documentation()) {
                documented.push(format!("{}.{}", enumeration.name(), value.name()));
            }
        }
    }
    if documented.is_empty() {
        Ok(())
    } else {
        super::fail(format!(
            "compact BFBS unexpectedly contains documentation: {}",
            documented.join(", ")
        ))
    }
}

fn has_documentation(
    documentation: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<&'_ str>>>,
) -> bool {
    documentation.is_some_and(|values| !values.is_empty())
}

pub fn validate(schema: &Schema) -> Result<()> {
    const FRAMES: &[&str] = &["none", "enu", "flu", "wgs84", "sensor", "image"];
    const CLOCKS: &[&str] = &["monotonic_boot", "unix_epoch", "gps", "correlated"];
    let mut problems = Vec::new();
    for object in &schema.objects {
        if object.documentation.is_empty() {
            problems.push(format!("{} has no source documentation", object.name));
        }
        if object.fixed_layout
            && (object.byte_size.is_none_or(|size| size <= 0)
                || object.alignment.is_none_or(|alignment| alignment <= 0))
        {
            problems.push(format!(
                "{} has invalid reflected fixed-layout size or alignment",
                object.name
            ));
        }
        for field in &object.fields {
            if !field.synthetic && field.documentation.is_empty() {
                problems.push(format!(
                    "{}.{} has no source documentation",
                    object.name, field.name
                ));
            }
            if field.type_info.fixed_length == Some(0) {
                problems.push(format!(
                    "{}.{} has a zero-length reflected array",
                    object.name, field.name
                ));
            }
            if field.unit.as_deref().is_some_and(str::is_empty) {
                problems.push(format!("{}.{} has an empty unit", object.name, field.name));
            }
            if let Some(frame) = field.frame.as_deref()
                && !FRAMES.contains(&frame)
            {
                problems.push(format!(
                    "{}.{} has unknown frame {frame}",
                    object.name, field.name
                ));
            }
            if let Some(clock) = field.clock.as_deref()
                && !CLOCKS.contains(&clock)
            {
                problems.push(format!(
                    "{}.{} has unknown clock {clock}",
                    object.name, field.name
                ));
            }
            if field.clock.is_some() && field.unit.as_deref() != Some("s") {
                problems.push(format!(
                    "{}.{} has a clock but is not expressed in seconds",
                    object.name, field.name
                ));
            }
            if let Some(scale) = field.scale.as_deref() {
                match scale.parse::<f64>() {
                    Ok(value) if value.is_finite() && value > 0.0 => {}
                    _ => problems.push(format!(
                        "{}.{} has invalid scale {scale}",
                        object.name, field.name
                    )),
                }
                if field.unit.is_none() {
                    problems.push(format!(
                        "{}.{} has a scale without a unit",
                        object.name, field.name
                    ));
                }
            }
            let min = validate_bound(
                field.min.as_deref(),
                "minimum",
                object,
                field,
                &mut problems,
            );
            let max = validate_bound(
                field.max.as_deref(),
                "maximum",
                object,
                field,
                &mut problems,
            );
            if (min.is_some() || max.is_some()) && field.unit.is_none() {
                problems.push(format!(
                    "{}.{} has semantic bounds without a unit",
                    object.name, field.name
                ));
            }
            if let (Some(min), Some(max)) = (min, max)
                && min > max
            {
                problems.push(format!(
                    "{}.{} has minimum {min} greater than maximum {max}",
                    object.name, field.name
                ));
            }
            if field.valid_if.as_deref().is_some_and(str::is_empty) {
                problems.push(format!(
                    "{}.{} has an empty validity condition",
                    object.name, field.name
                ));
            }
            if field.logical_type.as_deref().is_some_and(str::is_empty) {
                problems.push(format!(
                    "{}.{} has an empty logical type",
                    object.name, field.name
                ));
            }
        }
    }
    for enumeration in &schema.enums {
        if enumeration.documentation.is_empty() {
            problems.push(format!("{} has no source documentation", enumeration.name));
        }
        for value in &enumeration.values {
            if value.documentation.is_empty() {
                problems.push(format!(
                    "{}.{} has no source documentation",
                    enumeration.name, value.name
                ));
            }
            if enumeration.is_union && value.union_type.is_none() {
                problems.push(format!(
                    "{}.{} has no reflected union target type",
                    enumeration.name, value.name
                ));
            }
            if !enumeration.is_union && value.union_type.is_some() {
                problems.push(format!(
                    "{}.{} unexpectedly has a union target type",
                    enumeration.name, value.name
                ));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        super::fail(format!(
            "semantic schema validation failed:\n{}",
            problems.join("\n")
        ))
    }
}

fn validate_bound(
    bound: Option<&str>,
    label: &str,
    object: &Object,
    field: &Field,
    problems: &mut Vec<String>,
) -> Option<f64> {
    let bound = bound?;
    match bound.parse::<f64>() {
        Ok(value) if value.is_finite() => Some(value),
        _ => {
            problems.push(format!(
                "{}.{} has invalid {label} {bound}",
                object.name, field.name
            ));
            None
        }
    }
}

struct Documentation {
    entities: BTreeMap<String, String>,
    fields: BTreeMap<(String, String), String>,
}

impl Documentation {
    fn new(docs: &SchemaDoc) -> Result<Self> {
        let mut entities = BTreeMap::new();
        let mut fields = BTreeMap::new();
        for file in &docs.files {
            for entity in &file.entities {
                let name = qualified_name(&entity.namespace, &entity.name);
                if entities
                    .insert(name.clone(), entity.comments.join(" "))
                    .is_some()
                {
                    return super::fail(format!("duplicate source documentation path for {name}"));
                }
                for member in &entity.members {
                    let path = (name.clone(), member.name.clone());
                    if fields
                        .insert(path.clone(), member.comments.join(" "))
                        .is_some()
                    {
                        return super::fail(format!(
                            "duplicate source documentation path for {}.{}",
                            path.0, path.1
                        ));
                    }
                }
            }
        }
        Ok(Self { entities, fields })
    }

    fn entity(&self, name: &str) -> String {
        self.entities.get(name).cloned().unwrap_or_default()
    }

    fn field(&self, entity: &str, field: &str) -> String {
        self.fields
            .get(&(entity.to_string(), field.to_string()))
            .cloned()
            .unwrap_or_default()
    }
}

fn declaration_file(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .trim_start_matches("//")
        .replace('\\', "/")
}

fn attributes(
    values: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<reflection::KeyValue<'_>>>>,
) -> BTreeMap<String, String> {
    values
        .into_iter()
        .flat_map(|values| values.iter())
        .map(|entry| {
            (
                entry.key().to_string(),
                entry.value().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

fn split_name(name: &str) -> (String, String) {
    name.rsplit_once('.')
        .map(|(namespace, name)| (namespace.to_string(), name.to_string()))
        .unwrap_or_else(|| (String::new(), name.to_string()))
}

fn type_info(
    ty: reflection::Type<'_>,
    objects: &[reflection::Object<'_>],
    enums: &[reflection::Enum<'_>],
) -> TypeInfo {
    let base = ty.base_type();
    let element = ty.element();
    let scalar = if matches!(
        base,
        BaseType::Array | BaseType::Vector | BaseType::Vector64
    ) {
        element
    } else {
        base
    };
    let referenced_type = match base {
        BaseType::Obj => index(ty.index(), objects).map(|value| value.name().to_string()),
        BaseType::Union => index(ty.index(), enums).map(|value| value.name().to_string()),
        BaseType::Array | BaseType::Vector | BaseType::Vector64 if element == BaseType::Obj => {
            index(ty.index(), objects).map(|value| value.name().to_string())
        }
        _ if ty.index() >= 0 => index(ty.index(), enums).map(|value| value.name().to_string()),
        _ => None,
    };
    let (scalar_width_bits, signed) = scalar_properties(scalar);
    TypeInfo {
        base: scalar_name(base),
        element: (element != BaseType::None).then(|| scalar_name(element)),
        referenced_type,
        fixed_length: (base == BaseType::Array).then_some(ty.fixed_length()),
        variable_length: matches!(base, BaseType::Vector | BaseType::Vector64),
        base_size: ty.base_size(),
        element_size: ty.element_size(),
        scalar_width_bits,
        signed,
    }
}

fn scalar_properties(base: BaseType) -> (Option<u32>, Option<bool>) {
    match base {
        BaseType::Byte => (Some(8), Some(true)),
        BaseType::UByte | BaseType::UType => (Some(8), Some(false)),
        BaseType::Short => (Some(16), Some(true)),
        BaseType::UShort => (Some(16), Some(false)),
        BaseType::Int => (Some(32), Some(true)),
        BaseType::UInt => (Some(32), Some(false)),
        BaseType::Long => (Some(64), Some(true)),
        BaseType::ULong => (Some(64), Some(false)),
        BaseType::Float => (Some(32), None),
        BaseType::Double => (Some(64), None),
        BaseType::Bool => (Some(8), None),
        _ => (None, None),
    }
}

fn fbs_type(
    ty: reflection::Type<'_>,
    objects: &[reflection::Object<'_>],
    enums: &[reflection::Enum<'_>],
) -> String {
    match ty.base_type() {
        BaseType::Obj => index(ty.index(), objects)
            .map(|value| value.name().to_string())
            .unwrap_or_else(|| "object".into()),
        BaseType::Vector | BaseType::Vector64 => {
            format!("[{}]", element_fbs_type(ty, objects, enums))
        }
        BaseType::Array => format!(
            "[{}:{}]",
            element_fbs_type(ty, objects, enums),
            ty.fixed_length()
        ),
        BaseType::Union => index(ty.index(), enums)
            .map(|value| value.name().to_string())
            .unwrap_or_else(|| "union".into()),
        base => {
            if ty.index() >= 0 {
                index(ty.index(), enums)
                    .map(|value| value.name().to_string())
                    .unwrap_or_else(|| scalar_name(base).into())
            } else {
                scalar_name(base).into()
            }
        }
    }
}

fn element_fbs_type(
    ty: reflection::Type<'_>,
    objects: &[reflection::Object<'_>],
    enums: &[reflection::Enum<'_>],
) -> String {
    match ty.element() {
        BaseType::Obj => index(ty.index(), objects)
            .map(|value| value.name().to_string())
            .unwrap_or_else(|| "object".into()),
        BaseType::Union => index(ty.index(), enums)
            .map(|value| value.name().to_string())
            .unwrap_or_else(|| "union".into()),
        base => {
            if ty.index() >= 0 {
                index(ty.index(), enums)
                    .map(|value| value.name().to_string())
                    .unwrap_or_else(|| scalar_name(base).into())
            } else {
                scalar_name(base).into()
            }
        }
    }
}

fn scalar_name(base: BaseType) -> &'static str {
    match base {
        BaseType::None => "none",
        BaseType::UType => "utype",
        BaseType::Bool => "bool",
        BaseType::Byte => "byte",
        BaseType::UByte => "ubyte",
        BaseType::Short => "short",
        BaseType::UShort => "ushort",
        BaseType::Int => "int",
        BaseType::UInt => "uint",
        BaseType::Long => "long",
        BaseType::ULong => "ulong",
        BaseType::Float => "float",
        BaseType::Double => "double",
        BaseType::String => "string",
        BaseType::Vector => "vector",
        BaseType::Obj => "object",
        BaseType::Union => "union",
        BaseType::Array => "array",
        BaseType::Vector64 => "vector64",
        _ => "unknown",
    }
}

fn index<T: Copy>(index: i32, values: &[T]) -> Option<T> {
    usize::try_from(index)
        .ok()
        .and_then(|index| values.get(index))
        .copied()
}
