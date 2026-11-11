//! An instance tree as forge holds it: what the plugin sends (as JSON) and what the layout writes and reads.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use serde_json::{Value as Json, json};

use crate::value::{RefTarget, Value, ValueType};

#[derive(Debug, Clone, Default)]
pub struct Node {
    /// Unique within one tree; references point at it. The plugin numbers instances per message.
    pub id: u32,
    pub class: String,
    pub name: String,
    /// Saved properties that differ from their default (decision 0006).
    pub properties: BTreeMap<String, Value>,
    pub attributes: BTreeMap<String, Value>,
    pub tags: Vec<String>,
    /// A script's text, exactly as the editor has it.
    pub source: Option<String>,
    /// A MeshPart alone, as a model file (decision 0011).
    pub model: Option<Vec<u8>>,
    pub children: Vec<Node>,
    /// The name this instance had on disk, when read from disk. The writer keeps it when it can, so a change to one
    /// of two same-named siblings doesn't turn into a rename.
    pub key: Option<String>,
}

impl Node {
    /// Reads the plugin's JSON. An empty Luau table arrives as `[]`, so empty maps may be lists.
    pub fn from_json(v: &Json) -> Result<Node> {
        let s = |k: &str| {
            v[k].as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow!("an instance needs `{k}`: {v}"))
        };
        let class = s("class")?;
        let name = s("name")?;
        let id = v["id"]
            .as_u64()
            .and_then(|i| u32::try_from(i).ok())
            .ok_or_else(|| anyhow!("`{class}` {name:?} needs an id"))?;
        let context = || format!("{class} {name:?}");
        let mut properties = BTreeMap::new();
        for (k, p) in entries(&v["properties"]).with_context(context)? {
            let ty: ValueType = serde_json::from_value(p["type"].clone())
                .map_err(|_| anyhow!("{k} has an unknown type {}", p["type"]))
                .with_context(context)?;
            properties.insert(
                k.clone(),
                Value::from_json(&p["value"], ty).with_context(|| format!("{} {k}", context()))?,
            );
        }
        let mut attributes = BTreeMap::new();
        for (k, a) in entries(&v["attributes"]).with_context(context)? {
            attributes.insert(
                k.clone(),
                Value::attribute_from_json(a)
                    .with_context(|| format!("{} attribute {k}", context()))?,
            );
        }
        let tags = match &v["tags"] {
            Json::Null => vec![],
            t => t
                .as_array()
                .ok_or_else(|| anyhow!("tags must be a list"))?
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| anyhow!("a tag must be a string"))
                })
                .collect::<Result<_>>()?,
        };
        let source = v["source"].as_str().map(str::to_string);
        let model = match v["model"].as_str() {
            Some(b64) => Some(
                base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .with_context(context)?,
            ),
            None => None,
        };
        let children = match &v["children"] {
            Json::Null => vec![],
            c => c
                .as_array()
                .ok_or_else(|| anyhow!("children must be a list"))?
                .iter()
                .map(Node::from_json)
                .collect::<Result<_>>()?,
        };
        Ok(Node {
            id,
            class,
            name,
            properties,
            attributes,
            tags,
            source,
            model,
            children,
            key: None,
        })
    }

    /// The plugin's JSON form, for sending a tree back to Studio.
    pub fn to_json(&self) -> Json {
        let typed = |m: &BTreeMap<String, Value>| -> serde_json::Map<String, Json> {
            m.iter()
                .map(|(k, v)| (k.clone(), json!({ "type": v.ty(), "value": v.to_json() })))
                .collect()
        };
        let mut o = json!({
            "id": self.id,
            "class": self.class,
            "name": self.name,
            "properties": typed(&self.properties),
            "attributes": typed(&self.attributes),
            "tags": self.tags,
            "children": self.children.iter().map(Node::to_json).collect::<Vec<_>>(),
        });
        if let Some(s) = &self.source {
            o["source"] = json!(s);
        }
        if let Some(m) = &self.model {
            o["model"] = json!(base64::engine::general_purpose::STANDARD.encode(m));
        }
        o
    }

    /// Every node, parent before children.
    pub fn walk(&self) -> Vec<&Node> {
        let mut out = vec![self];
        for c in &self.children {
            out.extend(c.walk());
        }
        out
    }

    /// Checks ids are unique and every reference inside the tree points at one of them.
    pub fn check_ids(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for n in self.walk() {
            if !seen.insert(n.id) {
                bail!("two instances share id {}", n.id);
            }
        }
        for n in self.walk() {
            for (k, v) in &n.properties {
                if let Value::Ref(RefTarget::Node(id)) = v
                    && !seen.contains(id)
                {
                    bail!(
                        "{} {:?}: {k} points at id {id}, which isn't in the tree",
                        n.class,
                        n.name
                    );
                }
            }
        }
        Ok(())
    }
}

fn entries(v: &Json) -> Result<Vec<(&String, &Json)>> {
    match v {
        Json::Null => Ok(vec![]),
        Json::Array(a) if a.is_empty() => Ok(vec![]),
        Json::Object(m) => Ok(m.iter().collect()),
        other => bail!("expected a map, got {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_studio_sends() {
        let v: Json = serde_json::from_str(
            r#"{ "id": 1, "class": "Frame", "name": "Root", "properties": { "Size": { "type": "UDim2", "value": [0, 340, 0, 66] } },
                 "attributes": [], "tags": [], "children": [
                   { "id": 2, "class": "ModuleScript", "name": "M", "properties": [], "source": "return 1\r\n",
                     "attributes": { "Speed": { "type": "number", "value": 2 } } } ] }"#,
        )
        .unwrap();
        let n = Node::from_json(&v).unwrap();
        assert!(n.properties["Size"].same(&Value::UDim2(0.0, 340, 0.0, 66)));
        assert_eq!(n.children[0].source.as_deref(), Some("return 1\r\n"));
        assert!(n.children[0].attributes["Speed"].same(&Value::Float(2.0)));
        let again = Node::from_json(&n.to_json()).unwrap();
        assert_eq!(again.to_json(), n.to_json());
    }

    #[test]
    fn duplicate_ids_and_dangling_refs_are_errors() {
        let mut n = Node {
            id: 1,
            class: "Folder".into(),
            name: "A".into(),
            ..Default::default()
        };
        n.children.push(Node {
            id: 1,
            class: "Folder".into(),
            name: "B".into(),
            ..Default::default()
        });
        assert!(n.check_ids().is_err());
        n.children[0].id = 2;
        n.properties
            .insert("PrimaryPart".into(), Value::Ref(RefTarget::Node(9)));
        assert!(format!("{:#}", n.check_ids().unwrap_err()).contains("id 9"));
    }
}
