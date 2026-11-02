use super::*;

pub(super) fn command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(root);
    // Do not inherit another checkout/index, injected config, or object store.
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["--no-replace-objects", "-c", "core.fsmonitor=false"]);
    command
}

pub(super) fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>> {
    git_at(root, None, args, input)
}

pub(super) fn git_at(
    root: &Path,
    index: Option<&Path>,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut command = command(root);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command.spawn()?;
    let pipe = if input.is_some() {
        Some(child.stdin.take().ok_or("missing piped Git stdin")?)
    } else {
        None
    };
    // Feed concurrently: input may exceed a pipe while Git emits diagnostics.
    let output = std::thread::scope(|scope| {
        let writer = input
            .zip(pipe)
            .map(|(bytes, mut stdin)| scope.spawn(move || stdin.write_all(bytes)));
        let output = child.wait_with_output();
        let written = writer.map(|writer| {
            writer
                .join()
                .map_err(|_| std::io::Error::other("Git input writer panicked"))
                .and_then(|result| result)
        });
        (output, written)
    });
    let result = output.0?;
    if !result.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&result.stderr)).into());
    }
    if let Some(written) = output.1 {
        written?;
    }
    Ok(result.stdout)
}

pub(super) fn line(bytes: Vec<u8>) -> Result<String> {
    let value = String::from_utf8(bytes)?;
    Ok(value.strip_suffix('\n').unwrap_or(&value).to_owned())
}

pub(super) fn oid(value: &str) -> Result<&str> {
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected a full SHA-1 or SHA-256 object ID".into());
    }
    Ok(value)
}

pub(super) fn resolve(root: &Path, reference: &str, object: &str) -> Result<String> {
    let value = line(git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{{object}}}"),
        ],
        None,
    )?)?;
    oid(&value)?;
    Ok(value)
}

pub(super) fn names(bytes: &[u8]) -> Result<Vec<String>> {
    bytes
        .split(|b| *b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| Ok(String::from_utf8(name.to_vec())?))
        .collect()
}

/// One sanitized Git process per read operation. Requests are full object IDs,
/// never revisions; size framing keeps binary data out of the line protocol.
pub(super) struct Batch {
    child: std::process::Child,
    input: Option<std::process::ChildStdin>,
    output: std::io::BufReader<std::process::ChildStdout>,
}

impl Batch {
    pub(super) fn new(root: &Path) -> Result<Self> {
        let mut child = command(root)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take().ok_or("missing batch input")?;
        let output = child.stdout.take().ok_or("missing batch output")?;
        Ok(Self {
            child,
            input: Some(input),
            output: std::io::BufReader::new(output),
        })
    }

    pub(super) fn read(&mut self, object: &str, kind: &str) -> Result<Vec<u8>> {
        use std::io::{BufRead, Read};
        oid(object)?;
        let input = self.input.as_mut().ok_or("batch input closed")?;
        writeln!(input, "{object}")?;
        input.flush()?;
        let mut header = String::new();
        (&mut self.output).take(1024).read_line(&mut header)?;
        let fields: Vec<_> = header.trim_end_matches('\n').split(' ').collect();
        if !header.ends_with('\n') || fields.len() != 3 || fields[0] != object || fields[1] != kind
        {
            return Err("Git batch object identity/type mismatch or missing object".into());
        }
        let size: usize = fields[2].parse()?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(size)?;
        bytes.resize(size, 0);
        self.output.read_exact(&mut bytes)?;
        let mut end = [0];
        self.output.read_exact(&mut end)?;
        if end != *b"\n" {
            return Err("Git batch object terminator missing".into());
        }
        Ok(bytes)
    }

    pub(super) fn finish(mut self) -> Result<()> {
        self.input.take();
        if !self.child.wait()?.success() {
            return Err("Git batch process failed".into());
        }
        Ok(())
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        // Errors must also close pipes and reap the child, including partial reads.
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Hash raw files in one process; Git's C quoting supports arbitrary path bytes,
/// including newlines. No attributes, filters, or caller Git environment apply.
pub(super) fn hash_files(root: &Path, paths: &[PathBuf], write: bool) -> Result<Vec<String>> {
    let mut input = Vec::new();
    for path in paths {
        input.push(b'"');
        for byte in path.as_os_str().as_encoded_bytes() {
            input.extend_from_slice(format!("\\{byte:03o}").as_bytes());
        }
        input.extend_from_slice(b"\"\n");
    }
    let mut args = vec!["hash-object", "--no-filters", "--stdin-paths"];
    if write {
        args.push("-w");
    }
    let output = git(root, &args, Some(&input))?;
    let objects: Vec<_> = std::str::from_utf8(&output)?
        .lines()
        .map(str::to_owned)
        .collect();
    if objects.len() != paths.len() {
        return Err("Git file hash count mismatch".into());
    }
    for object in &objects {
        oid(object)?;
    }
    Ok(objects)
}
