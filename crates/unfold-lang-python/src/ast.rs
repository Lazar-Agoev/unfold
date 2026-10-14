//! Small helpers over Ruff's syntax tree that every pass shares: byte ranges, child expressions
//! in source order, argument order, and string literals.

use ruff_python_ast::{
    ArgOrKeyword, Arguments, Expr, ExprCall, InterpolatedStringElement, InterpolatedStringElements,
};
use ruff_text_size::TextRange;
use unfold_core::text::ByteRange;

/// A byte range from Ruff's range.
pub(crate) fn bytes(range: TextRange) -> ByteRange {
    ByteRange {
        start: range.start().to_u32(),
        end: range.end().to_u32(),
    }
}

/// Appends the direct child expressions of `expr`, in source order. Nothing is skipped: lambda
/// defaults and bodies, comprehension parts, and f-string interpolations (including those in
/// format specifications) are all children.
pub(crate) fn expr_children<'a>(expr: &'a Expr, out: &mut impl Extend<&'a Expr>) {
    match expr {
        Expr::BoolOp(node) => out.extend(&node.values),
        Expr::Named(node) => out.extend([&*node.target, &*node.value]),
        Expr::BinOp(node) => out.extend([&*node.left, &*node.right]),
        Expr::UnaryOp(node) => out.extend([&*node.operand]),
        Expr::Lambda(node) => {
            if let Some(parameters) = &node.parameters {
                out.extend(parameters.iter().filter_map(|parameter| parameter.default()));
            }
            out.extend([&*node.body]);
        }
        // Source order puts the body before the test: `body if test else orelse`.
        Expr::If(node) => out.extend([&*node.body, &*node.test, &*node.orelse]),
        Expr::Dict(node) => {
            for item in &node.items {
                out.extend(item.key.as_ref());
                out.extend([&item.value]);
            }
        }
        Expr::Set(node) => out.extend(&node.elts),
        Expr::List(node) => out.extend(&node.elts),
        Expr::Tuple(node) => out.extend(&node.elts),
        Expr::ListComp(node) => comprehension(&node.elt, None, &node.generators, out),
        Expr::SetComp(node) => comprehension(&node.elt, None, &node.generators, out),
        Expr::Generator(node) => comprehension(&node.elt, None, &node.generators, out),
        Expr::DictComp(node) => comprehension(&node.value, node.key.as_deref(), &node.generators, out),
        Expr::Await(node) => out.extend([&*node.value]),
        Expr::Yield(node) => out.extend(node.value.as_deref()),
        Expr::YieldFrom(node) => out.extend([&*node.value]),
        Expr::Compare(node) => out.extend(node.operands.iter()),
        Expr::Call(node) => {
            out.extend([&*node.func]);
            out.extend(arguments(&node.arguments).map(ArgOrKeyword::value));
        }
        Expr::FString(node) => {
            for part in node.value.f_strings() {
                interpolations(&part.elements, out);
            }
        }
        Expr::TString(node) => {
            for part in &node.value {
                interpolations(&part.elements, out);
            }
        }
        Expr::Attribute(node) => out.extend([&*node.value]),
        Expr::Subscript(node) => out.extend([&*node.value, &*node.slice]),
        Expr::Starred(node) => out.extend([&*node.value]),
        Expr::Slice(node) => {
            out.extend(node.lower.as_deref());
            out.extend(node.upper.as_deref());
            out.extend(node.step.as_deref());
        }
        Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_)
        | Expr::Name(_)
        | Expr::IpyEscapeCommand(_) => {}
    }
}

/// The parts of a comprehension in source order: the element (a dict comprehension's key, then
/// its value), then each generator's target, iterable, and conditions.
fn comprehension<'a>(
    element: &'a Expr,
    key: Option<&'a Expr>,
    generators: &'a [ruff_python_ast::Comprehension],
    out: &mut impl Extend<&'a Expr>,
) {
    out.extend(key);
    out.extend([element]);
    for generator in generators {
        out.extend([&generator.target, &generator.iter]);
        out.extend(&generator.ifs);
    }
}

/// The expressions interpolated into an f-string or t-string, including those nested in format
/// specifications, in source order. Format specifications nest only through interpolations, so
/// an explicit stack keeps this iterative.
fn interpolations<'a>(elements: &'a InterpolatedStringElements, out: &mut impl Extend<&'a Expr>) {
    let mut stack: Vec<std::slice::Iter<'a, InterpolatedStringElement>> = vec![elements.iter()];
    while let Some(top) = stack.last_mut() {
        let Some(element) = top.next() else {
            stack.pop();
            continue;
        };
        if let InterpolatedStringElement::Interpolation(interpolation) = element {
            out.extend([&*interpolation.expression]);
            if let Some(spec) = &interpolation.format_spec {
                stack.push(spec.elements.iter());
            }
        }
    }
}

/// A call's arguments and keywords in written order (Python allows `*args` after keywords).
pub(crate) fn arguments(arguments: &Arguments) -> impl Iterator<Item = ArgOrKeyword<'_>> {
    arguments.iter_source_order()
}

/// The decoded value of a plain string literal (implicit concatenation included), or `None`
/// for anything else: bytes, f-strings, t-strings, and non-literals.
pub(crate) fn string_literal(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::StringLiteral(literal) => Some(literal.value.to_str()),
        _ => None,
    }
}

/// The single argument of a call that names a module the way `importlib.import_module(name)`
/// accepts it: exactly one argument, positional or the `name` keyword, never an expansion.
pub(crate) fn single_name_argument(call: &ExprCall) -> Option<&Expr> {
    let mut written = arguments(&call.arguments);
    let only = written.next()?;
    if written.next().is_some() {
        return None;
    }
    match only {
        ArgOrKeyword::Arg(Expr::Starred(_)) => None,
        ArgOrKeyword::Arg(expr) => Some(expr),
        ArgOrKeyword::Keyword(keyword) => keyword
            .arg
            .as_ref()
            .filter(|name| name.id.as_str() == "name")
            .map(|_| &keyword.value),
    }
}
