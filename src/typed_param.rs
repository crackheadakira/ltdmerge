use anyhow::{Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use tomolib::formats::byml::Value;

pub trait BymlDecode: Sized {
    fn decode(value: &Value) -> Result<Self>;
}

pub fn decode<T: BymlDecode>(value: &Value) -> Result<T> {
    T::decode(value)
}

macro_rules! scalar {
    ($ty:ty, $variant:ident) => {
        impl BymlDecode for $ty {
            fn decode(value: &Value) -> Result<Self> {
                match value {
                    Value::$variant(value) => Ok(value.clone()),
                    _ => bail!("expected {}", stringify!($variant)),
                }
            }
        }
    };
}
scalar!(bool, Bool);
scalar!(String, String);
scalar!(f32, F32);
impl BymlDecode for i32 {
    fn decode(v: &Value) -> Result<Self> {
        match v {
            Value::I32(n) => Ok(*n),
            Value::U32(n) => Ok(i32::try_from(*n)?),
            _ => bail!("expected i32"),
        }
    }
}
impl BymlDecode for u32 {
    fn decode(v: &Value) -> Result<Self> {
        match v {
            Value::U32(n) => Ok(*n),
            Value::I32(n) => Ok(u32::try_from(*n)?),
            _ => bail!("expected u32"),
        }
    }
}
impl<T: BymlDecode> BymlDecode for Option<T> {
    fn decode(v: &Value) -> Result<Self> {
        if matches!(v, Value::Null) {
            Ok(None)
        } else {
            Ok(Some(T::decode(v)?))
        }
    }
}
impl<T: BymlDecode> BymlDecode for Vec<T> {
    fn decode(v: &Value) -> Result<Self> {
        let Value::Array(values) = v else {
            bail!("expected array");
        };
        values.iter().map(T::decode).collect()
    }
}
impl<T: BymlDecode> BymlDecode for BTreeMap<String, T> {
    fn decode(v: &Value) -> Result<Self> {
        let Value::Dict(values) = v else {
            bail!("expected string-keyed map");
        };
        values
            .iter()
            .map(|(k, v)| Ok((k.clone(), T::decode(v)?)))
            .collect()
    }
}

pub(crate) struct Field {
    pub name: &'static str,
    pub ty: &'static str,
    pub kind: &'static str,
    pub element: &'static str,
    pub default: Option<Value>,
    pub keys: &'static [(u32, &'static str)],
}
pub(crate) struct Schema {
    pub name: &'static str,
    pub fields: Vec<Field>,
}
fn schema(name: &str) -> &'static Schema {
    static SCHEMAS: OnceLock<Vec<Schema>> = OnceLock::new();
    SCHEMAS
        .get_or_init(crate::typed_param_schema::definitions)
        .iter()
        .find(|s| s.name == name)
        .expect("built-in TypedParam schema")
}

fn default_for(field: &Field) -> Option<Value> {
    field.default.clone().or_else(|| match field.kind {
        "embed" => Some(Value::Dict(defaults(field.ty))),
        "prop_map" | "typed_param_map" => Some(Value::Dict(BTreeMap::new())),
        "prop_buffer" | "typed_param_buffer" => Some(Value::Array(Vec::new())),
        "typed_param_enum_map" => Some(Value::Hash32(
            field
                .keys
                .iter()
                .map(|(hash, _)| (*hash, Value::Dict(defaults(field.element))))
                .collect(),
        )),
        _ => None,
    })
}
pub fn defaults(name: &str) -> BTreeMap<String, Value> {
    schema(name)
        .fields
        .iter()
        .filter_map(|f| default_for(f).map(|v| (f.name.into(), v)))
        .collect()
}

pub fn validate(name: &str, map: &BTreeMap<String, Value>) -> Result<()> {
    for f in &schema(name).fields {
        let Some(value) = map.get(f.name) else {
            continue;
        };
        let valid = match f.kind {
            "embed" => {
                if let Value::Dict(child) = value {
                    validate(f.ty, child)?;
                    true
                } else {
                    false
                }
            }
            "prop_map" => {
                if let Value::Dict(values) = value {
                    values.values().all(|v| scalar_matches(f.element, v))
                } else {
                    false
                }
            }
            "typed_param_buffer" => {
                if let Value::Array(values) = value {
                    for v in values {
                        let Value::Dict(child) = v else {
                            bail!("{}: expected parameter", f.name);
                        };
                        validate(f.element, child)?;
                    }
                    true
                } else {
                    false
                }
            }
            "typed_param_enum_map" => {
                if let Value::Hash32(values) = value {
                    for (key, v) in values {
                        // Preserve future keys in loaded documents instead of inventing meanings.
                        if !f.keys.iter().any(|(hash, _)| hash == key) {
                            continue;
                        }
                        let Value::Dict(child) = v else {
                            bail!("{}: expected parameter", f.name);
                        };
                        validate(f.element, child)?;
                    }
                    true
                } else {
                    false
                }
            }
            _ => scalar_matches(f.ty, value),
        };
        if !valid {
            bail!("{name}.{}: wrong BYML value type", f.name);
        }
    }
    Ok(())
}
fn scalar_matches(ty: &str, value: &Value) -> bool {
    match ty {
        "bool" => matches!(value, Value::Bool(_)),
        "s32" => {
            matches!(value, Value::I32(_)) || matches!(value,Value::U32(n) if *n <= i32::MAX as u32)
        }
        "u32" | "enum" => {
            matches!(value, Value::U32(_)) || matches!(value,Value::I32(n) if *n >= 0)
        }
        "f32" => matches!(value, Value::F32(_)),
        "sead::SafeString" => matches!(value, Value::String(_)),
        "sead::Vector2f" | "sead::Vector2i" => {
            matches!(value,Value::Dict(m) if ["X","Y"].iter().all(|k| m.get(*k).is_none_or(|v|scalar_matches(if ty.ends_with('i') {"s32"} else {"f32"},v))))
        }
        _ => false,
    }
}

pub fn materialize(name: &str, map: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    let mut expanded = defaults(name);
    expanded.extend(map.clone());

    for f in &schema(name).fields {
        match expanded.get_mut(f.name) {
            Some(Value::Dict(child)) if f.kind == "embed" => *child = materialize(f.ty, child),
            Some(Value::Hash32(children)) if f.kind == "typed_param_enum_map" => {
                for (hash, _) in f.keys {
                    children
                        .entry(*hash)
                        .or_insert_with(|| Value::Dict(defaults(f.element)));
                }

                for child in children.values_mut() {
                    if let Value::Dict(child) = child {
                        *child = materialize(f.element, child);
                    }
                }
            }
            _ => {}
        }
    }
    expanded
}

pub fn packed(mut map: BTreeMap<String, Value>) -> Value {
    for f in &schema("mii__Parts").fields {
        let empty = matches!(map.get(f.name),Some(Value::Dict(v)) if v.is_empty())
            || matches!(map.get(f.name),Some(Value::Hash32(v)) if v.is_empty())
            || matches!(map.get(f.name),Some(Value::Array(v)) if v.is_empty());

        if empty || map.get(f.name) == default_for(f).as_ref() {
            map.remove(f.name);
        }
    }

    map.remove("__RowId");
    map.remove("PartsType");
    Value::Dict(map)
}
pub fn row(map: BTreeMap<String, Value>) -> Value {
    let mut result = materialize("mii__Parts", &map);
    for f in &schema("mii__Parts").fields {
        if f.kind == "embed"
            && let Some(Value::Dict(child)) = result.get_mut(f.name)
        {
            *child = materialize(f.ty, child);
        }
    }

    result.remove("Components");
    result.remove("Label");
    Value::Dict(result)
}

#[derive(Debug, Clone, Default)]
pub struct BymlOrigin {
    original: Option<BTreeMap<String, Value>>,
    decoded: BTreeMap<String, Value>,
    pub manifest_fields: BTreeSet<String>,
}

impl BymlOrigin {
    pub fn loaded(original: BTreeMap<String, Value>, decoded: BTreeMap<String, Value>) -> Self {
        Self {
            original: Some(original),
            decoded,
            manifest_fields: BTreeSet::new(),
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.original.is_some()
    }

    pub fn originally_contains(&self, field: &str) -> bool {
        self.original
            .as_ref()
            .is_some_and(|map| map.contains_key(field))
    }

    pub fn write(&self, current: BTreeMap<String, Value>) -> Option<Value> {
        self.original.as_ref().map(|original| {
            let mut result = Value::Dict(original.clone());
            overlay_changes(
                &mut result,
                &Value::Dict(self.decoded.clone()),
                &Value::Dict(current),
            );
            result
        })
    }
}

fn overlay_changes(target: &mut Value, before: &Value, after: &Value) {
    if before == after {
        return;
    }

    match (target, before, after) {
        (Value::Dict(target), Value::Dict(before), Value::Dict(after)) => {
            for key in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
                if before.get(key) == after.get(key) {
                    continue;
                }

                match (before.get(key), after.get(key)) {
                    (Some(old), Some(new)) => {
                        let target = target.entry(key.clone()).or_insert_with(|| old.clone());
                        overlay_changes(target, old, new);
                    }
                    (_, Some(new)) => {
                        target.insert(key.clone(), new.clone());
                    }
                    (_, None) => {
                        target.remove(key);
                    }
                }
            }
        }

        (Value::Hash32(target), Value::Hash32(before), Value::Hash32(after)) => {
            for key in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
                if before.get(key) == after.get(key) {
                    continue;
                }

                match (before.get(key), after.get(key)) {
                    (Some(old), Some(new)) => {
                        overlay_changes(target.entry(*key).or_insert_with(|| old.clone()), old, new)
                    }
                    (_, Some(new)) => {
                        target.insert(*key, new.clone());
                    }
                    (_, None) => {
                        target.remove(key);
                    }
                }
            }
        }

        (Value::Array(target), Value::Array(before), Value::Array(after))
            if target.len() == before.len() && before.len() == after.len() =>
        {
            for ((target, before), after) in target.iter_mut().zip(before).zip(after) {
                overlay_changes(target, before, after);
            }
        }

        (target, _, after) => *target = after.clone(),
    }
}

pub fn murmur32(s: &str) -> u32 {
    let mut hash = 0u32;
    let mut chunks = s.as_bytes().chunks_exact(4);

    for chunk in &mut chunks {
        let k = u32::from_le_bytes(chunk.try_into().unwrap())
            .wrapping_mul(0xcc9e2d51)
            .rotate_left(15)
            .wrapping_mul(0x1b873593);

        hash = (hash ^ k)
            .rotate_left(13)
            .wrapping_mul(5)
            .wrapping_add(0xe6546b64);
    }

    let mut tail = 0u32;
    for (i, b) in chunks.remainder().iter().enumerate() {
        tail |= u32::from(*b) << (8 * i);
    }

    if !chunks.remainder().is_empty() {
        hash ^= tail
            .wrapping_mul(0xcc9e2d51)
            .rotate_left(15)
            .wrapping_mul(0x1b873593);
    }

    hash ^= s.len() as u32;
    hash = (hash ^ (hash >> 16)).wrapping_mul(0x85ebca6b);
    hash = (hash ^ (hash >> 13)).wrapping_mul(0xc2b2ae35);
    hash ^ (hash >> 16)
}
