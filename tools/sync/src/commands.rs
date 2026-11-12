//! `forge sync`'s commands. Each one opens a session (the game's `forge.toml` and a link to the Studio that has its
//! place open) and reads the trees from Studio the same way (decisions 0006, 0008, 0011):
//!
//! Two requests go to the plugin through the server. The first asks which classes are under the trees, with Studio's
//! reflection data for each. forge then chooses which properties to read (`crate::select`), so that choice lives
//! in Rust and in `tools/sync/properties.toml`. The second reads the trees.
//!
//! - `pull` writes them to disk. Every folder's changes are worked out and checked before any file is written, so a
//!   refused pull changes nothing.
//! - `status` says what's linked and lists scripts with an uncommitted draft.
//! - `diff` lists what differs between Studio and disk, both read as layout C has them.
//! - `push` works out what makes Studio match the files, keeps a backup of what it replaces, and sends the plan to
//!   the plugin, which applies it only when Apply is clicked in Studio (decision 0015).
//! - `check` builds a copy of each tree in Studio from its files, reads the copy, and compares it with the original
//!   value by value (decision 0011); the copy is gone afterwards.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use forge_core::client::Link;
use forge_core::{Cx, say};
use serde_json::{Value as Json, json};

use crate::schema::Schema;
use crate::settings::Settings;
use crate::trees::{self, Game};
use crate::{diff, disk, layout, push as plan, select, tree::Node, value::ValueType};

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
    /// The read request's fields, for reading the same way again (the check's copies).
    pub request: Json,
}

impl Session {
    /// The game around the current folder, and a link to its Studio (starting `forge serve` if needed).
    pub async fn open(cx: &Cx, settings: &Settings) -> Result<Session> {
        let core = cx.game("forge sync")?;
        let game = trees::read(&core)?;
        if game.trees.is_empty() {
            bail!(
                "{} has no [[sync]] trees",
                core.root.join(forge_core::game::FILE_NAME).display()
            );
        }
        let link = forge_core::session::link(cx, settings, &core).await?;
        Ok(Session { game, link })
    }

    fn tree_paths(&self) -> Vec<&Vec<String>> {
        self.game.trees.iter().map(|t| &t.studio).collect()
    }

    /// Asks the plugin's sync side for something.
    pub async fn ask(&mut self, action: &str, request: Json) -> Result<Json> {
        self.ask_for(action, request, None).await
    }

    /// The same, with its own time limit.
    pub async fn ask_for(
        &mut self,
        action: &str,
        mut request: Json,
        timeout_ms: Option<u64>,
    ) -> Result<Json> {
        request["tool"] = json!("sync");
        request["action"] = json!(action);
        request["trees"] = json!(self.tree_paths());
        self.link
            .ask_studio_for(self.game.place, request, timeout_ms)
            .await
    }

    /// Reads only tree `i` from Studio (watch reconciles one tree at a time).
    pub async fn read_only(&mut self, settings: &Settings, i: usize) -> Result<StudioTrees> {
        let one = vec![self.game.trees[i].clone()];
        let all = std::mem::replace(&mut self.game.trees, one);
        let read = self.read(settings).await;
        self.game.trees = all;
        read
    }

    /// Asks the plugin's sync side something about tree `i` only.
    pub async fn ask_only_for(
        &mut self,
        i: usize,
        action: &str,
        request: Json,
        timeout_ms: Option<u64>,
    ) -> Result<Json> {
        let one = vec![self.game.trees[i].clone()];
        let all = std::mem::replace(&mut self.game.trees, one);
        let reply = self.ask_for(action, request, timeout_ms).await;
        self.game.trees = all;
        reply
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
        let reply = self.ask("read", request.clone()).await?;
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
        Ok(StudioTrees {
            selection,
            trees,
            request,
        })
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

pub async fn pull(cx: &Cx, settings: &Settings, force: bool) -> Result<()> {
    let mut session = Session::open(cx, settings).await?;
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
pub async fn diff(cx: &Cx, settings: &Settings) -> Result<()> {
    let mut session = Session::open(cx, settings).await?;
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

/// Builds a copy of each tree (or only `only`, by Studio path or folder) from its files, reads the copy and compares
/// it with the original, value by value. Prints each difference; the copies are gone afterwards.
pub async fn check(cx: &Cx, settings: &Settings, only: Option<&str>) -> Result<()> {
    let mut session = Session::open(cx, settings).await?;
    if let Some(only) = only {
        session
            .game
            .trees
            .retain(|t| t.studio.join(".") == only || t.disk == Path::new(only));
        if session.game.trees.is_empty() {
            bail!(
                "forge.toml has no tree {only} (give its Studio path, like StarterGui.Hud, or its folder)"
            );
        }
    }
    let studio = session.read(settings).await?;
    let schema = &studio.selection.schema;
    let mut copies = Vec::new();
    let mut originals = Vec::new();
    for from in &studio.trees {
        let files = layout::write(&from.node, schema)?;
        let on_disk = layout::read(&files, schema)?;
        copies.push(plan::tree_json(&on_disk)?);
        originals.push(on_disk);
    }
    let mut request = studio.request.clone();
    request["copies"] = json!(copies);
    let reply = session.ask("check", request).await?;
    let back = reply["trees"]
        .as_array()
        .ok_or_else(|| anyhow!("the plugin sent no copies"))?;
    // Which trees were copied, by index; a tree the plugin couldn't copy is named with why.
    let kept: Vec<usize> = reply["kept"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|i| i.as_u64().map(|i| i as usize))
                .collect()
        })
        .unwrap_or_else(|| (0..back.len()).collect());
    if kept.len() != back.len() {
        bail!(
            "the plugin sent {} copies for {} trees",
            back.len(),
            kept.len()
        );
    }
    let mut total = 0;
    for (i, (from, original)) in studio.trees.iter().zip(&originals).enumerate() {
        let Some(at) = kept.iter().position(|k| *k == i) else {
            let why = reply["skipped"][i.to_string()]
                .as_str()
                .unwrap_or("no reason given");
            say!(
                "{}: not checked, the plugin can't build a copy of it ({why}); pull and push still sync its values",
                from.name
            );
            continue;
        };
        let copy =
            Node::from_json(&back[at]["tree"]).with_context(|| format!("{}'s copy", from.name))?;
        let copy = layout::read(&layout::write(&copy, schema)?, schema)?;
        let changes = diff::diff(original, &copy)?;
        total += changes.len();
        if changes.is_empty() {
            say!(
                "{}: {} instances, the copy matches value for value",
                from.name,
                original.walk().len()
            );
        } else {
            say!(
                "{}: {} instances, {} differences between the original and its copy (original -> copy):",
                from.name,
                original.walk().len(),
                changes.len()
            );
            for c in &changes {
                say!("  {c}");
            }
        }
    }
    print_unsynced(&studio.selection);
    if total > 0 {
        bail!("the round trip lost or changed {total} values");
    }
    Ok(())
}

/// Makes Studio match the files, once Apply is clicked in Studio. `from` reads the trees from another folder laid out
/// like the game's (a backup), instead of the game's own.
pub async fn push(cx: &Cx, settings: &Settings, from: Option<&Path>) -> Result<()> {
    let mut session = Session::open(cx, settings).await?;
    let studio = session.read(settings).await?;
    let schema = &studio.selection.schema;
    let base = match from {
        Some(dir) => {
            std::path::absolute(dir).with_context(|| format!("can't use {}", dir.display()))?
        }
        None => session.game.root.clone(),
    };

    let mut trees = Vec::new();
    let mut lines = Vec::new();
    let mut total = plan::Plan::default();
    let mut drafts = Vec::new();
    for (tree, from_studio) in session.game.trees.iter().zip(&studio.trees) {
        let folder = base.join(&tree.disk);
        if !folder.join("_.toml").is_file() {
            bail!(
                "{} has no files at {} to push: pull it first",
                from_studio.name,
                folder.display()
            );
        }
        let on_disk = layout::read(&layout::read_dir(&folder)?, schema)
            .with_context(|| format!("can't read {}", folder.display()))?;
        let paths = layout::disk_paths(&from_studio.node, schema);
        let p = plan::plan(&from_studio.node, paths, &on_disk)
            .with_context(|| format!("can't plan {}", from_studio.name))?;
        let in_studio = layout::read(&layout::write(&from_studio.node, schema)?, schema)?;
        let changes = diff::diff(&in_studio, &on_disk)?;
        if !changes.is_empty() {
            lines.push(format!("{} <- {}", from_studio.name, tree.disk.display()));
            lines.extend(changes.iter().map(|c| format!("  {}", c.push_line())));
        }
        drafts.extend(overwritten_drafts(from_studio, &p));
        total.changed += p.changed;
        total.added += p.added;
        total.removed += p.removed;
        total.moved += p.moved;
        trees.push(json!({ "ops": p.ops }));
        total.ops.extend(p.ops);
    }
    if total.is_empty() {
        say!("Studio already matches the files: nothing to push");
        return Ok(());
    }
    let summary = format!(
        "{} changed, {} added, {} removed, {} moved",
        total.changed, total.added, total.removed, total.moved
    );
    let backup = backup(&session.game, &studio)?;
    say!("push: {summary}");
    for l in &lines {
        say!("{l}");
    }
    if !drafts.is_empty() {
        say!("these scripts have an uncommitted draft, which the push writes over:");
        for d in &drafts {
            say!("  {d}");
        }
    }
    say!(
        "sent to Studio: forge's window waits for Apply, unless its Apply pushes setting says not to ask..."
    );
    let request = json!({
        "pushTrees": trees,
        "summary": summary,
        "changes": total.changed + total.added + total.removed + total.moved,
        "lines": lines,
        "drafts": drafts,
        "timeoutMs": settings.sync.confirm_timeout_ms,
    });
    let wait = settings.sync.confirm_timeout_ms + settings.server.request_timeout_ms;
    let reply = session.ask_for("push", request, Some(wait)).await?;
    match reply["result"].as_str() {
        Some("applied") => {
            say!("applied in Studio as one change: {summary}");
            say!(
                "what it replaced is in {}; `forge sync push --from {}` puts it back",
                backup.display(),
                backup.display()
            );
        }
        Some("cancelled") => say!("cancelled in Studio: nothing changed"),
        Some("timeout") => say!(
            "nobody clicked Apply within {} ms (`sync.confirm_timeout_ms`): nothing changed",
            settings.sync.confirm_timeout_ms
        ),
        _ => bail!("the plugin answered the push with {reply}"),
    }
    if reply["result"] != "applied" {
        // Nothing was replaced, so there's nothing to keep.
        let _ = std::fs::remove_dir_all(&backup);
    }
    Ok(())
}

/// Scripts the push writes that have an uncommitted draft: the draft is what the push replaces (#20).
fn overwritten_drafts(tree: &StudioTree, p: &plan::Plan) -> Vec<String> {
    p.scripts
        .iter()
        .map(|script| {
            if script.is_empty() {
                tree.name.clone()
            } else {
                format!("{}.{}", tree.name, script.replace('/', "."))
            }
        })
        .filter(|full| tree.drafts.contains(full))
        .collect()
}

/// Writes Studio's trees as they are now to `.forge/backup/<time>/`, laid out like the game, so a push can put them
/// back. `.forge/` gets a `.gitignore`, so backups never land in git.
fn backup(game: &Game, studio: &StudioTrees) -> Result<std::path::PathBuf> {
    let forge_dir = game.root.join(".forge");
    let dir = forge_dir.join("backup").join(timestamp());
    for (tree, from) in game.trees.iter().zip(&studio.trees) {
        let files = layout::write(&from.node, &studio.selection.schema)?;
        layout::write_files(&dir.join(&tree.disk), &files)
            .with_context(|| format!("can't write the backup in {}", dir.display()))?;
    }
    std::fs::write(
        forge_dir.join(".gitignore"),
        "# forge's own files (backups): never committed\n*\n",
    )
    .with_context(|| format!("can't write {}", forge_dir.display()))?;
    Ok(dir)
}

/// The time now, UTC, as `YYYYMMDD-HHMMSS`, so backup folders sort in order.
fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rest) = ((secs / 86400) as i64, secs % 86400);
    // Days since 1970 to a calendar date (Howard Hinnant's civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

/// What's linked, and the scripts with an uncommitted draft.
pub async fn status(cx: &Cx, settings: &Settings) -> Result<()> {
    let mut session = Session::open(cx, settings).await?;
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
    let clashes = crate::watch::status_lines(&session.game.root);
    if !clashes.is_empty() {
        say!(
            "{} clashes held by forge sync watch (forge sync pick <number> studio|disk):",
            clashes.len()
        );
        for c in clashes {
            say!("  {c}");
        }
    }
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
