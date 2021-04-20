use crate::{RepoFile, Result};

#[allow(unused)]
use tracing::{debug, error, info, instrument, span, warn};

#[derive(Debug, Clone)]
pub enum FileDelta {
    Added(RepoFile),
    Deleted(RepoFile),
    Modified { old: RepoFile, new: RepoFile },
    Renamed { old: RepoFile, new: RepoFile },
    Copied { old: RepoFile, new: RepoFile },
    Ignored(RepoFile),
    Untracked(RepoFile),
    Typechange { old: RepoFile, new: RepoFile },
    Unreadable(RepoFile),
    Conflicted { old: RepoFile, new: RepoFile },
}

impl FileDelta {
    /// # Panics
    /// If the delta is of status [`git2::Delta::Unmodified`].
    pub(crate) fn from_git2(from: &git2::DiffDelta) -> Self {
        use git2::Delta;
        match from.status() {
            Delta::Added => Self::Added(Self::get_new_file_only(&from)),
            Delta::Deleted => Self::Deleted(Self::get_old_file_only(&from)),
            Delta::Modified => {
                let (old, new) = Self::get_both_files(&from);
                Self::Modified { old, new }
            }
            Delta::Renamed => {
                let (old, new) = Self::get_both_files(&from);
                Self::Renamed { old, new }
            }
            Delta::Copied => {
                let (old, new) = Self::get_both_files(&from);
                Self::Copied { old, new }
            }
            Delta::Ignored => Self::Ignored(Self::get_new_file_only(&from)),
            Delta::Untracked => Self::Untracked(Self::get_new_file_only(&from)),
            Delta::Typechange => {
                let (old, new) = Self::get_both_files(&from);
                Self::Typechange { old, new }
            }
            Delta::Unreadable => Self::Unreadable(Self::get_new_file_only(&from)),
            Delta::Unmodified => unreachable!("We don't include unmodified files"),
            Delta::Conflicted => {
                let (old, new) = Self::get_both_files(&from);
                Self::Conflicted { old, new }
            }
        }
    }

    fn get_new_file_only(from: &git2::DiffDelta) -> RepoFile {
        assert_eq!(from.nfiles(), 1);
        RepoFile::from_diff_file(&from.new_file())
    }

    fn get_old_file_only(from: &git2::DiffDelta) -> RepoFile {
        assert_eq!(from.nfiles(), 1);
        RepoFile::from_diff_file(&from.old_file())
    }

    fn get_both_files(from: &git2::DiffDelta) -> (RepoFile, RepoFile) {
        assert_eq!(from.nfiles(), 2);
        (
            RepoFile::from_diff_file(&from.old_file()),
            RepoFile::from_diff_file(&from.new_file()),
        )
    }
}
