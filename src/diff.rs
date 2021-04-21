use crate::RepoFile;

#[allow(unused)]
use tracing::{debug, error, info, instrument, span, warn};

#[derive(Debug, Clone)]
pub enum Meta {
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

impl Meta {
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

#[derive(Debug, Clone)]
pub struct Details {
    meta: Meta,
    lines: Vec<Line>,
}

impl Details {
    pub(crate) fn new(meta: Meta, lines: Vec<Line>) -> Self {
        Self { meta, lines }
    }
}

#[derive(Debug, Clone)]
pub struct Line {
    /// Line number in old file or None for added line
    old_lineno: Option<u32>,
    /// Line number in new file or None for deleted line
    new_lineno: Option<u32>,
    /// Number of newline characters in content
    num_lines: u32,
    /// Offset in the original file to the content
    content_offset: i64,
    content: Vec<u8>,
    origin: git2::DiffLineType,
}

impl Line {
    pub(crate) fn from_git2(from: &git2::DiffLine) -> Self {
        Self {
            old_lineno: from.old_lineno(),
            new_lineno: from.new_lineno(),
            num_lines: from.num_lines(),
            content_offset: from.content_offset(),
            content: from.content().to_vec(),
            origin: from.origin_value(),
        }
    }
}
