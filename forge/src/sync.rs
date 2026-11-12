//! `forge sync`'s commands. Each one opens a session (the game's `forge.toml` and a link to the Studio that has its
//! place open) and reads the trees from Studio the same way (decisions 0006, 0008, 0011):
//!
//! Two requests go to the plugin through the server. The first asks which classes are under the trees, with Studio's
//! reflection data for each. forge then chooses which properties to read (`forge_sync::select`), so that choice lives
//! in Rust and in `tools/sync/properties.toml`. The second reads the trees.
//!
//! - `pull` writes them to disk. Every folder's changes are worked out and checked before any file is written, so a
//!   refused pull changes nothing.
//! - `status` says what's linked and lists scripts with an uncommitted draft.
//! - `diff` lists what differs between Studio and disk, both read as layout C has them.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use forge_sync::schema::Schema;
use forge_sync::{diff, disk, layout, select, tree::Node, value::ValueType};
use serde_json::{Value as Json, json};

use crate::client::{self, Link};
use crate::game::{self, Game};
use crate::protocol::Message;
use crate::settings::Settings;

/// A game and the link to the Studio that has its place open.
pub struct Session {
    pub game: Game,
    pub link: Link,
}

/// One tree as Studio has it now.
pub struct StudioTree {
    /// Its Studio path, dotted (`StarterGui.Hud`).
    pub name: String,
    pub node: Node,
    /// How long Studio took to read it.
    pub read_ms: f64,
    /// Scripts in it with an uncommitted draft, by full name.
    pub drafts: Vec<String>,
}

/// Every tree, and what forge chose to read.
pub struct StudioTrees {
    pub selection: select::Selection,
    pub trees: Vec<StudioTree>,
}

impl Session {
    /// The game around the current folder, and a link to its Studio (starting `forge serve` if needed).
    pub async fn open(settings: &Settings, settings_file: Option<&Path>) -> Result<Session> {
        let cwd = std::env::current_dir().context("can't read the current folder")?;
        let path = game::find(&cwd).ok_or_else(|| {
            anyhow!(
                "no {} here or above: forge sync runs in a game's repo",
                game::FILE_NAME
            )
        })?;
        let game = game::load(&path)?;
        if game.trees.is_empty() {
            bail!("{} has no [[sync]] trees", path.display());
        }
        let mut link = client::connect(settings, settings_file, crate::this_repo()?, true)
            .await?
            .expect("connect starts the server when asked to");
        if link.started {
            wait_for_studio(&mut link, settings, game.place).await?;
        }
        Ok(Session { game, link })
    }

    fn tree_paths(&self) -> Vec<&Vec<String>> {
        self.game.trees.iter().map(|t| &t.studio).collect()
    }

    /// Asks the plugin's sync side for something.
    pub async fn ask(&mut self, action: &str, mut request: Json) -> Result<Json> {
        request["tool"] = json!("sync");
        request["action"] = json!(action);
        request["trees"] = json!(self.tree_paths());
        self.link.ask_studio(self.game.place, request).await
    }

    /// Reads every tree from Studio.
    pub async fn read(&mut self, settings: &Settings) -> Result<StudioTrees> {
        let reply = self.ask("classes", json!({})).await?;
        let reflection: BTreeMap<String, Vec<select::Property>> =
            serde_json::from_value(reply["classes"].clone())
                .context("the plugin's class list can't be read")?;
        let selection = select::select(&reflection)?;
        let request = json!({
            "classes": reflection.keys().collect::<Vec<_>>(),
            "properties": typed(&selection, &selection.properties),
            "methods": typed(&selection, &selection.methods),
            "models": selection.models,
            "modelTimeoutMs": settings.sync.model_timeout_ms,
            "yieldEvery": settings.sync.yield_every,
        });
        let reply = self.ask("read", request).await?;
        let sent = reply["trees"]
            .as_array()
            .filter(|a| a.len() == self.game.trees.len())
            .ok_or_else(|| anyhow!("the plugin didn't send one tree per [[sync]] entry"))?;
        let mut trees = Vec::new();
        for (tree, sent) in self.game.trees.iter().zip(sent) {
            let name = tree.studio.join(".");
            let node =
                Node::from_json(&sent["tree"]).with_context(|| format!("{name} from Studio"))?;
            trees.push(StudioTree {
                name,
                node,
                read_ms: sent["ms"].as_f64().unwrap_or(0.0),
                drafts: strings(&sent["drafts"]),
            });
        }
        Ok(StudioTrees { selection, trees })
    }
}

/// The plugin labels each value with its disk type, so it gets the types along with the names.
fn typed(
    selection: &select::Selection,
    by_class: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, BTreeMap<String, ValueType>> {
    by_class
        .iter()
        .map(|(class, names)| {
            let types = names
                .iter()
                .filter_map(|n| Some((n.clone(), selection.schema.property(class, n)?)))
                .collect();
            (class.clone(), types)
        })
        .collect()
}

fn strings(v: &Json) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub async fn pull(settings: &Settings, settings_file: Option<&Path>, force: bool) -> Result<()> {
    let mut session = Session::open(settings, settings_file).await?;
    let studio = session.read(settings).await?;
    let schema = &studio.selection.schema;

    // Every folder's changes first, so a refusal (a folder forge didn't make, uncommitted changes) writes nothing.
    let mut work = Vec::new();
    for (tree, from) in session.game.trees.iter().zip(&studio.trees) {
        let files =
            layout::write(&from.node, schema).with_context(|| format!("{} on disk", from.name))?;
        read_back(&from.name, &from.node, &files, schema)?;
        let folder = session.game.root.join(&tree.disk);
        let plan = disk::plan(&folder, &files, force)
            .with_context(|| format!("can't pull {}", from.name))?;
        work.push((from, tree.disk.display().to_string(), folder, files, plan));
    }
    for (from, disk_name, folder, files, plan) in &work {
        let start = Instant::now();
        disk::apply(folder, files, plan)?;
        say!(
            "{} -> {disk_name}: {} instances; {} written, {} removed, {} unchanged (read {:.0} ms, write {} ms)",
            from.name,
            from.node.walk().len(),
            plan.write.len(),
            plan.remove.len(),
            plan.unchanged,
            from.read_ms,
            start.elapsed().as_millis()
        );
    }
    print_unsynced(&studio.selection);
    print_drafts(studio.trees.iter().map(|t| (t.name.as_str(), &t.drafts)));
    Ok(())
}

/// What differs between disk and Studio, tree by tree, going from disk to Studio.
pub async fn diff(settings: &Settings, settings_file: Option<&Path>) -> Result<()> {
    let mut session = Session::open(settings, settings_file).await?;
    let studio = session.read(settings).await?;
    let schema = &studio.selection.schema;
    let mut total = 0;
    for (tree, from) in session.game.trees.iter().zip(&studio.trees) {
        let folder = session.game.root.join(&tree.disk);
        if !folder.join("_.toml").is_file() {
            say!("{} -> {}: not pulled yet", from.name, tree.disk.display());
            continue;
        }
        let changes = changes(&from.node, &folder, schema)
            .with_context(|| format!("can't diff {}", from.name))?;
        total += changes.len();
        if changes.is_empty() {
            say!("{} -> {}: no differences", from.name, tree.disk.display());
            continue;
        }
        say!(
            "{} -> {}: {} differences, disk -> Studio",
            from.name,
            tree.disk.display(),
            changes.len()
        );
        for c in &changes {
            say!("  {c}");
        }
    }
    if total == 0 {
        say!("disk and Studio match");
    }
    print_unsynced(&studio.selection);
    print_drafts(studio.trees.iter().map(|t| (t.name.as_str(), &t.drafts)));
    Ok(())
}

/// The changes from a tree's folder to Studio's copy. Both sides are read as files, so disk names and values match.
pub fn changes(studio: &Node, folder: &Path, schema: &Schema) -> Result<Vec<diff::Change>> {
    let on_disk =
        layout::read(&layout::read_dir(folder)?, schema).context("reading the files on disk")?;
    let in_studio = layout::read(&layout::write(studio, schema)?, schema)?;
    diff::diff(&on_disk, &in_studio)
}

/// What's linked, and the scripts with an uncommitted draft.
pub async fn status(settings: &Settings, settings_file: Option<&Path>) -> Result<()> {
    let mut session = Session::open(settings, settings_file).await?;
    let reply = match session.ask("drafts", json!({})).await {
        Ok(reply) => reply,
        Err(err) => {
            say!("place {}: {err:#}", session.game.place);
            return Ok(());
        }
    };
    say!("place {}: linked to Studio", session.game.place);
    let lists: Vec<Vec<String>> = reply["drafts"]
        .as_array()
        .map(|a| a.iter().map(strings).collect())
        .unwrap_or_default();
    for (tree, drafts) in session.game.trees.iter().zip(&lists) {
        say!(
            "{} -> {}: {} scripts with a draft",
            tree.studio.join("."),
            tree.disk.display(),
            drafts.len()
        );
    }
    let names: Vec<String> = session
        .game
        .trees
        .iter()
        .map(|t| t.studio.join("."))
        .collect();
    print_drafts(names.iter().map(String::as_str).zip(&lists));
    Ok(())
}

pub fn print_unsynced(selection: &select::Selection) {
    if selection.unsynced.is_empty() {
        return;
    }
    say!(
        "not synced, because plugins can't read and write them (they stay as they are in the place file):"
    );
    for p in &selection.unsynced {
        say!("  {p}");
    }
}

/// The drafts warning (#20): the files hold the editor's text, which other people don't have until it's committed.
pub fn print_drafts<'a>(trees: impl Iterator<Item = (&'a str, &'a Vec<String>)>) {
    let all: Vec<&String> = trees.flat_map(|(_, d)| d).collect();
    if all.is_empty() {
        return;
    }
    say!(
        "{} scripts have an uncommitted draft. forge has the editor's text for them, but collaborators and the \
         published game get the last committed text until each draft is committed in Studio's Drafts window:",
        all.len()
    );
    for name in all {
        say!("  {name}");
    }
}

/// Reads the files back before any is written: every instance has to be there, and writing what was read has to give
/// the same bytes. A tree the files don't hold whole stops the pull instead of landing on disk (decision 0011: the
/// round trip on real data is the check that catches what reflection doesn't list).
fn read_back(name: &str, sent: &Node, files: &layout::Files, schema: &Schema) -> Result<()> {
    let read = layout::read(files, schema)
        .with_context(|| format!("{name}: forge can't read back the files it made"))?;
    let (sent_count, read_count) = (sent.walk().len(), read.walk().len());
    if sent_count != read_count {
        bail!("{name}: Studio sent {sent_count} instances, but the files hold {read_count}");
    }
    let again = layout::write(&read, schema)
        .with_context(|| format!("{name}: writing the read-back tree"))?;
    if let Some(path) = files
        .keys()
        .chain(again.keys())
        .find(|p| files.get(*p) != again.get(*p))
    {
        bail!(
            "{name}: {path} doesn't come out the same when its tree is read back and written again"
        );
    }
    Ok(())
}

/// A server this command just started has no Studio yet: the plugin finds it on its next try.
async fn wait_for_studio(link: &mut Link, settings: &Settings, place: u64) -> Result<()> {
    let wait = Duration::from_millis(settings.plugin.retry_ms + settings.plugin.open_timeout_ms);
    let deadline = Instant::now() + wait;
    loop {
        let linked = match link.request(&Message::Status).await? {
            Message::StatusReport(r) => r.studios.iter().any(|s| s.place == place),
            other => bail!("forge serve answered status with {other:?}"),
        };
        if linked || Instant::now() >= deadline {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
