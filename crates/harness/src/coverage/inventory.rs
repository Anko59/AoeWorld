use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use syn::spanned::Spanned;

pub(crate) type SourceInventory = BTreeMap<String, BTreeSet<usize>>;

struct ModuleContext<'a> {
    source_path: &'a Path,
    module_directory: &'a Path,
    path_attribute_base: &'a Path,
}

pub(crate) fn expected_sources(root: &Path) -> Result<SourceInventory, Box<dyn Error>> {
    let root = root.canonicalize()?;
    let candidates = source_candidates(&root)?;
    let roots = crate_roots(&root, &candidates)?;
    let mut inventory = SourceInventory::new();
    let mut visited = BTreeSet::new();
    for path in roots {
        walk_file(
            &root,
            &candidates,
            &path,
            path.parent().ok_or("crate root has no parent")?,
            &mut visited,
            &mut inventory,
        )?;
    }
    Ok(inventory)
}

fn crate_roots(
    root: &Path,
    candidates: &BTreeSet<PathBuf>,
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let manifest = root.join("Cargo.toml");
    if manifest.is_file() {
        let manifest = manifest.to_str().ok_or("non-UTF-8 Cargo manifest path")?;
        let output = Command::new("cargo")
            .args([
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
                "--locked",
                "--offline",
                "--manifest-path",
                manifest,
            ])
            .current_dir(root)
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let mut roots = BTreeSet::new();
        for package in metadata["packages"]
            .as_array()
            .ok_or("cargo metadata has no packages")?
        {
            for target in package["targets"]
                .as_array()
                .ok_or("cargo metadata target list missing")?
            {
                let production_target = production_target_kind(target)?;
                if !production_target {
                    continue;
                }
                let source = target["src_path"]
                    .as_str()
                    .ok_or("cargo target has no source")?;
                let path = Path::new(source).canonicalize()?;
                if is_excluded_scope(&path, root) {
                    continue;
                }
                if !candidates.contains(&path) {
                    return Err(format!(
                        "cargo target source is not an inventoried crate: {source}"
                    )
                    .into());
                }
                roots.insert(path);
            }
        }
        return Ok(roots.into_iter().collect());
    }
    Ok(candidates
        .iter()
        .filter(|path| is_conventional_crate_root(path, root))
        .cloned()
        .collect())
}

fn is_excluded_scope(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    relative.starts_with(Path::new("crates/client/src"))
        || relative.starts_with(Path::new("crates/rendering/src"))
}

fn production_target_kind(target: &serde_json::Value) -> Result<bool, Box<dyn Error>> {
    let kinds = target["kind"]
        .as_array()
        .ok_or("cargo metadata target kind list missing")?;
    let mut production = false;
    for kind in kinds {
        match kind.as_str() {
            Some("lib" | "bin" | "rlib" | "staticlib" | "cdylib" | "dylib" | "proc-macro") => {
                production = true
            }
            Some("example" | "test" | "bench" | "custom-build") => {}
            Some(other) => {
                return Err(format!("unrecognized Cargo target kind: {other}").into());
            }
            None => return Err("Cargo target kind is not a string".into()),
        }
    }
    Ok(production)
}

fn source_candidates(root: &Path) -> Result<BTreeSet<PathBuf>, Box<dyn Error>> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "crates",
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err("cannot inventory first-party Rust sources".into());
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|item| !item.is_empty())
        .map(|item| String::from_utf8_lossy(item).to_string())
        .filter(|name| {
            name.starts_with("crates/") && name.contains("/src/") && name.ends_with(".rs")
        })
        .map(|name| {
            let path = root.join(name).canonicalize()?;
            Ok(path)
        })
        .filter(|path| match path {
            Ok(path) => !is_excluded_scope(path, root),
            Err(_) => true,
        })
        .collect()
}

fn is_conventional_crate_root(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let components = relative.components().collect::<Vec<_>>();
    let Some(src_index) = components
        .iter()
        .position(|component| component.as_os_str() == "src")
    else {
        return false;
    };
    let after_src = &components[src_index + 1..];
    after_src.len() == 1
        && matches!(
            after_src[0].as_os_str().to_str(),
            Some("lib.rs" | "main.rs")
        )
        || after_src.len() == 2
            && after_src[0].as_os_str() == "bin"
            && after_src[1]
                .as_os_str()
                .to_str()
                .is_some_and(|name| name.ends_with(".rs"))
        || after_src.len() == 3
            && after_src[0].as_os_str() == "bin"
            && after_src[2].as_os_str() == "main.rs"
}

fn walk_file(
    root: &Path,
    candidates: &BTreeSet<PathBuf>,
    path: &Path,
    module_directory: &Path,
    visited: &mut BTreeSet<PathBuf>,
    inventory: &mut SourceInventory,
) -> Result<(), Box<dyn Error>> {
    let path = path.canonicalize()?;
    if !candidates.contains(&path) || !visited.insert(path.clone()) {
        return Ok(());
    }
    let syntax = syn::parse_file(&fs::read_to_string(&path)?)?;
    let mut lines = BTreeSet::new();
    let context = ModuleContext {
        source_path: &path,
        module_directory,
        path_attribute_base: path.parent().ok_or("source file has no parent")?,
    };
    collect_items(
        root,
        candidates,
        &context,
        &syntax.items,
        &mut lines,
        visited,
        inventory,
    )?;
    if !lines.is_empty() {
        let relative = path
            .strip_prefix(root)?
            .to_str()
            .ok_or("non-UTF-8 source path")?;
        inventory.insert(relative.to_owned(), lines);
    }
    Ok(())
}

fn collect_items(
    root: &Path,
    candidates: &BTreeSet<PathBuf>,
    context: &ModuleContext<'_>,
    items: &[syn::Item],
    lines: &mut BTreeSet<usize>,
    visited: &mut BTreeSet<PathBuf>,
    inventory: &mut SourceInventory,
) -> Result<(), Box<dyn Error>> {
    for item in items {
        match item {
            syn::Item::Fn(item) if !cfg_test_only(&item.attrs) => add_span(lines, item.span()),
            syn::Item::Macro(item) if !cfg_test_only(&item.attrs) => add_span(lines, item.span()),
            syn::Item::Impl(item) if !cfg_test_only(&item.attrs) => {
                for child in &item.items {
                    match child {
                        syn::ImplItem::Fn(child) if !cfg_test_only(&child.attrs) => {
                            add_span(lines, child.span())
                        }
                        syn::ImplItem::Macro(child) if !cfg_test_only(&child.attrs) => {
                            add_span(lines, child.span())
                        }
                        _ => {}
                    }
                }
            }
            syn::Item::Trait(item) if !cfg_test_only(&item.attrs) => {
                for child in &item.items {
                    match child {
                        syn::TraitItem::Fn(child)
                            if child.default.is_some() && !cfg_test_only(&child.attrs) =>
                        {
                            add_span(lines, child.span())
                        }
                        syn::TraitItem::Macro(child) if !cfg_test_only(&child.attrs) => {
                            add_span(lines, child.span())
                        }
                        _ => {}
                    }
                }
            }
            syn::Item::Mod(item) if !cfg_test_only(&item.attrs) => {
                if let Some((_, nested)) = &item.content {
                    let inline_directory = context.module_directory.join(item.ident.to_string());
                    let nested_context = ModuleContext {
                        source_path: context.source_path,
                        module_directory: &inline_directory,
                        path_attribute_base: &inline_directory,
                    };
                    collect_items(
                        root,
                        candidates,
                        &nested_context,
                        nested,
                        lines,
                        visited,
                        inventory,
                    )?;
                } else {
                    let (path, child_module_directory) = resolve_module(
                        context.source_path,
                        context.module_directory,
                        context.path_attribute_base,
                        item,
                        candidates,
                    )?;
                    walk_file(
                        root,
                        candidates,
                        &path,
                        &child_module_directory,
                        visited,
                        inventory,
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn resolve_module(
    source_path: &Path,
    module_directory: &Path,
    path_attribute_base: &Path,
    item: &syn::ItemMod,
    candidates: &BTreeSet<PathBuf>,
) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    if let Some(path) = item.attrs.iter().find(|attr| attr.path().is_ident("path")) {
        let syn::Meta::NameValue(value) = &path.meta else {
            return Err(format!(
                "unsupported path attribute on module {} in {}",
                item.ident,
                source_path.display()
            )
            .into());
        };
        let syn::Expr::Lit(expr) = &value.value else {
            return Err(format!(
                "non-literal path attribute on module {} in {}",
                item.ident,
                source_path.display()
            )
            .into());
        };
        let syn::Lit::Str(path) = &expr.lit else {
            return Err(format!(
                "non-string path attribute on module {} in {}",
                item.ident,
                source_path.display()
            )
            .into());
        };
        let path = candidate_path(&path_attribute_base.join(path.value()), candidates).ok_or_else(
            || -> Box<dyn Error> {
                format!(
                    "unresolved production module {} in {}",
                    item.ident,
                    source_path.display()
                )
                .into()
            },
        )?;
        let directory = path.parent().ok_or("path module has no parent")?.to_owned();
        return Ok((path, directory));
    }
    let name = item.ident.to_string();
    let file = candidate_path(&module_directory.join(format!("{name}.rs")), candidates);
    let module = candidate_path(&module_directory.join(&name).join("mod.rs"), candidates);
    match (file, module) {
        (Some(path), None) | (None, Some(path)) => Ok((path, module_directory.join(name))),
        (Some(_), Some(_)) => Err(format!(
            "ambiguous production module {} in {}",
            item.ident,
            source_path.display()
        )
        .into()),
        (None, None) => Err(format!(
            "unresolved production module {} in {}",
            item.ident,
            source_path.display()
        )
        .into()),
    }
}

fn candidate_path(path: &Path, candidates: &BTreeSet<PathBuf>) -> Option<PathBuf> {
    let canonical = path.canonicalize().ok()?;
    candidates.contains(&canonical).then_some(canonical)
}

fn cfg_test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        list.path.is_ident("cfg")
            && list
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )
                .map(|metas| metas.iter().any(cfg_test_only_meta))
                .unwrap_or(false)
    })
}

fn cfg_test_only_meta(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) if list.path.is_ident("all") => nested_meta(list)
            .map(|metas| metas.iter().any(cfg_test_only_meta))
            .unwrap_or(false),
        syn::Meta::List(list) if list.path.is_ident("any") => nested_meta(list)
            .map(|metas| metas.iter().all(cfg_test_only_meta))
            .unwrap_or(false),
        _ => false,
    }
}

fn nested_meta(
    list: &syn::MetaList,
) -> Result<syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>, syn::Error> {
    list.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
}

fn add_span(lines: &mut BTreeSet<usize>, span: proc_macro2::Span) {
    let start = span.start().line;
    let end = span.end().line.max(start);
    lines.extend(start..=end);
}

#[cfg(test)]
#[path = "tests/inventory.rs"]
mod tests;
