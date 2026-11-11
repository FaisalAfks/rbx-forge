//! Which properties each class has, and their types. The disk form doesn't name types (a `[0, 340, 0, 66]` could be
//! a UDim2 or a Rect), so reading a property file needs this. It's plain data: the plugin can send it, and
//! `forge build` can keep one on disk without Studio (decision 0017).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::value::ValueType;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    pub classes: BTreeMap<String, Class>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Class {
    #[serde(default)]
    pub superclass: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, ValueType>,
}

impl Schema {
    pub fn knows(&self, class: &str) -> bool {
        self.classes.contains_key(class)
    }

    /// The type of `class.property`, looking through superclasses.
    pub fn property(&self, class: &str, property: &str) -> Option<ValueType> {
        let mut current = Some(class);
        // A bound on the walk, so a superclass loop in a bad schema can't hang the reader.
        for _ in 0..64 {
            let c = self.classes.get(current?)?;
            if let Some(ty) = c.properties.get(property) {
                return Some(*ty);
            }
            current = c.superclass.as_deref();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_through_superclasses() {
        let mut s = Schema::default();
        s.classes.insert(
            "GuiObject".into(),
            Class {
                superclass: None,
                properties: [("Size".into(), ValueType::UDim2)].into(),
            },
        );
        s.classes.insert(
            "Frame".into(),
            Class {
                superclass: Some("GuiObject".into()),
                properties: BTreeMap::new(),
            },
        );
        assert_eq!(s.property("Frame", "Size"), Some(ValueType::UDim2));
        assert_eq!(s.property("Frame", "Nope"), None);
        assert_eq!(s.property("Nope", "Size"), None);
    }

    #[test]
    fn a_superclass_loop_ends() {
        let mut s = Schema::default();
        s.classes.insert(
            "A".into(),
            Class {
                superclass: Some("A".into()),
                properties: BTreeMap::new(),
            },
        );
        assert_eq!(s.property("A", "X"), None);
    }
}
