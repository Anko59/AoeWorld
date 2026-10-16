//! Fixed deployment observations, not independent service authentication.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
pub(super) const TEMPLATE: &str = "/etc/aoeworld/supervisor/worker-template.json";
pub(super) const PROGRAM: &str = "/usr/bin/docker";
pub(super) const SOCKET: &str = "/run/aoeworld-supervisor/docker.sock";
pub(super) const CLIENT: &str = "/etc/aoeworld/supervisor/docker-client";
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub(super) schema: u32,
    pub(super) image: String,
    pub(super) uid: u32,
    pub(super) memory_mib: u32,
    pub(super) pids: u32,
    pub(super) cpus: u32,
    pub(super) workload_s: u32,
    pub(super) cleanup_s: u32,
    pub(super) command_s: u32,
    pub(super) tmp_mib: u32,
    pub(super) target_mib: u32,
}
impl Config {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > 32768 {
            return Err("template bounds");
        }
        // Derived struct deserialization rejects duplicates AND unknown fields.
        let config: Self = serde_json::from_slice(bytes).map_err(|_| "template schema")?;
        let valid_image = config.image.strip_prefix("sha256:").is_some_and(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        });
        if config.schema != 1
            || !valid_image
            || config.uid == 0
            || !(64..=32768).contains(&config.memory_mib)
            || !(1..=1024).contains(&config.pids)
            || !(1..=32).contains(&config.cpus)
            || !(1..=3600).contains(&config.workload_s)
            || !(1..=15).contains(&config.cleanup_s)
            || !(1..=5).contains(&config.command_s)
            || config.command_s > config.cleanup_s
            || !(16..=1024).contains(&config.tmp_mib)
            || !(64..=8192).contains(&config.target_mib)
        {
            return Err("template constraints");
        }
        Ok(config)
    }
}
type Identity = (u64, u64, u32, u32, u64, u64, i64, i64, i64, i64);
#[cfg(unix)]
fn identity(m: &fs::Metadata) -> Identity {
    use std::os::unix::fs::MetadataExt;
    (
        m.dev(),
        m.ino(),
        m.uid(),
        m.mode(),
        m.nlink(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
pub(super) struct Loaded {
    pub(super) config: Config,
    pub(super) digest: String,
    template: Held,
    client: Option<Held>,
    endpoints: Vec<(PathBuf, Identity)>,
    directories: Vec<(PathBuf, File, Identity)>,
}
struct Held {
    path: PathBuf,
    file: File,
    observed: Identity,
    bytes: Vec<u8>,
}
impl Held {
    #[cfg(unix)]
    fn open(path: &Path) -> Result<Self, &'static str> {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let m = fs::symlink_metadata(path).map_err(|_| "deployment file absent")?;
        if !m.is_file() || m.uid() != 0 || m.mode() & 0o7777 != 0o600 || m.nlink() != 1 {
            return Err("deployment file hygiene");
        }
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(path)
            .map_err(|_| "deployment file open")?;
        if identity(&file.metadata().map_err(|_| "deployment handle")?) != identity(&m) {
            return Err("deployment file raced");
        }
        let mut bytes = Vec::new();
        file.by_ref()
            .take(32769)
            .read_to_end(&mut bytes)
            .map_err(|_| "deployment read")?;
        if bytes.len() > 32768 {
            return Err("deployment bounds");
        }
        let mut held = Self {
            path: path.into(),
            file,
            observed: identity(&m),
            bytes,
        };
        held.verify()?;
        Ok(held)
    }
    #[cfg(unix)]
    fn verify(&mut self) -> Result<(), &'static str> {
        if identity(&fs::symlink_metadata(&self.path).map_err(|_| "deployment endpoint")?)
            != self.observed
            || identity(&self.file.metadata().map_err(|_| "deployment handle")?) != self.observed
        {
            return Err("deployment changed");
        }
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|_| "deployment seek")?;
        let mut bytes = Vec::new();
        self.file
            .by_ref()
            .take(32769)
            .read_to_end(&mut bytes)
            .map_err(|_| "deployment reread")?;
        if bytes != self.bytes {
            return Err("deployment bytes changed");
        }
        Ok(())
    }
}
impl Loaded {
    #[cfg(unix)]
    pub(super) fn load() -> Result<Self, &'static str> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
        let mut directories: Vec<(PathBuf, File, Identity)> = Vec::new();
        for path in [TEMPLATE, PROGRAM, SOCKET, CLIENT] {
            let end = if path == CLIENT {
                Path::new(path)
            } else {
                Path::new(path).parent().ok_or("parent")?
            };
            let mut cursor = PathBuf::new();
            for component in end.components() {
                cursor.push(component);
                if directories.iter().any(|(p, _, _)| p == &cursor) {
                    continue;
                }
                let m = fs::symlink_metadata(&cursor).map_err(|_| "deployment ancestor absent")?;
                if !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
                    return Err("deployment ancestor hygiene");
                }
                let file = fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_DIRECTORY)
                    .open(&cursor)
                    .map_err(|_| "deployment ancestor open")?;
                if identity(&file.metadata().map_err(|_| "ancestor handle")?) != identity(&m) {
                    return Err("ancestor race");
                }
                directories.push((cursor.clone(), file, identity(&m)));
            }
        }
        let program = fs::symlink_metadata(PROGRAM).map_err(|_| "Docker absent")?;
        if !program.is_file()
            || program.uid() != 0
            || program.nlink() != 1
            || program.mode() & 0o7022 != 0
            || program.mode() & 0o100 == 0
        {
            return Err("Docker hygiene");
        }
        let socket = fs::symlink_metadata(SOCKET).map_err(|_| "socket absent")?;
        if !socket.file_type().is_socket()
            || socket.uid() != 0
            || socket.mode() & 0o7777 != 0o600
            || socket.nlink() != 1
        {
            return Err("socket hygiene");
        }
        let mut client = None;
        for entry in fs::read_dir(CLIENT).map_err(|_| "client absent")? {
            let entry = entry.map_err(|_| "client entry")?;
            if entry.file_name() != "config.json" || client.is_some() {
                return Err("client inventory");
            }
            let held = Held::open(&entry.path())?;
            if held.bytes.len() > 4096
                || serde_json::from_slice::<serde_json::Value>(&held.bytes).ok()
                    != Some(serde_json::json!({}))
            {
                return Err("client credentials/helpers forbidden");
            }
            client = Some(held);
        }
        let template = Held::open(Path::new(TEMPLATE))?;
        let config = Config::parse(&template.bytes)?;
        let digest = blake3::hash(&template.bytes).to_hex().to_string();
        let mut loaded = Self {
            config,
            digest,
            template,
            client,
            endpoints: vec![
                (PROGRAM.into(), identity(&program)),
                (SOCKET.into(), identity(&socket)),
            ],
            directories,
        };
        loaded.verify()?;
        Ok(loaded)
    }
    #[cfg(unix)]
    pub(super) fn verify_transport(&mut self) -> Result<(), &'static str> {
        for (path, file, initial) in &self.directories {
            let endpoint = identity(&fs::symlink_metadata(path).map_err(|_| "ancestor endpoint")?);
            let held = identity(&file.metadata().map_err(|_| "ancestor handle")?);
            // Directory content timestamps may change when a template is replaced;
            // cleanup still uses the original immutable binding on the same safe transport.
            let stable = |observed: Identity| {
                (observed.0, observed.1, observed.2, observed.3)
                    == (initial.0, initial.1, initial.2, initial.3)
            };
            if !stable(endpoint) || !stable(held) {
                return Err("ancestor changed");
            }
        }
        for (path, initial) in &self.endpoints {
            if identity(&fs::symlink_metadata(path).map_err(|_| "transport endpoint")?) != *initial
            {
                return Err("transport changed");
            }
        }
        let count = fs::read_dir(CLIENT).map_err(|_| "client endpoint")?.count();
        if count != usize::from(self.client.is_some()) {
            return Err("client inventory changed");
        }
        if let Some(client) = &mut self.client {
            client.verify()?;
        }
        Ok(())
    }
    #[cfg(unix)]
    pub(super) fn verify(&mut self) -> Result<(), &'static str> {
        self.verify_transport()?;
        self.template.verify()
    }
    #[cfg(not(unix))]
    pub(super) fn load() -> Result<Self, &'static str> {
        Err("Unix restricted workers unavailable")
    }
    #[cfg(not(unix))]
    pub(super) fn verify_transport(&mut self) -> Result<(), &'static str> {
        Err("Unix transport unavailable")
    }
    #[cfg(not(unix))]
    pub(super) fn verify(&mut self) -> Result<(), &'static str> {
        Err("Unix template unavailable")
    }
}
#[cfg(test)]
mod tests;
