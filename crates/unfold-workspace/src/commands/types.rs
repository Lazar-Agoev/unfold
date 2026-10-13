//! Typed lines (`docs/reader.md`, "Typed lines"): `call_contract` (one selected line),
//! `type_definition` (a type opened under a contract row), and `type_uses` (where a type is
//! written). Each wakes its project and reads
//! the committed graph in one store snapshot; the reader's contracts module computes the
//! payloads.

use serde::Deserialize;
use ts_rs::TS;
use unfold_reader::contracts::{self, CallContract, Facts, TypeDefinition, TypeUses};
use unfold_store::Reader;

use super::key_arg;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::workspace::Workspace;

/// The most places one `type_uses` page lists.
pub const MAX_USES: u32 = 200;
/// Places per `type_uses` page when the request doesn't say.
const DEFAULT_USES: u32 = 50;

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
