//! What the cap lint knows about names: which constants bound work, which places
//! count it, and a stable rendering of an expression for allowlist keys.
use syn::{BinOp, Expr, ExprPath, Lit, UnOp};

/// Name parts of a constant that bounds work or capacity.
const CAP_NAMES: [&str; 21] = [
    "MAX_",
    "_MAX",
    "PER_TICK",
    "PER_PLAYER",
    "PER_CONNECTION",
    "PER_LOOK",
    "BUDGET",
    "LIMIT",
    "CAPACITY",
    "_CAP",
    "SLOTS",
    "JOBS",
    "QUOTA",
    "ASKS",
    "VISITS",
    "ADMISSIONS",
    "PLANNERS",
    "SEARCHES",
    "INSPECTIONS",
    "CANDIDATES",
    "ACQUISITIONS",
];

/// Name parts of a per-tick counter or a shared budget.
const COUNTER_NAMES: [&str; 19] = [
    "used",
    "left",
    "budget",
    "scanned",
    "spent",
    "searches",
    "planners",
    "inspect",
    "admission",
    "taken",
    "engaged",
    "retries",
    "failures",
    "attempts",
    "pending",
    "jobs",
    "asks",
    "visits",
    "tokens",
];

/// Name parts of a counter that is only ever drained.
const DRAIN_NAMES: [&str; 3] = ["left", "budget", "searches"];

/// Names that look like counters but carry motion or rules, not a cap.
const NOT_COUNTERS: [&str; 2] = ["speed_budget", "left"];

/// Methods that do not name what they measure.
const NEUTRAL_METHODS: [&str; 11] = [
    "len",
    "min",
    "max",
    "saturating_sub",
    "saturating_add",
    "clamp",
    "get",
    "abs",
    "iter",
    "unwrap_or",
    "is_some",
];

pub(super) fn cap_named(name: &str) -> bool {
    CAP_NAMES.iter().any(|part| name.contains(part))
}

fn upper_case(name: &str) -> bool {
    name.len() > 1
        && name.chars().any(|c| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// The upper-case constant a path names, if it is one.
pub(super) fn const_name(path: &ExprPath) -> Option<String> {
    let last = path.path.segments.last()?.ident.to_string();
    upper_case(&last).then_some(last)
}

/// Whether the expression is only constants and literals of at least 2, and
/// whether one of those constants is named like a cap (the `use` shape's).
pub(super) fn bound(expr: &Expr, least: u64) -> Option<bool> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(int) => int
                .base10_parse::<u64>()
                .is_ok_and(|value| value >= least)
                .then_some(false),
            _ => None,
        },
        Expr::Path(path) => const_name(path).map(|name| cap_named(&name)),
        Expr::Paren(inner) => bound(&inner.expr, least),
        Expr::Cast(cast) => bound(&cast.expr, least),
        Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => bound(&unary.expr, least),
        Expr::Binary(binary) => Some(bound(&binary.left, least)? | bound(&binary.right, least)?),
        _ => None,
    }
}

/// The rendering of a bound that names no cap constant: the `use` shape
/// already holds those.
fn plain_bound(expr: &Expr, least: u64) -> Option<String> {
    (bound(expr, least) == Some(false)).then(|| render_expr(expr))
}

/// A bound of at least 2, which is how a literal cap differs from a flag.
pub(super) fn cap_bound(expr: &Expr) -> Option<String> {
    plain_bound(expr, 2)
}

/// Whether the expression is a collection length.
pub(super) fn length(expr: &Expr) -> bool {
    matches!(expr, Expr::MethodCall(call) if call.method == "len" && call.args.is_empty())
}

fn names(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Path(path) => {
            if let Some(segment) = path.path.segments.last() {
                out.push(segment.ident.to_string());
            }
        }
        Expr::Field(field) => {
            if let syn::Member::Named(name) = &field.member {
                out.push(name.to_string());
            }
            names(&field.base, out);
        }
        Expr::MethodCall(call) => {
            let name = call.method.to_string();
            if !NEUTRAL_METHODS.contains(&name.as_str()) {
                out.push(name);
            }
            names(&call.receiver, out);
        }
        Expr::Paren(inner) => names(&inner.expr, out),
        Expr::Cast(cast) => names(&cast.expr, out),
        Expr::Unary(unary) => names(&unary.expr, out),
        Expr::Reference(reference) => names(&reference.expr, out),
        Expr::Index(index) => names(&index.expr, out),
        Expr::Binary(binary) => {
            names(&binary.left, out);
            names(&binary.right, out);
        }
        Expr::Call(call) => {
            names(&call.func, out);
            for argument in &call.args {
                names(argument, out);
            }
        }
        _ => {}
    }
}

fn named_like(expr: &Expr, parts: &[&str]) -> bool {
    let mut found = Vec::new();
    names(expr, &mut found);
    found.iter().any(|name| {
        let lower = name.to_lowercase();
        name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
            && !NOT_COUNTERS.contains(&name.as_str())
            && parts.iter().any(|part| lower.contains(part))
    })
}

pub(super) fn counter_like(expr: &Expr) -> bool {
    named_like(expr, &COUNTER_NAMES)
}

pub(super) fn drain_like(expr: &Expr) -> bool {
    named_like(expr, &DRAIN_NAMES)
}

/// A stable rendering for allowlist keys: no line numbers, no spacing.
pub(super) fn render_expr(expr: &Expr) -> String {
    let list = |args: &syn::punctuated::Punctuated<Expr, syn::Token![,]>| {
        args.iter().map(render_expr).collect::<Vec<_>>().join(", ")
    };
    match expr {
        Expr::Path(path) => path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(int) => int.base10_digits().to_owned(),
            _ => "_".to_owned(),
        },
        Expr::Field(field) => {
            let member = match &field.member {
                syn::Member::Named(name) => name.to_string(),
                syn::Member::Unnamed(index) => index.index.to_string(),
            };
            format!("{}.{member}", render_expr(&field.base))
        }
        Expr::MethodCall(call) => format!(
            "{}.{}({})",
            render_expr(&call.receiver),
            call.method,
            list(&call.args)
        ),
        Expr::Call(call) => format!("{}({})", render_expr(&call.func), list(&call.args)),
        Expr::Paren(inner) => format!("({})", render_expr(&inner.expr)),
        Expr::Cast(cast) => render_expr(&cast.expr),
        Expr::Reference(reference) => render_expr(&reference.expr),
        Expr::Unary(unary) => {
            let sign = match unary.op {
                UnOp::Deref(_) => "*",
                UnOp::Not(_) => "!",
                _ => "-",
            };
            format!("{sign}{}", render_expr(&unary.expr))
        }
        Expr::Index(index) => format!("{}[_]", render_expr(&index.expr)),
        Expr::Binary(binary) => format!(
            "{} {} {}",
            render_expr(&binary.left),
            arithmetic(&binary.op),
            render_expr(&binary.right)
        ),
        _ => "_".to_owned(),
    }
}

fn arithmetic(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add(_) => "+",
        BinOp::Sub(_) => "-",
        BinOp::Mul(_) => "*",
        BinOp::Div(_) => "/",
        BinOp::Rem(_) => "%",
        BinOp::Shl(_) => "<<",
        BinOp::Shr(_) => ">>",
        _ => "?",
    }
}
