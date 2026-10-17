//! Where the dev server keeps everything: one directory in the repository's
//! common Git directory, shared by every worktree and owned by no session.
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

pub(crate) struct Layout {
    /// `<git common dir>/aoe-dev-server`.
    pub(crate) home: PathBuf,
    /// The checkout that owns the common Git directory (where `local-assets`
    /// and the geodata cache usually live when a worktree has none).
    pub(crate) main: PathBuf,
}

impl Layout {
    pub(crate) fn resolve(root: &Path) -> Result<Self> {
        let common = crate::ship::git::git(
            root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        Ok(Self::at(Path::new(&common)))
    }

    pub(crate) fn at(common: &Path) -> Self {
        let main = if common.file_name().is_some_and(|name| name == ".git") {
            common.parent().unwrap_or(common).to_path_buf()
        } else {
            common.to_path_buf()
        };
        Self {
            home: common.join("aoe-dev-server"),
            main,
        }
    }

    pub(crate) fn state(&self) -> PathBuf {
        self.home.join("state.json")
    }

    pub(crate) fn lock(&self) -> PathBuf {
        self.home.join("lock")
    }

    pub(crate) fn logs(&self) -> PathBuf {
        self.home.join("logs")
    }

    /// The exported, built sources of one revision.
    pub(crate) fn tree(&self, revision: &str) -> PathBuf {
        self.home.join("trees").join(revision)
    }

    pub(crate) fn trees(&self) -> PathBuf {
        self.home.join("trees")
    }

    /// The cargo target directory shared by every revision's build.
    pub(crate) fn target(&self) -> PathBuf {
        self.home.join("target")
    }

    /// The prepared maps (and the preset list) of one map-code fingerprint.
    pub(crate) fn presets(&self, fingerprint: &str) -> PathBuf {
        self.home.join("presets").join(fingerprint)
    }

    pub(crate) fn all_presets(&self) -> PathBuf {
        self.home.join("presets")
    }

    /// `relative` below the invoking checkout, else below the main checkout:
    /// `local-assets` and `.cache/geodata` are not in Git, so a fresh worktree
    /// has none.
    pub(crate) fn shared(&self, root: &Path, relative: &str) -> Option<PathBuf> {
        [root, self.main.as_path()]
            .into_iter()
            .map(|base| base.join(relative))
            .find(|path| path.exists())
    }
}
