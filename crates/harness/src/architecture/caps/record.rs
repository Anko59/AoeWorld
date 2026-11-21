//! Thresholds of one record (`docs/movement/caps.md` §5). A constant whose doc
//! comment has the line `Record threshold: unit` (or `cell`, or `passage`)
//! bounds one unit, one tile cell or one passage, not the world or the tick.
//! It is accepted only where it is compared with a field of one such record;
//! anywhere else, and every unmarked constant, is linted as before.
use super::expr::const_name;
use std::collections::BTreeSet;
use syn::{
    Attribute, Expr, ExprLit, ImplItemConst, ItemConst, Lit, Meta,
    visit::{self, Visit},
};

pub(super) const MARKER: &str = "Record threshold:";
const RECORDS: [&str; 3] = ["unit", "cell", "passage"];

/// The names of the constants `file` marks as thresholds of one record.
pub(super) fn marked(file: &syn::File) -> BTreeSet<String> {
    let mut finder = Marks::default();
    finder.visit_file(file);
    finder.0
}

#[derive(Default)]
struct Marks(BTreeSet<String>);

impl<'ast> Visit<'ast> for Marks {
    fn visit_item_const(&mut self, node: &'ast ItemConst) {
        if marks(&node.attrs) {
            self.0.insert(node.ident.to_string());
        }
        visit::visit_item_const(self, node);
    }

    fn visit_impl_item_const(&mut self, node: &'ast ImplItemConst) {
        if marks(&node.attrs) {
            self.0.insert(node.ident.to_string());
        }
        visit::visit_impl_item_const(self, node);
    }
}

fn marks(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let Meta::NameValue(doc) = &attr.meta else {
            return false;
        };
        let Expr::Lit(ExprLit {
            lit: Lit::Str(text),
            ..
        }) = &doc.value
        else {
            return false;
        };
        attr.path().is_ident("doc")
            && text
                .value()
                .trim()
                .strip_prefix(MARKER)
                .is_some_and(|rest| RECORDS.contains(&rest.trim().trim_end_matches('.')))
    })
}

/// Whether `expr` is a marked constant (possibly cast or scaled by a literal).
pub(super) fn marked_constant(expr: &Expr, marked: &BTreeSet<String>) -> bool {
    match expr {
        Expr::Path(path) => const_name(path).is_some_and(|name| marked.contains(&name)),
        Expr::Paren(inner) => marked_constant(&inner.expr, marked),
        Expr::Cast(cast) => marked_constant(&cast.expr, marked),
        Expr::Binary(binary) => {
            (marked_constant(&binary.left, marked) && literal(&binary.right))
                || (literal(&binary.left) && marked_constant(&binary.right, marked))
        }
        _ => false,
    }
}

fn literal(expr: &Expr) -> bool {
    matches!(expr, Expr::Lit(_))
}

/// Whether `expr` reads a field of one record: a field chain (or its length)
/// that goes through an index, or that starts at a binding other than `self`.
/// `self.counter` and `self.a.b` are world state, never one record.
pub(super) fn record_field(expr: &Expr) -> bool {
    match expr {
        Expr::Paren(inner) => record_field(&inner.expr),
        Expr::Cast(cast) => record_field(&cast.expr),
        Expr::MethodCall(call) if call.method == "len" && call.args.is_empty() => {
            record_field(&call.receiver)
        }
        Expr::Field(field) => record_root(&field.base, false),
        _ => false,
    }
}

fn record_root(expr: &Expr, indexed: bool) -> bool {
    match expr {
        Expr::Field(field) => record_root(&field.base, indexed),
        Expr::Index(index) => record_root(&index.expr, true),
        Expr::Paren(inner) => record_root(&inner.expr, indexed),
        Expr::Reference(reference) => record_root(&reference.expr, indexed),
        Expr::Unary(unary) => record_root(&unary.expr, indexed),
        Expr::Path(path) => indexed || !path.path.is_ident("self"),
        _ => indexed,
    }
}
