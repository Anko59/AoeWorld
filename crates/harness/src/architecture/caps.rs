//! Semantic lint for world-wide behaviour and capacity caps (movement rework
//! plan P4, `docs/movement/caps.md`).
//!
//! A cap is a constant, a literal or a shared budget that a per-tick counter
//! is compared to, refilled from or truncated by. The lint finds four shapes in
//! behaviour code (production code of the simulation, the gameplay server and
//! the resource overlay), whatever the names involved:
//!
//! - `use`: a path to a constant named like a cap (`MAX_`, `PER_TICK`, ...);
//! - `cmp`: a counter-like operand compared to another constant or a literal;
//! - `take`: `.take(..)` or `.truncate(..)` of a constant or a literal;
//! - `refill`: a counter-like place assigned a constant, or a budget drained.
//!
//! `gates/movement-caps.json` holds today's findings as a ratcheting allowlist:
//! a finding past its allowance is new and fails, and an allowance past its
//! findings is stale and fails too, so each phase that removes a cap must
//! shrink the file in the same change. Nothing may ever be added to it.
use expr::{
    bound, cap_bound, cap_named, const_name, counter_like, drain_like, length, render_expr,
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
};
use syn::{
    BinOp, ExprAssign, ExprBinary, ExprMethodCall, ExprPath, ImplItemFn, ItemFn, ItemMod,
    spanned::Spanned,
    visit::{self, Visit},
};

/// Where the allowlist lives, relative to the repository root.
pub const ALLOWLIST: &str = "gates/movement-caps.json";

/// Directories (or files) holding behaviour code, relative to the root.
const SCOPES: [&str; 7] = [
    "crates/simulation/src",
    "crates/server/src/gameplay",
    "crates/server/src/gameplay.rs",
    "crates/server/src/gameplay_match",
    "crates/server/src/gameplay_match.rs",
    "crates/map/src/overlay",
    "crates/map/src/overlay.rs",
];

/// Directories under the scopes that only build workloads for benchmarks and demos.
const WORKLOAD_DIRS: [&str; 2] = ["benchmark", "demo"];

#[derive(Deserialize)]
struct Allowlist {
    entries: Vec<Allowed>,
}

#[derive(Deserialize)]
struct Allowed {
    file: String,
    kind: String,
    expr: String,
    count: usize,
}

/// One use of a cap in behaviour code.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub file: String,
    pub kind: &'static str,
    pub expr: String,
    pub line: usize,
}

/// Lints the repository at `root` and returns one message per violation.
pub fn check(root: &Path, base_source: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let findings = scan(root)?;
    let allowlist = match fs::read_to_string(root.join(ALLOWLIST)) {
        Ok(text) => serde_json::from_str::<Allowlist>(&text)
            .map_err(|error| format!("{ALLOWLIST}: {error}"))?,
        Err(_) => {
            // No allowlist at all: print today's findings as the one to commit.
            return Ok(vec![format!(
                "{ALLOWLIST} is missing; today's caps are:\n{}",
                render(&findings)
            )]);
        }
    };
    let base = ratchet::baseline(base_source)?;
    let mut problems = ratchet::compare(&allowlist, &base);
    problems.extend(compare(&findings, &allowlist));
    Ok(problems)
}

/// Every cap use in the behaviour code under `root`.
pub fn scan(root: &Path) -> Result<Vec<Finding>, Box<dyn Error>> {
    let mut findings = Vec::new();
    for scope in SCOPES {
        for path in sources(&root.join(scope))? {
            let label = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            if is_test_file(&label) || is_workload(&label) {
                continue;
            }
            findings.extend(lint(&label, &fs::read_to_string(&path)?)?);
        }
    }
    Ok(findings)
}

/// Lints one source file: `label` is its path relative to the root.
pub fn lint(label: &str, source: &str) -> Result<Vec<Finding>, Box<dyn Error>> {
    let file = syn::parse_file(source).map_err(|error| format!("{label}: {error}"))?;
    let mut finder = Finder {
        label,
        found: Vec::new(),
    };
    finder.visit_file(&file);
    Ok(finder.found)
}

/// The allowlist file's text for these findings.
pub fn render(findings: &[Finding]) -> String {
    let mut entries = BTreeMap::new();
    for finding in findings {
        *entries
            .entry((finding.file.as_str(), finding.kind, finding.expr.as_str()))
            .or_insert(0usize) += 1;
    }
    let mut out = String::from("{\n  \"entries\": [\n");
    let last = entries.len().saturating_sub(1);
    for (n, ((file, kind, expr), count)) in entries.into_iter().enumerate() {
        let entry = serde_json::json!({"file": file, "kind": kind, "expr": expr, "count": count});
        out.push_str(&format!(
            "    {entry}{}\n",
            if n == last { "" } else { "," }
        ));
    }
    out.push_str("  ]\n}\n");
    out
}

fn compare(findings: &[Finding], allowlist: &Allowlist) -> Vec<String> {
    let mut found: BTreeMap<(&str, &str, &str), Vec<usize>> = BTreeMap::new();
    for finding in findings {
        found
            .entry((finding.file.as_str(), finding.kind, finding.expr.as_str()))
            .or_default()
            .push(finding.line);
    }
    let mut allowed: BTreeMap<(&str, &str, &str), usize> = BTreeMap::new();
    for entry in &allowlist.entries {
        *allowed
            .entry((
                entry.file.as_str(),
                entry.kind.as_str(),
                entry.expr.as_str(),
            ))
            .or_insert(0) += entry.count;
    }
    let mut problems = Vec::new();
    for (key, lines) in &found {
        let allowance = allowed.get(key).copied().unwrap_or(0);
        if lines.len() > allowance {
            problems.push(format!(
                "{}:{}: new world cap `{} {}` ({} found, {allowance} allowed): behaviour code \
                 must not compare a per-tick counter to a constant; see docs/movement/caps.md",
                key.0,
                lines[allowance.min(lines.len() - 1)],
                key.1,
                key.2,
                lines.len(),
            ));
        }
    }
    for (key, allowance) in &allowed {
        let now = found.get(key).map_or(0, Vec::len);
        if now < *allowance {
            problems.push(format!(
                "{ALLOWLIST}: `{} {} {}` allows {allowance} but only {now} remain; lower or \
                 delete the entry (the allowlist only shrinks)",
                key.0, key.1, key.2,
            ));
        }
    }
    problems
}

struct Finder<'a> {
    label: &'a str,
    found: Vec<Finding>,
}

impl Finder<'_> {
    fn add(&mut self, kind: &'static str, expr: String, line: usize) {
        self.found.push(Finding {
            file: self.label.to_owned(),
            kind,
            expr,
            line,
        });
    }
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        if !test_attr(&node.attrs) {
            visit::visit_item_mod(self, node);
        }
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if !test_attr(&node.attrs) {
            visit::visit_item_fn(self, node);
        }
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        if !test_attr(&node.attrs) {
            visit::visit_impl_item_fn(self, node);
        }
    }

    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        if let Some(name) = const_name(node).filter(|name| cap_named(name)) {
            self.add("use", name, node.span().start().line);
        }
        visit::visit_expr_path(self, node);
    }

    fn visit_expr_binary(&mut self, node: &'ast ExprBinary) {
        let line = node.span().start().line;
        match node.op {
            BinOp::Lt(_)
            | BinOp::Le(_)
            | BinOp::Gt(_)
            | BinOp::Ge(_)
            | BinOp::Eq(_)
            | BinOp::Ne(_) => {
                let sides = [(&*node.left, &*node.right), (&*node.right, &*node.left)];
                for (counter, other) in sides {
                    // A counter against a constant or a literal, a length
                    // against a constant, or a drained budget against anything.
                    let counted = (counter_like(counter) && cap_bound(other).is_some())
                        || (length(counter) && bound(other, u64::MAX) == Some(false))
                        || drain_like(counter);
                    if counted {
                        let expr = format!(
                            "{} {} {}",
                            render_expr(counter),
                            op(&node.op),
                            render_expr(other)
                        );
                        self.add("cmp", expr, line);
                        break;
                    }
                }
            }
            BinOp::SubAssign(_) if drain_like(&node.left) => {
                self.add("refill", format!("{} -=", render_expr(&node.left)), line);
            }
            _ => {}
        }
        visit::visit_expr_binary(self, node);
    }

    fn visit_expr_assign(&mut self, node: &'ast ExprAssign) {
        if let Some(bound) = cap_bound(&node.right)
            && counter_like(&node.left)
        {
            let expr = format!("{} = {bound}", render_expr(&node.left));
            self.add("refill", expr, node.span().start().line);
        }
        visit::visit_expr_assign(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let method = node.method.to_string();
        if matches!(method.as_str(), "take" | "truncate")
            && node.args.len() == 1
            && let Some(argument) = node.args.first()
            && let Some(bound) = cap_bound(argument)
        {
            self.add(
                "take",
                format!(".{method}({bound})"),
                node.span().start().line,
            );
        }
        visit::visit_expr_method_call(self, node);
    }
}

fn op(op: &BinOp) -> &'static str {
    match op {
        BinOp::Lt(_) => "<",
        BinOp::Le(_) => "<=",
        BinOp::Gt(_) => ">",
        BinOp::Ge(_) => ">=",
        BinOp::Eq(_) => "==",
        _ => "!=",
    }
}

/// An item that only exists in test builds.
fn test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = attr.path();
        path.is_ident("test")
            || (path.is_ident("cfg")
                && attr
                    .meta
                    .require_list()
                    .is_ok_and(|list| list.tokens.to_string() == "test"))
    })
}

fn is_test_file(label: &str) -> bool {
    let name = label.rsplit('/').next().unwrap_or(label);
    label.split('/').any(|part| part == "tests")
        || name == "tests.rs"
        || name.ends_with("_tests.rs")
}

fn is_workload(label: &str) -> bool {
    label.split('/').any(|part| WORKLOAD_DIRS.contains(&part))
}

fn sources(path: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    if path.is_file() {
        return Ok(vec![path.to_owned()]);
    }
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            result.extend(sources(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            result.push(path);
        }
    }
    Ok(result)
}

mod expr;
mod ratchet;
#[cfg(test)]
mod tests;
