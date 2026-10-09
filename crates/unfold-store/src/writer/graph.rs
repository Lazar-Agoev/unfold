//! Writing files: rows, blocks, slots, edges, definitions, and their search rows.

use rustc_hash::FxHashSet;
use unfold_core::{Key, RepoPath};

use super::mirror::Mirror;
use super::statements::Statements;
use crate::codec::to_i64;
use crate::error::Result;
use crate::search::{CandidateKind, rowid};
use crate::update::{Definition, FileChange, Structure};
use crate::writer::CommitStats;

/// One commit in progress: its statements, the key mirror, and what it has written so far.
pub(crate) struct Apply<'c, 'm> {
    pub st: Statements<'c>,
    pub mirror: &'m mut Mirror,
    pub stats: CommitStats,
}

impl Apply<'_, '_> {
    /// Deletes everything stored for the file at `path`, including its cached facts: a path
    /// that is no longer indexed would otherwise keep its entries forever. A path that is not
    /// stored is ignored.
    pub(crate) fn remove_file(&mut self, path: &RepoPath) -> Result<()> {
        let Some(file_id) = self.mirror.remove_file(path.as_str()) else {
            return Ok(());
        };
        self.delete_definitions(file_id)?;
        self.st.delete_edges.execute([file_id])?;
        self.st.delete_slots.execute([file_id])?;
        self.st.delete_blocks.execute([file_id])?;
        self.st.delete_file.execute([file_id])?;
        self.st.delete_facts.execute([path.as_str()])?;
        if let Some(root) = self.mirror.key_id(Key::file_root(path).as_str()) {
            self.st
                .delete_search
                .execute([rowid(CandidateKind::File, root)])?;
        }
        self.stats.files_removed += 1;
        Ok(())
    }

    /// Removes every stored file that `files` does not name ([`super::Writer::replace`]).
    pub(crate) fn remove_files_except(&mut self, files: &[FileChange<'_>]) -> Result<()> {
        let keep: FxHashSet<&str> = files.iter().map(|change| change.record.path.as_str()).collect();
        for path in self.mirror.file_paths_except(&keep) {
            self.remove_file(&path)?;
        }
        Ok(())
    }

    /// Writes the file's row and replaces each part the change carries.
    pub(crate) fn write_file(&mut self, change: &FileChange<'_>) -> Result<()> {
        let file_id = self.write_row(change)?;
        if let Some(structure) = change.structure {
            self.replace_structure(file_id, structure)?;
        }
        if let Some(edges) = change.edges {
            self.replace_edges(file_id, edges)?;
        }
        if let Some(definitions) = change.definitions {
            self.replace_definitions(file_id, &change.record.path, definitions)?;
        }
        self.stats.files_written += 1;
        Ok(())
    }

    /// Updates or inserts the file's row, returning its ID. A new file also gets its search row.
    fn write_row(&mut self, change: &FileChange<'_>) -> Result<i64> {
        let record = change.record;
        let facts_hash = record.hash.as_ref().map(|hash| &hash.as_bytes()[..]);
        let disk_hash = change.disk.hash.as_ref().map(|hash| &hash.as_bytes()[..]);
        let size = change
            .disk
            .size
            .map(|size| to_i64("file size", size))
            .transpose()?;
        let language = record.language.as_str();
        let state = record.state.as_str();
        let failure = record.failure.as_deref();
        if let Some(file_id) = self.mirror.file_id(record.path.as_str()) {
            self.st.update_file.execute((
                file_id,
                language,
                state,
                facts_hash,
                disk_hash,
                change.disk.mtime_ns,
                size,
                failure,
            ))?;
            return Ok(file_id);
        }
        let file_id = self.mirror.add_file(&record.path);
        self.st.insert_file.execute((
            file_id,
            record.path.as_str(),
            language,
            state,
            facts_hash,
            disk_hash,
            change.disk.mtime_ns,
            size,
            failure,
        ))?;
        let root = Key::file_root(&record.path);
        let root_id = self.mirror.intern(&root, &mut self.st.insert_key)?;
        self.st.insert_search.execute((
            rowid(CandidateKind::File, root_id),
            record.path.file_name(),
            "",
            record.path.as_str(),
            "file",
            root.as_str(),
        ))?;
        Ok(file_id)
    }

    fn replace_structure(&mut self, file_id: i64, structure: Structure<'_>) -> Result<()> {
        self.st.delete_blocks.execute([file_id])?;
        self.st.delete_slots.execute([file_id])?;
        for block in structure.blocks {
            let key_id = self.mirror.intern(&block.key, &mut self.st.insert_key)?;
            self.st.insert_block.execute((
                file_id,
                block.local_id,
                key_id,
                block.parent,
                block.kind.as_str(),
                block.role.map(|role| role.as_str()),
                block.name.as_deref(),
                block.range.start,
                block.range.end,
                block.lines.start,
                block.lines.end,
            ))?;
        }
        for slot in structure.slots {
            // A single slot's index is stored as -1, so the primary key never contains NULL.
            let index = slot.index.map_or(-1, i64::from);
            self.st.insert_slot.execute((
                file_id,
                slot.block,
                slot.slot.as_str(),
                index,
                slot.range.start,
                slot.range.end,
            ))?;
        }
        self.stats.blocks_written += structure.blocks.len() as u64;
        self.stats.slots_written += structure.slots.len() as u64;
        Ok(())
    }

    fn replace_edges(&mut self, file_id: i64, edges: &[unfold_core::Edge]) -> Result<()> {
        self.st.delete_edges.execute([file_id])?;
        for edge in edges {
            let target = self
                .mirror
                .intern_opt(edge.target.as_ref(), &mut self.st.insert_key)?;
            self.st.insert_edge.execute((
                file_id,
                edge.source,
                edge.seq,
                edge.kind.as_str(),
                target,
                &*edge.text,
                edge.resolution.as_str(),
                edge.reason.map(|reason| reason.as_str()),
                edge.module.as_ref().map(|module| module.as_str()),
                edge.position,
            ))?;
        }
        self.stats.edges_written += edges.len() as u64;
        Ok(())
    }

    fn replace_definitions(
        &mut self,
        file_id: i64,
        path: &RepoPath,
        definitions: &[Definition],
    ) -> Result<()> {
        self.delete_definitions(file_id)?;
        for definition in definitions {
            let key_id = self.mirror.intern(&definition.key, &mut self.st.insert_key)?;
            let kind = definition.kind.as_str();
            self.st.insert_definition.execute((
                key_id,
                file_id,
                definition.local_id,
                kind,
                &*definition.qualified_name,
            ))?;
            self.st.insert_search.execute((
                rowid(CandidateKind::Definition, key_id),
                &*definition.label,
                &*definition.qualified_name,
                path.as_str(),
                kind,
                definition.key.as_str(),
            ))?;
        }
        self.stats.definitions_written += definitions.len() as u64;
        Ok(())
    }

    /// Deletes a file's definitions and their search rows.
    fn delete_definitions(&mut self, file_id: i64) -> Result<()> {
        self.st.delete_definition_search.execute([file_id])?;
        self.st.delete_definitions.execute([file_id])?;
        Ok(())
    }
}
