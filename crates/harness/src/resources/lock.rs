//! Immediate Linux kernel locks. Metadata is diagnostic; only the kernel owns admission.
use std::{
    error::Error,
    fmt,
    fs::{self, File, OpenOptions, TryLockError},
    io::{Read, Seek, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug)]
pub(crate) struct Busy(pub(crate) String);

impl fmt::Display for Busy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "busy: {}, retry later", self.0)
    }
}
impl Error for Busy {}

pub(crate) fn exit_code(error: &(dyn Error + 'static)) -> i32 {
    if error.is::<Busy>() { 75 } else { 1 }
}

#[derive(Debug)]
pub(crate) struct LongLock {
    file: Option<File>,
    token: String,
}

impl LongLock {
    pub(crate) fn acquire(
        path: &Path,
        holder: &str,
        nonce: &str,
        inherited: Option<&str>,
    ) -> Result<Self> {
        let mut file = open(path)?;
        match file.try_lock() {
            Ok(()) => {
                if nonce.is_empty() || nonce.contains('\n') {
                    return Err("lock proof must be nonempty and contain no newline".into());
                }
                // Keep the parent's descriptor close-on-exec. Intended children receive
                // their own inherited duplicate, never unrelated concurrent execs.
                file.set_len(0)?;
                file.rewind()?;
                write!(file, "{nonce}\n{holder}")?;
                file.sync_data()?;
                Ok(Self {
                    file: Some(file),
                    token: nonce.into(),
                })
            }
            Err(TryLockError::WouldBlock) => {
                let (proof, description) = record(&mut file)?;
                if inherited.is_some_and(|token| !token.is_empty() && token == proof) {
                    Ok(Self {
                        file: None,
                        token: proof,
                    })
                } else {
                    Err(Box::new(Busy(description)))
                }
            }
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }

    pub(crate) fn token(&self) -> &str {
        &self.token
    }
    pub(crate) fn owned(&self) -> bool {
        self.file.is_some()
    }

    pub(crate) fn duplicate(&self) -> std::io::Result<Option<File>> {
        self.file.as_ref().map(File::try_clone).transpose()
    }
}

/// Status probes never keep a lock, never wait, and ignore a dead holder's text.
pub(crate) fn verify(path: &Path, token: &str) -> Result<bool> {
    if token.is_empty() || !path.exists() {
        return Ok(false);
    }
    let mut file = open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(false),
        Err(TryLockError::WouldBlock) => Ok(record(&mut file)?.0 == token),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

/// Status probes never keep a lock, never wait, and ignore a dead holder's text.
pub(crate) fn status(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let mut file = open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(None),
        Err(TryLockError::WouldBlock) => Ok(Some(record(&mut file)?.1)),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

fn open(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)?)
}

fn record(file: &mut File) -> Result<(String, String)> {
    let mut text = String::new();
    file.rewind()?;
    file.read_to_string(&mut text)?;
    Ok(text.split_once('\n').map_or_else(
        || (String::new(), "holder metadata is being recorded".into()),
        |(proof, holder)| (proof.into(), holder.into()),
    ))
}
