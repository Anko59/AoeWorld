//! Limited structural assertion checks. This cannot certify behavior or coverage.
use super::Result;
use syn::{Expr, Item, Stmt, UseTree, spanned::Spanned};
fn assertion(name: &str) -> bool {
    matches!(name, "assert" | "assert_eq" | "assert_ne")
}
fn imported(tree: &UseTree) -> bool {
    match tree {
        UseTree::Name(value) => assertion(&value.ident.to_string()),
        UseTree::Rename(value) => assertion(&value.rename.to_string()),
        UseTree::Path(value)
            if matches!(value.tree.as_ref(), UseTree::Glob(_))
                && matches!(
                    value.ident.to_string().as_str(),
                    "super" | "self" | "std" | "core"
                ) =>
        {
            false
        }
        UseTree::Path(value) => imported(&value.tree),
        UseTree::Group(value) => value.items.iter().any(imported),
        UseTree::Glob(_) => true,
    }
}
pub(super) fn masked(items: &[Item]) -> bool {
    items.iter().any(|item| match item {
        Item::Macro(value) => value
            .ident
            .as_ref()
            .is_some_and(|name| assertion(&name.to_string())),
        Item::Use(value) => imported(&value.tree),
        _ => false,
    })
}
fn named_import(tree: &UseTree) -> bool {
    match tree {
        UseTree::Name(value) => assertion(&value.ident.to_string()),
        UseTree::Rename(value) => assertion(&value.rename.to_string()),
        UseTree::Path(value) => named_import(&value.tree),
        UseTree::Group(value) => value.items.iter().any(named_import),
        UseTree::Glob(_) => false,
    }
}
fn globs(tree: &UseTree, prefix: &[String], found: &mut Vec<Vec<String>>) {
    match tree {
        UseTree::Path(value) => {
            let mut next = prefix.to_vec();
            next.push(value.ident.to_string());
            globs(&value.tree, &next, found);
        }
        UseTree::Group(value) => {
            for tree in &value.items {
                globs(tree, prefix, found);
            }
        }
        UseTree::Glob(_) => found.push(prefix.to_vec()),
        _ => (),
    }
}
// A local glob is supported only after reading its declared module, not by name.
// Ancestor super/self globs retain the already-checked inherited mask. External
// and crate-wide glob export resolution remains unsupported, never guessed safe.
pub(super) fn scope_masked(
    items: &[Item],
    root: &std::path::Path,
    base: &std::path::Path,
    depth: usize,
    files: &mut std::collections::BTreeSet<std::path::PathBuf>,
) -> Result<bool> {
    scope_masked_inner(items, root, base, depth, files, &mut 0)
}
fn scope_masked_inner(
    items: &[Item],
    root: &std::path::Path,
    base: &std::path::Path,
    depth: usize,
    files: &mut std::collections::BTreeSet<std::path::PathBuf>,
    work: &mut usize,
) -> Result<bool> {
    *work += 1;
    if *work > super::MAX_FILES || depth > super::MAX_DEPTH {
        return Err("assertion import graph exceeds work/depth bound".into());
    }
    let mut imports = Vec::new();
    for item in items {
        match item {
            Item::Macro(value)
                if value
                    .ident
                    .as_ref()
                    .is_some_and(|name| assertion(&name.to_string())) =>
            {
                return Ok(true);
            }
            Item::Use(value) => {
                if named_import(&value.tree) {
                    return Ok(true);
                }
                let mut found = Vec::new();
                globs(&value.tree, &[], &mut found);
                if value.leading_colon.is_some() && !found.is_empty() {
                    return Ok(true);
                }
                imports.extend(found);
            }
            _ => (),
        }
    }
    for import in imports {
        if import.len() == 1 && matches!(import[0].as_str(), "super" | "self") {
            continue;
        }
        if import.is_empty() {
            return Ok(true);
        }
        let mut current = items.to_vec();
        let mut directory = base.to_path_buf();
        let mut import_depth = depth;
        for name in import {
            *work += 1;
            if *work > super::MAX_FILES {
                return Err("assertion import graph exceeds work bound".into());
            }
            import_depth += 1;
            if import_depth > super::MAX_DEPTH {
                return Err("assertion import graph exceeds depth bound".into());
            }
            let modules: Vec<_> = current
                .iter()
                .filter_map(|item| match item {
                    Item::Mod(value) if value.ident == name => Some(value),
                    _ => None,
                })
                .collect();
            if modules.len() != 1 {
                return Ok(true);
            }
            let module = modules[0];
            if !super::module_supported(&module.attrs) {
                return Ok(true);
            }
            let next = directory.join(&name);
            if let Some((_, children)) = &module.content {
                current = children.clone();
            } else {
                let flat = directory.join(format!("{name}.rs"));
                let nested = next.join("mod.rs");
                let flat_exists = super::regular(root, &flat)?;
                let nested_exists = super::regular(root, &nested)?;
                if flat_exists == nested_exists {
                    return Ok(true);
                }
                let path = if flat_exists { flat } else { nested };
                files.insert(path.clone());
                if files.len() > super::MAX_FILES {
                    return Err("assertion import graph exceeds file bound".into());
                }
                let parsed = syn::parse_file(&super::source(root, &path)?)?;
                if !super::module_supported(&parsed.attrs) {
                    return Ok(true);
                }
                current = parsed.items;
            }
            directory = next;
        }
        if scope_masked_inner(&current, root, &directory, import_depth, files, work)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn enabled(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().all(|attr| {
        if attr.path().is_ident("cfg") {
            attr.parse_args::<syn::Path>()
                .is_ok_and(|path| path.is_ident("test"))
        } else {
            attr.path().get_ident().is_some_and(|name| {
                matches!(
                    name.to_string().as_str(),
                    "allow" | "warn" | "deny" | "forbid" | "doc"
                )
            })
        }
    })
}
fn literal(expr: &Expr) -> bool {
    match expr {
        Expr::Lit(_) => true,
        Expr::Paren(value) => literal(&value.expr),
        Expr::Group(value) => literal(&value.expr),
        Expr::Unary(value) => literal(&value.expr),
        Expr::Binary(value) => literal(&value.left) && literal(&value.right),
        _ => false,
    }
}
fn truth(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Lit(value) => match &value.lit {
            syn::Lit::Bool(value) => Some(value.value),
            _ => None,
        },
        Expr::Paren(value) => truth(&value.expr),
        Expr::Group(value) => truth(&value.expr),
        Expr::Unary(value) if matches!(value.op, syn::UnOp::Not(_)) => {
            truth(&value.expr).map(|value| !value)
        }
        Expr::Binary(value) if matches!(value.op, syn::BinOp::Or(_)) => {
            match (truth(&value.left), truth(&value.right)) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            }
        }
        Expr::Binary(value) if matches!(value.op, syn::BinOp::And(_)) => {
            match (truth(&value.left), truth(&value.right)) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            }
        }
        _ => None,
    }
}
fn offset(text: &str, position: proc_macro2::LineColumn) -> Result<usize> {
    if position.line == 0 {
        return Err("assertion span unavailable".into());
    }
    let mut start = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if index + 1 == position.line {
            if position.column > line.len() {
                return Err("assertion column outside source".into());
            }
            return Ok(start + position.column);
        }
        start += line.len();
    }
    Err("assertion span outside source".into())
}
fn spelling(expr: &Expr, text: &str) -> Result<String> {
    let span = expr.span();
    let start = offset(text, span.start())?;
    let end = offset(text, span.end())?;
    let source = text
        .get(start..end)
        .ok_or("assertion source span invalid")?;
    Ok(source.parse::<proc_macro2::TokenStream>()?.to_string())
}
fn check_macro(value: &syn::Macro, text: &str) -> Result<usize> {
    let Some(name) = value.path.get_ident() else {
        return Ok(0);
    };
    let name = name.to_string();
    if !assertion(&name) {
        return Ok(0);
    }
    use syn::parse::Parser;
    let args = syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated
        .parse2(value.tokens.clone())?;
    let mut args = args.iter();
    let first = args.next().ok_or("empty assertion payload")?;
    if name == "assert" {
        if literal(first) || truth(first).is_some() {
            return Err("constant assertion is not a contract check".into());
        }
        if let Expr::Binary(value) = first
            && spelling(&value.left, text)? == spelling(&value.right, text)?
        {
            return Err("identical assertion operands are not a contract check".into());
        }
        if let Expr::Macro(value) = first {
            if value.mac.path.is_ident("matches") {
                let tokens = value.mac.tokens.clone();
                let subject = (|input: syn::parse::ParseStream<'_>| -> syn::Result<Expr> {
                    let subject = input.parse::<Expr>()?;
                    input.parse::<syn::Token![,]>()?;
                    let _pattern = input.parse::<proc_macro2::TokenStream>()?;
                    Ok(subject)
                })
                .parse2(tokens)?;
                if literal(&subject) {
                    return Err("constant matches assertion is not a contract check".into());
                }
            } else {
                return Err("unsupported assertion predicate macro".into());
            }
        }
    } else {
        let second = args
            .next()
            .ok_or("comparison assertion requires two operands")?;
        if (literal(first) && literal(second)) || spelling(first, text)? == spelling(second, text)?
        {
            return Err("constant or identical comparison is not a contract check".into());
        }
    }
    Ok(1)
}
fn block(value: &syn::Block, text: &str) -> Result<usize> {
    if value.stmts.iter().any(|stmt| match stmt {
        Stmt::Item(item) => masked(std::slice::from_ref(item)),
        _ => false,
    }) {
        return Err("locally masked assertion macro is unsupported".into());
    }
    let mut count = 0;
    for stmt in &value.stmts {
        match stmt {
            Stmt::Expr(value, _) => {
                count += expression(value, text)?;
                if matches!(value, Expr::Return(_) | Expr::Break(_) | Expr::Continue(_)) {
                    break;
                }
            }
            Stmt::Macro(value) if enabled(&value.attrs) => count += check_macro(&value.mac, text)?,
            Stmt::Local(value) if enabled(&value.attrs) => {
                if let Some(value) = &value.init {
                    count += expression(&value.expr, text)?;
                }
            }
            _ => (),
        }
    }
    Ok(count)
}
fn expression(value: &Expr, text: &str) -> Result<usize> {
    Ok(match value {
        Expr::Macro(value) if enabled(&value.attrs) => check_macro(&value.mac, text)?,
        Expr::Block(value) if enabled(&value.attrs) => block(&value.block, text)?,
        Expr::Unsafe(value) if enabled(&value.attrs) => block(&value.block, text)?,
        Expr::TryBlock(value) if enabled(&value.attrs) => block(&value.block, text)?,
        Expr::If(value) if enabled(&value.attrs) => {
            let condition = expression(&value.cond, text)?;
            let then = if truth(&value.cond) == Some(false) {
                0
            } else {
                block(&value.then_branch, text)?
            };
            let other = if truth(&value.cond) == Some(true) {
                0
            } else {
                match &value.else_branch {
                    Some((_, value)) => expression(value, text)?,
                    None => 0,
                }
            };
            condition + then + other
        }
        Expr::ForLoop(value) if enabled(&value.attrs) => {
            expression(&value.expr, text)? + block(&value.body, text)?
        }
        Expr::While(value) if enabled(&value.attrs) => {
            expression(&value.cond, text)?
                + if truth(&value.cond) == Some(false) {
                    0
                } else {
                    block(&value.body, text)?
                }
        }
        Expr::Loop(value) if enabled(&value.attrs) => block(&value.body, text)?,
        Expr::Match(value) if enabled(&value.attrs) => {
            let mut count = expression(&value.expr, text)?;
            for arm in &value.arms {
                if enabled(&arm.attrs) {
                    if let Some((_, guard)) = &arm.guard {
                        count += expression(guard, text)?;
                    }
                    count += expression(&arm.body, text)?;
                }
            }
            count
        }
        Expr::Call(value) => {
            let mut count = expression(&value.func, text)?;
            for arg in &value.args {
                count += expression(arg, text)?;
            }
            count
        }
        Expr::MethodCall(value) => {
            let mut count = expression(&value.receiver, text)?;
            for arg in &value.args {
                count += expression(arg, text)?;
            }
            count
        }
        Expr::Paren(value) => expression(&value.expr, text)?,
        Expr::Group(value) => expression(&value.expr, text)?,
        Expr::Unary(value) => expression(&value.expr, text)?,
        Expr::Reference(value) => expression(&value.expr, text)?,
        Expr::Try(value) => expression(&value.expr, text)?,
        Expr::Binary(value) => {
            let skip = (matches!(value.op, syn::BinOp::And(_))
                && truth(&value.left) == Some(false))
                || (matches!(value.op, syn::BinOp::Or(_)) && truth(&value.left) == Some(true));
            expression(&value.left, text)?
                + if skip {
                    0
                } else {
                    expression(&value.right, text)?
                }
        }
        Expr::Assign(value) => expression(&value.left, text)? + expression(&value.right, text)?,
        Expr::Field(value) => expression(&value.base, text)?,
        Expr::Index(value) => expression(&value.expr, text)? + expression(&value.index, text)?,
        // Closures, async bodies and nested items do not prove execution of checks.
        _ => 0,
    })
}
pub(super) fn test(value: &syn::Block, text: &str, inherited_mask: bool) -> Result<()> {
    if inherited_mask {
        return Err("masked assertion macros are unsupported".into());
    }
    if block(value, text)? == 0 {
        return Err("test has no recognized nonconstant assertion check".into());
    }
    Ok(())
}
