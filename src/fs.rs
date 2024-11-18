use std::path::{Component, Path, PathBuf};

use anyhow::{ensure, Context};
use rayon::iter::{ParallelBridge, ParallelIterator};

use crate::Result;

pub fn ls<'a, P: AsRef<Path> + 'a, T: IntoIterator<Item = &'a P>>(
    dirs: T,
) -> Result<Vec<String>> {
    let mut entries = vec![];
    for entry in dirs {
        let entry = entry.as_ref();
        ensure!(
            entry.exists(),
            "ls item `{}` does not exist",
            entry.display()
        );
        if entry.is_file() {
            entries.push(format!("{}", entry.display()));
        } else {
            for entry in entry.read_dir()?.flatten() {
                entries.push(entry.path().to_string_lossy().to_string());
            }
        }
    }
    Ok(entries)
}

pub fn rm<'a, P: AsRef<Path> + 'a, T: IntoIterator<Item = &'a P>>(
    entries: T,
    force: bool,
) -> Result<()> {
    for entry in entries {
        let entry = entry.as_ref();
        if !entry.exists() {
            continue;
        }
        if entry.is_file() {
            std::fs::remove_file(entry)?;
            continue;
        }
        if entry.is_dir() {
            if force {
                std::fs::remove_dir_all(entry)?;
            } else {
                std::fs::remove_dir(entry)?;
            }
        }
    }
    Ok(())
}

pub fn mkdir<'a, P: AsRef<Path> + 'a, T: IntoIterator<Item = &'a P>>(
    dirs: T,
) -> Result<()> {
    for dir in dirs {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}

pub fn abs<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    let path = path.as_ref();
    if path.exists() {
        return Ok(dunce::canonicalize(path)?);
    }
    let path = if path.is_relative() {
        std::env::current_dir()?.join(path)
    } else {
        path.to_path_buf()
    };

    let mut components = path.components().peekable();
    let mut ret =
        if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
            components.next();
            PathBuf::from(c.as_os_str())
        } else {
            PathBuf::new()
        };
    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    Ok(ret)
}

#[derive(Debug, Clone)]
pub struct Copier {
    src: Vec<PathBuf>,
    dst: PathBuf,
    includes: Vec<String>,
    excludes: Vec<String>,
}

impl Copier {
    pub fn run(&self) -> Result<()> {
        let should_create_dst = match self.src.len() {
            0 => return Ok(()),
            1 => !self.src[0].is_file(),
            _ => true,
        };
        if should_create_dst {
            mkdir(&[self.dst.as_path()])?;
        }
        for src in self.src.iter() {
            let src_ref = src.as_path();
            let dst_ref = self.dst.as_path();
            walkdir::WalkDir::new(src_ref)
                .into_iter()
                .par_bridge()
                .flatten()
                .filter(|d| d.file_type().is_file())
                .try_for_each(|entry| -> Result<()> {
                    let source_file = entry.path();

                    if !is_interested_file(
                        src_ref,
                        source_file,
                        &self.includes,
                        &self.excludes,
                    ) {
                        return Ok(());
                    }

                    let relatvie_path = source_file.strip_prefix(src_ref)?;

                    let dest_file = dst_ref.join(relatvie_path);
                    let dest_file_dir = dest_file.parent().context(format!(
                        "failed to get parent directory from {}",
                        dest_file.display()
                    ))?;
                    if !dest_file_dir.exists() {
                        std::fs::create_dir_all(dest_file_dir).context(
                            format!(
                                "failed to create directory {}",
                                dest_file_dir.display()
                            ),
                        )?;
                    }

                    std::fs::copy(source_file, &dest_file).context(format!(
                        "failed to copy {} to {}",
                        source_file.display(),
                        dest_file.display()
                    ))?;

                    Ok(())
                })?;
        }

        Ok(())
    }
}

pub struct CopierBuilder {
    copier: Copier,
}

impl CopierBuilder {
    pub fn new<P: AsRef<Path>>(dst: P) -> Self {
        CopierBuilder {
            copier: Copier {
                src: vec![],
                dst: dst.as_ref().to_path_buf(),
                includes: vec![],
                excludes: vec![],
            },
        }
    }

    pub fn add<P: AsRef<Path>>(mut self, src: P) -> Self {
        self.copier.src.push(src.as_ref().to_path_buf());
        self
    }

    pub fn ipat<P: AsRef<str>>(mut self, pat: P) -> Self {
        self.copier.includes.push(pat.as_ref().to_string());
        self
    }

    pub fn epat<P: AsRef<str>>(mut self, epat: P) -> Self {
        self.copier.includes.push(epat.as_ref().to_string());
        self
    }

    pub fn build(&self) -> Copier {
        self.copier.clone()
    }
}

/// is_interested_file checks if file `file_path` under directory `root` is
/// interested by caller with include patterns `include_patterns` and exclude
/// patterns `exclude_patterns`.
///
/// If `file_path` is not under `root`, return false.
///
/// `include_patterns` will be ignored if it's empty or its an absolute path,
/// the same applys to `exclude_patterns`. Both `include_patterns` and
/// `exclude_patterns` obey the general `glob` syntax, see
/// [glob::Pattern](https://docs.rs/glob/latest/glob/struct.Pattern.html) and
/// [man7/glob.7](https://man7.org/linux/man-pages/man7/glob.7.html) for details.
///
/// When both `include_patterns` and `exclude_patterns` are provided, and
/// `file_path` matches both of them, `exclude_patterns` takes precedence.
///
/// When both `include_patterns` and `exclude_patterns` are empty, the
/// result is true.
///
pub fn is_interested_file<
    R: AsRef<Path>,
    F: AsRef<Path>,
    I: AsRef<str>,
    E: AsRef<str>,
>(
    root: R,
    file_path: F,
    include_patterns: &[I],
    exclude_patterns: &[E],
) -> bool {
    let root = root.as_ref();
    let file_path = file_path.as_ref();

    if !file_path.is_file() {
        return false;
    }

    let relative_file_path = if file_path.is_absolute() {
        let Ok(p) = file_path.strip_prefix(root) else {
            return false;
        };
        p
    } else {
        file_path
    };

    let options = glob::MatchOptions {
        case_sensitive: !cfg!(windows),
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };

    if !exclude_patterns.is_empty() {
        for pat in exclude_patterns {
            let pat = pat.as_ref();
            if Path::new(pat).is_absolute() {
                continue;
            }
            let Ok(pat) = glob::Pattern::new(pat) else {
                continue;
            };
            if pat.matches_path_with(relative_file_path, options) {
                return false;
            }
        }
        return true;
    }

    if !include_patterns.is_empty() {
        for pat in include_patterns {
            let pat = pat.as_ref();
            if Path::new(pat).is_absolute() {
                continue;
            }
            let Ok(pat) = glob::Pattern::new(pat) else {
                continue;
            };
            if pat.matches_path_with(relative_file_path, options) {
                return true;
            }
        }
        return false;
    }

    true
}
