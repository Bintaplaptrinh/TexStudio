use crate::project::Project;
use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Symbol {
    pub name: String,
    pub path: String,
    pub args: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ProjectIndex {
    pub files: Vec<String>,
    pub labels: Vec<Symbol>,
    pub citations: Vec<Symbol>,
    pub commands: Vec<Symbol>,
    pub environments: Vec<Symbol>,
    pub packages: Vec<String>,
    pub truncated: bool,
}

static LABEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\label\s*\{([^{}]+)\}").unwrap());
static CITE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)@([a-z]+)\s*[\{(]\s*([^,\s{}()]+)\s*,").unwrap());
static COMMAND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(?:newcommand|renewcommand|providecommand|DeclareRobustCommand|DeclareMathOperator)\*?\s*\{?\s*\\([A-Za-z@]+)\s*\}?\s*(?:\[([0-9])\])?").unwrap()
});
static DEF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(?:gdef|edef|xdef|def)\s*\\([A-Za-z@]+)((?:#[1-9])*)").unwrap()
});
static ENVIRONMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(?:newenvironment|renewenvironment)\*?\s*\{([^{}]+)\}").unwrap()
});
static PACKAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(?:usepackage|RequirePackage)(?:\[[^\]]*\])?\s*\{([^{}]+)\}").unwrap()
});

fn without_comments(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut slashes = 0;
            for (index, c) in line.char_indices() {
                if c == '%' && slashes % 2 == 0 {
                    return &line[..index];
                }
                if c == '\\' {
                    slashes += 1;
                } else {
                    slashes = 0;
                }
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl ProjectIndex {
    pub fn build(project: &Project) -> Result<Self> {
        let mut index = Self::default();
        let mut bytes = 0;
        let mut documents = 0;
        for entry in project.entries()? {
            if entry.directory {
                continue;
            }
            index.files.push(entry.path.clone());
            if ![".tex", ".sty", ".cls", ".bib"]
                .iter()
                .any(|extension| entry.path.ends_with(extension))
            {
                continue;
            }
            if bytes + entry.bytes > 16 * 1024 * 1024
                || documents >= 256
                || entry.bytes > 4 * 1024 * 1024
            {
                index.truncated = true;
                continue;
            }
            if let Ok(text) = project.read(&entry.path) {
                bytes += entry.bytes;
                documents += 1;
                index.add(&entry.path, &text);
            }
        }
        index.packages.sort();
        index.packages.dedup();
        Ok(index)
    }

    fn add(&mut self, path: &str, source: &str) {
        let source = without_comments(source);
        if path.ends_with(".bib") {
            for captures in CITE.captures_iter(&source) {
                if ["string", "comment", "preamble"]
                    .contains(&captures[1].to_ascii_lowercase().as_str())
                {
                    continue;
                }
                self.citations.push(Symbol {
                    name: captures[2].to_owned(),
                    path: path.into(),
                    args: 0,
                });
            }
            return;
        }
        for captures in LABEL.captures_iter(&source) {
            self.labels.push(Symbol {
                name: captures[1].trim().into(),
                path: path.into(),
                args: 0,
            });
        }
        for captures in COMMAND.captures_iter(&source) {
            self.commands.push(Symbol {
                name: captures[1].into(),
                path: path.into(),
                args: captures
                    .get(2)
                    .and_then(|s| s.as_str().parse().ok())
                    .unwrap_or(0),
            });
        }
        for captures in DEF.captures_iter(&source) {
            self.commands.push(Symbol {
                name: captures[1].into(),
                path: path.into(),
                args: captures[2].matches('#').count(),
            });
        }
        for captures in ENVIRONMENT.captures_iter(&source) {
            self.environments.push(Symbol {
                name: captures[1].trim().into(),
                path: path.into(),
                args: 0,
            });
        }
        for captures in PACKAGE.captures_iter(&source) {
            self.packages.extend(
                captures[1]
                    .split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indexes_project_symbols_and_external_changes() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let project = Project::create(&temporary.path().join("paper"))?;
        project.write("sections/definitions.tex", "% \\label{ignored}\n\\label{sec:intro}\n\\newcommand{\\myvector}[2]{#1+#2}\n\\def\\vect#1{#1}\n\\newenvironment{claim}{}{}\n\\usepackage{amsmath,hyperref}")?;
        project.write("references.bib", "@article{doe2026, title={Example}}\n@string{ignored={Text}}\n% @book{hidden, title={x}}")?;
        let index = ProjectIndex::build(&project)?;
        assert!(index.labels.iter().any(|s| s.name == "sec:intro"));
        assert!(!index.labels.iter().any(|s| s.name == "ignored"));
        assert!(
            index
                .commands
                .iter()
                .any(|s| s.name == "myvector" && s.args == 2)
        );
        assert!(
            index
                .commands
                .iter()
                .any(|s| s.name == "vect" && s.args == 1)
        );
        assert!(index.environments.iter().any(|s| s.name == "claim"));
        assert_eq!(
            index
                .citations
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            vec!["doe2026"]
        );
        project.write("references.bib", "@book{newkey, title={New}}")?;
        assert_eq!(ProjectIndex::build(&project)?.citations[0].name, "newkey");
        Ok(())
    }
}
