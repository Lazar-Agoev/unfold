//! Writing the folder-wide records: entrances and their targets, inputs, applications,
//! memberships, launch records, and metadata.

use rusqlite::Connection;
use rustc_hash::FxHashSet;
use unfold_core::{Entrance, InputRecord, Key, Launches};

use super::graph::Apply;
use crate::error::Result;
use crate::schema::meta_key;
use crate::search::{CandidateKind, entrance_text, rowid};
use crate::update::{Application, EntranceChanges, GraphMeta, Membership};

impl Apply<'_, '_> {
    /// Removes and upserts entrances by key. An entrance's row ID is its key's ID.
    pub(crate) fn write_entrances(&mut self, conn: &Connection, changes: &EntranceChanges<'_>) -> Result<()> {
        for key in &changes.removals {
            if let Some(id) = self.mirror.key_id(key.as_str()) {
                self.remove_entrance(conn, id)?;
            }
        }

        let mut delete_entrance = conn.prepare_cached("DELETE FROM entrances WHERE id = ?1")?;
        let mut delete_targets =
            conn.prepare_cached("DELETE FROM entrance_targets WHERE entrance_id = ?1")?;
        let mut insert_entrance = conn.prepare_cached(
            "INSERT INTO entrances (id, key_id, kind, label, method, trigger, trigger_status, detector, \
             evidence_path, evidence_start_byte, evidence_end_byte, evidence_block_key_id, evidence_pointer, \
             binding_key_id) VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?;
        let mut insert_target = conn.prepare_cached(
            "INSERT INTO entrance_targets (entrance_id, position, role, target_key_id, target_text, \
             resolution, reason, module) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for entrance in &changes.upserts {
            let id = self.mirror.intern(&entrance.key, &mut self.st.insert_key)?;
            delete_entrance.execute([id])?;
            delete_targets.execute([id])?;
            self.st
                .delete_search
                .execute([rowid(CandidateKind::Entrance, id)])?;
            let evidence = &entrance.evidence;
            let evidence_block = self
                .mirror
                .intern_opt(evidence.block.as_ref(), &mut self.st.insert_key)?;
            let binding = self
                .mirror
                .intern_opt(entrance.binding.as_ref(), &mut self.st.insert_key)?;
            insert_entrance.execute((
                id,
                entrance.kind.as_str(),
                entrance.label.as_deref(),
                entrance.method.map(|method| method.as_str()),
                entrance.trigger.as_deref(),
                entrance.trigger_status.as_str(),
                entrance.detector.as_str(),
                evidence.path.as_str(),
                evidence.range.start,
                evidence.range.end,
                evidence_block,
                evidence.pointer.as_deref(),
                binding,
            ))?;
            for target in &entrance.targets {
                let target_id = self
                    .mirror
                    .intern_opt(target.target.as_ref(), &mut self.st.insert_key)?;
                insert_target.execute((
                    id,
                    target.position,
                    target.role.as_str(),
                    target_id,
                    &*target.text,
                    target.resolution.as_str(),
                    target.reason.map(|reason| reason.as_str()),
                    target.module.as_ref().map(|module| module.as_str()),
                ))?;
            }
            self.insert_entrance_search(id, entrance)?;
            self.stats.entrances_written += 1;
        }
        Ok(())
    }

    /// Removes every stored entrance whose key is not among `keep` ([`super::Writer::replace`]).
    pub(crate) fn remove_entrances_except(&mut self, conn: &Connection, keep: &[&Entrance]) -> Result<()> {
        let keep: FxHashSet<i64> = keep
            .iter()
            .filter_map(|entrance| self.mirror.key_id(entrance.key.as_str()))
            .collect();
        let stored: Vec<i64> = {
            let mut statement = conn.prepare_cached("SELECT id FROM entrances ORDER BY id")?;
            let ids = statement.query_map([], |row| row.get(0))?;
            ids.collect::<rusqlite::Result<_>>()?
        };
        for id in stored.into_iter().filter(|id| !keep.contains(id)) {
            self.remove_entrance(conn, id)?;
        }
        Ok(())
    }

    /// Deletes the entrance with row ID `id`, its targets, memberships, and search row.
    fn remove_entrance(&mut self, conn: &Connection, id: i64) -> Result<()> {
        let deleted = conn
            .prepare_cached("DELETE FROM entrances WHERE id = ?1")?
            .execute([id])?;
        conn.prepare_cached("DELETE FROM entrance_targets WHERE entrance_id = ?1")?
            .execute([id])?;
        conn.prepare_cached("DELETE FROM memberships WHERE entrance_id = ?1")?
            .execute([id])?;
        self.st
            .delete_search
            .execute([rowid(CandidateKind::Entrance, id)])?;
        self.stats.entrances_removed += deleted as u64;
        Ok(())
    }

    fn insert_entrance_search(&mut self, id: i64, entrance: &Entrance) -> Result<()> {
        let (name, qualified) = entrance_text(entrance);
        self.st.insert_search.execute((
            rowid(CandidateKind::Entrance, id),
            name,
            qualified,
            entrance.evidence.path.as_str(),
            entrance.kind.as_str(),
            entrance.key.as_str(),
        ))?;
        Ok(())
    }

    /// Replaces the input set.
    pub(crate) fn write_inputs(&mut self, conn: &Connection, inputs: &[InputRecord]) -> Result<()> {
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

    /// Replaces the applications.
    pub(crate) fn write_applications(
        &mut self,
        conn: &Connection,
        applications: &[Application],
    ) -> Result<()> {
        conn.prepare_cached("DELETE FROM applications")?.execute([])?;
        let mut insert = conn.prepare_cached(
            "INSERT INTO applications (id, name, description, declared, category) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for application in applications {
            insert.execute((
                &*application.id,
                &*application.name,
                application.description.as_deref(),
                application.declared,
                application.category.as_str(),
            ))?;
        }
        Ok(())
    }

    /// Replaces the memberships.
    pub(crate) fn write_memberships(&mut self, conn: &Connection, memberships: &[Membership]) -> Result<()> {
        conn.prepare_cached("DELETE FROM memberships")?.execute([])?;
        let mut insert = conn.prepare_cached(
            "INSERT INTO memberships (entrance_id, application, basis) VALUES (?1, ?2, ?3)",
        )?;
        for membership in memberships {
            let entrance = self.intern(&membership.entrance)?;
            insert.execute((entrance, &*membership.application, membership.basis.as_str()))?;
        }
        Ok(())
    }

    fn intern(&mut self, key: &Key) -> Result<i64> {
        self.mirror.intern(key, &mut self.st.insert_key)
    }
}

/// Writes the launch records.
pub(crate) fn write_launches(conn: &Connection, launches: &Launches) -> Result<()> {
    // Strings, numbers, and keys: serializing can't fail.
    let text = serde_json::to_string(launches).unwrap_or_default();
    conn.prepare_cached(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )?
    .execute((meta_key::LAUNCHES, text))?;
    Ok(())
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
