//! The typed part of a file's facts: what extraction knows about written types, parameters,
//! members, and where each call's value lands. Pairing and type links depend on other files, so
//! they are decided at link time ([`crate::pairing`], [`crate::edges`]); extraction records the
//! syntax they need: each argument's expansion and keyword (in [`ArgumentFact`](super::ArgumentFact)),
//! and each written type name ([`TypeRefFact`]).

use serde::{Deserialize, Serialize};
use unfold_core::text::{ByteRange, short_label};
use unfold_core::typed::{CallResult, Member, Parameter, Signature};
use unfold_core::vocab::{BlockKind, Heritage, ResultBinding, SlotName};
use unfold_core::{Block, Slot};

use super::FactsError;

/// One type name written in a recorded annotation or a heritage clause: `Cart`, or a qualified
/// name whole (`Stripe.PaymentIntent`), without type arguments. Its language resolves it with
/// [`Linked::type_evidence`](crate::Linked::type_evidence), from what its payload records at the
/// same index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeRefFact {
    /// The innermost block whose range contains the name (set by [`TypeFacts::new`]).
    pub block: u32,
    /// The written name's range.
    pub range: ByteRange,
    /// The name as written, cut to 120 UTF-16 units.
    pub text: Box<str>,
    /// Whether it is a qualified name (`a.B`), whose root is a namespace or module.
    pub qualified: bool,
    /// When it names a base of its block (a class or interface) in a heritage clause: whether
    /// the block extends or implements it.
    pub heritage: Option<Heritage>,
}

impl TypeRefFact {
    /// A type name at `range`; its block is found when the facts are assembled.
    pub fn new(range: ByteRange, text: &str, qualified: bool) -> Self {
        Self {
            block: 0,
            range,
            text: short_label(text).into(),
            qualified,
            heritage: None,
        }
    }

    /// The same name, as a base its class or interface extends or implements.
    #[must_use]
    pub fn with_heritage(self, heritage: Option<Heritage>) -> Self {
        Self { heritage, ..self }
    }
}

/// One file's typed facts, each list in canonical order (by block, then index or range; type
/// references in the order the language recorded them, which its payload is aligned with).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TypeFacts {
    /// One per function-like block.
    pub signatures: Vec<Signature>,
    /// One per `parameter` slot.
    pub parameters: Vec<Parameter>,
    /// The written members of type declarations and classes.
    pub members: Vec<Member>,
    /// One per `call` or `new` block.
    pub results: Vec<CallResult>,
    /// Every type name of a recorded annotation or a heritage clause, in the language's order.
    pub type_refs: Vec<TypeRefFact>,
}

impl TypeFacts {
    /// Typed facts in canonical order, with texts cut to 120 UTF-16 units and each type
    /// reference's block set to the innermost block of `blocks` containing it.
    pub fn new(
        blocks: &[Block],
        mut signatures: Vec<Signature>,
        mut parameters: Vec<Parameter>,
        mut members: Vec<Member>,
        mut results: Vec<CallResult>,
        mut type_refs: Vec<TypeRefFact>,
    ) -> Self {
        let cut = |text: &mut Box<str>| {
            if text.len() > unfold_core::text::LABEL_MAX_UTF16 {
                *text = short_label(text).into();
            }
        };
        for signature in &mut signatures {
            if let Some(returns) = &mut signature.returns {
                cut(&mut returns.text);
            }
        }
        for parameter in &mut parameters {
            if let Some(name) = &mut parameter.name {
                cut(name);
            }
            if let Some(ty) = &mut parameter.ty {
                cut(&mut ty.text);
            }
        }
        for member in &mut members {
            for text in [member.name.as_mut(), member.value.as_mut()]
                .into_iter()
                .flatten()
            {
                cut(text);
            }
            if let Some(ty) = &mut member.ty {
                cut(&mut ty.text);
            }
        }
        signatures.sort_by_key(|signature| signature.block);
        parameters.sort_by_key(|parameter| (parameter.block, parameter.index));
        members.sort_by_key(|member| (member.block, member.index));
        results.sort_by_key(|result| result.call);
        for reference in &mut type_refs {
            reference.block = innermost(blocks, reference.range);
        }
        Self {
            signatures,
            parameters,
            members,
            results,
            type_refs,
        }
    }

    /// Checks that the facts are well formed against the file's blocks and slots: each record
    /// names a block of a kind it belongs to, in order and at most once per block (or slot), and
    /// every parameter names a parameter slot. Completeness (one signature per function-like
    /// block, one result per call, ...) is the graph validator's.
    pub(super) fn check(&self, blocks: &[Block], slots: &[Slot]) -> Result<(), FactsError> {
        let kind = |block: u32| blocks.get(block as usize).map(|found| found.kind);
        let bad = |what: &'static str, block: u32| FactsError::Typed { what, block };
        if let Some(pair) = self
            .signatures
            .windows(2)
            .find(|pair| pair[0].block >= pair[1].block)
        {
            return Err(bad("signature", pair[1].block));
        }
        if let Some(signature) = self
            .signatures
            .iter()
            .find(|signature| !kind(signature.block).is_some_and(BlockKind::is_function_like))
        {
            return Err(bad("signature", signature.block));
        }
        let parameter_slots: rustc_hash::FxHashSet<(u32, u32)> = slots
            .iter()
            .filter(|slot| slot.slot == SlotName::Parameter)
            .filter_map(|slot| Some((slot.block, slot.index?)))
            .collect();
        let mut previous = None;
        for parameter in &self.parameters {
            let position = (parameter.block, parameter.index);
            if previous.is_some_and(|previous| previous >= position) || !parameter_slots.contains(&position) {
                return Err(bad("parameter", parameter.block));
            }
            previous = Some(position);
        }
        let mut previous = None;
        for member in &self.members {
            let position = (member.block, member.index);
            if previous.is_some_and(|previous| previous >= position)
                || !kind(member.block).is_some_and(BlockKind::is_type_declaration)
            {
                return Err(bad("member", member.block));
            }
            previous = Some(position);
        }
        if let Some(pair) = self.results.windows(2).find(|pair| pair[0].call >= pair[1].call) {
            return Err(bad("result", pair[1].call));
        }
        for result in &self.results {
            let call = matches!(kind(result.call), Some(BlockKind::Call | BlockKind::New));
            let target = match result.binding {
                ResultBinding::Variable => result
                    .target
                    .is_some_and(|target| kind(target) == Some(BlockKind::Variable)),
                ResultBinding::Argument => result
                    .target
                    .is_some_and(|target| matches!(kind(target), Some(BlockKind::Call | BlockKind::New))),
                _ => result.target.is_none(),
            };
            if !call || !target {
                return Err(bad("result", result.call));
            }
        }
        for reference in &self.type_refs {
            let base = reference.heritage.is_none_or(|_| {
                matches!(
                    kind(reference.block),
                    Some(BlockKind::Class | BlockKind::Interface)
                )
            });
            if kind(reference.block).is_none() || reference.range.is_empty() || !base {
                return Err(bad("type reference", reference.block));
            }
        }
        Ok(())
    }

    /// Rewrites every local ID through `renumber`, dropping records of dropped blocks.
    pub(super) fn renumber(&mut self, renumber: impl Fn(u32) -> Option<u32>) {
        let mut types = unfold_core::FileTypes {
            signatures: std::mem::take(&mut self.signatures),
            parameters: std::mem::take(&mut self.parameters),
            members: std::mem::take(&mut self.members),
            results: std::mem::take(&mut self.results),
            ..unfold_core::FileTypes::default()
        };
        types.renumber(&renumber);
        self.signatures = types.signatures;
        self.parameters = types.parameters;
        self.members = types.members;
        self.results = types.results;
        self.type_refs
            .retain_mut(|reference| match renumber(reference.block) {
                Some(block) => {
                    reference.block = block;
                    true
                }
                None => false,
            });
    }
}

/// The innermost block of `blocks` (in preorder, the root first) whose range contains `range`:
/// among the blocks containing it, which form a chain from the root, the last in preorder.
pub fn innermost(blocks: &[Block], range: ByteRange) -> u32 {
    // Siblings are in start order, so the blocks that start after the range can't contain it.
    let end = blocks.partition_point(|block| block.range.start <= range.start);
    blocks[..end]
        .iter()
        .rev()
        .find(|block| block.range.contains(range))
        .map_or(0, |block| block.local_id)
}
