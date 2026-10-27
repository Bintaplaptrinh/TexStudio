use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    io::Write,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const CONFIG: &str = "texstudio.json";

pub fn is_link(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}
pub const STARTER: &str = r#"\documentclass[11pt]{article}
\usepackage[T1]{fontenc}
\usepackage{amsmath}
\usepackage[margin=1in]{geometry}

\title{A space for your next idea}
\author{Your name}
\date{\today}

\begin{document}
\maketitle

\begin{abstract}
Good ideas begin with a little space to think.
This document lives on your computer. Make it your own.
\end{abstract}

\section{Introduction}
Welcome to TexStudio, your local LaTeX workspace.
Write a sentence, finish an equation, and let the preview follow.

\section{A beautiful equation}
The Gaussian integral connects analysis, geometry, and probability:
\begin{equation}
  \int_{-\infty}^{\infty} e^{-x^2}\,dx = \sqrt{\pi}.
\end{equation}

\section{What comes next}
Add your notes, explore an idea, or start a new chapter.
Your files are always yours.

\end{document}
"#;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub name: String,
    pub main: String,
    pub engine: String,
    pub debounce_ms: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            name: "Untitled project".into(),
            main: "main.tex".into(),
            engine: "pdftex".into(),
            debounce_ms: 650,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub root: PathBuf,
    pub config: Config,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub path: String,
    pub directory: bool,
    pub bytes: u64,
}

impl Project {
    pub fn create(root: &Path) -> Result<Self> {
        ensure!(
            !root.exists() || fs::read_dir(root)?.next().is_none(),
            "Choose a new or empty project folder"
        );
        fs::create_dir_all(root)?;
        let root = root.canonicalize()?;
        let config = Config {
            name: root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            ..Config::default()
        };
        fs::write(root.join(CONFIG), serde_json::to_vec_pretty(&config)?)?;
        fs::write(root.join("main.tex"), STARTER)?;
        Self::open(&root)
    }

    pub fn open(root: &Path) -> Result<Self> {
        let root = root
            .canonicalize()
            .context("Project folder does not exist")?;
        ensure!(root.is_dir(), "Expected a project folder");
        let config = if root.join(CONFIG).exists() {
            serde_json::from_slice(&fs::read(root.join(CONFIG))?)
                .context("Invalid texstudio.json")?
        } else {
            Config {
                name: root
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                ..Config::default()
            }
        };
        ensure!(
            ["pdftex", "xetex", "luatex"].contains(&config.engine.as_str()),
            "Engine must be pdftex, xetex, or luatex"
        );
        ensure!(
            (150..=10_000).contains(&config.debounce_ms),
            "debounce_ms must be between 150 and 10000"
        );
        let project = Self { root, config };
        let main = project.resolve(&project.config.main)?;
        ensure!(
            main.is_file() && main.extension().is_some_and(|e| e == "tex"),
            "Main document must be an existing .tex file (set main in texstudio.json)"
        );
        Ok(project)
    }

    /// Reject traversal, absolute paths, and every symlink/junction component.
    pub fn resolve(&self, relative: &str) -> Result<PathBuf> {
        let path = Path::new(relative);
        ensure!(
            !relative.is_empty()
                && path.components().all(|c| matches!(c, Component::Normal(_)))
                && relative
                    .split('/')
                    .all(|p| !p.is_empty() && p != "." && p != ".."),
            "Use a relative path without . or .."
        );
        ensure!(
            !relative.contains(':') && !relative.contains('\\'),
            "Use slash-separated project paths"
        );
        ensure!(
            !path
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.')),
            "Hidden and internal paths are reserved"
        );
        let mut target = self.root.clone();
        for part in path.components() {
            target.push(part);
            if let Ok(meta) = fs::symlink_metadata(&target) {
                ensure!(!is_link(&meta), "Symbolic links are not supported");
                let canonical = target.canonicalize()?;
                ensure!(canonical.starts_with(&self.root), "Path leaves the project");
            }
        }
        Ok(target)
    }

    pub fn entries(&self) -> Result<Vec<Entry>> {
        fn visit(project: &Project, dir: &Path, out: &mut Vec<Entry>) -> Result<()> {
            for item in fs::read_dir(dir)? {
                let item = item?;
                let name = item.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') || name == "target" {
                    continue;
                }
                let path = item.path();
                let relative = path
                    .strip_prefix(&project.root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                // Never follow links, including Windows directory junctions.
                if project.resolve(&relative).is_err() {
                    continue;
                }
                let meta = fs::symlink_metadata(&path)?;
                if is_link(&meta) {
                    continue;
                }
                out.push(Entry {
                    path: relative,
                    directory: meta.is_dir(),
                    bytes: meta.len(),
                });
                if meta.is_dir() {
                    visit(project, &path, out)?;
                }
            }
            Ok(())
        }
        let mut entries = Vec::new();
        visit(self, &self.root, &mut entries)?;
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    pub fn read(&self, path: &str) -> Result<String> {
        let full = self.resolve(path)?;
        ensure!(
            full.metadata()?.len() <= 4 * 1024 * 1024,
            "Text editor limit is 4 MiB; use import for larger assets"
        );
        fs::read_to_string(full).context("File is not UTF-8 text")
    }

    pub fn write(&self, path: &str, content: &str) -> Result<()> {
        let full = self.resolve(path)?;
        let parent = full.parent().context("Missing parent directory")?;
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(content.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(&full)
            .map_err(|e| anyhow::anyhow!("Could not save {path}: {}", e.error))?;
        Ok(())
    }

    pub fn create_file(&self, path: &str, content: &str) -> Result<()> {
        let target = self.resolve(path)?;
        let parent = target.parent().context("Missing parent directory")?;
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(content.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist_noclobber(target)
            .map_err(|e| anyhow::anyhow!("Could not create {path}: {}", e.error))?;
        Ok(())
    }

    pub fn rename(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        ensure!(
            !name.is_empty() && name.chars().count() <= 120 && !name.chars().any(char::is_control),
            "Project names must contain 1–120 visible characters"
        );
        let mut config = self.config.clone();
        config.name = name.to_owned();
        self.write(CONFIG, &serde_json::to_string_pretty(&config)?)?;
        self.config = config;
        Ok(())
    }

    pub fn mkdir(&self, path: &str) -> Result<()> {
        fs::create_dir_all(self.resolve(path)?)?;
        Ok(())
    }

    pub fn import(&self, source: &Path, to: &str) -> Result<()> {
        ensure!(source.is_file(), "Import expects one file");
        let target = self.resolve(to)?;
        // create_new prevents accidental overwrites and preserves binary content.
        fs::create_dir_all(target.parent().unwrap())?;
        let mut input = fs::File::open(source)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        std::io::copy(&mut input, &mut output)?;
        Ok(())
    }

    pub fn move_file(&self, from: &str, to: &str) -> Result<()> {
        let source = self.resolve(from)?;
        let target = self.resolve(to)?;
        ensure!(!target.exists(), "Destination already exists");
        ensure!(
            !self.protects_main(from),
            "Update main in texstudio.json before moving the main document or its folder"
        );
        ensure!(from != CONFIG, "Project configuration cannot be moved");
        ensure!(
            !target.starts_with(&source),
            "Cannot move a directory into itself"
        );
        fs::create_dir_all(target.parent().unwrap())?;
        fs::rename(source, target)?;
        Ok(())
    }

    fn protects_main(&self, path: &str) -> bool {
        Path::new(&self.config.main).starts_with(Path::new(path))
    }

    /// A recoverable delete; never recursively removes user data.
    pub fn trash(&self, path: &str) -> Result<PathBuf> {
        ensure!(
            !self.protects_main(path) && path != CONFIG,
            "Main document and project configuration cannot be deleted"
        );
        let source = self.resolve(path)?;
        ensure!(source.exists(), "File does not exist");
        let trash = self.internal_dir()?.join("trash");
        fs::create_dir_all(&trash)?;
        let id = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let destination = trash.join(format!(
            "{id}-{}",
            source.file_name().unwrap().to_string_lossy()
        ));
        fs::rename(source, &destination)?;
        Ok(destination)
    }

    pub fn internal_dir(&self) -> Result<PathBuf> {
        let dir = self.root.join(".texstudio");
        if dir.exists() {
            ensure!(
                !is_link(&fs::symlink_metadata(&dir)?)
                    && dir.canonicalize()?.starts_with(&self.root),
                "Internal folder must stay inside the project"
            );
        }
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn fingerprint(&self) -> Result<u64> {
        let mut state = DefaultHasher::new();
        for entry in self.entries()? {
            entry.path.hash(&mut state);
            entry.bytes.hash(&mut state);
            if !entry.directory {
                let path = self.resolve(&entry.path)?;
                fs::metadata(&path)?.modified()?.hash(&mut state);
                if ["tex", "bib", "sty", "cls", "json"]
                    .contains(&path.extension().and_then(|s| s.to_str()).unwrap_or(""))
                {
                    fs::read(path)?.hash(&mut state);
                }
            }
        }
        Ok(state.finish())
    }

    pub fn ready(&self) -> Result<bool> {
        if !crate::syntax::ready(&self.read(&self.config.main)?) {
            return Ok(false);
        }
        Ok(true)
    }
}

pub fn text_file(path: &str) -> bool {
    ["tex", "bib", "sty", "cls", "txt", "md", "json", "csv"].contains(
        &Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or(""),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confined_file_lifecycle() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let p = Project::create(&dir.path().join("paper"))?;
        for bad in [
            "../secret",
            "/absolute",
            "C:/secret",
            "a/../../secret",
            ".texstudio/log",
            "a\\b",
            "a/./b",
        ] {
            assert!(p.resolve(bad).is_err(), "{bad}");
        }
        p.write("sections/intro.tex", "hello")?;
        p.move_file("sections/intro.tex", "intro.tex")?;
        assert_eq!(p.read("intro.tex")?, "hello");
        assert!(p.trash("main.tex").is_err());
        let trashed = p.trash("intro.tex")?;
        assert_eq!(fs::read_to_string(trashed)?, "hello");
        assert!(!p.root.join("intro.tex").exists());
        Ok(())
    }
    #[test]
    fn binary_import_and_fingerprints() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let p = Project::create(&dir.path().join("paper"))?;
        let before = p.fingerprint()?;
        let source = dir.path().join("image.bin");
        fs::write(&source, [0, 255, 1])?;
        p.import(&source, "figures/image.bin")?;
        assert!(p.import(&source, "figures/image.bin").is_err());
        assert_ne!(before, p.fingerprint()?);
        assert_eq!(fs::read(p.resolve("figures/image.bin")?)?, [0, 255, 1]);
        Ok(())
    }
}
