use crate::project::Project;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownProject {
    pub root: PathBuf,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub autocomplete_enabled: bool,
    pub auto_closing_enabled: bool,
    pub snippets_enabled: bool,
    pub workshop_enabled: bool,
    pub hsnips_enabled: bool,
    pub hsnips_path: PathBuf,
    pub projects: Vec<KnownProject>,
    pub last_project: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        // Discover local catalogs at runtime; never embed the builder's filesystem path.
        let mut candidates: Vec<PathBuf> = std::env::current_dir().into_iter().collect();
        if let Ok(exe) = std::env::current_exe() {
            candidates.extend(exe.ancestors().skip(1).take(5).map(Path::to_path_buf));
        }
        let hsnips_path = candidates
            .iter()
            .flat_map(|base| {
                [
                    base.join("repo/Hsnips-Snippets/for 0.2.7+/latex.hsnips"),
                    base.join("../repo/Hsnips-Snippets/for 0.2.7+/latex.hsnips"),
                ]
            })
            .find(|p| p.is_file())
            .and_then(|p| p.canonicalize().ok())
            .unwrap_or_default();
        Self {
            autocomplete_enabled: true,
            auto_closing_enabled: false,
            snippets_enabled: false,
            workshop_enabled: true,
            hsnips_enabled: true,
            hsnips_path,
            projects: vec![],
            last_project: None,
        }
    }
}

pub fn path() -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .context("Could not locate the local settings directory")?;
    Ok(base.join("TexStudio/settings.json"))
}

impl Settings {
    pub fn load() -> Result<Self> {
        let path = path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&fs::read(path)?).context("Invalid TexStudio settings")
    }
    pub fn save(&self) -> Result<()> {
        self.save_to(&path()?)
    }
    fn save_to(&self, path: &Path) -> Result<()> {
        let parent = path.parent().context("Missing settings folder")?;
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&serde_json::to_vec_pretty(self)?)?;
        temp.as_file().sync_all()?;
        temp.persist(path).map_err(|e| anyhow::anyhow!(e.error))?;
        Ok(())
    }
    pub fn remember(&mut self, project: &Project) {
        self.projects.retain(|p| p.root != project.root);
        self.projects.insert(
            0,
            KnownProject {
                root: project.root.clone(),
                name: project.config.name.clone(),
            },
        );
        self.last_project = Some(project.root.clone());
    }
    pub fn refresh_name(&mut self, project: &Project) {
        if let Some(known) = self.projects.iter_mut().find(|p| p.root == project.root) {
            known.name = project.config.name.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_and_opt_in_survive_restart() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let mut paper = Project::create(&temp.path().join("paper"))?;
        let other = Project::create(&temp.path().join("other"))?;
        let mut settings = Settings::default();
        assert!(!settings.snippets_enabled);
        assert!(settings.autocomplete_enabled);
        assert!(!settings.auto_closing_enabled);
        settings.autocomplete_enabled = false;
        settings.auto_closing_enabled = false;
        settings.remember(&paper);
        settings.remember(&other);
        settings.remember(&paper);
        assert_eq!(settings.projects.len(), 2);
        paper.rename("Research notes")?;
        settings.refresh_name(&paper);
        settings.snippets_enabled = true;
        let file = temp.path().join("preferences/settings.json");
        settings.save_to(&file)?;
        let restored: Settings = serde_json::from_slice(&fs::read(file)?)?;
        assert_eq!(restored, settings);
        assert_eq!(restored.projects[0].name, "Research notes");
        assert_eq!(Project::open(&paper.root)?.config.name, "Research notes");
        Ok(())
    }
}
