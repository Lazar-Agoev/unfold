//! Writing files: rows, nodes, links, typed records, definitions, and their search rows.

use rusqlite::params;
use rustc_hash::FxHashSet;
use unfold_core::{FileTypes, Key, Link, LinkTarget, Node, Pairing, RepoPath, WrittenType};

use super::mirror::Mirror;
use super::statements::Statements;
use crate::codec::{flag_bits, position, range_columns, to_i64};
use crate::error::Result;
use crate::search::{CandidateKind, rowid};
use crate::update::{Definition, FileChange};
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
        self.delete_types(file_id)?;
        self.st.delete_links.execute([file_id])?;
        self.st.delete_nodes.execute([file_id])?;
        self.st.delete_file.execute([file_id])?;
        self.st.delete_facts.execute([path.as_str()])?;
        if let Some(module) = self.mirror.key_id(Key::module(path).as_str()) {
            self.st
                .delete_search
                .execute([rowid(CandidateKind::File, module)])?;
        }
        self.stats.files_removed += 1;
        Ok(())
    }

    /// Removes every stored file that `files` does not name ([`super::Writer::replace`]).
    pub(crate) fn remove_files_except(&mut self, files: &[FileChange<'_>]) -> Result<()> {
        let keep: FxHashSet<&str> = files.iter().map(|change| change.file.path.as_str()).collect();
        for path in self.mirror.file_paths_except(&keep) {
            self.remove_file(&path)?;
        }
        Ok(())
    }

    /// Writes the file's row and replaces each part the change carries.
    pub(crate) fn write_file(&mut self, change: &FileChange<'_>) -> Result<()> {
        let file_id = self.write_row(change)?;
        if let Some(nodes) = change.nodes {
            self.replace_nodes(file_id, nodes)?;
        }
        if let Some(links) = change.links {
            self.replace_links(file_id, links)?;
        }
        if let Some(types) = change.types {
            self.replace_types(file_id, types)?;
        }
        if let Some(definitions) = change.definitions {
            self.replace_definitions(file_id, &change.file.path, definitions)?;
        }
        self.stats.files_written += 1;
        Ok(())
    }

    /// Updates or inserts the file's row, returning its ID. A new file also gets its search row.
    fn write_row(&mut self, change: &FileChange<'_>) -> Result<i64> {
        let file = change.file;
        let facts_hash = file.hash.as_ref().map(|hash| &hash.as_bytes()[..]);
        let disk_hash = change.disk.hash.as_ref().map(|hash| &hash.as_bytes()[..]);
        let size = change
            .disk
            .size
            .map(|size| to_i64("file size", size))
            .transpose()?;
        let language = file.language.as_str();
        let state = file.state.as_str();
        let failure = file.failure.as_deref();
        if let Some(file_id) = self.mirror.file_id(file.path.as_str()) {
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
        let file_id = self.mirror.add_file(&file.path);
        self.st.insert_file.execute((
            file_id,
            file.path.as_str(),
            language,
            state,
            facts_hash,
            disk_hash,
            change.disk.mtime_ns,
            size,
            failure,
        ))?;
        let module = Key::module(&file.path);
        let module_id = self.mirror.intern(&module, &mut self.st.insert_key)?;
        self.st.insert_search.execute((
            rowid(CandidateKind::File, module_id),
            file.path.file_name(),
            "",
            file.path.as_str(),
            "module",
            module.as_str(),
        ))?;
        Ok(file_id)
    }

    fn replace_nodes(&mut self, file_id: i64, nodes: &[Node]) -> Result<()> {
        self.st.delete_nodes.execute([file_id])?;
        for (local_id, node) in nodes.iter().enumerate() {
            let key_id = self.mirror.intern(&node.key, &mut self.st.insert_key)?;
            let (head_start, head_end) = range_columns(node.head);
            let (body_start, body_end) = range_columns(node.body);
            let (doc_start, doc_end) = range_columns(node.doc);
            let (stmt_start, stmt_end) = range_columns(node.stmt);
            let (value_start, value_end) = range_columns(node.value);
            self.st.insert_node.execute(params![
                file_id,
                position("local ID", local_id)?,
                key_id,
                node.parent,
                node.form.as_str(),
                node.name.as_deref(),
                node.range.start,
                node.range.end,
                node.lines.start,
                node.lines.end,
                head_start,
                head_end,
                body_start,
                body_end,
                doc_start,
                doc_end,
                stmt_start,
                stmt_end,
                value_start,
                value_end,
                node.binds.as_deref(),
                node.role.map(|role| role.as_str()),
                node.r#for,
                flag_bits(node.flags),
                node.target,
            ])?;
        }
        self.stats.nodes_written += nodes.len() as u64;
        Ok(())
    }

    fn replace_links(&mut self, file_id: i64, links: &[Link]) -> Result<()> {
        self.st.delete_links.execute([file_id])?;
        for link in links {
            let (target, module, reason) = match &link.to {
                LinkTarget::Project(key) => (
                    Some(self.mirror.intern(key, &mut self.st.insert_key)?),
                    None,
                    None,
                ),
                LinkTarget::Library(label) | LinkTarget::Builtin(label) => (None, Some(label.as_str()), None),
                LinkTarget::Unknown(reason) => (None, None, Some(reason.as_str())),
            };
            self.st.insert_link.execute(params![
                file_id,
                link.from,
                link.seq,
                link.kind.as_str(),
                link.range.start,
                link.range.end,
                &*link.text,
                link.to.outcome().as_str(),
                target,
                module,
                reason,
                link.implements,
                link.bound,
            ])?;
        }
        self.stats.links_written += links.len() as u64;
        Ok(())
    }

    /// Deletes a file's typed records.
    fn delete_types(&mut self, file_id: i64) -> Result<()> {
        self.st.delete_signatures.execute([file_id])?;
        self.st.delete_parameters.execute([file_id])?;
        self.st.delete_members.execute([file_id])?;
        self.st.delete_arguments.execute([file_id])?;
        Ok(())
    }

    fn replace_types(&mut self, file_id: i64, types: &FileTypes) -> Result<()> {
        self.delete_types(file_id)?;
        for signature in &types.signatures {
            let returns = signature.returns.as_ref();
            let (start, end) = written_range(returns);
            self.st
                .insert_signature
                .execute((file_id, signature.node, text(returns), start, end))?;
        }
        for parameter in &types.parameters {
            let ty = parameter.ty.as_ref();
            let (type_start, type_end) = written_range(ty);
            let flags = u8::from(parameter.optional) | (u8::from(parameter.default) << 1);
            self.st.insert_parameter.execute((
                file_id,
                parameter.node,
                parameter.index,
                parameter.range.start,
                parameter.range.end,
                parameter.kind.as_str(),
                parameter.name.as_deref(),
                flags,
                text(ty),
                type_start,
                type_end,
            ))?;
        }
        for member in &types.members {
            let ty = member.ty.as_ref();
            let (type_start, type_end) = written_range(ty);
            let flags = u8::from(member.optional)
                | (u8::from(member.readonly) << 1)
                | (u8::from(member.default) << 2);
            self.st.insert_member.execute((
                file_id,
                member.node,
                member.index,
                member.range.start,
                member.range.end,
                member.kind.as_str(),
                member.name.as_deref(),
                flags,
                text(ty),
                type_start,
                type_end,
                member.value.as_deref(),
            ))?;
        }
        for argument in &types.arguments {
            let (parameter, unpaired) = match argument.pairing {
                Some(Pairing::Parameter(index)) => (Some(index), None),
                Some(Pairing::Unpaired(reason)) => (None, Some(reason.as_str())),
                None => (None, None),
            };
            self.st.insert_argument.execute((
                file_id,
                argument.call,
                argument.index,
                argument.range.start,
                argument.range.end,
                argument.expand.map(|expand| expand.as_str()),
                argument.keyword.as_deref(),
                parameter,
                unpaired,
                argument.bound,
            ))?;
        }
        self.stats.typed_records_written +=
            (types.signatures.len() + types.parameters.len() + types.members.len() + types.arguments.len())
                as u64;
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
                definition.node,
                kind,
                &*definition.qualified_name,
                &*definition.label,
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

/// A written type's text column.
fn text(written: Option<&WrittenType>) -> Option<&str> {
    written.map(|written| &*written.text)
}

/// A written type's start and end columns.
fn written_range(written: Option<&WrittenType>) -> (Option<u32>, Option<u32>) {
    range_columns(written.map(|written| written.range))
}
