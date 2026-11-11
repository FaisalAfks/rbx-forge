//! Roblox values and their two written forms: the JSON the plugin sends, and the TOML on disk
//! (`docs/sync/format.md`). Every type Studio saves and Luau can read has a form here; numbers keep every bit.

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::{number, text};

/// The type of a property, as the schema gives it. The JSON form names it with these strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ValueType {
    #[serde(rename = "bool")]
    Bool,
    #[serde(rename = "int")]
    Int,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "string")]
    String,
    Content,
    Enum,
    BrickColor,
    Color3,
    ColorSequence,
    NumberSequence,
    NumberRange,
    CFrame,
    Faces,
    Axes,
    Font,
    PhysicalProperties,
    Ray,
    Rect,
    Ref,
    SecurityCapabilities,
    UDim,
    UDim2,
    Vector2,
    Vector3,
    Vector3int16,
}

impl ValueType {
    fn name(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }

    fn from_name(name: &str) -> Result<ValueType> {
        serde_json::from_value(Json::String(name.to_string()))
            .map_err(|_| anyhow!("unknown value type {name:?}"))
    }
}

/// What a reference points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefTarget {
    /// An instance in the same tree, by its id in that tree.
    Node(u32),
    /// An instance outside the tree, by its names from `game` down (`["Workspace", "Arena"]`).
    Outside(Vec<String>),
    /// A disk path not yet matched to an instance. Only seen inside the layout reader, which resolves it.
    Path(String),
}

/// `CustomPhysicalProperties`. The acoustic part is optional, for Studios that don't have it.
#[derive(Debug, Clone, PartialEq)]
pub struct Physical {
    pub density: f64,
    pub friction: f64,
    pub elasticity: f64,
    pub friction_weight: f64,
    pub elasticity_weight: f64,
    pub acoustic_absorption: Option<f64>,
}

/// A property or attribute value. Floats are `f64` holding exactly what Studio reported.
#[derive(Debug, Clone)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Content(String),
    /// An enum item: its name for properties (`"Center"`), `"Type.Item"` for attributes.
    Enum(String),
    BrickColor(i64),
    Color3([f64; 3]),
    ColorSequence(Vec<(f64, [f64; 3])>),
    /// Keypoints as time, value, envelope.
    NumberSequence(Vec<[f64; 3]>),
    NumberRange([f64; 2]),
    /// Position, then the rotation matrix row by row.
    CFrame([f64; 12]),
    Faces(Vec<String>),
    Axes(Vec<String>),
    Font {
        family: String,
        weight: String,
        style: String,
    },
    PhysicalProperties(Physical),
    Ray {
        origin: [f64; 3],
        direction: [f64; 3],
    },
    Rect([f64; 4]),
    Ref(RefTarget),
    SecurityCapabilities(Vec<String>),
    UDim(f64, i64),
    /// X scale, X offset, Y scale, Y offset.
    UDim2(f64, i64, f64, i64),
    Vector2([f64; 2]),
    Vector3([f64; 3]),
    Vector3int16([i64; 3]),
}

/// Writes a reference as TOML text; the layout writer knows the paths.
pub type RefWriter<'a> = &'a dyn Fn(&RefTarget) -> Result<String>;

impl Value {
    pub fn ty(&self) -> ValueType {
        match self {
            Value::Bool(_) => ValueType::Bool,
            Value::Int(_) => ValueType::Int,
            Value::Float(_) => ValueType::Float,
            Value::String(_) => ValueType::String,
            Value::Content(_) => ValueType::Content,
            Value::Enum(_) => ValueType::Enum,
            Value::BrickColor(_) => ValueType::BrickColor,
            Value::Color3(_) => ValueType::Color3,
            Value::ColorSequence(_) => ValueType::ColorSequence,
            Value::NumberSequence(_) => ValueType::NumberSequence,
            Value::NumberRange(_) => ValueType::NumberRange,
            Value::CFrame(_) => ValueType::CFrame,
            Value::Faces(_) => ValueType::Faces,
            Value::Axes(_) => ValueType::Axes,
            Value::Font { .. } => ValueType::Font,
            Value::PhysicalProperties(_) => ValueType::PhysicalProperties,
            Value::Ray { .. } => ValueType::Ray,
            Value::Rect(_) => ValueType::Rect,
            Value::Ref(_) => ValueType::Ref,
            Value::SecurityCapabilities(_) => ValueType::SecurityCapabilities,
            Value::UDim(..) => ValueType::UDim,
            Value::UDim2(..) => ValueType::UDim2,
            Value::Vector2(_) => ValueType::Vector2,
            Value::Vector3(_) => ValueType::Vector3,
            Value::Vector3int16(_) => ValueType::Vector3int16,
        }
    }

    /// The value as TOML text, in the forms `docs/sync/format.md` lists.
    pub fn to_toml(&self, refs: RefWriter) -> Result<String> {
        Ok(match self {
            Value::Bool(b) => b.to_string(),
            Value::Int(i) | Value::BrickColor(i) => i.to_string(),
            Value::Float(f) => number::float(*f),
            Value::String(s) | Value::Content(s) | Value::Enum(s) => text::string(s),
            Value::Color3(c) => color(c),
            Value::ColorSequence(keys) => list(
                keys.iter()
                    .map(|(t, c)| format!("[{}, {}]", number::float(*t), color(c))),
            ),
            Value::NumberSequence(keys) => list(keys.iter().map(|k| {
                // The envelope is almost always 0; it's only written when it isn't.
                let shown = if k[2] == 0.0 && k[2].is_sign_positive() {
                    &k[..2]
                } else {
                    &k[..]
                };
                floats(shown)
            })),
            Value::NumberRange(r) => floats(r),
            Value::CFrame(m) => floats(m),
            Value::Rect(r) => floats(r),
            Value::Vector2(v) => floats(v),
            Value::Vector3(v) => floats(v),
            Value::Faces(names) | Value::Axes(names) | Value::SecurityCapabilities(names) => {
                list(names.iter().map(|n| text::string(n)))
            }
            Value::Font {
                family,
                weight,
                style,
            } => format!(
                "{{ family = {}, weight = {}, style = {} }}",
                text::string(family),
                text::string(weight),
                text::string(style)
            ),
            Value::PhysicalProperties(p) => {
                let mut parts = vec![
                    format!("density = {}", number::float(p.density)),
                    format!("friction = {}", number::float(p.friction)),
                    format!("elasticity = {}", number::float(p.elasticity)),
                    format!("friction_weight = {}", number::float(p.friction_weight)),
                    format!("elasticity_weight = {}", number::float(p.elasticity_weight)),
                ];
                if let Some(a) = p.acoustic_absorption {
                    parts.push(format!("acoustic_absorption = {}", number::float(a)));
                }
                format!("{{ {} }}", parts.join(", "))
            }
            Value::Ray { origin, direction } => {
                format!(
                    "{{ origin = {}, direction = {} }}",
                    floats(origin),
                    floats(direction)
                )
            }
            Value::Ref(target) => text::string(&refs(target)?),
            Value::UDim(s, o) => format!("[{}, {o}]", number::float(*s)),
            Value::UDim2(xs, xo, ys, yo) => format!(
                "[{}, {xo}, {}, {yo}]",
                number::float(*xs),
                number::float(*ys)
            ),
            Value::Vector3int16(v) => format!("[{}, {}, {}]", v[0], v[1], v[2]),
        })
    }

    /// Reads a value of type `ty` from TOML. A reference comes back as `RefTarget::Path` for the reader to resolve.
    pub fn from_toml(v: &toml::Value, ty: ValueType) -> Result<Value> {
        read_toml(v, ty).with_context(|| format!("expected {} (docs/sync/format.md)", describe(ty)))
    }

    /// Reads the plugin's JSON form of a value of type `ty`.
    pub fn from_json(v: &Json, ty: ValueType) -> Result<Value> {
        read_json(v, ty).with_context(|| format!("bad {} value {v}", ty.name()))
    }

    /// The plugin's JSON form. Non-finite floats are strings, since JSON has no infinity (decision 0014).
    pub fn to_json(&self) -> Json {
        use serde_json::json;
        let f = |x: f64| json_float(x);
        let fs = |xs: &[f64]| Json::Array(xs.iter().map(|x| json_float(*x)).collect());
        match self {
            Value::Bool(b) => json!(b),
            Value::Int(i) | Value::BrickColor(i) => json!(i),
            Value::Float(x) => f(*x),
            Value::String(s) | Value::Content(s) | Value::Enum(s) => json!(s),
            Value::Color3(c) | Value::Vector3(c) => fs(c),
            Value::ColorSequence(k) => {
                Json::Array(k.iter().map(|(t, c)| json!([f(*t), fs(c)])).collect())
            }
            Value::NumberSequence(k) => Json::Array(k.iter().map(|k| fs(k)).collect()),
            Value::NumberRange(r) | Value::Vector2(r) => fs(r),
            Value::CFrame(m) => fs(m),
            Value::Rect(r) => fs(r),
            Value::Faces(n) | Value::Axes(n) | Value::SecurityCapabilities(n) => json!(n),
            Value::Font {
                family,
                weight,
                style,
            } => json!({ "family": family, "weight": weight, "style": style }),
            Value::PhysicalProperties(p) => {
                let mut o = json!({
                    "density": f(p.density), "friction": f(p.friction), "elasticity": f(p.elasticity),
                    "friction_weight": f(p.friction_weight), "elasticity_weight": f(p.elasticity_weight),
                });
                if let Some(a) = p.acoustic_absorption {
                    o["acoustic_absorption"] = f(a);
                }
                o
            }
            Value::Ray { origin, direction } => {
                json!({ "origin": fs(origin), "direction": fs(direction) })
            }
            Value::Ref(RefTarget::Node(id)) => json!(id),
            Value::Ref(RefTarget::Outside(names)) => json!(names),
            Value::Ref(RefTarget::Path(p)) => json!(p),
            Value::UDim(s, o) => json!([f(*s), o]),
            Value::UDim2(xs, xo, ys, yo) => json!([f(*xs), xo, f(*ys), yo]),
            Value::Vector3int16(v) => json!(v),
        }
    }

    /// An attribute as TOML: strings, booleans and numbers plain, every other type as `{ type, value }`
    /// (decision 0011), since an attribute has no schema to say its type.
    pub fn attribute_to_toml(&self) -> Result<String> {
        let no_refs: RefWriter = &|_| bail!("attributes can't hold references");
        match self {
            Value::String(_) | Value::Bool(_) | Value::Float(_) => self.to_toml(no_refs),
            Value::Int(i) => Ok(number::float(*i as f64)),
            other => Ok(format!(
                "{{ type = {}, value = {} }}",
                text::string(&other.ty().name()),
                other.to_toml(no_refs)?
            )),
        }
    }

    /// Reads an attribute written by `attribute_to_toml`.
    pub fn attribute_from_toml(v: &toml::Value) -> Result<Value> {
        match v {
            toml::Value::String(s) => Ok(Value::String(s.clone())),
            toml::Value::Boolean(b) => Ok(Value::Bool(*b)),
            toml::Value::Integer(_) | toml::Value::Float(_) => {
                Value::from_toml(v, ValueType::Float)
            }
            toml::Value::Table(t) => {
                let ty = t
                    .get("type")
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| anyhow!("a typed attribute needs `type`"))?;
                let value = t
                    .get("value")
                    .ok_or_else(|| anyhow!("a typed attribute needs `value`"))?;
                if t.len() != 2 {
                    bail!("a typed attribute has only `type` and `value`");
                }
                Value::from_toml(value, ValueType::from_name(ty)?)
            }
            other => bail!(
                "attribute value {other} isn't a string, boolean, number or {{ type, value }} table"
            ),
        }
    }

    /// Reads an attribute in the plugin's JSON form: `{ "type": ..., "value": ... }`. Numbers are always floats.
    pub fn attribute_from_json(v: &Json) -> Result<Value> {
        let ty = v["type"]
            .as_str()
            .ok_or_else(|| anyhow!("an attribute needs a type: {v}"))?;
        let ty = match ty {
            "number" => ValueType::Float,
            "boolean" => ValueType::Bool,
            "EnumItem" => ValueType::Enum,
            other => ValueType::from_name(other)?,
        };
        Value::from_json(&v["value"], ty)
    }

    /// Whether two values are the same, bit for bit: `-0.0` isn't `0.0`, and NaN equals NaN. Their written forms
    /// are exact, so comparing those is comparing the values.
    pub fn same(&self, other: &Value) -> bool {
        let r: RefWriter = &|t| Ok(format!("{t:?}"));
        self.ty() == other.ty() && self.to_toml(r).ok() == other.to_toml(r).ok()
    }
}

fn floats(xs: &[f64]) -> String {
    list(xs.iter().map(|x| number::float(*x)))
}

fn list(items: impl Iterator<Item = String>) -> String {
    format!("[{}]", items.collect::<Vec<_>>().join(", "))
}

/// `#rrggbb` when every part is exactly an 8-bit value as Studio stores it (an `f32`), otherwise `[r, g, b]`.
fn color(c: &[f64; 3]) -> String {
    let bytes: Option<Vec<u8>> = c.iter().map(|v| color_byte(*v)).collect();
    match bytes {
        Some(b) => format!("\"#{:02x}{:02x}{:02x}\"", b[0], b[1], b[2]),
        None => floats(c),
    }
}

fn color_byte(v: f64) -> Option<u8> {
    if !(0.0..=1.0).contains(&v) {
        return None;
    }
    let q = (v * 255.0).round();
    (byte_to_color(q as u8).to_bits() == v.to_bits()).then_some(q as u8)
}

/// The value Studio stores for an 8-bit colour part: `k / 255` rounded to `f32` (checked against `Color3.fromRGB`).
fn byte_to_color(k: u8) -> f64 {
    (k as f64 / 255.0) as f32 as f64
}

fn describe(ty: ValueType) -> &'static str {
    match ty {
        ValueType::Bool => "true or false",
        ValueType::Int | ValueType::BrickColor => "a whole number",
        ValueType::Float => "a number",
        ValueType::String | ValueType::Content => "a string",
        ValueType::Enum => "an enum item name, as a string",
        ValueType::Color3 => "\"#rrggbb\" or [r, g, b]",
        ValueType::ColorSequence => "[[time, colour], ...]",
        ValueType::NumberSequence => "[[time, value], ...] or [[time, value, envelope], ...]",
        ValueType::NumberRange => "[min, max]",
        ValueType::CFrame => "12 numbers: [x, y, z, r00, r01, r02, r10, r11, r12, r20, r21, r22]",
        ValueType::Faces | ValueType::Axes | ValueType::SecurityCapabilities => "a list of names",
        ValueType::Font => "{ family, weight, style }",
        ValueType::PhysicalProperties => {
            "{ density, friction, elasticity, friction_weight, elasticity_weight, acoustic_absorption? }"
        }
        ValueType::Ray => "{ origin = [x, y, z], direction = [x, y, z] }",
        ValueType::Rect => "[min x, min y, max x, max y]",
        ValueType::Ref => "\"@path\"",
        ValueType::UDim => "[scale, offset]",
        ValueType::UDim2 => "[x scale, x offset, y scale, y offset]",
        ValueType::Vector2 => "[x, y]",
        ValueType::Vector3 | ValueType::Vector3int16 => "[x, y, z]",
    }
}

fn read_toml(v: &toml::Value, ty: ValueType) -> Result<Value> {
    use toml::Value as T;
    let f = |v: &T| -> Result<f64> {
        match v {
            T::Float(x) => Ok(*x),
            T::Integer(i) => Ok(*i as f64),
            other => bail!("{other} isn't a number"),
        }
    };
    let int = |v: &T| {
        v.as_integer()
            .ok_or_else(|| anyhow!("{v} isn't a whole number"))
    };
    let arr = |v: &T, n: usize| -> Result<Vec<T>> {
        let a = v.as_array().ok_or_else(|| anyhow!("{v} isn't a list"))?;
        if n != 0 && a.len() != n {
            bail!("{v} has {} items, not {n}", a.len());
        }
        Ok(a.clone())
    };
    let fa = |v: &T, n: usize| -> Result<Vec<f64>> { arr(v, n)?.iter().map(f).collect() };
    let names = |v: &T| -> Result<Vec<String>> {
        arr(v, 0)?
            .iter()
            .map(|x| {
                x.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("{x} isn't a name"))
            })
            .collect()
    };
    let s = |v: &T| {
        v.as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("{v} isn't a string"))
    };
    let col = |v: &T| -> Result<[f64; 3]> {
        match v {
            T::String(h) => parse_hex(h),
            other => Ok(to3(&fa(other, 3)?)),
        }
    };
    let table = |v: &T| {
        v.as_table()
            .cloned()
            .ok_or_else(|| anyhow!("{v} isn't a {{ ... }} table"))
    };
    Ok(match ty {
        ValueType::Bool => Value::Bool(
            v.as_bool()
                .ok_or_else(|| anyhow!("{v} isn't true or false"))?,
        ),
        ValueType::Int => Value::Int(int(v)?),
        ValueType::BrickColor => Value::BrickColor(int(v)?),
        ValueType::Float => Value::Float(f(v)?),
        ValueType::String => Value::String(s(v)?),
        ValueType::Content => Value::Content(s(v)?),
        ValueType::Enum => Value::Enum(s(v)?),
        ValueType::Color3 => Value::Color3(col(v)?),
        ValueType::ColorSequence => Value::ColorSequence(
            arr(v, 0)?
                .iter()
                .map(|k| {
                    let k = arr(k, 2)?;
                    Ok((f(&k[0])?, col(&k[1])?))
                })
                .collect::<Result<_>>()?,
        ),
        ValueType::NumberSequence => Value::NumberSequence(
            arr(v, 0)?
                .iter()
                .map(|k| {
                    let k = fa(k, 0)?;
                    match k.len() {
                        2 => Ok([k[0], k[1], 0.0]),
                        3 => Ok([k[0], k[1], k[2]]),
                        n => bail!("a keypoint has 2 or 3 numbers, not {n}"),
                    }
                })
                .collect::<Result<_>>()?,
        ),
        ValueType::NumberRange => {
            let r = fa(v, 2)?;
            Value::NumberRange([r[0], r[1]])
        }
        ValueType::CFrame => {
            Value::CFrame(fa(v, 12)?.try_into().map_err(|_| anyhow!("12 numbers"))?)
        }
        ValueType::Rect => Value::Rect(fa(v, 4)?.try_into().map_err(|_| anyhow!("4 numbers"))?),
        ValueType::Vector2 => {
            let r = fa(v, 2)?;
            Value::Vector2([r[0], r[1]])
        }
        ValueType::Vector3 => Value::Vector3(to3(&fa(v, 3)?)),
        ValueType::Vector3int16 => {
            let a = arr(v, 3)?;
            Value::Vector3int16([int(&a[0])?, int(&a[1])?, int(&a[2])?])
        }
        ValueType::Faces => Value::Faces(names(v)?),
        ValueType::Axes => Value::Axes(names(v)?),
        ValueType::SecurityCapabilities => Value::SecurityCapabilities(names(v)?),
        ValueType::Font => {
            let t = table(v)?;
            only_keys(&t, &["family", "weight", "style"])?;
            let get = |k: &str| t.get(k).ok_or_else(|| anyhow!("missing {k}")).and_then(s);
            Value::Font {
                family: get("family")?,
                weight: get("weight")?,
                style: get("style")?,
            }
        }
        ValueType::PhysicalProperties => {
            let t = table(v)?;
            only_keys(
                &t,
                &[
                    "density",
                    "friction",
                    "elasticity",
                    "friction_weight",
                    "elasticity_weight",
                    "acoustic_absorption",
                ],
            )?;
            let get = |k: &str| t.get(k).ok_or_else(|| anyhow!("missing {k}")).and_then(f);
            Value::PhysicalProperties(Physical {
                density: get("density")?,
                friction: get("friction")?,
                elasticity: get("elasticity")?,
                friction_weight: get("friction_weight")?,
                elasticity_weight: get("elasticity_weight")?,
                acoustic_absorption: t.get("acoustic_absorption").map(f).transpose()?,
            })
        }
        ValueType::Ray => {
            let t = table(v)?;
            only_keys(&t, &["origin", "direction"])?;
            let get = |k: &str| {
                t.get(k)
                    .ok_or_else(|| anyhow!("missing {k}"))
                    .and_then(|x| fa(x, 3))
            };
            Value::Ray {
                origin: to3(&get("origin")?),
                direction: to3(&get("direction")?),
            }
        }
        ValueType::Ref => {
            let p = s(v)?;
            if !p.starts_with('@') {
                bail!("{p:?} doesn't start with @");
            }
            Value::Ref(RefTarget::Path(p))
        }
        ValueType::UDim => {
            let a = arr(v, 2)?;
            Value::UDim(f(&a[0])?, int(&a[1])?)
        }
        ValueType::UDim2 => {
            let a = arr(v, 4)?;
            Value::UDim2(f(&a[0])?, int(&a[1])?, f(&a[2])?, int(&a[3])?)
        }
    })
}

fn only_keys(t: &toml::Table, allowed: &[&str]) -> Result<()> {
    match t.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => bail!("unknown key {k:?}"),
        None => Ok(()),
    }
}

fn to3(v: &[f64]) -> [f64; 3] {
    [v[0], v[1], v[2]]
}

fn parse_hex(h: &str) -> Result<[f64; 3]> {
    let digits = h
        .strip_prefix('#')
        .filter(|d| d.len() == 6)
        .ok_or_else(|| anyhow!("{h:?} isn't #rrggbb"))?;
    let byte = |i: usize| {
        u8::from_str_radix(&digits[i..i + 2], 16).map_err(|_| anyhow!("{h:?} isn't #rrggbb"))
    };
    Ok([
        byte_to_color(byte(0)?),
        byte_to_color(byte(2)?),
        byte_to_color(byte(4)?),
    ])
}

fn json_float(x: f64) -> Json {
    match serde_json::Number::from_f64(x) {
        Some(n) => Json::Number(n),
        None if x.is_nan() => Json::String("nan".into()),
        None if x > 0.0 => Json::String("inf".into()),
        None => Json::String("-inf".into()),
    }
}

fn read_json(v: &Json, ty: ValueType) -> Result<Value> {
    let f = |v: &Json| -> Result<f64> {
        match v {
            Json::Number(n) => n.as_f64().ok_or_else(|| anyhow!("{n} isn't a number")),
            Json::String(s) if s == "inf" => Ok(f64::INFINITY),
            Json::String(s) if s == "-inf" => Ok(f64::NEG_INFINITY),
            Json::String(s) if s == "nan" => Ok(f64::NAN),
            other => bail!("{other} isn't a number"),
        }
    };
    let int = |v: &Json| -> Result<i64> {
        v.as_i64()
            .or_else(|| v.as_f64().filter(|x| x.fract() == 0.0).map(|x| x as i64))
            .ok_or_else(|| anyhow!("{v} isn't a whole number"))
    };
    let arr = |v: &Json, n: usize| -> Result<Vec<Json>> {
        let a = v.as_array().ok_or_else(|| anyhow!("{v} isn't a list"))?;
        if n != 0 && a.len() != n {
            bail!("{v} has {} items, not {n}", a.len());
        }
        Ok(a.clone())
    };
    let fa = |v: &Json, n: usize| -> Result<Vec<f64>> { arr(v, n)?.iter().map(f).collect() };
    let s = |v: &Json| {
        v.as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("{v} isn't a string"))
    };
    let names = |v: &Json| -> Result<Vec<String>> { arr(v, 0)?.iter().map(s).collect() };
    Ok(match ty {
        ValueType::Bool => Value::Bool(v.as_bool().ok_or_else(|| anyhow!("not a boolean"))?),
        ValueType::Int => Value::Int(int(v)?),
        ValueType::BrickColor => Value::BrickColor(int(v)?),
        ValueType::Float => Value::Float(f(v)?),
        ValueType::String => Value::String(s(v)?),
        ValueType::Content => Value::Content(s(v)?),
        ValueType::Enum => Value::Enum(s(v)?),
        ValueType::Color3 => Value::Color3(to3(&fa(v, 3)?)),
        ValueType::ColorSequence => Value::ColorSequence(
            arr(v, 0)?
                .iter()
                .map(|k| {
                    let k = arr(k, 2)?;
                    Ok((f(&k[0])?, to3(&fa(&k[1], 3)?)))
                })
                .collect::<Result<_>>()?,
        ),
        ValueType::NumberSequence => Value::NumberSequence(
            arr(v, 0)?
                .iter()
                .map(|k| {
                    let k = fa(k, 3)?;
                    Ok([k[0], k[1], k[2]])
                })
                .collect::<Result<_>>()?,
        ),
        ValueType::NumberRange => {
            let r = fa(v, 2)?;
            Value::NumberRange([r[0], r[1]])
        }
        ValueType::CFrame => {
            Value::CFrame(fa(v, 12)?.try_into().map_err(|_| anyhow!("12 numbers"))?)
        }
        ValueType::Rect => Value::Rect(fa(v, 4)?.try_into().map_err(|_| anyhow!("4 numbers"))?),
        ValueType::Vector2 => {
            let r = fa(v, 2)?;
            Value::Vector2([r[0], r[1]])
        }
        ValueType::Vector3 => Value::Vector3(to3(&fa(v, 3)?)),
        ValueType::Vector3int16 => {
            let a = arr(v, 3)?;
            Value::Vector3int16([int(&a[0])?, int(&a[1])?, int(&a[2])?])
        }
        ValueType::Faces => Value::Faces(names(v)?),
        ValueType::Axes => Value::Axes(names(v)?),
        ValueType::SecurityCapabilities => Value::SecurityCapabilities(names(v)?),
        ValueType::Font => Value::Font {
            family: s(&v["family"])?,
            weight: s(&v["weight"])?,
            style: s(&v["style"])?,
        },
        ValueType::PhysicalProperties => Value::PhysicalProperties(Physical {
            density: f(&v["density"])?,
            friction: f(&v["friction"])?,
            elasticity: f(&v["elasticity"])?,
            friction_weight: f(&v["friction_weight"])?,
            elasticity_weight: f(&v["elasticity_weight"])?,
            acoustic_absorption: v.get("acoustic_absorption").map(f).transpose()?,
        }),
        ValueType::Ray => Value::Ray {
            origin: to3(&fa(&v["origin"], 3)?),
            direction: to3(&fa(&v["direction"], 3)?),
        },
        ValueType::Ref => match v {
            Json::Number(_) => Value::Ref(RefTarget::Node(u32::try_from(int(v)?)?)),
            Json::Array(_) => Value::Ref(RefTarget::Outside(names(v)?)),
            other => bail!("a reference is a node id or a list of names, not {other}"),
        },
        ValueType::UDim => {
            let a = arr(v, 2)?;
            Value::UDim(f(&a[0])?, int(&a[1])?)
        }
        ValueType::UDim2 => {
            let a = arr(v, 4)?;
            Value::UDim2(f(&a[0])?, int(&a[1])?, f(&a[2])?, int(&a[3])?)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_refs(t: &RefTarget) -> Result<String> {
        Ok(format!("@{t:?}"))
    }

    /// Every type, with awkward numbers in it.
    pub(crate) fn samples() -> Vec<Value> {
        let odd = 0.1 + 0.2;
        vec![
            Value::Bool(true),
            Value::Int(-7),
            Value::Int(i64::MAX),
            Value::Float(-0.0),
            Value::Float(f64::INFINITY),
            Value::Float(odd),
            Value::String("a \"quoted\"\nline".into()),
            Value::Content("rbxassetid://123".into()),
            Value::Enum("Center".into()),
            Value::BrickColor(194),
            Value::Color3([byte_to_color(18), byte_to_color(21), byte_to_color(32)]),
            Value::Color3([0.5, 1.25, -0.0]),
            Value::ColorSequence(vec![(0.0, [1.0, 0.0, 0.0]), (1.0, [0.2, 0.4, odd])]),
            Value::NumberSequence(vec![[0.0, 1.0, 0.0], [1.0, 0.5, 0.25]]),
            Value::NumberRange([1.0, f64::MAX]),
            Value::CFrame([
                1.0, 2.0, 3.0, 1.0, 0.0, -0.0, 0.0, 1.0, 0.0, 5e-324, 0.0, 1.0,
            ]),
            Value::Faces(vec!["Top".into(), "Front".into()]),
            Value::Axes(vec!["X".into()]),
            Value::Font {
                family: "rbxasset://fonts/families/Arial.json".into(),
                weight: "Bold".into(),
                style: "Italic".into(),
            },
            Value::PhysicalProperties(Physical {
                density: 0.7,
                friction: 0.3,
                elasticity: 0.5,
                friction_weight: 1.0,
                elasticity_weight: 1.0,
                acoustic_absorption: Some(1.0),
            }),
            Value::Ray {
                origin: [0.0, 1.0, 2.0],
                direction: [0.0, -1.0, 0.0],
            },
            Value::Rect([0.0, 0.0, 10.0, odd]),
            Value::SecurityCapabilities(vec![]),
            Value::UDim(0.5, -3),
            Value::UDim2(0.0, 340, 1.0, -22),
            Value::Vector2([0.5, 1.0]),
            Value::Vector3([f64::NAN, 1e-7, -2.0]),
            Value::Vector3int16([-32768, 0, 32767]),
        ]
    }

    #[test]
    fn every_type_round_trips_through_toml() {
        for v in samples() {
            let text = v.to_toml(&no_refs).unwrap();
            let parsed: toml::Table = format!("v = {text}")
                .parse()
                .unwrap_or_else(|e| panic!("{text}: {e}"));
            let back = Value::from_toml(&parsed["v"], v.ty()).unwrap();
            assert!(v.same(&back), "{v:?} -> {text} -> {back:?}");
        }
    }

    #[test]
    fn every_type_round_trips_through_json() {
        for v in samples() {
            let json = serde_json::to_string(&v.to_json()).unwrap();
            let back = Value::from_json(&serde_json::from_str(&json).unwrap(), v.ty()).unwrap();
            assert!(v.same(&back), "{v:?} -> {json} -> {back:?}");
        }
    }

    #[test]
    fn studio_json_reads_as_studio_sent_it() {
        // Studio's JSONEncode writes `-0`, and 17 digits with an odd last one (decision 0014).
        let v: Json = serde_json::from_str("[-0, 0.30000000000000007]").unwrap();
        assert_eq!(
            Value::from_json(&v[0], ValueType::Float)
                .unwrap()
                .to_toml(&no_refs)
                .unwrap(),
            "-0.0"
        );
        assert!(
            Value::from_json(&v[1], ValueType::Float)
                .unwrap()
                .same(&Value::Float(0.1 + 0.2))
        );
    }

    #[test]
    fn colours_are_hex_only_when_exact() {
        for k in 0..=255u8 {
            let v = Value::Color3([byte_to_color(k), byte_to_color(255 - k), 0.0]);
            let text = v.to_toml(&no_refs).unwrap();
            assert_eq!(text, format!("\"#{:02x}{:02x}00\"", k, 255 - k));
        }
        assert_eq!(
            Value::Color3([18.0 / 255.0, 0.0, 0.0])
                .to_toml(&no_refs)
                .unwrap()
                .chars()
                .next(),
            Some('[')
        );
        assert!(
            Value::Color3([2.0, 0.0, 0.0])
                .to_toml(&no_refs)
                .unwrap()
                .starts_with('[')
        );
    }

    #[test]
    fn attributes_are_typed_unless_plain() {
        let plain = Value::Float(3.0).attribute_to_toml().unwrap();
        assert_eq!(plain, "3");
        let typed = Value::Vector3([1.0, 2.0, 3.0]).attribute_to_toml().unwrap();
        assert_eq!(typed, "{ type = \"Vector3\", value = [1, 2, 3] }");
        for v in samples()
            .into_iter()
            .filter(|v| !matches!(v, Value::Ref(_) | Value::Int(_)))
        {
            let text = v.attribute_to_toml().unwrap();
            let parsed: toml::Table = format!("v = {text}").parse().unwrap();
            assert!(
                v.same(&Value::attribute_from_toml(&parsed["v"]).unwrap()),
                "{text}"
            );
        }
    }

    #[test]
    fn wrong_shapes_name_the_expected_form() {
        let t: toml::Table = "v = [1, 2]".parse().unwrap();
        let err = format!(
            "{:#}",
            Value::from_toml(&t["v"], ValueType::UDim2).unwrap_err()
        );
        assert!(
            err.contains("[x scale, x offset, y scale, y offset]"),
            "{err}"
        );
    }
}
