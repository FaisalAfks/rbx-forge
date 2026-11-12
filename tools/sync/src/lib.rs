//! forge sync (docs/sync/): a game's trees between Studio and its repo. Roblox values and their written forms, the
//! schema that types them, the instance tree and layout C on disk, then the commands built on them. `SYNC` is the tool
//! as the binary, `forge mcp` and the plugin list it (decision 0019).

pub mod commands;
pub mod diff;
pub mod disk;
pub mod ignore;
pub mod layout;
pub mod merge;
pub mod names;
pub mod number;
pub mod packages;
pub mod push;
pub mod schema;
pub mod select;
pub mod settings;
pub mod sourcemap;
pub mod text;
pub mod tool;
pub mod tree;
pub mod trees;
pub mod value;
pub mod watch;

pub use tool::SYNC;
