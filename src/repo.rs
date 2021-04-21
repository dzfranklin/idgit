use std::{ffi::OsString, fmt, fs, path::Path, str::FromStr};

use crate::{file::File, Error, FileDelta, Result};
#[allow(unused)]
use tracing::{debug, error, info, instrument, span, warn};

pub struct Repo<'r> {
    pub(crate) internal: RepoInternal,
    history: undo::History<Change<'r>>,
}

impl<'r> Repo<'r> {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let internal = RepoInternal::open(path)?;
        let history = undo::History::new();
        Ok(Self { internal, history })
    }

    pub fn path(&self) -> &Path {
        self.internal.path()
    }

    pub fn uncommitted(&self) -> Result<Vec<FileDelta>> {
        self.internal.uncommitted()
    }

    pub fn stage_file(&mut self, file: &'r File) -> Result<()> {
        self.apply(Change::StageFile(file))
    }

    pub fn unstage_file(&mut self, file: &'r File) -> Result<()> {
        self.apply(Change::UnstageFile(file))
    }

    fn apply(&mut self, change: Change<'r>) -> Result<()> {
        self.history.apply(&mut self.internal, change)
    }
}

impl fmt::Debug for Repo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let history = format!("{}", self.history.display());
        f.debug_struct("Repo")
            .field("internal", &self.internal)
            .field("history", &history)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
enum Change<'r> {
    StageFile(&'r File),
    UnstageFile(&'r File),
}

impl<'r> undo::Action for Change<'r> {
    type Target = RepoInternal;
    type Output = ();
    type Error = Error;

    fn apply(&mut self, target: &mut Self::Target) -> undo::Result<Self> {
        match self {
            Change::StageFile(file) => target.do_stage_file(file),
            Change::UnstageFile(file) => target.do_unstage_file(file),
        }
    }

    fn undo(&mut self, target: &mut Self::Target) -> undo::Result<Self> {
        match self {
            Change::StageFile(file) => target.do_stage_file(file),
            Change::UnstageFile(file) => target.do_unstage_file(file),
        }
    }
}

impl fmt::Display for Change<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

pub(crate) struct RepoInternal {
    git: git2::Repository,
}

impl RepoInternal {
    fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let git = git2::Repository::open(path)?;
        Ok(Self { git })
    }

    pub(crate) fn path(&self) -> &Path {
        let path = self.git.path();
        if path.ends_with(".git") {
            path.parent().expect("If non-bare has parent")
        } else {
            path
        }
    }

    fn head_is_unborn(&self) -> bool {
        if let Err(err) = self.git.head() {
            err.code() == git2::ErrorCode::UnbornBranch
        } else {
            false
        }
    }

    fn uncommitted(&self) -> Result<Vec<FileDelta>> {
        let head = if self.head_is_unborn() {
            None
        } else {
            Some(self.git.head()?.peel_to_commit()?.tree()?)
        };

        let mut opts = git2::DiffOptions::new();
        opts.include_untracked(true)
            .include_typechange(true)
            .include_unmodified(false)
            .include_unreadable(true)
            .include_untracked(true)
            .include_ignored(true);

        let deltas = self
            .git
            .diff_tree_to_workdir_with_index(head.as_ref(), Some(&mut opts))?
            .deltas()
            .map(|delta| FileDelta::from_git2(&delta))
            .collect();

        Ok(deltas)
    }

    fn do_stage_file(&self, file: &File) -> Result<()> {
        let path = file.rel_path_required()?;
        if self.git.status_should_ignore(path)? {
            debug!("Ignoring {:?}", file);
        } else {
            self.git.index()?.add_path(path)?;
        }
        Ok(())
    }

    fn do_unstage_file(&self, file: &File) -> Result<()> {
        todo!();
        Ok(())
    }
}

impl fmt::Debug for RepoInternal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RepoInternal")
            .field("path", &self.git.path())
            .finish_non_exhaustive()
    }
}
