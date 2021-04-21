use std::{fmt, path::Path};

use crate::{diff, file::File, Error, Result};
#[allow(unused)]
use tracing::{debug, error, info, instrument, span, warn};

pub struct Repo<'r> {
    pub(crate) internal: Internal,
    history: undo::History<Change<'r>>,
}

impl<'r> Repo<'r> {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let internal = Internal::open(path)?;
        let history = undo::History::new();
        Ok(Self { internal, history })
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) -> Result<()> {
        self.history
            .undo(&mut self.internal)
            .ok_or(Error::UndoEmpty)
            .flatten()
    }

    pub fn redo(&mut self) -> Result<()> {
        self.history
            .redo(&mut self.internal)
            .ok_or(Error::RedoEmpty)
            .flatten()
    }

    pub fn path(&self) -> &Path {
        self.internal.path()
    }

    pub fn uncommitted_files(&self) -> Result<Vec<diff::Meta>> {
        self.internal.uncommitted_files()
    }

    pub fn diff_details(&self, diff: &diff::Meta) -> Result<diff::Details> {
        self.internal.diff_details(diff)
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
    type Target = Internal;
    type Output = ();
    type Error = Error;

    fn apply(&mut self, target: &mut Self::Target) -> undo::Result<Self> {
        match self {
            Change::StageFile(file) => target.stage_file(file),
            Change::UnstageFile(file) => target.unstage_file(file),
        }
    }

    fn undo(&mut self, target: &mut Self::Target) -> undo::Result<Self> {
        match self {
            Change::StageFile(file) => target.unstage_file(file),
            Change::UnstageFile(file) => target.stage_file(file),
        }
    }
}

impl fmt::Display for Change<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Internal manages everything that doesn't require history. This is so that
/// actions on the history can mutably borrow something that doesn't contain the
/// history itself.
pub(crate) struct Internal {
    git: git2::Repository,
}

impl Internal {
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

    fn head_assuming_born(&self) -> std::result::Result<git2::Tree, git2::Error> {
        self.git.head()?.peel_to_commit()?.tree()
    }

    fn head(&self) -> Result<Option<git2::Tree>> {
        match self.head_assuming_born() {
            Ok(head) => Ok(Some(head)),
            Err(err) if err.code() == git2::ErrorCode::UnbornBranch => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn uncommitted_files(&self) -> Result<Vec<diff::Meta>> {
        let head = self.head()?;
        let mut opts = Self::uncommitted_opts();

        let deltas = self
            .git
            .diff_tree_to_workdir_with_index(head.as_ref(), Some(&mut opts))?
            .deltas()
            .map(|delta| diff::Meta::from_git2(&delta))
            .collect();

        Ok(deltas)
    }

    fn diff_details(&self, meta: &diff::Meta) -> Result<diff::Details> {
        match meta {
            crate::Meta::Added(f)
            | crate::Meta::Deleted(f)
            | crate::Meta::Modified { new: f, .. }
            | crate::Meta::Renamed { new: f, .. }
            | crate::Meta::Copied { new: f, .. }
            | crate::Meta::Ignored(f)
            | crate::Meta::Untracked(f)
            | crate::Meta::Typechange { new: f, .. }
            | crate::Meta::Unreadable(f)
            | crate::Meta::Conflicted { new: f, .. } => {
                let path = f.rel_path_required()?;
                self._diff_details(path)
            }
        }
    }

    fn _diff_details(&self, path: &Path) -> Result<diff::Details> {
        let head = self.head()?;

        let mut opts = Self::uncommitted_opts();
        opts.pathspec(path);

        let mut meta: Option<diff::Meta> = None;
        let mut file_cb = |delta: git2::DiffDelta<'_>, _progress| {
            if let Some(delta_path) = Self::delta_path(&delta) {
                if delta_path == path {
                    meta = Some(diff::Meta::from_git2(&delta));
                    return true;
                }
            }

            // NOTE: If we ask to stop once we get the target lines_cb isn't
            // called, so we exit on the first subsequent delta.

            meta.is_none()
        };

        let mut lines = vec![];
        let mut line_cb = |delta: git2::DiffDelta<'_>,
                           _hunk: Option<git2::DiffHunk<'_>>,
                           line: git2::DiffLine<'_>| {
            if let Some(delta_path) = Self::delta_path(&delta) {
                if delta_path == path {
                    let line = diff::Line::from_git2(&line);
                    lines.push(line);
                }
            }

            true
        };

        match self
            .git
            .diff_tree_to_workdir_with_index(head.as_ref(), Some(&mut opts))?
            .foreach(&mut file_cb, None, None, Some(&mut line_cb))
        {
            Ok(()) => (),
            Err(err) if err.code() == git2::ErrorCode::User => (),
            Err(err) => return Err(err.into()),
        }

        let meta = meta.ok_or_else(|| Error::PathNotFound(path.to_path_buf()))?;

        Ok(diff::Details::new(meta, lines))
    }

    fn delta_path<'a, 'b>(delta: &'a git2::DiffDelta<'b>) -> Option<&'b Path> {
        delta
            .new_file()
            .path()
            .map_or_else(|| delta.old_file().path(), |delta_path| Some(delta_path))
    }

    fn uncommitted_opts() -> git2::DiffOptions {
        let mut opts = git2::DiffOptions::new();
        opts.include_untracked(true)
            .include_typechange(true)
            .include_unmodified(false)
            .include_unreadable(true)
            .include_untracked(true)
            .include_ignored(true);
        opts
    }

    fn stage_file(&self, file: &File) -> Result<()> {
        let path = file.rel_path_required()?;
        if self.git.status_should_ignore(path)? {
            debug!("Ignoring {:?}", file);
        } else {
            self.git.index()?.add_path(path)?;
        }
        Ok(())
    }

    fn unstage_file(&self, file: &File) -> Result<()> {
        let path = file.rel_path_required()?;
        self.git.index()?.remove_path(path)?;
        Ok(())
    }
}

impl fmt::Debug for Internal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RepoInternal")
            .field("path", &self.git.path())
            .finish_non_exhaustive()
    }
}
