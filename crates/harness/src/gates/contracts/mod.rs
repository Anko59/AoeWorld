//! Structural contract references, never behavior/coverage or role authority.
mod ast;
mod schema;
#[cfg(test)]
mod tests;
use super::registry::Registry;
use serde_json::{Value, json};
use std::{error::Error, fs, io::Read, path::Path};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_BYTES: usize = 131072;
fn catalog_bytes(root: &Path) -> Result<Vec<u8>> {
    let mut path = root.to_path_buf();
    for component in ["gates", "contracts.json"] {
        path.push(component);
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return Err("contract catalog path must not be linked".into());
        }
    }
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err("contract catalog must be a bounded regular file".into());
    }
    let bytes = bounded(&path)?;
    if bounded(&path)? != bytes {
        return Err("contract catalog endpoint changed or exceeds128KiB".into());
    }
    Ok(bytes)
}
fn bounded(path: &Path) -> Result<Vec<u8>> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err("contract catalog handle is not regular".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err("contract catalog exceeds128KiB".into());
    }
    Ok(bytes)
}
pub(crate) fn check(root: &Path) -> Result<Value> {
    let root = fs::canonicalize(root)?;
    let bytes = catalog_bytes(&root)?;
    let registry = Registry::load(&root)?;
    let catalog = schema::Catalog::parse(&bytes, &registry)?;
    let mut bindings = Vec::new();
    for requirement in &catalog.requirements {
        for invariant in &requirement.invariants {
            for boundary in &invariant.boundaries {
                bindings.push(json!({"requirement":requirement.id,"invariant":invariant.id,"binding":ast::bind(&root,&boundary.path,&boundary.symbol,boundary.kind.name())?}));
            }
        }
    }
    for case in &catalog.cases {
        bindings.push(json!({"case":case.id,"purpose_claim":case.kind.name(),"binding":ast::bind(&root,&case.path,&case.symbol,"test")?}));
    }
    if catalog_bytes(&root)? != bytes
        || Registry::load(&root)?.fingerprint()? != registry.fingerprint()?
    {
        return Err("contract catalog/registry changed during observation".into());
    }
    let mut digest = blake3::Hasher::new();
    digest.update(b"aoeworld:contracts-raw:v1\0");
    digest.update(&bytes);
    let deferred: Vec<_> = catalog.deferred.iter().map(|item|json!({"id":item.id,"capability":item.capability,"status":"UNAVAILABLE","reason":item.reason})).collect();
    Ok(
        json!({"schema":1,"status":"STRUCTURAL_REFERENCES_VALID","authoritative":false,"semantic_test_evidence":"NOT_ASSESSED","coverage_claim":"NOT_ASSESSED","source_scope":"caller source observation; snapshot identity supplied separately by gate launcher","contracts_hash_algorithm":"blake3:aoeworld-contracts-raw-v1","contracts_blake3":digest.finalize().to_hex().to_string(),"registry_hash":registry.fingerprint()?,"requirements":catalog.requirements.len(),"bindings":bindings,"deferred":deferred,"limits":["AST reachability and limited anti-vacuity checks do not prove test behavior or detect all tautologies","test execution/coverage/mutation and protected independent review remain separate required evidence","working checks are not tracked-source, hostile-race or qualified service proof"]}),
    )
}
