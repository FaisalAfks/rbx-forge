//! Layout C on trees made for the tests (never game data): every value type, scripts, models, references,
//! attributes, tags, repeated and awkward names. A tree written and read back must be the same tree, and writing it
//! again must give the same bytes.

use forge_sync::layout::{self, Files};
use forge_sync::schema::{Class, Schema};
use forge_sync::tree::Node;
use forge_sync::value::{Physical, RefTarget, Value, ValueType as T};

fn schema() -> Schema {
    let class = |sup: Option<&str>, props: &[(&str, T)]| Class {
        superclass: sup.map(str::to_string),
        properties: props.iter().map(|(k, t)| (k.to_string(), *t)).collect(),
    };
    let mut s = Schema::default();
    let mut add = |n: &str, c: Class| {
        s.classes.insert(n.to_string(), c);
    };
    add(
        "Instance",
        class(None, &[("Capabilities", T::SecurityCapabilities)]),
    );
    add("Folder", class(Some("Instance"), &[]));
    add(
        "Model",
        class(
            Some("Instance"),
            &[("PrimaryPart", T::Ref), ("WorldPivotData", T::CFrame)],
        ),
    );
    add(
        "ScreenGui",
        class(
            Some("Instance"),
            &[("DisplayOrder", T::Int), ("Enabled", T::Bool)],
        ),
    );
    add(
        "GuiObject",
        class(
            Some("Instance"),
            &[
                ("Size", T::UDim2),
                ("Position", T::UDim2),
                ("AnchorPoint", T::Vector2),
                ("BackgroundColor3", T::Color3),
                ("BackgroundTransparency", T::Float),
                ("LayoutOrder", T::Int),
                ("Visible", T::Bool),
            ],
        ),
    );
    add("Frame", class(Some("GuiObject"), &[]));
    add(
        "TextLabel",
        class(
            Some("GuiObject"),
            &[
                ("Text", T::String),
                ("FontFace", T::Font),
                ("TextXAlignment", T::Enum),
            ],
        ),
    );
    add(
        "TextButton",
        class(Some("GuiObject"), &[("Text", T::String)]),
    );
    add(
        "ImageLabel",
        class(
            Some("GuiObject"),
            &[("Image", T::Content), ("SliceCenter", T::Rect)],
        ),
    );
    add(
        "UICorner",
        class(Some("Instance"), &[("CornerRadius", T::UDim)]),
    );
    add(
        "BasePart",
        class(
            Some("Instance"),
            &[
                ("CFrame", T::CFrame),
                ("Size", T::Vector3),
                ("Color", T::Color3),
                ("Anchored", T::Bool),
                ("BrickColor", T::BrickColor),
                ("CustomPhysicalProperties", T::PhysicalProperties),
                ("Material", T::Enum),
            ],
        ),
    );
    add("Part", class(Some("BasePart"), &[]));
    add("MeshPart", class(Some("BasePart"), &[]));
    add(
        "Beam",
        class(
            Some("Instance"),
            &[
                ("Color", T::ColorSequence),
                ("Transparency", T::NumberSequence),
                ("Attachment0", T::Ref),
            ],
        ),
    );
    add(
        "ParticleEmitter",
        class(Some("Instance"), &[("Lifetime", T::NumberRange)]),
    );
    add("Handles", class(Some("Instance"), &[("Faces", T::Faces)]));
    add("ArcHandles", class(Some("Instance"), &[("Axes", T::Axes)]));
    add("RayValue", class(Some("Instance"), &[("Value", T::Ray)]));
    add("ObjectValue", class(Some("Instance"), &[("Value", T::Ref)]));
    add(
        "TerrainRegion",
        class(Some("Instance"), &[("ExtentsMax", T::Vector3int16)]),
    );
    add("LuaSourceContainer", class(Some("Instance"), &[]));
    add("ModuleScript", class(Some("LuaSourceContainer"), &[]));
    add(
        "Script",
        class(Some("LuaSourceContainer"), &[("Disabled", T::Bool)]),
    );
    add("LocalScript", class(Some("Script"), &[]));
    s
}

struct Ids(u32);

impl Ids {
    fn node(
        &mut self,
        class: &str,
        name: &str,
        props: Vec<(&str, Value)>,
        children: Vec<Node>,
    ) -> Node {
        self.0 += 1;
        Node {
            id: self.0,
            class: class.into(),
            name: name.into(),
            properties: props.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            children,
            ..Default::default()
        }
    }
}

fn c8(r: u8, g: u8, b: u8) -> [f64; 3] {
    [r, g, b].map(|k| (k as f64 / 255.0) as f32 as f64)
}

/// A tree touching every rule in docs/sync/format.md.
fn sample() -> Node {
    let mut ids = Ids(0);
    let mut label = ids.node(
        "TextLabel",
        "Title",
        vec![
            ("Text", Value::String("Hello \"world\"".into())),
            (
                "FontFace",
                Value::Font {
                    family: "rbxasset://fonts/families/Arial.json".into(),
                    weight: "Bold".into(),
                    style: "Normal".into(),
                },
            ),
            ("TextXAlignment", Value::Enum("Left".into())),
            ("BackgroundColor3", Value::Color3(c8(18, 21, 32))),
            ("BackgroundTransparency", Value::Float(0.1 + 0.2)),
        ],
        vec![],
    );
    label.tags = vec!["b".into(), "a".into()];
    label.attributes.insert("Speed".into(), Value::Float(2.5));
    label
        .attributes
        .insert("Tint".into(), Value::Color3([0.5, 0.25, 1.5]));
    label
        .attributes
        .insert("Spot".into(), Value::Vector3([1.0, -0.0, 3.0]));
    // A child named like a property of its parent's class (Text), and one named like forge's own keys.
    let text_child = ids.node(
        "TextLabel",
        "Text",
        vec![("Text", Value::String("inner".into()))],
        vec![],
    );
    let class_child = ids.node("Frame", "class", vec![], vec![]);
    let button = ids.node(
        "TextButton",
        "Play",
        vec![("Text", Value::String("Play".into()))],
        vec![text_child, class_child],
    );
    let corner = ids.node(
        "UICorner",
        "UICorner",
        vec![("CornerRadius", Value::UDim(0.0, 8))],
        vec![],
    );
    let image = ids.node(
        "ImageLabel",
        "Icon",
        vec![
            ("Image", Value::Content("rbxassetid://1".into())),
            ("SliceCenter", Value::Rect([1.0, 2.0, 3.0, 4.0])),
        ],
        vec![],
    );
    let bar = ids.node(
        "Frame",
        "Bar",
        vec![
            ("Size", Value::UDim2(1.0, 0, 0.0, 66)),
            ("Position", Value::UDim2(0.5, 0, 1.0, -22)),
            ("AnchorPoint", Value::Vector2([0.5, 1.0])),
            ("Visible", Value::Bool(false)),
        ],
        vec![label, button, corner, image],
    );
    let mut module = ids.node("ModuleScript", "Config", vec![], vec![]);
    module.source = Some("\u{feff}return {\r\n\tx = 1,\r\n}".into()); // BOM, CRLF, no final newline
    let mut script = ids.node(
        "Script",
        "Main.luau",
        vec![("Disabled", Value::Bool(true))],
        vec![],
    );
    script.source = Some(String::new());
    let mut script_with_kids = ids.node("Script", "Runner", vec![], vec![module]);
    script_with_kids.source = Some("print('run')\n".into());
    let mut mesh = ids.node(
        "MeshPart",
        "Rock",
        vec![("Color", Value::Color3(c8(120, 90, 60)))],
        vec![],
    );
    mesh.model = Some(vec![0x3c, 0x72, 0x6f, 0x62, 0, 1, 2, 255]);
    let strip = |ids: &mut Ids, x: f64| {
        let mut cf = [0.0; 12];
        cf[0] = x;
        cf[3] = 1.0;
        cf[7] = 1.0;
        cf[11] = 1.0;
        ids.node(
            "Part",
            "Strip",
            vec![
                ("CFrame", Value::CFrame(cf)),
                ("Anchored", Value::Bool(true)),
            ],
            vec![],
        )
    };
    let strip_a = strip(&mut ids, 35.0);
    let strip_b = strip(&mut ids, -35.0);
    let strip_target = strip_a.id;
    let physics = ids.node(
        "Part",
        "Heavy",
        vec![
            (
                "CustomPhysicalProperties",
                Value::PhysicalProperties(Physical {
                    density: 2.0,
                    friction: 0.3,
                    elasticity: 0.5,
                    friction_weight: 1.0,
                    elasticity_weight: 1.0,
                    acoustic_absorption: None,
                }),
            ),
            ("Size", Value::Vector3([f64::MAX, 5e-324, f64::INFINITY])),
            ("BrickColor", Value::BrickColor(194)),
            ("Material", Value::Enum("Plastic".into())),
        ],
        vec![],
    );
    let beam = ids.node(
        "Beam",
        "Beam",
        vec![
            (
                "Color",
                Value::ColorSequence(vec![(0.0, c8(255, 0, 0)), (1.0, [0.2, 0.4, 0.6])]),
            ),
            (
                "Transparency",
                Value::NumberSequence(vec![[0.0, 0.0, 0.0], [1.0, 1.0, 0.5]]),
            ),
        ],
        vec![],
    );
    let odd = vec![
        ids.node("Folder", "CON", vec![], vec![]),
        ids.node("Folder", "a/b:c?", vec![], vec![]),
        ids.node("Folder", "", vec![], vec![]),
        ids.node("Folder", "trailing.", vec![], vec![]),
        ids.node("Folder", "_", vec![], vec![]),
        ids.node("Folder", &"long".repeat(60), vec![], vec![]),
        ids.node("Folder", "Case", vec![], vec![]),
        ids.node("Folder", "case", vec![], vec![]),
        ids.node(
            "Handles",
            "H",
            vec![("Faces", Value::Faces(vec!["Top".into(), "Left".into()]))],
            vec![],
        ),
        ids.node(
            "ArcHandles",
            "A",
            vec![("Axes", Value::Axes(vec!["Y".into()]))],
            vec![],
        ),
        ids.node(
            "RayValue",
            "R",
            vec![(
                "Value",
                Value::Ray {
                    origin: [0.0, 1.0, 2.0],
                    direction: [0.0, -1.0, 0.0],
                },
            )],
            vec![],
        ),
        ids.node(
            "ParticleEmitter",
            "P",
            vec![("Lifetime", Value::NumberRange([1.0, 2.5]))],
            vec![],
        ),
        ids.node(
            "TerrainRegion",
            "T",
            vec![("ExtentsMax", Value::Vector3int16([1, -2, 32767]))],
            vec![],
        ),
        ids.node(
            "Folder",
            "Caps",
            vec![(
                "Capabilities",
                Value::SecurityCapabilities(vec!["RunServerScript".into()]),
            )],
            vec![],
        ),
    ];
    let mut world = ids.node(
        "Model",
        "World",
        vec![],
        vec![strip_a, strip_b, physics, beam, mesh],
    );
    world.properties.insert(
        "PrimaryPart".into(),
        Value::Ref(RefTarget::Node(strip_target)),
    );
    let pointers = vec![
        ids.node(
            "ObjectValue",
            "ToRoot",
            vec![("Value", Value::Ref(RefTarget::Node(1000)))],
            vec![],
        ),
        ids.node(
            "ObjectValue",
            "ToOutside",
            vec![(
                "Value",
                Value::Ref(RefTarget::Outside(vec!["Workspace".into(), "a/b%".into()])),
            )],
            vec![],
        ),
    ];
    let mut children = vec![bar, script, script_with_kids, world];
    children.extend(odd);
    children.extend(pointers);
    let mut root = Node {
        id: 1000,
        class: "ScreenGui".into(),
        name: "PlayerHud".into(),
        children,
        ..Default::default()
    };
    root.properties
        .insert("DisplayOrder".into(), Value::Int(10));
    root.source = None;
    root
}

/// Instances compared field by field, siblings matched by class and name (then content), references by the files.
fn assert_same(a: &Node, b: &Node, path: &str) {
    assert_eq!(a.class, b.class, "{path}: class");
    assert_eq!(a.name, b.name, "{path}: name");
    let (mut ta, mut tb) = (a.tags.clone(), b.tags.clone());
    ta.sort();
    tb.sort();
    assert_eq!(ta, tb, "{path}: tags");
    assert_eq!(a.source, b.source, "{path}: source");
    assert_eq!(a.model, b.model, "{path}: model");
    for (map_a, map_b, what) in [
        (&a.properties, &b.properties, "property"),
        (&a.attributes, &b.attributes, "attribute"),
    ] {
        assert_eq!(
            map_a.keys().collect::<Vec<_>>(),
            map_b.keys().collect::<Vec<_>>(),
            "{path}: {what} names"
        );
        for (k, va) in map_a {
            let vb = &map_b[k];
            match (va, vb) {
                (Value::Ref(_), Value::Ref(_)) => {} // checked through the written files
                _ => assert!(va.same(vb), "{path}: {what} {k}: {va:?} vs {vb:?}"),
            }
        }
    }
    assert_eq!(a.children.len(), b.children.len(), "{path}: child count");
    let mut used = vec![false; b.children.len()];
    for ca in &a.children {
        let i = (0..b.children.len())
            .find(|&i| {
                !used[i]
                    && b.children[i].class == ca.class
                    && b.children[i].name == ca.name
                    && same_values(ca, &b.children[i])
            })
            .unwrap_or_else(|| panic!("{path}/{}: no match", ca.name));
        used[i] = true;
        assert_same(ca, &b.children[i], &format!("{path}/{}", ca.name));
    }
}

fn same_values(a: &Node, b: &Node) -> bool {
    a.properties.len() == b.properties.len()
        && a.properties.iter().all(|(k, v)| {
            b.properties
                .get(k)
                .is_some_and(|w| matches!(v, Value::Ref(_)) || v.same(w))
        })
}

fn text(files: &Files, path: &str) -> String {
    String::from_utf8(
        files
            .get(path)
            .unwrap_or_else(|| panic!("no {path} in {:?}", files.keys().collect::<Vec<_>>()))
            .clone(),
    )
    .unwrap()
}

#[test]
fn a_tree_reads_back_as_itself_and_writes_the_same_bytes() {
    let s = schema();
    let tree = sample();
    let files = layout::write(&tree, &s).unwrap();
    let back = layout::read(&files, &s).unwrap_or_else(|e| panic!("{e:#}"));
    assert_same(&tree, &back, "root");
    let again = layout::write(&back, &s).unwrap();
    assert_eq!(files, again, "writing what was read gives other bytes");
}

#[test]
fn sibling_order_doesnt_change_the_files() {
    let s = schema();
    let tree = sample();
    let mut shuffled = tree.clone();
    shuffled.children.reverse();
    for c in shuffled.children.iter_mut() {
        c.children.reverse();
        for g in c.children.iter_mut() {
            g.tags.reverse();
        }
    }
    assert_eq!(
        layout::write(&tree, &s).unwrap(),
        layout::write(&shuffled, &s).unwrap()
    );
}

#[test]
fn old_files_read_the_same_after_studio_adds_a_property() {
    // `Caps` is a leaf child of the ScreenGui. If a later Studio gives ScreenGui a property called Caps, the old
    // `[Caps]` section must still read as the child, not as that property.
    let s = schema();
    let files = layout::write(&sample(), &s).unwrap();
    let mut newer = s.clone();
    newer
        .classes
        .get_mut("ScreenGui")
        .unwrap()
        .properties
        .insert("Caps".into(), forge_sync::value::ValueType::Bool);
    let back = layout::read(&files, &newer).unwrap_or_else(|e| panic!("{e:#}"));
    assert!(
        back.children
            .iter()
            .any(|c| c.name == "Caps" && c.class == "Folder")
    );
}

#[test]
fn the_layout_looks_as_documented() {
    let files = layout::write(&sample(), &schema()).unwrap();
    let root = text(&files, "_.toml");
    assert!(
        root.starts_with("class = \"ScreenGui\"\nname = \"PlayerHud\"\nDisplayOrder = 10\n"),
        "{root}"
    );
    assert!(
        root.contains("\n[ToRoot]\nclass = \"ObjectValue\"\nValue = \"@\"\n"),
        "{root}"
    );
    assert!(root.contains("Value = \"@/Workspace/a%2Fb%25\""), "{root}");
    assert!(
        !root.contains("[CON]") && root.contains("[\"%43ON\"]"),
        "{root}"
    );
    let bar = text(&files, "Bar/_.toml");
    assert!(bar.contains("Size = [1, 0, 0, 66]\n"), "{bar}");
    assert!(bar.contains("\n[Title]\nclass = \"TextLabel\"\ntags = [\"a\", \"b\"]\nBackgroundColor3 = \"#121520\"\n"), "{bar}");
    assert!(
        bar.contains("BackgroundTransparency = 0.30000000000000004\n"),
        "{bar}"
    );
    assert!(bar.contains("FontFace = { family = \"rbxasset://fonts/families/Arial.json\", weight = \"Bold\", style = \"Normal\" }"), "{bar}");
    assert!(bar.contains("[Title.attributes]\nSpeed = 2.5\nSpot = { type = \"Vector3\", value = [1, -0.0, 3] }\nTint = { type = \"Color3\", value = [0.5, 0.25, 1.5] }\n"), "{bar}");
    // The TextButton has children, so it's a folder; its child named Text is escaped, since Text is a property.
    let play = text(&files, "Bar/Play/_.toml");
    assert!(
        play.contains("Text = \"Play\"\n")
            && play.contains("[\"%54ext\"]\nclass = \"TextLabel\"\nname = \"Text\"\n"),
        "{play}"
    );
    assert!(
        play.contains("[\"%63lass\"]\nclass = \"Frame\"\nname = \"class\"\n"),
        "{play}"
    );
    assert!(
        !files.contains_key("Config.luau") && files.contains_key("Runner/Config.luau"),
        "a leaf of Runner is in Runner's folder"
    );
}

#[test]
fn scripts_models_and_names_land_where_documented() {
    let files = layout::write(&sample(), &schema()).unwrap();
    assert_eq!(
        files["Runner/Config.luau"],
        "\u{feff}return {\r\n\tx = 1,\r\n}".as_bytes()
    );
    // A script with children keeps its text in its own folder, named the way Luau tools read it (decision 0020).
    assert_eq!(files["Runner/init.server.luau"], b"print('run')\n");
    assert_eq!(files["Main%2Eluau.server.luau"], b"");
    // Runner and its module have nothing to say beyond their file names: no section, no `_.toml`.
    assert!(!files.contains_key("Runner/_.toml"), "{:?}", files.keys());
    // Main has a property, so it keeps a section, with its class.
    let root = text(&files, "_.toml");
    assert!(
        root.contains(
            "[\"Main%2Eluau\"]\nclass = \"Script\"\nname = \"Main.luau\"\nDisabled = true\n"
        ),
        "{root}"
    );
    assert_eq!(
        files["World/Rock.rbxm"],
        vec![0x3c, 0x72, 0x6f, 0x62, 0, 1, 2, 255]
    );
    let world = text(&files, "World/_.toml");
    assert!(
        world.contains("PrimaryPart = \"@World/Strip\"")
            || world.contains("PrimaryPart = \"@World/Strip~2\""),
        "{world}"
    );
    let root = text(&files, "_.toml");
    for key in [
        "[\"%5F\"]",
        "[\"~0\"]",
        "[\"a%2Fb%3Ac%3F\"]",
        "[\"trailing%2E\"]",
        "[Case]",
        "[\"case~2\"]",
    ] {
        assert!(
            root.contains(key) || root.contains(&key.replace("Case]", "Case~2]")),
            "{key} in {root}"
        );
    }
    assert!(root.contains("~h"), "long names are cut with a hash");
}

#[test]
fn with_disk_names_kept_one_change_is_one_line() {
    let s = schema();
    let files = layout::write(&sample(), &s).unwrap();
    let mut back = layout::read(&files, &s).unwrap();
    // Recolour one of the two Strips: with the names read from disk kept, only its own line changes.
    let world = back
        .children
        .iter_mut()
        .find(|c| c.name == "World")
        .unwrap();
    let strip = world
        .children
        .iter_mut()
        .find(|c| c.name == "Strip")
        .unwrap();
    strip
        .properties
        .insert("Anchored".into(), Value::Bool(false));
    let after = layout::write(&back, &s).unwrap();
    let changed: Vec<&String> = files
        .keys()
        .filter(|k| files.get(*k) != after.get(*k))
        .collect();
    assert_eq!(changed, ["World/_.toml"]);
    let (old, new) = (text(&files, "World/_.toml"), text(&after, "World/_.toml"));
    let diff = old.lines().zip(new.lines()).filter(|(a, b)| a != b).count();
    assert_eq!((diff, old.lines().count()), (1, new.lines().count()));
}

#[test]
fn a_leaf_that_gets_a_child_becomes_a_folder() {
    let s = schema();
    let mut tree = sample();
    let files = layout::write(&tree, &s).unwrap();
    assert!(text(&files, "_.toml").contains("[Caps]"));
    let caps = tree.children.iter_mut().find(|c| c.name == "Caps").unwrap();
    caps.children.push(Node {
        id: 5000,
        class: "Folder".into(),
        name: "Inner".into(),
        ..Default::default()
    });
    let files = layout::write(&tree, &s).unwrap();
    assert!(!text(&files, "_.toml").contains("[Caps]"));
    assert!(text(&files, "Caps/_.toml").contains("[Inner]"));
}

#[test]
fn bad_files_give_errors_that_name_the_file() {
    let s = schema();
    let good = layout::write(&sample(), &s).unwrap();
    let err = |files: &Files| format!("{:#}", layout::read(files, &s).unwrap_err());

    let mut f = good.clone();
    f.insert(
        "Bar/_.toml".into(),
        b"class = \"Frame\"\nSize = [1, 2]\n".to_vec(),
    );
    let e = err(&f);
    assert!(
        e.contains("Bar/_.toml")
            && e.contains("Size")
            && e.contains("[x scale, x offset, y scale, y offset]"),
        "{e}"
    );

    let mut f = good.clone();
    f.insert(
        "Bar/_.toml".into(),
        b"class = \"Frame\"\nSise = 3\n".to_vec(),
    );
    assert!(err(&f).contains("\"Sise\" isn't a property of Frame"));

    let mut f = good.clone();
    f.insert("Bar/Stray.rbxm".into(), b"".to_vec());
    assert!(err(&f).contains("Bar/Stray.rbxm"));

    let mut f = good.clone();
    f.remove("World/_.toml");
    assert!(err(&f).contains("World/_.toml is missing"));

    let mut f = good.clone();
    let root = text(&f, "_.toml").replace("Value = \"@\"", "Value = \"@Nowhere\"");
    f.insert("_.toml".into(), root.into_bytes());
    assert!(err(&f).contains("points at nothing"));

    let mut f = good.clone();
    f.insert("Bar/notes.txt".into(), b"hi".to_vec());
    assert!(err(&f).contains("Bar/notes.txt"));
}

#[test]
fn a_dangling_reference_or_reserved_property_is_an_error_when_writing() {
    let s = schema();
    let mut tree = sample();
    tree.children.push(Node {
        id: 7000,
        class: "ObjectValue".into(),
        name: "Lost".into(),
        properties: [("Value".to_string(), Value::Ref(RefTarget::Node(424242)))].into(),
        ..Default::default()
    });
    assert!(layout::write(&tree, &s).is_err());
    let mut tree = sample();
    tree.properties
        .insert("name".into(), Value::String("x".into()));
    assert!(format!("{:#}", layout::write(&tree, &s).unwrap_err()).contains("forge's files use"));
}

#[test]
fn files_go_to_disk_and_back() {
    let s = schema();
    let files = layout::write(&sample(), &s).unwrap();
    let dir = std::env::temp_dir().join(format!("forge-layout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    layout::write_files(&dir, &files).unwrap();
    std::fs::write(dir.join(".DS_Store"), b"x").unwrap();
    let back = layout::read_dir(&dir).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(files, back);
}

#[test]
fn the_plugin_json_goes_to_the_same_files() {
    let s = schema();
    let tree = sample();
    let json = serde_json::to_string(&tree.to_json()).unwrap();
    let from_json = Node::from_json(&serde_json::from_str(&json).unwrap()).unwrap();
    assert_eq!(
        layout::write(&tree, &s).unwrap(),
        layout::write(&from_json, &s).unwrap()
    );
}

fn file(files: &mut Files, path: &str, text: &str) {
    files.insert(path.into(), text.as_bytes().to_vec());
}

#[test]
fn script_files_say_their_class() {
    let s = schema();
    let mut files = Files::new();
    file(
        &mut files,
        "_.toml",
        "class = \"Folder\"\nname = \"Code\"\n",
    );
    file(&mut files, "Shared.luau", "return {}");
    file(&mut files, "Boot.server.luau", "print(1)");
    file(&mut files, "Input.client.luau", "print(2)");
    file(&mut files, "Game/init.server.luau", "print(3)");
    file(&mut files, "Game/Rules.luau", "return 1");
    let tree = layout::read(&files, &s).unwrap();
    let child = |name: &str| tree.children.iter().find(|c| c.name == name).unwrap();
    assert_eq!(child("Shared").class, "ModuleScript");
    assert_eq!(child("Boot").class, "Script");
    assert_eq!(child("Input").class, "LocalScript");
    assert_eq!(child("Game").class, "Script");
    assert_eq!(child("Game").source.as_deref(), Some("print(3)"));
    assert_eq!(child("Game").children[0].class, "ModuleScript");
    // They write back as the same files.
    assert_eq!(layout::write(&tree, &s).unwrap(), files);
}

#[test]
fn scripts_named_like_forges_files_round_trip() {
    let s = schema();
    let mut ids = 0;
    let mut script = |class: &str, name: &str| {
        ids += 1;
        Node {
            id: ids,
            class: class.into(),
            name: name.into(),
            source: Some(format!("-- {name}")),
            ..Default::default()
        }
    };
    let root = Node {
        id: 100,
        class: "Folder".into(),
        name: "Code".into(),
        children: vec![
            script("ModuleScript", "init"),
            script("ModuleScript", "Foo.server"),
            script("Script", "Twin"),
            script("ModuleScript", "Twin"),
            script("LocalScript", "INIT"),
        ],
        ..Default::default()
    };
    let files = layout::write(&root, &s).unwrap();
    assert!(
        !files.keys().any(|k| k.to_lowercase().starts_with("init")),
        "{:?}",
        files.keys()
    );
    let back = layout::read(&files, &s).unwrap();
    let mut names: Vec<(String, String)> = back
        .children
        .iter()
        .map(|c| (c.class.clone(), c.name.clone()))
        .collect();
    names.sort();
    let want: Vec<(String, String)> = [
        ("LocalScript", "INIT"),
        ("ModuleScript", "Foo.server"),
        ("ModuleScript", "Twin"),
        ("ModuleScript", "init"),
        ("Script", "Twin"),
    ]
    .iter()
    .map(|(c, n)| (c.to_string(), n.to_string()))
    .collect();
    assert_eq!(names, want);
    assert_eq!(layout::write(&back, &s).unwrap(), files);
}

#[test]
fn files_from_forge_0_1_say_to_pull_again() {
    let s = schema();
    let err = |files: &Files| format!("{:#}", layout::read(files, &s).unwrap_err());

    // The tree's own text as `_.luau`.
    let mut f = Files::new();
    file(&mut f, "_.toml", "class = \"Script\"\nname = \"Main\"\n");
    file(&mut f, "_.luau", "print(1)");
    assert!(err(&f).contains("forge sync pull"), "{}", err(&f));

    // A leaf Script as `Name.luau`, its section saying Script.
    let mut f = Files::new();
    file(
        &mut f,
        "_.toml",
        "class = \"Folder\"\nname = \"Code\"\n\n[Boot]\nclass = \"Script\"\n",
    );
    file(&mut f, "Boot.luau", "print(1)");
    assert!(err(&f).contains("forge sync pull"), "{}", err(&f));

    // A script with children, its text beside its folder.
    let mut f = Files::new();
    file(&mut f, "_.toml", "class = \"Folder\"\nname = \"Code\"\n");
    file(
        &mut f,
        "Runner/_.toml",
        "class = \"ModuleScript\"\n\n[Part]\nclass = \"Folder\"\n",
    );
    file(&mut f, "Runner.luau", "return 1");
    assert!(err(&f).contains("forge sync pull"), "{}", err(&f));
}
