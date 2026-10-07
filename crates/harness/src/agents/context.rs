//! Who is acting and where writes may land.
use super::{
    paths,
    role::{self, Role},
    shell::Word,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) type Verdict = Result<(), String>;

/// Records only the hook itself writes; agents may add `BLOCKED.md` there.
pub(crate) const RECORDS: &str = ".cache/claude-hook";
pub(crate) const BLOCKED: &str = ".cache/claude-hook/BLOCKED.md";
const WALK_LIMIT: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Access {
    /// Create or change one path.
    Put,
    /// Delete or move away a path and, for a directory, everything under it.
    Remove,
    /// Unpack or copy an unknown set of files under a directory.
    Tree,
}

#[derive(Debug)]
pub(crate) struct Context {
    pub(crate) root: PathBuf,
    pub(crate) common: PathBuf,
    pub(crate) role: Role,
    pub(crate) home: Option<PathBuf>,
    pub(crate) temp: Vec<PathBuf>,
}

impl Context {
    pub(crate) fn new(root: &Path, role: Role, scratchpad: Option<&Path>) -> Result<Self, String> {
        let root = fs::canonicalize(root).map_err(|e| format!("checkout root: {e}"))?;
        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .env_remove("GIT_DIR")
            .env_remove("GIT_COMMON_DIR")
            .output()
            .map_err(|e| format!("git: {e}"))?;
        if !output.status.success() {
            return Err("the checkout is not a Git repository".into());
        }
        let common = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let common = fs::canonicalize(common).map_err(|e| format!("Git directory: {e}"))?;
        let home = env::var_os("HOME").map(PathBuf::from);
        let mut temp = vec![PathBuf::from("/tmp"), PathBuf::from("/var/tmp")];
        temp.extend(env::var_os("TMPDIR").map(PathBuf::from));
        temp.extend(scratchpad.map(Path::to_path_buf));
        let temp = temp
            .iter()
            .filter(|path| path.is_absolute())
            .filter_map(|path| paths::land(&paths::lexical(path)))
            .collect();
        Ok(Self {
            root,
            common,
            role,
            home,
            temp,
        })
    }

    fn agent(&self) -> bool {
        self.role.is_agent()
    }

    /// Resolve a word naming a path, relative to `cwd` when it is relative.
    pub(crate) fn resolve(
        &self,
        cwd: Option<&Path>,
        word: &Word,
    ) -> Result<Option<PathBuf>, String> {
        if !word.plain() {
            return if self.agent() {
                Err(format!(
                    "the path `{}` is only known at run time; name it plainly",
                    word.text
                ))
            } else {
                Ok(None)
            };
        }
        let path = paths::expand_home(&word.text, self.home.as_deref());
        if word.text.starts_with('~') && !path.is_absolute() {
            return if self.agent() {
                Err(format!(
                    "`{}` names a home directory the policy cannot resolve; use an absolute path",
                    word.text
                ))
            } else {
                Ok(None)
            };
        }
        let absolute = match (path.is_absolute(), cwd) {
            (true, _) => path,
            (false, Some(cwd)) => cwd.join(path),
            (false, None) if self.agent() => {
                return Err(format!(
                    "the working directory is unknown here, so `{}` cannot be judged; use an absolute path",
                    word.text
                ));
            }
            (false, None) => return Ok(None),
        };
        match paths::land(&paths::lexical(&absolute)) {
            Some(landing) => Ok(Some(landing)),
            None if self.agent() => Err(format!("`{}` follows too many symlinks", word.text)),
            None => Ok(None),
        }
    }

    pub(crate) fn write(&self, cwd: Option<&Path>, word: &Word, access: Access) -> Verdict {
        if word.plain()
            && matches!(
                word.text.as_str(),
                "/dev/null" | "/dev/stdout" | "/dev/stderr"
            )
        {
            return Ok(());
        }
        match self.resolve(cwd, word)? {
            Some(landing) => self.landing(&landing, access),
            None => Ok(()),
        }
    }

    pub(crate) fn landing(&self, landing: &Path, access: Access) -> Verdict {
        if landing.starts_with("/dev/fd") || landing == Path::new("/dev/null") {
            return Ok(());
        }
        self.never(landing, access)?;
        if !self.agent() {
            return Ok(());
        }
        // The checkout's own rules win even when it lives under a temp directory.
        let Some(relative) = paths::relative_to(landing, &self.root) else {
            return if self.temp.iter().any(|t| paths::within(landing, t)) {
                Ok(())
            } else {
                Err(format!(
                    "agents write inside the checkout or a temp directory only, not `{}`",
                    landing.display()
                ))
            };
        };
        if paths::within(landing, &self.root.join(".cache/tmp")) {
            return Ok(());
        }
        if !relative.is_ascii() {
            return Err("agents only write ASCII paths (look-alike letters are refused)".into());
        }
        if access == Access::Tree {
            return Err(format!(
                "copying or unpacking a whole tree into `{}` hides which files change; unpack into a temp directory and copy named files",
                landing.display()
            ));
        }
        self.relative(&relative)?;
        if access == Access::Remove && landing.is_dir() {
            let mut seen = 0;
            return self.walk(landing, &mut seen);
        }
        Ok(())
    }

    /// Denied to every role, the main session included.
    fn never(&self, landing: &Path, access: Access) -> Verdict {
        let git = landing
            .components()
            .any(|c| c.as_os_str().eq_ignore_ascii_case(".git"));
        if git || paths::within(landing, &self.common) {
            return Err("Git metadata (hooks, config, refs) is never edited by hand; use git commands, and `make hooks-install` for hooks".into());
        }
        let records = self.root.join(RECORDS);
        if paths::within(landing, &records) {
            let blocked = paths::relative_to(landing, &self.root).as_deref()
                == Some(&BLOCKED.to_ascii_lowercase());
            return if blocked && self.agent() {
                Ok(())
            } else {
                Err(format!(
                    "{RECORDS} holds the harness's own records; only the hook writes them (agents may write {BLOCKED})"
                ))
            };
        }
        if access == Access::Tree && paths::within(&records, landing) {
            return Err(format!(
                "a whole-tree write into `{}` would reach the harness records under {RECORDS}; use a temp directory",
                landing.display()
            ));
        }
        Ok(())
    }

    /// Role rules for a repository-relative, lower-case path.
    pub(crate) fn relative(&self, relative: &str) -> Verdict {
        if let Some(class) = role::protected(relative) {
            return Err(format!(
                "`{relative}` is in the protected {} class: only the main session edits it. {}",
                class.name, class.remedy
            ));
        }
        match self.role {
            Role::Reviewer => {
                Err("reviewers only read: report findings instead of changing files".into())
            }
            Role::Tester if !role::is_test(relative) => Err(format!(
                "the Tester writes tests only (tests.rs, tests/, *_tests.rs, *.spec.ts, fixtures), not `{relative}`; describe the needed code change for the Implementer"
            )),
            Role::Implementer if role::is_test(relative) => Err(format!(
                "the Implementer never edits tests (`{relative}`); if a test looks wrong, report it instead"
            )),
            _ => Ok(()),
        }
    }

    fn walk(&self, directory: &Path, seen: &mut usize) -> Verdict {
        let entries =
            fs::read_dir(directory).map_err(|e| format!("{}: {e}", directory.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            *seen += 1;
            if *seen > WALK_LIMIT {
                return Err(format!(
                    "`{}` holds more than {WALK_LIMIT} files; remove a narrower path",
                    directory.display()
                ));
            }
            let path = entry.path();
            let relative = paths::relative_to(&path, &self.root).unwrap_or_default();
            self.never(&path, Access::Remove)?;
            self.relative(&relative)?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                self.walk(&path, seen)?;
            }
        }
        Ok(())
    }
}
