#![feature(with_options, assert_matches)]

use idgit::{FileDelta, Repo, Result};
use rand::Rng;
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

use cmd_lib::run_cmd;
use tempfile::TempDir;
#[allow(unused)]
use tracing::{debug, error, info, instrument, span, warn};

struct SampleRepoDir(TempDir);

impl SampleRepoDir {
    fn new() -> Self {
        let mut this = Self(tempfile::tempdir().unwrap());
        this.init();
        this
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn path_str(&self) -> &str {
        self.path().to_str().unwrap()
    }

    fn init(&mut self) {
        let path = self.path_str();

        (run_cmd! {
            cd $path;
            git init;
        })
        .unwrap();
    }

    fn change_something(&mut self) {
        let name = format!("something_{}.txt", rand::thread_rng().gen::<u64>());
        self.set_file(&name, b"some change");
    }

    fn create_dir<N: AsRef<Path>>(&mut self, name: N) {
        fs::create_dir(self.path().join(name)).unwrap();
    }

    fn set_file<N: AsRef<Path>>(&mut self, name: N, contents: &'_ [u8]) {
        let path = self.path().join(name);

        File::with_options()
            .write(true)
            .create(true)
            .open(path)
            .unwrap()
            .write_all(contents)
            .unwrap();
    }

    fn delete_file<N: AsRef<Path>>(&mut self, name: N) {
        fs::remove_file(self.path().join(name)).unwrap();
    }

    fn commit_all(&mut self) {
        let path = self.path_str();
        (run_cmd! {
            cd $path;
            git add *;
            git commit -am "Make some change";
        })
        .unwrap();
    }

    fn add<N: AsRef<Path>>(&mut self, name: N) {
        let path = self.path_str();
        let name = name.as_ref().to_str().unwrap();

        (run_cmd! {
            cd $path;
            git add $name;
        })
        .unwrap();
    }
}

fn init_logs() {
    let _ = tracing_subscriber::fmt::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .pretty()
        .try_init();
}

#[test]
fn can_open_blank() {
    init_logs();

    let dir = SampleRepoDir::new();
    Repo::open(dir.path()).unwrap();
}

#[test]
fn can_open_used() -> Result<()> {
    init_logs();

    let mut dir = SampleRepoDir::new();
    dir.change_something();
    dir.commit_all();

    let path = dir.path_str();
    (run_cmd! {
        cd $path;
        git log;
    })
    .unwrap();

    debug!("{:#?}", Repo::open(dir.path())?);

    Ok(())
}

#[test]
fn uncommitted() -> Result<()> {
    init_logs();

    let mut dir = SampleRepoDir::new();
    let repo = Repo::open(dir.path())?;

    dir.set_file("foo.txt", b"foo");
    dir.set_file("unchanged.txt", b"unchanged");
    dir.set_file("deleted.txt", b"deleted");
    dir.set_file("changed_to_bin", b"text");
    dir.set_file(".gitignore", b"*.ignored\n");
    dir.commit_all();

    dir.set_file("example.ignored", b"ignored");

    dir.delete_file("deleted.txt");

    dir.set_file("staged.txt", b"already staged");
    dir.add("staged.txt");

    dir.set_file("foo.txt", b"foobar");
    dir.set_file("qux.txt", b"quz");

    dir.create_dir("example_dir");

    debug!("{:#?}", repo.uncommitted()?);

    Ok(())
}

#[test]
fn uncommitted_no_commits_no_untracked() -> Result<()> {
    init_logs();

    let dir = SampleRepoDir::new();
    let repo = Repo::open(dir.path())?;
    assert_eq!(repo.uncommitted()?.len(), 0);

    Ok(())
}

#[test]
fn uncommitted_no_commits_with_untracked() -> Result<()> {
    init_logs();

    let mut dir = SampleRepoDir::new();
    dir.set_file("name", b"contents");
    dir.create_dir("example_dir");
    dir.set_file("example_dir/directories_arent_recursed_into", b"");

    let repo = Repo::open(dir.path())?;

    assert_matches!(repo.uncommitted()?.as_slice(), [
        FileDelta::Untracked(a), FileDelta::Untracked(b)] if
            a.rel_path().unwrap().to_str().unwrap() == "example_dir/" &&
            b.rel_path().unwrap().to_str().unwrap() == "name"
    );

    Ok(())
}

#[test]
fn stage_file() -> Result<()> {
    init_logs();

    let mut dir = SampleRepoDir::new();

    dir.set_file("a.txt", b"a");
    dir.set_file("b.txt", b"b");

    let mut repo = Repo::open(dir.path())?;

    let uncommitted = repo.uncommitted()?;
    let first_file = if let FileDelta::Untracked(file) = &uncommitted[0] {
        file
    } else {
        panic!();
    };

    repo.stage_file(&first_file)?;

    assert_matches!(
        repo.uncommitted()?.as_slice(),
        [FileDelta::Added(_), FileDelta::Untracked(_)]
    );

    Ok(())
}

#[test]
fn unstage_file() -> Result<()> {
    init_logs();
    let mut dir = SampleRepoDir::new();
    let mut repo = Repo::open(dir.path())?;

    dir.set_file("f", b"contents");
    let uncommitted = repo.uncommitted()?;
    assert_eq!(uncommitted.len(), 1);
    let file = if let FileDelta::Untracked(file) = &uncommitted[0] {
        file
    } else {
        panic!();
    };

    repo.stage_file(file)?;

    assert_matches!(repo.uncommitted()?.as_slice(), [FileDelta::Added(_)]);

    repo.unstage_file(file)?;

    assert_matches!(repo.uncommitted()?.as_slice(), [FileDelta::Untracked(_)]);

    Ok(())
}
