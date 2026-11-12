//! `forge sync watch`: keeps Studio and the files in step both ways while it runs (decision 0013), and `forge sync
//! pick`, which settles a clash.
//!
//! The plugin says when a tree in Studio has been quiet after a change; the files are watched here, and a tree whose
//! folder has been quiet after a change is handled the same way. Either way the tree is reconciled: Studio's copy and
//! the files are each merged with the state they last agreed on (`crate::merge`). What only one side changed goes
//! to the other: files are written, and Studio gets a push (which asks for Apply as the plugin's setting says). A value
//! both changed differently is held as a clash until a side is picked; everything else keeps syncing.
//!
//! forge's own writes come back as changes too, and merge to nothing, since values are compared, not flags.
//!
//! A packages tree (decision 0021) goes one way: disk owns it, so nothing is merged or written into its folder. When
//! its files or its copy in Studio change, Studio gets a push of the files, which puts back any edit made in Studio.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use forge_core::{Cx, client, say};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::commands::{Session, StudioTree, print_drafts};
use crate::merge::{self, Clash, Flat};
use crate::settings::Settings;
use crate::tree::Node;
use crate::trees::Kind;
use crate::{diff, disk, layout, push as plan};

/// How often the loop looks for trees that have gone quiet.
const TICK: Duration = Duration::from_millis(50);

/// The clashes a running watch holds, where `forge sync status` and `forge sync pick` read them.
const CLASHES_FILE: &str = ".forge/clashes.json";
/// Picks waiting for the running watch to apply.
const PICKS_FILE: &str = ".forge/picks.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeldClash {
    /// The tree, by its Studio path.
    pub tree: String,
    #[serde(flatten)]
    pub clash: Clash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Pick {
    tree: String,
    path: String,
    what: String,
    /// "studio" or "disk": the side whose value wins.
    side: String,
}

/// One tree's state between reconciles.
struct Tree {
    name: String,
    folder: PathBuf,
    kind: Kind,
    /// The state Studio and the files last agreed on.
    base: Flat,
    /// The last copies seen on each side, which a pick takes its value from.
    studio: Flat,
    disk: Flat,
    clashes: Vec<Clash>,
}

pub async fn watch(cx: &Cx, settings: &Settings) -> Result<()> {
    let mut session = Session::open(cx, settings).await?;
    let studio = session.read(settings).await?;
    let schema = studio.selection.schema.clone();

    // Watch starts from Studio and the files agreeing, so there's a state to merge against.
    let mut trees = Vec::new();
    for (t, from) in session.game.trees.iter().zip(&studio.trees) {
        let folder = session.game.root.join(&t.disk);
        if let Some(why) = crate::commands::missing(t.kind, &folder) {
            bail!("{} has no files to watch: {why}", from.name);
        }
        let changes = crate::commands::changes(t.kind, &from.node, &folder, &schema)?;
        if !changes.is_empty() {
            bail!(
                "{} differs between Studio and its files ({} differences, `forge sync diff` lists them): run \
                 `forge sync pull` or `forge sync push` first, so watch starts from both sides agreeing",
                from.name,
                changes.len()
            );
        }
        let flat = merge::flatten(&layout::read(
            &layout::write(&from.node, &schema)?,
            &schema,
        )?)?;
        trees.push(Tree {
            name: from.name.clone(),
            folder,
            kind: t.kind,
            base: flat.clone(),
            studio: flat.clone(),
            disk: flat,
            clashes: Vec::new(),
        });
    }
    print_drafts(studio.trees.iter().map(|t| (t.name.as_str(), &t.drafts)));

    let mut start = studio.request.clone();
    start["quietMs"] = json!(settings.watch.studio_quiet_ms);
    start["pollMs"] = json!(settings.watch.script_poll_ms);
    session.ask("watch", start).await?;
    let mut events = client::connect(settings, cx.settings_file(), None, false)
        .await?
        .ok_or_else(|| anyhow!("forge serve stopped"))?;
    events.subscribe(session.game.place).await?;

    // The files: each change lands here as the tree it belongs to.
    let (tx, mut changed_files) = tokio::sync::mpsc::unbounded_channel::<usize>();
    let folders: Vec<PathBuf> = trees.iter().map(|t| t.folder.clone()).collect();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            for path in event.paths {
                if let Some(i) = folders.iter().position(|f| path.starts_with(f)) {
                    let _ = tx.send(i);
                }
            }
        }
    })
    .context("can't watch the synced folders")?;
    for t in &trees {
        notify::Watcher::watch(&mut watcher, &t.folder, notify::RecursiveMode::Recursive)
            .with_context(|| format!("can't watch {}", t.folder.display()))?;
    }

    let root = session.game.root.clone();
    save_clashes(&root, &trees)?;
    say!(
        "watching {} trees: Studio and the files stay in step until Ctrl+C",
        trees.len()
    );
    let mut disk_quiet: BTreeMap<usize, Instant> = BTreeMap::new();
    let mut studio_dirty: Vec<usize> = Vec::new();
    let mut tick = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            event = events.next_event() => {
                let event = event?;
                if event["kind"] == "changed"
                    && let Some(i) = event["tree"].as_u64().map(|i| i as usize).filter(|i| *i < trees.len())
                {
                    studio_dirty.push(i);
                }
            }
            Some(i) = changed_files.recv() => {
                disk_quiet.insert(i, Instant::now());
            }
            _ = tick.tick() => {
                let quiet = Duration::from_millis(settings.watch.disk_quiet_ms);
                let mut due: Vec<usize> = std::mem::take(&mut studio_dirty);
                due.extend(disk_quiet.iter().filter(|(_, at)| at.elapsed() >= quiet).map(|(i, _)| *i));
                disk_quiet.retain(|_, at| at.elapsed() < quiet);
                if apply_picks(&root, &mut trees)? {
                    due.extend(0..trees.len());
                }
                due.sort_unstable();
                due.dedup();
                for i in due {
                    if let Err(err) = reconcile(&mut session, settings, &mut trees, i).await {
                        say!("{}: {err:#}", trees[i].name);
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                let _ = session.ask("unwatch", json!({})).await;
                let _ = std::fs::remove_file(root.join(CLASHES_FILE));
                say!("stopped watching");
                return Ok(());
            }
        }
    }
}

/// Merges one tree's two sides against their agreed state, and sends each side what it's missing.
async fn reconcile(
    session: &mut Session,
    settings: &Settings,
    trees: &mut [Tree],
    i: usize,
) -> Result<()> {
    let studio = session.read_only(settings, i).await?;
    let schema = &studio.selection.schema;
    let from = &studio.trees[0];
    let t = &mut trees[i];
    if t.kind == Kind::Packages {
        let target = crate::commands::disk_tree(t.kind, &from.node.name, &t.folder, schema)?;
        return send(session, settings, i, from, &target, schema, true).await;
    }
    let in_studio = merge::flatten(&layout::read(&layout::write(&from.node, schema)?, schema)?)?;
    let on_disk = merge::flatten(
        &layout::read(&layout::read_dir(&t.folder)?, schema)
            .with_context(|| format!("can't read {}", t.folder.display()))?,
    )?;
    let m = merge::merge(&t.base, &in_studio, &on_disk);

    // The files: only what changed is written.
    let files = layout::write(&merge::unflatten(&m.to_disk)?, schema)?;
    let p = disk::plan(&t.folder, &files, true)?;
    if !p.write.is_empty() || !p.remove.is_empty() {
        disk::apply(&t.folder, &files, &p)?;
        say!(
            "{} -> disk: {} written, {} removed",
            t.name,
            p.write.len(),
            p.remove.len()
        );
    }

    // Studio: a push of what it's missing, as one undo step.
    let target = merge::unflatten(&m.to_studio)?;
    send(session, settings, i, from, &target, schema, false).await?;

    let t = &mut trees[i];
    t.base = m.base;
    t.studio = in_studio;
    t.disk = on_disk;
    if t.clashes != m.clashes {
        for c in &m.clashes {
            say!(
                "{}: clash on {} {}: Studio has {}, the files have {} (held until a side is picked)",
                t.name,
                show_path(&c.path),
                c.what,
                c.studio.as_deref().unwrap_or("nothing"),
                c.disk.as_deref().unwrap_or("nothing")
            );
        }
        t.clashes = m.clashes;
        let root = session.game.root.clone();
        save_clashes(&root, trees)?;
        let lines = clash_lines(&held(trees));
        session.ask("clashes", json!({ "lines": lines })).await?;
    }
    Ok(())
}

/// Pushes what makes tree `i` in Studio match `target`, as one undo step, if anything differs. `commit_text` is for
/// packages trees: their script text is committed, not left as drafts.
async fn send(
    session: &mut Session,
    settings: &Settings,
    i: usize,
    from: &StudioTree,
    target: &Node,
    schema: &crate::schema::Schema,
    commit_text: bool,
) -> Result<()> {
    let paths = layout::disk_paths(&from.node, schema);
    let push = plan::plan(&from.node, paths, target)?;
    if push.is_empty() {
        return Ok(());
    }
    let in_studio_read = layout::read(&layout::write(&from.node, schema)?, schema)?;
    let lines: Vec<String> = diff::diff(&in_studio_read, target)?
        .iter()
        .map(|c| format!("  {}", c.push_line()))
        .collect();
    let summary = format!(
        "{} changed, {} added, {} removed, {} moved",
        push.changed, push.added, push.removed, push.moved
    );
    let request = json!({
        "pushTrees": [{ "ops": push.ops, "commitText": commit_text }],
        "summary": summary,
        "changes": push.changed + push.added + push.removed + push.moved,
        "lines": lines,
        "drafts": [],
        "timeoutMs": settings.sync.confirm_timeout_ms,
    });
    let wait = settings.sync.confirm_timeout_ms + settings.server.request_timeout_ms;
    let reply = session.ask_only_for(i, "push", request, Some(wait)).await?;
    say!(
        "{} -> Studio: {summary} ({})",
        from.name,
        reply["result"].as_str().unwrap_or("?")
    );
    Ok(())
}

fn show_path(p: &str) -> &str {
    if p.is_empty() { "(top)" } else { p }
}

fn held(trees: &[Tree]) -> Vec<HeldClash> {
    trees
        .iter()
        .flat_map(|t| {
            t.clashes.iter().map(|c| HeldClash {
                tree: t.name.clone(),
                clash: c.clone(),
            })
        })
        .collect()
}

fn clash_lines(clashes: &[HeldClash]) -> Vec<String> {
    clashes
        .iter()
        .enumerate()
        .map(|(n, h)| {
            format!(
                "{}. {} {} {}: Studio {} / files {}",
                n + 1,
                h.tree,
                show_path(&h.clash.path),
                h.clash.what,
                h.clash.studio.as_deref().unwrap_or("(none)"),
                h.clash.disk.as_deref().unwrap_or("(none)")
            )
        })
        .collect()
}

fn save_clashes(root: &Path, trees: &[Tree]) -> Result<()> {
    let path = root.join(CLASHES_FILE);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        std::fs::write(
            dir.join(".gitignore"),
            "# forge's own files (backups, watch's clashes): never committed\n*\n",
        )?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(&held(trees))?)
        .with_context(|| format!("can't write {}", path.display()))
}

/// The clashes a running watch holds, if one runs here.
pub fn read_clashes(root: &Path) -> Vec<HeldClash> {
    std::fs::read_to_string(root.join(CLASHES_FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// A pick settles a clash by moving the agreed state to the losing side's value: the next merge then sees only the
/// winning side as changed, and sends its value across.
fn apply_picks(root: &Path, trees: &mut [Tree]) -> Result<bool> {
    let path = root.join(PICKS_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(false);
    };
    let _ = std::fs::remove_file(&path);
    let picks: Vec<Pick> = serde_json::from_str(&text).unwrap_or_default();
    for p in &picks {
        let Some(t) = trees.iter_mut().find(|t| t.name == p.tree) else {
            continue;
        };
        let loser = if p.side == "studio" {
            &t.disk
        } else {
            &t.studio
        };
        let Some(losing) = loser.get(&p.path).cloned() else {
            continue;
        };
        if p.what == "instance" {
            t.base.insert(p.path.clone(), losing);
            continue;
        }
        let entry = t
            .base
            .entry(p.path.clone())
            .or_insert_with(|| merge::Entry {
                fields: BTreeMap::new(),
                ..losing.clone()
            });
        if p.what == "name" {
            entry.name = losing.name.clone();
        } else {
            match losing.fields.get(&p.what) {
                Some(f) => {
                    entry.fields.insert(p.what.clone(), f.clone());
                }
                None => {
                    entry.fields.remove(&p.what);
                }
            }
        }
    }
    Ok(!picks.is_empty())
}

/// `forge sync pick <n> studio|disk`: the running watch applies it on its next tick.
pub fn pick(number: usize, side: &str) -> Result<()> {
    if side != "studio" && side != "disk" {
        bail!("pick a side: studio or disk");
    }
    let cwd = std::env::current_dir()?;
    let path =
        forge_core::game::find(&cwd).ok_or_else(|| anyhow!("no forge.toml here or above"))?;
    let root = path.parent().unwrap_or(&cwd).to_path_buf();
    let clashes = read_clashes(&root);
    let Some(h) = number.checked_sub(1).and_then(|n| clashes.get(n)) else {
        bail!(
            "there's no clash {number}: {} held (`forge sync status` lists them, while watch runs)",
            clashes.len()
        );
    };
    let picks_path = root.join(PICKS_FILE);
    let mut picks: Vec<Pick> = std::fs::read_to_string(&picks_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    picks.push(Pick {
        tree: h.tree.clone(),
        path: h.clash.path.clone(),
        what: h.clash.what.clone(),
        side: side.to_string(),
    });
    std::fs::write(&picks_path, serde_json::to_string_pretty(&picks)?)?;
    say!(
        "picked {side} for {} {} {}: the running watch applies it now",
        h.tree,
        show_path(&h.clash.path),
        h.clash.what
    );
    Ok(())
}

/// Lines for `forge sync status`: the clashes a running watch holds.
pub fn status_lines(root: &Path) -> Vec<String> {
    clash_lines(&read_clashes(root))
}
