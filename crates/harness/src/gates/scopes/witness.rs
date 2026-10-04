use super::*;
use std::collections::BTreeMap;

pub(super) type Objects = BTreeMap<(String, String), String>;

pub(super) fn referenced_objects(source: &Path, kind: &Kind, observed: &Probe) -> Result<Objects> {
    let mut objects = BTreeMap::new();
    let mut add = |object: &str, kind: &str| -> Result<()> {
        let bytes = git(source, &["cat-file", kind, object], None)?;
        objects.insert(
            (object.to_owned(), kind.to_owned()),
            blake3::hash(&bytes).to_hex().to_string(),
        );
        Ok(())
    };
    add(&observed.head, "commit")?;
    let selected = if let Kind::Commit(commit) = kind {
        commit.as_str()
    } else {
        observed.head.as_str()
    };
    add(&resolve(source, selected, "tree")?, "tree")?;
    let trees = git(
        source,
        &["ls-tree", "-r", "-t", "--full-tree", "-z", selected],
        None,
    )?;
    for record in trees
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let header = record
            .split(|byte| *byte == b'\t')
            .next()
            .ok_or("tree header missing")?;
        let header: Vec<_> = std::str::from_utf8(header)?.split(' ').collect();
        if header.len() != 3 {
            return Err("tree header malformed".into());
        }
        if header[1] == "tree" {
            add(header[2], "tree")?;
        }
    }
    let records = if let Kind::Commit(commit) = kind {
        add(commit, "commit")?;
        tree_records(source, commit)?
    } else {
        observed.entries.clone()
    };
    if *kind != Kind::Working {
        for entry in entries(&records)? {
            add(&entry.object, "blob")?;
        }
    }
    Ok(objects)
}

impl Snapshot {
    /// Complete immutable raw-tree inventory, only after sealed metadata checks.
    pub(crate) fn inventory(&self) -> Result<Vec<(String, String, String)>> {
        if self.identity.tree.is_none() {
            return Err("inventory requires an immutable tree".into());
        }
        self.run_checked(|root| {
            let mut records: Vec<_> = entries(&tree_records(root, "HEAD")?)?
                .into_iter()
                .map(|entry| (entry.name, entry.mode, entry.object))
                .collect();
            records.sort();
            Ok(records)
        })
    }

    pub fn fingerprints(&self) -> Result<(String, String)> {
        self.run_checked(|_| Ok(()))?;
        let source = serde_json::to_vec(&(
            &self.identity,
            &self.probe,
            self.objects.iter().collect::<Vec<_>>(),
        ))?;
        let private =
            serde_json::to_vec(&(&self.identity.tree, &self.metadata, &self.probe.working))?;
        Ok((
            blake3::hash(&source).to_hex().to_string(),
            blake3::hash(&private).to_hex().to_string(),
        ))
    }
}
