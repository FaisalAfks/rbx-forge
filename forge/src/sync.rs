//! `forge sync pull`: reads each tree named in the game's `forge.toml` from the Studio that has the place open, and
//! writes it to its folder in layout C (decisions 0006, 0008, 0011).
//!
//! Two requests go to the plugin through the server. The first asks which classes are under the trees, with Studio's
//! reflection data for each. forge then chooses which properties to read (`forge_sync::select`), so that choice lives
//! in Rust and in `tools/sync/properties.toml`. The second reads the trees. Every folder's changes are worked out and
//! checked before any file is written, so a refused pull changes nothing.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use forge_sync::{disk, layout, select, tree::Node, value::ValueType};
use serde_json::json;

use crate::client::{self, Link};
use crate::game;
use crate::protocol::Message;
use crate::settings::Settings;

pub async fn pull(settings: &Settings, settings_file: Option<&Path>, force: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("can't read the current folder")?;
    let path = game::find(&cwd).ok_or_else(|| {
        anyhow!(
            "no {} here or above: pull runs in a game's repo",
            game::FILE_NAME
        )
    })?;
    let game = game::load(&path)?;
    if game.trees.is_empty() {
        bail!("{} has no [[sync]] trees to pull", path.display());
    }
    let mut link = client::connect(settings, settings_file, crate::this_repo()?, true)
        .await?
        .expect("connect starts the server when asked to");
    if link.started {
        wait_for_studio(&mut link, settings, game.place).await?;
    }
    let trees: Vec<&Vec<String>> = game.trees.iter().map(|t| &t.studio).collect();

    let reply = link
        .ask_studio(
            game.place,
            json!({ "tool": "sync", "action": "classes", "trees": trees }),
        )
        .await?;
    let reflection: BTreeMap<String, Vec<select::Property>> =
        serde_json::from_value(reply["classes"].clone())
            .context("the plugin's class list can't be read")?;
    let selection = select::select(&reflection)?;

    // The plugin labels each value with its disk type, so it gets the types along with the names.
    let typed = |by_class: &BTreeMap<String, Vec<String>>| -> BTreeMap<String, BTreeMap<String, ValueType>> {
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
    };
    let request = json!({
        "tool": "sync",
        "action": "read",
        "trees": trees,
        "classes": reflection.keys().collect::<Vec<_>>(),
        "properties": typed(&selection.properties),
        "methods": typed(&selection.methods),
        "models": selection.models,
        "modelTimeoutMs": settings.sync.model_timeout_ms,
        "yieldEvery": settings.sync.yield_every,
    });
    let reply = link.ask_studio(game.place, request).await?;
    let read = reply["trees"]
        .as_array()
        .filter(|a| a.len() == game.trees.len())
        .ok_or_else(|| anyhow!("the plugin didn't send one tree per [[sync]] entry"))?;

    // Every folder's changes first, so a refusal (a folder forge didn't make, uncommitted changes) writes nothing.
    let mut work = Vec::new();
    for (tree, sent) in game.trees.iter().zip(read) {
        let name = tree.studio.join(".");
        let node = Node::from_json(&sent["tree"]).with_context(|| format!("{name} from Studio"))?;
        let files =
            layout::write(&node, &selection.schema).with_context(|| format!("{name} on disk"))?;
        read_back(&name, &node, &files, &selection.schema)?;
        let folder = game.root.join(&tree.disk);
        let plan =
            disk::plan(&folder, &files, force).with_context(|| format!("can't pull {name}"))?;
        let read_ms = sent["ms"].as_f64().unwrap_or(0.0);
        work.push((
            name,
            tree.disk.display().to_string(),
            folder,
            node.walk().len(),
            files,
            plan,
            read_ms,
        ));
    }
    for (name, disk_name, folder, count, files, plan, read_ms) in &work {
        let start = Instant::now();
        disk::apply(folder, files, plan)?;
        println!(
            "{name} -> {disk_name}: {count} instances; {} written, {} removed, {} unchanged (read {read_ms:.0} ms, \
             write {} ms)",
            plan.write.len(),
            plan.remove.len(),
            plan.unchanged,
            start.elapsed().as_millis()
        );
    }
    if !selection.unsynced.is_empty() {
        println!(
            "not synced, because plugins can't read and write them (they stay as they are in the place file):"
        );
        for p in &selection.unsynced {
            println!("  {p}");
        }
    }
    Ok(())
}

/// Reads the files back before any is written: every instance has to be there, and writing what was read has to give
/// the same bytes. A tree the files don't hold whole stops the pull instead of landing on disk (decision 0011: the
/// round trip on real data is the check that catches what reflection doesn't list).
fn read_back(
    name: &str,
    sent: &Node,
    files: &layout::Files,
    schema: &forge_sync::schema::Schema,
) -> Result<()> {
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
