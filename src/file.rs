use crate::{Error, Repo, Result};
use std::path::{Path, PathBuf};

use crate::RepoInternal;

#[derive(Debug, Clone)]
pub struct File {
    id: Option<git2::Oid>,
    rel_path: Option<PathBuf>,
    size: u64,
}

impl File {
    pub(crate) fn new(id: Option<git2::Oid>, rel_path: Option<PathBuf>, size: u64) -> Self {
        Self { id, rel_path, size }
    }

    pub(crate) fn from_diff_file(from: &git2::DiffFile) -> Self {
        let id = from.id();
        let id = if id.is_zero() { None } else { Some(id) };

        Self::new(id, from.path().map(Path::to_path_buf), from.size())
    }

    pub fn id(&self) -> Option<git2::Oid> {
        self.id
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn rel_path(&self) -> Option<&Path> {
        self.rel_path.as_deref()
    }

    pub fn abs_path(&self, repo: &Repo) -> Option<PathBuf> {
        self.abs_path_int(&repo.internal)
    }

    pub(crate) fn id_required(&self) -> Result<git2::Oid> {
        self.id().ok_or_else(|| Error::MissingId(self.clone()))
    }

    pub(crate) fn rel_path_required(&self) -> Result<&Path> {
        self.rel_path()
            .ok_or_else(|| Error::MissingPath(self.clone()))
    }

    pub(crate) fn abs_path_int(&self, repo: &RepoInternal) -> Option<PathBuf> {
        self.rel_path.as_ref().map(|rel| repo.path().join(rel))
    }
}
