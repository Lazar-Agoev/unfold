//! The typed records extraction knows on its own: what each function takes and returns, and the
//! written members of types and classes. Argument pairings depend on other files, so they are
//! decided at link time ([`crate::pairing`]) from each call's [`ArgumentFact`](super::ArgumentFact)s.

use serde::{Deserialize, Serialize};
use unfold_core::Node;
use unfold_core::typed::{Member, Parameter, Signature};
use unfold_core::vocab::NodeKind;

use super::{FactsError, cut};

/// One file's signatures, parameters, and members, each in canonical order: by node, then
/// index.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TypeFacts {
    /// One per function node.
    pub signatures: Vec<Signature>,
    /// One per parameter of a function node.
    pub parameters: Vec<Parameter>,
    /// The written members of type and class nodes.
    pub members: Vec<Member>,
}

impl TypeFacts {
    /// Typed records in canonical order, with texts cut to 120 UTF-16 units.
    pub fn new(
        mut signatures: Vec<Signature>,
        mut parameters: Vec<Parameter>,
        mut members: Vec<Member>,
    ) -> Self {
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
        signatures.sort_by_key(|signature| signature.node);
        parameters.sort_by_key(|parameter| (parameter.node, parameter.index));
        members.sort_by_key(|member| (member.node, member.index));
        Self {
            signatures,
            parameters,
            members,
        }
    }

    /// Checks that each record names a node of a kind it belongs to, in order and at most once
    /// per node (or per index). Completeness (one signature per function, contiguous indexes,
    /// ranges) is the graph validator's.
    pub(super) fn check(&self, nodes: &[Node]) -> Result<(), FactsError> {
        let kind = |node: u32| nodes.get(node as usize).map(Node::kind);
        let bad = |what: &'static str, node: u32| FactsError::Typed { what, node };
        let mut previous = None;
        for signature in &self.signatures {
            if previous.is_some_and(|previous| previous >= signature.node)
                || kind(signature.node) != Some(NodeKind::Function)
            {
                return Err(bad("signature", signature.node));
            }
            previous = Some(signature.node);
        }
        let mut previous = None;
        for parameter in &self.parameters {
            let position = (parameter.node, parameter.index);
            if previous.is_some_and(|previous| previous >= position)
                || kind(parameter.node) != Some(NodeKind::Function)
            {
                return Err(bad("parameter", parameter.node));
            }
            previous = Some(position);
        }
        let mut previous = None;
        for member in &self.members {
            let position = (member.node, member.index);
            if previous.is_some_and(|previous| previous >= position)
                || !matches!(kind(member.node), Some(NodeKind::Class | NodeKind::Type))
            {
                return Err(bad("member", member.node));
            }
            previous = Some(position);
        }
        Ok(())
    }
}
