//! Which properties pull reads, chosen from the running Studio's reflection data (decision 0006): saved, readable,
//! writable and not deprecated, plus the twins, method readers and model classes in `properties.toml`. A saved
//! property that none of those cover, and that isn't listed as left in the place file, is reported, never dropped
//! without a word.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

use crate::schema::{Class, Schema};
use crate::value::ValueType;

const CONFIG: &str = include_str!("../properties.toml");

/// Properties that are part of every instance in another way: the name is the instance's own, and the parent is its
/// place in the tree.
const STRUCTURE: [&str; 2] = ["Name", "Parent"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    types: BTreeMap<String, ValueType>,
    twins: BTreeMap<String, String>,
    methods: BTreeMap<String, ValueType>,
    models: Models,
    left_in_place: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Models {
    classes: Vec<String>,
}

/// One property as the plugin reports it from `ReflectionService:GetPropertiesOfClass`.
#[derive(Debug, Clone, Deserialize)]
pub struct Property {
    pub name: String,
    /// The class that declares it.
    pub owner: String,
    pub engine: String,
    pub serialized: bool,
    pub read: bool,
    pub write: bool,
    pub deprecated: bool,
}

/// What pull reads, per class.
#[derive(Debug, Default)]
pub struct Selection {
    /// Every class's properties as written on disk, with their types.
    pub schema: Schema,
    /// Properties the plugin reads by name.
    pub properties: BTreeMap<String, Vec<String>>,
    /// Properties the plugin reads through a method.
    pub methods: BTreeMap<String, Vec<String>>,
    /// Classes sent as a model file of the instance alone.
    pub models: Vec<String>,
    /// Saved properties pull can't read, as `Owner.Property (engine type)`, once each.
    pub unsynced: Vec<String>,
}

fn config() -> Result<Config> {
    toml::from_str(CONFIG).context("tools/sync/properties.toml is broken")
}

/// Chooses what to read for each class in `reflection`.
pub fn select(reflection: &BTreeMap<String, Vec<Property>>) -> Result<Selection> {
    let config = config()?;
    let mut out = Selection::default();
    let mut unsynced = std::collections::BTreeSet::new();
    for (class, props) in reflection {
        let is_model = config.models.classes.contains(class);
        let by_name: BTreeMap<&str, &Property> =
            props.iter().map(|p| (p.name.as_str(), p)).collect();
        let mut types = BTreeMap::new();
        let mut read = Vec::new();
        let mut methods = Vec::new();
        for p in props {
            if !p.serialized || p.deprecated || STRUCTURE.contains(&p.name.as_str()) {
                continue;
            }
            let key = format!("{}.{}", p.owner, p.name);
            let ty = config.types.get(&p.engine).copied();
            if let (true, true, Some(ty)) = (p.read, p.write, ty) {
                types.insert(p.name.clone(), ty);
                read.push(p.name.clone());
            } else if let Some(twin) = config.twins.get(&key) {
                let t = by_name.get(twin.as_str()).ok_or_else(|| {
                    anyhow!("{class}: the twin {twin} of {key} isn't in Studio's reflection data")
                })?;
                let ty = config.types.get(&t.engine).copied();
                match (t.read && t.write, ty) {
                    (true, Some(ty)) => {
                        types.insert(t.name.clone(), ty);
                        read.push(t.name.clone());
                    }
                    _ => {
                        unsynced
                            .insert(format!("{key} (its twin {twin} can't be read and written)"));
                    }
                }
            } else if let Some(ty) = config.methods.get(&key) {
                types.insert(p.name.clone(), *ty);
                methods.push(p.name.clone());
            } else if is_model || config.left_in_place.contains_key(&key) {
                // Carried by the model file, or left in the place file on purpose.
            } else {
                unsynced.insert(format!("{key} ({})", p.engine));
            }
        }
        read.sort();
        read.dedup();
        out.schema.classes.insert(
            class.clone(),
            Class {
                superclass: None,
                properties: types,
            },
        );
        if !read.is_empty() {
            out.properties.insert(class.clone(), read);
        }
        if !methods.is_empty() {
            out.methods.insert(class.clone(), methods);
        }
        if is_model {
            out.models.push(class.clone());
        }
    }
    out.unsynced = unsynced.into_iter().collect();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(
        name: &str,
        owner: &str,
        engine: &str,
        serialized: bool,
        read: bool,
        write: bool,
    ) -> Property {
        Property {
            name: name.into(),
            owner: owner.into(),
            engine: engine.into(),
            serialized,
            read,
            write,
            deprecated: false,
        }
    }

    fn part() -> Vec<Property> {
        vec![
            p("Name", "Instance", "string", true, true, true),
            p("Parent", "Instance", "RefType", false, true, true),
            p("Anchored", "BasePart", "bool", true, true, true),
            p("Color3uint8", "BasePart", "Color3uint8", true, false, false),
            p("Color", "BasePart", "Color3", false, true, true),
            p("BrickColor", "BasePart", "BrickColor", false, true, true),
            p("CFrame", "BasePart", "CoordinateFrame", true, true, true),
            p("AssemblyMass", "BasePart", "float", false, true, false),
            Property {
                deprecated: true,
                ..p("Velocity", "BasePart", "Vector3", true, true, true)
            },
        ]
    }

    #[test]
    fn saved_readable_properties_and_twins_are_read() {
        let s = select(&[("Part".to_string(), part())].into()).unwrap();
        assert_eq!(s.properties["Part"], ["Anchored", "CFrame", "Color"]);
        assert_eq!(s.schema.property("Part", "Color"), Some(ValueType::Color3));
        assert_eq!(s.schema.property("Part", "CFrame"), Some(ValueType::CFrame));
        // Not saved (BrickColor is a view of Color), computed, deprecated, or structure: none of them.
        for name in ["BrickColor", "AssemblyMass", "Velocity", "Name", "Parent"] {
            assert_eq!(s.schema.property("Part", name), None, "{name}");
        }
        assert!(s.unsynced.is_empty(), "{:?}", s.unsynced);
    }

    #[test]
    fn a_saved_property_nothing_covers_is_reported_once() {
        let mut props = part();
        props.push(p("RunContext", "BaseScript", "Enum", true, true, false));
        let s = select(
            &[
                ("Part".to_string(), props.clone()),
                ("Other".to_string(), props),
            ]
            .into(),
        )
        .unwrap();
        assert_eq!(s.unsynced, ["BaseScript.RunContext (Enum)"]);
    }

    #[test]
    fn method_values_models_and_known_leftovers() {
        let model = vec![
            p("ScaleFactor", "Model", "float", true, false, false),
            p("ModelMeshData", "Model", "SharedString", true, false, false),
        ];
        let mesh = vec![
            p("MeshContent", "MeshPart", "Content", true, true, false),
            p("Transparency", "BasePart", "float", true, true, true),
        ];
        let s =
            select(&[("Model".to_string(), model), ("MeshPart".to_string(), mesh)].into()).unwrap();
        assert_eq!(s.methods["Model"], ["ScaleFactor"]);
        assert_eq!(
            s.schema.property("Model", "ScaleFactor"),
            Some(ValueType::Float)
        );
        assert_eq!(s.models, ["MeshPart"]);
        assert_eq!(s.properties["MeshPart"], ["Transparency"]);
        assert!(s.unsynced.is_empty(), "{:?}", s.unsynced);
    }

    #[test]
    fn an_unknown_engine_type_is_reported() {
        let props = vec![p("Blob", "Thing", "NetAssetRef", true, true, true)];
        let s = select(&[("Thing".to_string(), props)].into()).unwrap();
        assert_eq!(s.unsynced, ["Thing.Blob (NetAssetRef)"]);
    }

    #[test]
    fn the_config_reads() {
        let c = config().unwrap();
        assert_eq!(c.types["CoordinateFrame"], ValueType::CFrame);
        assert_eq!(c.twins["BasePart.size"], "Size");
    }
}
