//! Writing the folder-wide records: apps with their entrances and targets, inputs, and
//! metadata.

use rusqlite::Connection;
use unfold_core::{App, EntranceTarget, Input};

use super::graph::Apply;
use crate::codec::position;
use crate::error::Result;
use crate::schema::meta_key;
use crate::search::{CandidateKind, rowid};
use crate::update::GraphMeta;

impl Apply<'_, '_> {
    /// Replaces every app, entrance, target, and their search rows with `apps`. An app's or
    /// entrance's row ID is its key's ID; an app's own `code` is its target at position 0.
    pub(crate) fn write_apps(&mut self, conn: &Connection, apps: &[App]) -> Result<()> {
        conn.prepare_cached(
            "DELETE FROM search WHERE rowid IN (SELECT id * 4 + ?1 FROM apps UNION ALL \
             SELECT id * 4 + ?2 FROM entrances)",
        )?
        .execute((CandidateKind::App.tag(), CandidateKind::Entrance.tag()))?;
        conn.execute_batch("DELETE FROM apps; DELETE FROM entrances; DELETE FROM entrance_targets;")?;
        let mut insert_app = conn.prepare_cached(
            "INSERT INTO apps (id, position, name, run, about) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut insert_entrance = conn.prepare_cached(
            "INSERT INTO entrances (id, app_id, position, name, on_text, about) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for (index, app) in apps.iter().enumerate() {
            let app_id = self.mirror.intern(&app.key, &mut self.st.insert_key)?;
            insert_app.execute((
                app_id,
                position("app position", index)?,
                &*app.name,
                app.run.as_deref(),
                app.about.as_deref(),
            ))?;
            self.write_targets(conn, app_id, app.code.as_slice())?;
            self.st.insert_search.execute((
                rowid(CandidateKind::App, app_id),
                &*app.name,
                app.run.as_deref().unwrap_or_default(),
                "",
                "app",
                app.key.as_str(),
            ))?;
            for (index, entrance) in app.entrances.iter().enumerate() {
                let id = self.mirror.intern(&entrance.key, &mut self.st.insert_key)?;
                insert_entrance.execute((
                    id,
                    app_id,
                    position("entrance position", index)?,
                    &*entrance.name,
                    entrance.on.as_deref(),
                    entrance.about.as_deref(),
                ))?;
                self.write_targets(conn, id, &entrance.code)?;
                let qualified = match &entrance.on {
                    Some(on) => format!("{} · {on}", app.name),
                    None => app.name.to_string(),
                };
                self.st.insert_search.execute((
                    rowid(CandidateKind::Entrance, id),
                    &*entrance.name,
                    qualified,
                    "",
                    "entrance",
                    entrance.key.as_str(),
                ))?;
            }
            self.stats.apps_written += 1;
            self.stats.entrances_written += app.entrances.len() as u64;
        }
        Ok(())
    }

    /// Writes the `code` targets of the app or entrance with row ID `owner`, in order.
    fn write_targets(&mut self, conn: &Connection, owner: i64, targets: &[EntranceTarget]) -> Result<()> {
        let mut insert = conn.prepare_cached(
            "INSERT INTO entrance_targets (owner_id, position, code_key_id, state) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (index, target) in targets.iter().enumerate() {
            let code = self.mirror.intern(&target.code, &mut self.st.insert_key)?;
            insert.execute((
                owner,
                position("target position", index)?,
                code,
                target.state.as_str(),
            ))?;
        }
        Ok(())
    }

    /// Replaces the input set.
    pub(crate) fn write_inputs(&mut self, conn: &Connection, inputs: &[Input]) -> Result<()> {
        conn.prepare_cached("DELETE FROM inputs")?.execute([])?;
        let mut insert = conn.prepare_cached("INSERT INTO inputs (path, role, hash) VALUES (?1, ?2, ?3)")?;
        for input in inputs {
            insert.execute((
                input.path.as_str(),
                input.role.as_str(),
                &input.hash.as_bytes()[..],
            ))?;
        }
        Ok(())
    }
}

/// Writes the graph metadata.
pub(crate) fn write_meta(conn: &Connection, meta: &GraphMeta) -> Result<()> {
    let mut upsert = conn.prepare_cached(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )?;
    upsert.execute((meta_key::PRODUCERS, &*meta.producers))?;
    upsert.execute((meta_key::SETTINGS_HASH, meta.settings_hash.to_string()))?;
    upsert.execute((meta_key::COMPILED_AT, meta.compiled_at_ms.to_string()))?;
    upsert.execute((meta_key::COMPLETE, if meta.complete { "true" } else { "false" }))?;
    upsert.execute((meta_key::LINK_BASIS, &*meta.link_basis))?;
    Ok(())
}
