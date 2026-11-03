//! Admission and inherited ownership for registry gates and direct Make commands.
use super::{DIR_VAR, JobEnv, Result, TOKEN_VAR, lock::LongLock};
use crate::{
    gates::registry::{Gate, Registry},
    ship::git,
};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

type Env = Vec<(&'static str, String)>;
struct Held {
    env: Env,
    descriptor: Option<File>,
}
static HELD: Mutex<Option<Held>> = Mutex::new(None);

pub(crate) fn prepare_child(command: &mut std::process::Command) -> std::io::Result<()> {
    if let Some(held) = HELD.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        command.envs(held.env.clone());
        if let Some(file) = &held.descriptor {
            super::lock::inherit_file(command, file.try_clone()?);
        }
    }
    Ok(())
}

pub(crate) fn cap_args() -> Vec<String> {
    crate::resources::policy::cap_args()
}

#[derive(Debug)]
pub(crate) struct Guard {
    _lock: Option<LongLock>,
    env: Env,
    outermost: bool,
}

impl Guard {
    fn none() -> Self {
        Self {
            _lock: None,
            env: Vec::new(),
            outermost: false,
        }
    }
    pub(crate) fn env(&self) -> Env {
        self.env.clone()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.outermost {
            *HELD.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }
}

pub(crate) fn lock_path(root: &Path, env: &JobEnv) -> Result<PathBuf> {
    if let Some(directory) = env.dir.as_ref().filter(|dir| !dir.is_empty()) {
        let path = PathBuf::from(directory).join("long.lock");
        if let Some(token) = env.token.as_deref()
            && super::lock::verify(&path, token)?
        {
            return Ok(path);
        }
    }
    let common = git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common).join("aoe-ship/long.lock"))
}

pub(crate) fn checkout_of(from: &Path) -> Result<Option<PathBuf>> {
    Ok(from
        .canonicalize()?
        .ancestors()
        .find(|dir| dir.join("gates/registry.json").is_file())
        .map(Path::to_path_buf))
}

fn long_gate(gate: &Gate, env: &JobEnv) -> bool {
    let focused = env.e2e_specs.as_ref().is_some_and(|s| !s.trim().is_empty());
    gate.long && !(focused && (gate.id == "test-e2e" || gate.command == "make test-e2e"))
}

pub(crate) fn is_long(registry: &Registry, gate: &str, env: &JobEnv) -> bool {
    let command = format!("make {gate}");
    registry
        .gates
        .iter()
        .any(|g| (g.id == gate || g.command == command) && long_gate(g, env))
}

pub(crate) fn hold_plan(
    source: &Path,
    command: &str,
    gates: &[&Gate],
    env: &JobEnv,
) -> Result<Guard> {
    if gates.iter().any(|gate| long_gate(gate, env)) {
        acquire_in(source, command, env)
    } else {
        Ok(Guard::none())
    }
}

pub(crate) fn guard(gate: &str, env: &JobEnv) -> Result<Guard> {
    hold(Path::new("."), gate, env)
}

pub(crate) fn hold(from: &Path, gate: &str, env: &JobEnv) -> Result<Guard> {
    let Some(root) = checkout_of(from)? else {
        return Ok(Guard::none());
    };
    hold_in(&root, &root, gate, env)
}

pub(crate) fn hold_timed(from: &Path, gate: &str, env: &JobEnv) -> Result<Guard> {
    crate::timeline::stage(
        from,
        "job-admission",
        serde_json::json!({"gate":gate}),
        || hold(from, gate, env),
    )
}

pub(crate) fn hold_snapshot(
    root: &Path,
    source: &Path,
    gate: &str,
    env: &JobEnv,
    _cancellation: &crate::process::Cancellation,
) -> Result<Guard> {
    hold_in(root, source, gate, env)
}

fn hold_in(root: &Path, source: &Path, gate: &str, env: &JobEnv) -> Result<Guard> {
    if !is_long(&Registry::load(root)?, gate, env) {
        return Ok(Guard::none());
    }
    acquire_in(source, gate, env)
}

fn acquire_in(source: &Path, gate: &str, env: &JobEnv) -> Result<Guard> {
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(held) = held.as_ref() {
        return Ok(Guard {
            _lock: None,
            env: held.env.clone(),
            outermost: false,
        });
    }
    let path = lock_path(source, env)?;
    let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let holder = format!(
        "`make {gate}` on {} in {} (started {})",
        git::branch(source).unwrap_or_else(|_| "(detached)".into()),
        source.canonicalize()?.display(),
        crate::ship::lock::utc(started)
    );
    let lock = LongLock::acquire(&path, &holder, &nonce()?, env.token.as_deref())?;
    if lock.owned() {
        eprintln!("long job admitted: {holder}");
    }
    let env = vec![
        (TOKEN_VAR, lock.token().to_owned()),
        (
            DIR_VAR,
            path.parent()
                .ok_or("long lock has no parent")?
                .display()
                .to_string(),
        ),
    ];
    *held = Some(Held {
        env: env.clone(),
        descriptor: lock.duplicate()?,
    });
    Ok(Guard {
        _lock: Some(lock),
        env,
        outermost: true,
    })
}

pub(crate) fn run_forced(from: &Path, gate: &str, argv: &[String], env: &JobEnv) -> Result<i32> {
    let guard = hold(from, gate, env)?;
    let (program, args) = argv.split_first().ok_or("job: no command to run")?;
    let mut command = std::process::Command::new(program);
    command.args(args).envs(guard.env());
    if let Some(lock) = &guard._lock {
        lock.inherit_to(&mut command)?;
    } else {
        prepare_child(&mut command)?;
    }
    let status = command.status()?;
    Ok(status.code().unwrap_or(1))
}

pub(crate) fn nonce() -> Result<String> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
