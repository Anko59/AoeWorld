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
