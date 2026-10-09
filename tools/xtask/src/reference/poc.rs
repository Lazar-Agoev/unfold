//! Reading one raw PoC generation: `manifest.json` and the five CSV tables (PoC schema 3).
//!
//! Offsets in these rows are UTF-16 code units and IDs are generation-local (`b1`, `e1`); the
//! converter turns them into bytes and keys. Empty CSV fields are absent values.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use unfold_core::vocab::Language;

/// The manifest fields the importer relies on.
#[derive(Debug, Deserialize)]
pub(crate) struct Manifest {
    /// Must be 3.
    pub schema_version: u32,
    /// Must be `utf16`.
    pub offset_unit: String,
    /// Must be true.
    pub end_offset_exclusive: bool,
    /// Must be 1.
    pub line_base: u32,
    /// Indexed sources by PoC path.
    pub files: BTreeMap<String, ManifestFile>,
    /// Configuration inputs by PoC path.
    #[serde(default)]
    pub inputs: BTreeMap<String, ManifestInput>,
}

/// One indexed source in the manifest.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestFile {
    /// SHA-256 of the source bytes, lowercase hex.
    pub sha256: String,
    /// The source's language.
    pub language: Language,
}

/// One configuration input in the manifest.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestInput {
    /// SHA-256 of the input bytes, lowercase hex.
    pub sha256: String,
}

/// A `blocks.csv` row.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BlockRow {
    pub id: Box<str>,
    pub file_path: Box<str>,
    pub kind: Box<str>,
    pub name: Option<Box<str>>,
    pub parent_id: Option<Box<str>>,
    pub role: Option<Box<str>>,
    pub start_offset: u32,
    pub end_offset: u32,
    pub start_line: u32,
    pub end_line: u32,
}

/// A `slots.csv` row.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SlotRow {
    pub block_id: Box<str>,
    pub slot: Box<str>,
    pub index: Option<u32>,
    pub start_offset: u32,
    pub end_offset: u32,
}

/// An `edges.csv` row.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EdgeRow {
    pub source_id: Box<str>,
    pub target_id: Option<Box<str>>,
    pub kind: Box<str>,
    pub target_text: Box<str>,
    pub resolution: Box<str>,
    pub reason: Option<Box<str>>,
    pub module: Option<Box<str>>,
    pub position: Option<u32>,
}

/// An `entrypoints.csv` row.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EntranceRow {
    pub id: Box<str>,
    pub kind: Box<str>,
    pub label: Option<Box<str>>,
    pub method: Option<Box<str>>,
    pub trigger: Option<Box<str>>,
    pub trigger_status: Box<str>,
    pub detector: Box<str>,
    pub evidence_block_id: Option<Box<str>>,
    pub evidence_file_path: Box<str>,
    pub evidence_start_offset: u32,
    pub evidence_end_offset: u32,
    pub evidence_pointer: Option<Box<str>>,
}

/// An `entrypoint_targets.csv` row.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TargetRow {
    pub entrypoint_id: Box<str>,
    pub position: u32,
    pub role: Box<str>,
    pub target_id: Option<Box<str>>,
    pub target_text: Box<str>,
    pub resolution: Box<str>,
    pub reason: Option<Box<str>>,
    pub module: Option<Box<str>>,
}

/// One raw generation.
#[derive(Debug)]
pub(crate) struct Generation {
    pub manifest: Manifest,
    pub blocks: Vec<BlockRow>,
    pub slots: Vec<SlotRow>,
    pub edges: Vec<EdgeRow>,
    pub entrances: Vec<EntranceRow>,
    pub targets: Vec<TargetRow>,
}

impl Generation {
    /// Reads the generation in `dir` and checks the manifest's conventions.
    pub(crate) fn read(dir: &Path) -> anyhow::Result<Self> {
        let manifest_path = dir.join("manifest.json");
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("cannot read {}", manifest_path.display()))?;
        let manifest: Manifest = serde_json::from_str(&manifest_text)
            .with_context(|| format!("invalid {}", manifest_path.display()))?;
        if manifest.schema_version != 3
            || manifest.offset_unit != "utf16"
            || !manifest.end_offset_exclusive
            || manifest.line_base != 1
        {
            bail!(
                "{}: expected schema 3 with exclusive UTF-16 offsets and one-based lines",
                manifest_path.display()
            );
        }
        Ok(Self {
            manifest,
            blocks: read_table(dir, "blocks.csv")?,
            slots: read_table(dir, "slots.csv")?,
            edges: read_table(dir, "edges.csv")?,
            entrances: read_table(dir, "entrypoints.csv")?,
            targets: read_table(dir, "entrypoint_targets.csv")?,
        })
    }
}

/// Reads every record of one CSV table, addressing columns by header name.
fn read_table<T: DeserializeOwned>(dir: &Path, name: &str) -> anyhow::Result<Vec<T>> {
    let path = dir.join(name);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    reader
        .deserialize()
        .enumerate()
        .map(|(index, row)| row.with_context(|| format!("{}: record {}", path.display(), index + 1)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_fields_and_absent_values() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("edges.csv"),
            "source_id,target_id,kind,target_text,resolution,reason,module,position\n\
             b2,,calls,\"log(\"\"a,b\"\")\",unresolved,dynamic,,\n\
             b3,b9,references,handler,resolved,,,1\n",
        )
        .unwrap();
        let rows: Vec<EdgeRow> = read_table(dir.path(), "edges.csv").unwrap();
        assert_eq!(&*rows[0].target_text, "log(\"a,b\")");
        assert!(rows[0].target_id.is_none() && rows[0].module.is_none() && rows[0].position.is_none());
        assert_eq!(rows[1].target_id.as_deref(), Some("b9"));
        assert_eq!(rows[1].position, Some(1));
    }

    #[test]
    fn rejects_unknown_columns() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("slots.csv"),
            "block_id,slot,index,start_offset,end_offset,extra\nb1,body,,0,1,x\n",
        )
        .unwrap();
        assert!(read_table::<SlotRow>(dir.path(), "slots.csv").is_err());
    }
}
