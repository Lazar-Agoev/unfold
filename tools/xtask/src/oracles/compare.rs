//! The precision and recall gates ([docs/testing.md#choose-checks-for-a-local-change](../../../../docs/testing.md#choose-checks-for-a-local-change)):
//! every resolved call, construction, and named reference in Unfold's graph must name the
//! oracle's target, and recall is measured exactly as the proof of concept did (sites the oracle
//! resolves that Unfold resolves to the same target, over sites the oracle resolves).

use std::collections::BTreeSet;
use std::fmt;

use rustc_hash::FxHashMap;
use unfold_core::vocab::{Language, LinkKind};
use unfold_core::{FileGraph, Graph, Link, LinkTarget};

use super::file::OracleFile;
use super::justified::{Justified, JustifiedEntry};

/// A count over a total, printed as `n/m (p%)`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Ratio {
    /// The numerator.
    pub part: usize,
    /// The denominator.
    pub total: usize,
}

impl Ratio {
    /// Whether `self` is at least `floor`, compared exactly (an empty total counts as 100%).
    pub(crate) fn at_least(self, floor: Ratio) -> bool {
        let (part, total) = if self.total == 0 {
            (1, 1)
        } else {
            (self.part, self.total)
        };
        let (floor_part, floor_total) = if floor.total == 0 {
            (1, 1)
        } else {
            (floor.part, floor.total)
        };
        part * floor_total >= floor_part * total
    }

    /// Whether `self` is strictly above `floor`.
    pub(crate) fn above(self, floor: Ratio) -> bool {
        self.at_least(floor) && !floor.at_least(self)
    }

    /// The ratio as a percentage (100 for an empty total).
    pub(crate) fn percent(self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            // Corpus counts are far below 2^52, so the conversion is exact.
            #[allow(clippy::cast_precision_loss)]
            let value = self.part as f64 * 100.0 / self.total as f64;
            value
        }
    }
}

impl fmt::Display for Ratio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{} ({:.2}%)", self.part, self.total, self.percent())
    }
}

/// One resolved Unfold link whose target the oracle does not confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Disagreement {
    /// The call node.
    pub site: Box<str>,
    /// The argument position, for a reference.
    pub position: Option<u32>,
    /// Unfold's target.
    pub unfold: Box<str>,
    /// What the oracle says instead.
    pub oracle: Box<str>,
}

impl fmt::Display for Disagreement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.position {
            Some(position) => write!(f, "{} (argument {position})", self.site)?,
            None => write!(f, "{}", self.site)?,
        }
        write!(
            f,
            ": Unfold resolves {}, the oracle says {}",
            self.unfold, self.oracle
        )
    }
}

/// The comparison of one graph with one oracle file.
#[derive(Debug, Default)]
pub(crate) struct Comparison {
    /// Resolved calls and constructions that agree, over all of them.
    pub calls: Ratio,
    /// Resolved references that agree, over all of them.
    pub references: Ratio,
    /// Disagreements no justification covers.
    pub unjustified: Vec<Disagreement>,
    /// Disagreements a justification covers, with its reason.
    pub justified: Vec<(Disagreement, Box<str>)>,
    /// Justifications for this corpus and language that matched nothing.
    pub stale: Vec<JustifiedEntry>,
    /// Call recall: oracle-resolved sites Unfold resolves to the same target.
    pub recall: Ratio,
    /// Reference recall (reported, not gated).
    pub reference_recall: Ratio,
}

impl Comparison {
    /// Precision over calls, constructions, and references together, counting justified
    /// disagreements as confirmed.
    pub(crate) fn precision(&self) -> Ratio {
        Ratio {
            part: self.calls.part + self.references.part + self.justified.len(),
            total: self.calls.total + self.references.total,
        }
    }

    /// Whether precision holds: every disagreement is justified and no justification is stale.
    pub(crate) fn precise(&self) -> bool {
        self.unjustified.is_empty() && self.stale.is_empty()
    }
}

/// The written position of the argument a `passes` link names: the argument of its call whose
/// range holds the passed name.
fn argument_position(file: &FileGraph, link: &Link) -> Option<u32> {
    file.types
        .arguments
        .iter()
        .find(|argument| argument.call == link.from && argument.range.contains(link.range))
        .map(|argument| argument.index)
}

/// Compares `graph`'s `language` links with `oracle`: each call's `calls` link and each
/// `passes` link to a project definition.
pub(crate) fn compare(
    corpus: &str,
    language: Language,
    graph: &Graph,
    oracle: &OracleFile,
    justified: &Justified,
) -> Comparison {
    let calls = oracle.calls_by_site();
    let arguments = oracle.references_by_argument();
    let mut comparison = Comparison::default();
    let mut unfold_calls: FxHashMap<&str, Option<&str>> = FxHashMap::default();
    let mut unfold_references: FxHashMap<(&str, u32), &str> = FxHashMap::default();
    let mut disagreements = Vec::new();
    for file in graph.files.iter().filter(|file| file.file.language == language) {
        for link in &file.links {
            let Some(node) = file.nodes.get(link.from as usize) else {
                continue;
            };
            let site = node.key.as_str();
            let target = match &link.to {
                LinkTarget::Project(key) => Some(key.as_str()),
                _ => None,
            };
            match link.kind {
                LinkKind::Calls => {
                    unfold_calls.insert(site, target);
                    let Some(target) = target else { continue };
                    comparison.calls.total += 1;
                    let row = calls.get(site);
                    if row.and_then(|row| row.target.as_deref()) == Some(target) {
                        comparison.calls.part += 1;
                    } else {
                        let oracle = match row {
                            Some(row) => match (&row.target, &row.outcome) {
                                (Some(other), _) => format!("target {other}"),
                                (None, Some(outcome)) => format!("no target ({outcome})"),
                                (None, None) => "nothing".to_owned(),
                            },
                            None => "nothing (no call site there)".to_owned(),
                        };
                        disagreements.push(Disagreement {
                            site: site.into(),
                            position: None,
                            unfold: target.into(),
                            oracle: oracle.into(),
                        });
                    }
                }
                LinkKind::Passes => {
                    let (Some(target), Some(position)) = (target, argument_position(file, link)) else {
                        continue;
                    };
                    unfold_references.insert((site, position), target);
                    comparison.references.total += 1;
                    match arguments.get(&(site, position)) {
                        Some(row) if &*row.target == target => comparison.references.part += 1,
                        row => disagreements.push(Disagreement {
                            site: site.into(),
                            position: Some(position),
                            unfold: target.into(),
                            oracle: row
                                .map_or_else(
                                    || "no target".to_owned(),
                                    |row| format!("target {}", row.target),
                                )
                                .into(),
                        }),
                    }
                }
                LinkKind::Extends | LinkKind::Imports | LinkKind::Types => {}
            }
        }
    }
    let mut used = BTreeSet::new();
    for disagreement in disagreements {
        match justified.find(corpus, language, &disagreement) {
            Some((index, entry)) => {
                used.insert(index);
                comparison.justified.push((disagreement, entry.reason.clone()));
            }
            None => comparison.unjustified.push(disagreement),
        }
    }
    comparison.stale = justified.unused(corpus, language, &used);

    for row in &oracle.calls {
        let (Some(target), None) = (&row.target, &row.refused) else {
            continue;
        };
        comparison.recall.total += 1;
        let matched = row
            .site
            .as_deref()
            .and_then(|site| unfold_calls.get(site).copied().flatten())
            .is_some_and(|unfold| unfold == &**target);
        if matched {
            comparison.recall.part += 1;
        }
    }
    for row in oracle.references.iter().filter(|row| row.refused.is_none()) {
        comparison.reference_recall.total += 1;
        if unfold_references.get(&(&*row.site, row.position)) == Some(&&*row.target) {
            comparison.reference_recall.part += 1;
        }
    }
    comparison
}
