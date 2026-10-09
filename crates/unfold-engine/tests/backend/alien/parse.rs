//! The alien syntax: parenthesized forms of atoms, like `(fn work (param x) (call (ref x)))`.
//!
//! A form's first atom is its head; the rest are its items: atoms (names, `:options`,
//! `"literals"`) and nested forms. The parser is iterative, so deep nesting cannot overflow.

use unfold_core::ByteRange;

/// One parsed node: a form or an atom.
#[derive(Debug, Clone)]
pub struct Node<'s> {
    /// The head of a form, or the atom's text.
    pub text: &'s str,
    /// Whether this is a form.
    pub form: bool,
    /// Its byte range, from `(` through `)` for a form.
    pub range: ByteRange,
    /// The items of a form.
    pub children: Vec<usize>,
    /// The enclosing form.
    pub parent: Option<usize>,
}

/// A parsed file. Node 0 is the root, a synthetic `file` form spanning the whole text.
#[derive(Debug, Clone)]
pub struct Syntax<'s> {
    pub nodes: Vec<Node<'s>>,
    /// The start and end of every token, sorted.
    pub boundaries: Vec<u32>,
}

/// A syntax error and its offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: &'static str,
    pub offset: u32,
}

impl<'s> Syntax<'s> {
    pub fn node(&self, index: usize) -> &Node<'s> {
        &self.nodes[index]
    }

    /// Whether `index` is an atom of this kind: a plain name, an `:option`, or a `"literal"`.
    pub fn is_name(&self, index: usize) -> bool {
        let node = self.node(index);
        !node.form && !node.text.starts_with(':') && !node.text.starts_with('"')
    }

    pub fn is_option(&self, index: usize) -> bool {
        let node = self.node(index);
        !node.form && node.text.starts_with(':')
    }

    pub fn is_literal(&self, index: usize) -> bool {
        let node = self.node(index);
        !node.form && node.text.starts_with('"')
    }

    /// The form's first plain-name item, if it has one.
    pub fn name(&self, index: usize) -> Option<usize> {
        self.node(index)
            .children
            .iter()
            .copied()
            .find(|&child| self.is_name(child))
    }

    /// The form's options as `(key, value)`; a flag has no value.
    pub fn options(&self, index: usize) -> Vec<(&'s str, Option<&'s str>)> {
        self.node(index)
            .children
            .iter()
            .filter(|&&child| self.is_option(child))
            .map(|&child| {
                let text = &self.node(child).text[1..];
                match text.split_once('=') {
                    Some((key, value)) => (key, Some(value)),
                    None => (text, None),
                }
            })
            .collect()
    }

    /// The value of option `key`, or `Some("")` for a flag.
    pub fn option(&self, index: usize, key: &str) -> Option<&'s str> {
        self.options(index)
            .into_iter()
            .find(|(found, _)| *found == key)
            .map(|(_, value)| value.unwrap_or(""))
    }

    /// The form's items that are forms or literals (not names or options).
    pub fn items(&self, index: usize) -> Vec<usize> {
        self.node(index)
            .children
            .iter()
            .copied()
            .filter(|&child| self.node(child).form || self.is_literal(child))
            .collect()
    }
}

pub fn parse(text: &str) -> Result<Syntax<'_>, ParseError> {
    let len = u32::try_from(text.len()).map_err(|_| ParseError {
        message: "file too large",
        offset: 0,
    })?;
    let mut nodes = vec![Node {
        text: "file",
        form: true,
        range: ByteRange { start: 0, end: len },
        children: Vec::new(),
        parent: None,
    }];
    let mut boundaries = vec![];
    // Open forms, and whether each has read its head yet.
    let mut open: Vec<(usize, bool)> = vec![(0, true)];
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let start = index;
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        let at = start as u32;
        let error = |message| ParseError { message, offset: at };
        match byte {
            b'(' => {
                let parent = open.last().map(|&(form, _)| form).ok_or(error("unbalanced"))?;
                if !open.last().is_some_and(|&(_, head)| head) {
                    return Err(error("a form's head must be an atom"));
                }
                nodes.push(Node {
                    text: "",
                    form: true,
                    range: ByteRange { start: at, end: at },
                    children: Vec::new(),
                    parent: Some(parent),
                });
                let node = nodes.len() - 1;
                nodes[parent].children.push(node);
                open.push((node, false));
                boundaries.extend([at, at + 1]);
                index += 1;
            }
            b')' => {
                let (node, head) = open.pop().ok_or(error("unbalanced"))?;
                if node == 0 {
                    return Err(error("unexpected `)`"));
                }
                if !head {
                    return Err(error("empty form"));
                }
                nodes[node].range.end = at + 1;
                boundaries.extend([at, at + 1]);
                index += 1;
            }
            _ => {
                if byte == b'"' {
                    let close = text[index + 1..].find('"').ok_or(error("unterminated literal"))?;
                    index += close + 2;
                } else {
                    while index < bytes.len()
                        && !bytes[index].is_ascii_whitespace()
                        && !b"()\"".contains(&bytes[index])
                    {
                        index += 1;
                    }
                }
                let atom = &text[start..index];
                let end = index as u32;
                boundaries.extend([at, end]);
                let (form, head) = *open.last().ok_or(error("unbalanced"))?;
                if !head {
                    if atom.starts_with('"') || atom.starts_with(':') {
                        return Err(error("a form's head must be a name"));
                    }
                    nodes[form].text = atom;
                    open.last_mut().ok_or(error("unbalanced"))?.1 = true;
                    continue;
                }
                nodes.push(Node {
                    text: atom,
                    form: false,
                    range: ByteRange { start: at, end },
                    children: Vec::new(),
                    parent: Some(form),
                });
                let node = nodes.len() - 1;
                nodes[form].children.push(node);
            }
        }
    }
    if open.len() != 1 {
        return Err(ParseError {
            message: "unclosed form",
            offset: len,
        });
    }
    boundaries.push(0);
    boundaries.push(len);
    boundaries.sort_unstable();
    boundaries.dedup();
    Ok(Syntax { nodes, boundaries })
}
