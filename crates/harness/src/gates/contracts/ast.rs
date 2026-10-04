//! Reachable native Rust references, not execution or semantic test evidence.
mod checks;
#[cfg(test)]
mod tests;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use syn::{Attribute, Item, Type};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_FILES: usize = 256;
const MAX_DEPTH: usize = 64;
#[derive(Serialize)]
pub(super) struct Binding {
    pub(super) path: String,
    pub(super) symbol: String,
    pub(super) kind: String,
    pub(super) line: usize,
    pub(super) file_blake3: String,
}
#[derive(Clone, Copy)]
struct Profile {
    production: bool,
    tests: bool,
    test_context: bool,
    uncertain: bool,
}
impl Profile {
    fn attributes(mut self, attrs: &[Attribute]) -> Self {
        for attr in attrs {
            if attr.path().is_ident("cfg") {
                match attr.parse_args::<syn::Meta>() {
                    Ok(syn::Meta::Path(path)) if path.is_ident("test") => {
                        self.production = false;
                        self.test_context = true;
                    }
                    Ok(syn::Meta::List(list))
                        if list.path.is_ident("not")
                            && list
                                .parse_args::<syn::Path>()
                                .is_ok_and(|path| path.is_ident("test")) =>
                    {
                        self.tests = false
                    }
                    _ => self.uncertain = true,
                }
            } else if !attr.path().get_ident().is_some_and(|name| {
                matches!(
                    name.to_string().as_str(),
                    "test"
                        | "ignore"
                        | "should_panic"
                        | "doc"
                        | "allow"
                        | "warn"
                        | "deny"
                        | "forbid"
                        | "inline"
                        | "cold"
                        | "must_use"
                        | "deprecated"
                        | "track_caller"
                )
            }) {
                self.uncertain = true;
            }
        }
        self
    }
}
fn module_supported(attrs: &[Attribute]) -> bool {
    !Profile {
        production: true,
        tests: true,
        test_context: false,
        uncertain: false,
    }
    .attributes(attrs)
    .uncertain
}
#[derive(Clone)]
struct Context<'a> {
    file: &'a Path,
    base: PathBuf,
    prefix: String,
    profile: Profile,
    depth: usize,
    masked: bool,
    text: &'a str,
}
struct Walker<'a> {
    root: &'a Path,
    target: &'a Path,
    path: &'a str,
    symbol: &'a str,
    kind: &'a str,
    files: BTreeSet<PathBuf>,
    active: BTreeSet<PathBuf>,
    found: Vec<Binding>,
    blocked: bool,
}
fn normal(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
fn regular(root: &Path, relative: &Path) -> Result<bool> {
    let mut path = root.to_path_buf();
    for part in relative.components() {
        if !matches!(part, std::path::Component::Normal(_)) {
            return Err("non-normal Rust source path".into());
        }
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(value) if value.file_type().is_symlink() => {
                return Err("linked Rust source ancestor".into());
            }
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
    }
    let value = fs::symlink_metadata(path)?;
    if !value.is_file() {
        return Err("Rust source must be regular".into());
    }
    Ok(true)
}
#[cfg(unix)]
fn identity(value: &fs::Metadata) -> (u64, u64, u64, u32, i64, i64, i64, i64) {
    use std::os::unix::fs::MetadataExt;
    (
        value.dev(),
        value.ino(),
        value.len(),
        value.mode(),
        value.mtime(),
        value.mtime_nsec(),
        value.ctime(),
        value.ctime_nsec(),
    )
}
fn source(root: &Path, relative: &Path) -> Result<String> {
    if !regular(root, relative)? {
        return Err("declared Rust source is absent".into());
    }
    let path = root.join(relative);
    let initial = fs::symlink_metadata(&path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let mut file = options.open(&path)?;
    let opened = file.metadata()?;
    if !opened.is_file() || opened.len() > MAX_BYTES {
        return Err("Rust source exceeds regular-file bound".into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Rust source exceeds 2MiB".into());
    }
    let end = fs::symlink_metadata(&path)?;
    if !end.is_file() {
        return Err("Rust source endpoint changed".into());
    }
    #[cfg(unix)]
    if identity(&initial) != identity(&opened)
        || identity(&opened) != identity(&file.metadata()?)
        || identity(&opened) != identity(&end)
    {
        return Err("Rust source handle/path identity changed".into());
    }
    #[cfg(not(unix))]
    if initial.len() != opened.len() || opened.len() != end.len() {
        return Err("Rust source size changed".into());
    }
    Ok(String::from_utf8(bytes)?)
}
fn namespace(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.into()
    } else {
        format!("{prefix}::{name}")
    }
}
fn self_name(value: &Type) -> Option<String> {
    match value {
        Type::Path(value)
            if value.qself.is_none()
                && value
                    .path
                    .segments
                    .iter()
                    .all(|segment| matches!(segment.arguments, syn::PathArguments::None)) =>
        {
            Some(
                value
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            )
        }
        _ => None,
    }
}
impl Walker<'_> {
    fn file(&mut self, path: &Path, profile: Profile, depth: usize, masked: bool) -> Result<()> {
        if depth > MAX_DEPTH || !self.active.insert(path.to_path_buf()) {
            return Err("Rust module graph depth/cycle bound".into());
        }
        self.files.insert(path.to_path_buf());
        if self.files.len() > MAX_FILES {
            return Err("Rust module graph exceeds 256 files".into());
        }
        let text = source(self.root, path)?;
        let file = syn::parse_file(&text)?;
        let profile = profile.attributes(&file.attrs);
        let parent = path.parent().ok_or("Rust source parent missing")?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Rust source name invalid")?;
        let base = if matches!(name, "lib.rs" | "main.rs" | "mod.rs") {
            parent.to_path_buf()
        } else {
            parent.join(path.file_stem().ok_or("Rust module stem absent")?)
        };
        self.items(
            &file.items,
            &Context {
                file: path,
                base,
                prefix: String::new(),
                profile,
                depth,
                masked,
                text: &text,
            },
        )?;
        self.active.remove(path);
        Ok(())
    }
    fn items(&mut self, items: &[Item], context: &Context<'_>) -> Result<()> {
        let Context {
            file,
            base,
            prefix,
            profile,
            depth,
            masked: inherited_mask,
            ..
        } = context;
        let (file, profile, depth, inherited_mask) = (*file, *profile, *depth, *inherited_mask);
        if depth > MAX_DEPTH {
            return Err("Rust inline module depth exceeds 64".into());
        }
        let masked =
            inherited_mask || checks::scope_masked(items, self.root, base, depth, &mut self.files)?;
        for item in items {
            match item {
                Item::Fn(value)
                    if file == self.target
                        && namespace(prefix, &value.sig.ident.to_string()) == self.symbol =>
                {
                    self.function(
                        (&value.sig, &value.attrs, &value.block),
                        &Context {
                            masked,
                            ..context.clone()
                        },
                        self.kind == "function" || self.kind == "test",
                    )?;
                }
                Item::Impl(value) if file == self.target => {
                    if let Some(owner) = self_name(&value.self_ty) {
                        for child in &value.items {
                            if let syn::ImplItem::Fn(method) = child {
                                let name =
                                    namespace(prefix, &format!("{owner}::{}", method.sig.ident));
                                if name == self.symbol {
                                    let inherited = profile.attributes(&value.attrs);
                                    self.function(
                                        (&method.sig, &method.attrs, &method.block),
                                        &Context {
                                            profile: inherited,
                                            masked,
                                            ..context.clone()
                                        },
                                        self.kind == "method" && value.trait_.is_none(),
                                    )?;
                                }
                            }
                        }
                    }
                }
                Item::Mod(value) => {
                    let name = value.ident.to_string();
                    let next = base.join(&name);
                    let child_profile = profile.attributes(&value.attrs);
                    if let Some((_, children)) = &value.content {
                        if file == self.target || self.target.starts_with(&next) {
                            self.items(
                                children,
                                &Context {
                                    base: next,
                                    prefix: namespace(prefix, &name),
                                    profile: child_profile,
                                    depth: depth + 1,
                                    masked,
                                    ..context.clone()
                                },
                            )?;
                        }
                    } else {
                        let flat = base.join(format!("{name}.rs"));
                        let nested = next.join("mod.rs");
                        if self.target == flat || self.target.starts_with(&next) {
                            if child_profile.uncertain {
                                self.blocked = true;
                                continue;
                            }
                            let flat_exists = regular(self.root, &flat)?;
                            let nested_exists = regular(self.root, &nested)?;
                            if flat_exists && nested_exists {
                                return Err("ambiguous declared Rust module".into());
                            }
                            let selected = if flat_exists {
                                flat
                            } else if nested_exists {
                                nested
                            } else {
                                return Err("declared Rust module absent".into());
                            };
                            self.file(&selected, child_profile, depth + 1, masked)?;
                        }
                    }
                }
                _ => (),
            }
        }
        Ok(())
    }
    fn function(
        &mut self,
        value: (&syn::Signature, &[Attribute], &syn::Block),
        context: &Context<'_>,
        right_kind: bool,
    ) -> Result<()> {
        let (sig, attrs, block) = value;
        let profile = context.profile.attributes(attrs);
        if profile.uncertain {
            self.blocked = true;
            return Ok(());
        }
        let is_test = attrs.iter().any(|attr| attr.path().is_ident("test"));
        let valid = if self.kind == "test" {
            profile.tests
                && profile.test_context
                && is_test
                && sig.inputs.is_empty()
                && sig.generics.params.is_empty()
                && sig.asyncness.is_none()
                && sig.unsafety.is_none()
                && !attrs.iter().any(|attr| {
                    attr.path().is_ident("ignore") || attr.path().is_ident("should_panic")
                })
        } else {
            profile.production && !profile.test_context && !is_test
        };
        if !right_kind || !valid {
            self.blocked = true;
            return Ok(());
        }
        if self.kind == "test" {
            checks::test(block, context.text, context.masked)
                .map_err(|error| format!("{}::{}: {error}", self.path, self.symbol))?;
        }
        self.found.push(Binding {
            path: self.path.into(),
            symbol: self.symbol.into(),
            kind: self.kind.into(),
            line: sig.ident.span().start().line,
            file_blake3: blake3::hash(context.text.as_bytes()).to_hex().to_string(),
        });
        if context.file != self.target {
            return Err("Rust binding target changed".into());
        }
        Ok(())
    }
}
pub(super) fn bind(root: &Path, path: &str, symbol: &str, kind: &str) -> Result<Binding> {
    if !normal(path)
        || symbol.is_empty()
        || symbol.len() > 512
        || !matches!(kind, "function" | "method" | "test")
    {
        return Err("invalid Rust binding path/symbol/kind".into());
    }
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() < 4 || parts[0] != "crates" || parts[2] != "src" || !path.ends_with(".rs") {
        return Err("binding must name a crate src Rust file".into());
    }
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err("linked binding root".into());
    }
    let root = fs::canonicalize(root)?;
    let target = Path::new(path);
    if !regular(&root, target)? {
        return Err("Rust binding target missing".into());
    }
    let mut walker = Walker {
        root: &root,
        target,
        path,
        symbol,
        kind,
        files: BTreeSet::new(),
        active: BTreeSet::new(),
        found: Vec::new(),
        blocked: false,
    };
    let profile = Profile {
        production: true,
        tests: true,
        test_context: false,
        uncertain: false,
    };
    for entry in ["lib.rs", "main.rs"] {
        let entry = Path::new("crates").join(parts[1]).join("src").join(entry);
        if regular(&root, &entry)? {
            walker.file(&entry, profile, 0, false)?;
        }
    }
    if walker.blocked || walker.found.len() != 1 {
        return Err("Rust binding is unreachable, unsupported, invalid or ambiguous".into());
    }
    walker
        .found
        .pop()
        .ok_or_else(|| "Rust binding missing".into())
}
