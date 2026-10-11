//! Typed lines (`docs/reader.md`, "Typed lines"): `line_labels` (the at-rest labels of a page's
//! arrows), `call_contract` (one selected line), `type_definition` (a type opened under a
//! contract row), and `type_uses` (where a type is written). Each wakes its project and reads
//! the committed graph in one store snapshot; the reader's contracts module computes the
//! payloads.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_reader::contracts::{self, CallContract, Facts, LineLabel, TypeDefinition, TypeUses};
use unfold_store::Reader;

use super::key_arg;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::workspace::Workspace;

/// The most lines one `line_labels` request labels.
pub const MAX_LINES: usize = 500;
/// The most places one `type_uses` page lists.
pub const MAX_USES: u32 = 200;
/// Places per `type_uses` page when the request doesn't say.
const DEFAULT_USES: u32 = 50;

/// One arrow: an occurrence and one of its edges.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineRef {
    /// The occurrence block's key.
    pub source: String,
    /// The edge's sequence: 0 for the call, construction, or import itself; then passed
    /// references in argument order.
    pub seq: u32,
}

/// The arguments of `line_labels`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineLabelsArgs {
    /// The project of every line.
    pub project: ProjectId,
    /// The arrows to label (at most 500).
    pub lines: Vec<LineRef>,
}

/// One arrow's at-rest label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LabeledLine {
    /// The occurrence block's key, as asked.
    pub source: String,
    /// The edge's sequence, as asked.
    pub seq: u32,
    /// The label; `null` for an external or unresolved edge, an import, or an unknown arrow.
    pub label: Option<LineLabel>,
}

/// The arguments of `call_contract`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallContractArgs {
    /// The project.
    pub project: ProjectId,
    /// The occurrence block's key.
    pub source: String,
    /// The edge's sequence (a contract exists for 0: the call or construction itself).
    pub seq: u32,
}

/// The arguments of `type_uses`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TypeUsesArgs {
    /// The project.
    pub project: ProjectId,
    /// The type declaration's or class's key.
    pub key: String,
    /// How many places to skip (0 when absent).
    #[serde(default)]
    #[ts(optional)]
    pub offset: Option<u32>,
    /// How many places to list (at most 200; 50 when absent).
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
}

/// Runs `compute` over the project's committed graph in one store snapshot, with source read
/// and verified under the project's folder.
fn with_facts<T>(
    workspace: &Workspace,
    compute: impl FnOnce(&mut Facts<'_>) -> unfold_reader::Result<T>,
) -> CommandResult<T> {
    let reader: Reader = workspace.readers()?.get()?;
    let root = workspace.root();
    let computed = reader.snapshot(|reader| Ok(compute(&mut Facts::new(reader, root))))?;
    Ok(computed?)
}

/// `line_labels`: the at-rest label of every arrow asked for, in the order asked.
pub fn line_labels(ctx: &Ctx<'_>, args: LineLabelsArgs) -> CommandResult<Vec<LabeledLine>> {
    if args.lines.len() > MAX_LINES {
        return Err(CommandError::invalid_args(format!(
            "lines lists at most {MAX_LINES} arrows"
        )));
    }
    let workspace = ctx.live(&args.project)?;
    let labels = with_facts(&workspace, |facts| {
        let mut labels = Vec::with_capacity(args.lines.len());
        for line in &args.lines {
            if ctx.cancel.is_cancelled() {
                break;
            }
            labels.push(LabeledLine {
                label: contracts::line_label(facts, &line.source, line.seq)?,
                source: line.source.clone(),
                seq: line.seq,
            });
        }
        Ok(labels)
    })?;
    ctx.check_cancelled()?;
    Ok(labels)
}

/// `call_contract`: the contract of one resolved call or construction.
pub fn call_contract(ctx: &Ctx<'_>, args: CallContractArgs) -> CommandResult<CallContract> {
    let source = key_arg(&args.source)?;
    let workspace = ctx.live(&args.project)?;
    with_facts(&workspace, |facts| {
        contracts::call_contract(facts, source, args.seq)
    })?
    .ok_or_else(|| {
        CommandError::not_found(format!(
            "no resolved call or construction `{source}` (edge {})",
            args.seq
        ))
    })
}

/// `type_definition`: a type declaration or class with its members and use count.
pub fn type_definition(ctx: &Ctx<'_>, args: super::ProjectKeyArgs) -> CommandResult<TypeDefinition> {
    let key = key_arg(&args.key)?;
    let workspace = ctx.live(&args.project)?;
    with_facts(&workspace, |facts| contracts::type_definition(facts, key))?
        .ok_or_else(|| CommandError::not_found(format!("no type declaration or class `{key}`")))
}

/// `type_uses`: one page of the places a type declaration or class is written.
pub fn type_uses(ctx: &Ctx<'_>, args: TypeUsesArgs) -> CommandResult<TypeUses> {
    let key = key_arg(&args.key)?;
    let limit = args.limit.unwrap_or(DEFAULT_USES);
    if limit > MAX_USES {
        return Err(CommandError::invalid_args(format!("limit is at most {MAX_USES}")));
    }
    let offset = args.offset.unwrap_or(0) as usize;
    let workspace = ctx.live(&args.project)?;
    with_facts(&workspace, |facts| {
        contracts::type_uses(facts, key, offset, limit as usize)
    })?
    .ok_or_else(|| CommandError::not_found(format!("no type declaration or class `{key}`")))
}
