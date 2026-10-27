use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::{
    io::{self, Read},
    path::PathBuf,
};
use texstudio_local::{compiler, project::Project};

#[cfg(feature = "desktop")]
mod ui;

#[derive(Parser)]
#[command(version, about = "A local LaTeX workspace. No account. No server.")]
struct Args {
    /// Project directory (defaults to the current directory).
    #[arg(short, long, global = true, default_value = ".")]
    project: PathBuf,
    #[command(subcommand)]
    command: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// Create a starter project in a new or empty folder.
    New { directory: PathBuf },
    /// Check the local MiKTeX installation.
    Doctor,
    /// Return the configured snippet catalog, including HyperSnips metadata.
    Snippets,
    /// Launch the desktop editor.
    Open,
    /// Return project configuration and build status as JSON.
    Info,
    /// List project files as JSON.
    List,
    /// Read a UTF-8 file as a JSON object.
    Read { path: String },
    /// Atomically write UTF-8 text from stdin. Use --create to prevent overwrites.
    Write {
        path: String,
        #[arg(long)]
        create: bool,
    },
    /// Import a binary or text file without overwriting.
    Import {
        source: PathBuf,
        destination: String,
    },
    /// Create a project directory.
    Mkdir { path: String },
    /// Move or rename a file or directory without overwriting.
    Move { source: String, destination: String },
    /// Move a file or directory into .texstudio/trash (recoverable).
    Delete { path: String },
    /// Build once; emits JSON, exits nonzero on failure.
    Build,
    /// Watch disk changes and build automatically; emits one JSON report per build.
    Watch,
}

fn emit(value: impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

fn run() -> Result<()> {
    let args = Args::parse();
    match args.command {
        Some(Action::New { directory }) => {
            let p = Project::create(&directory)?;
            emit(serde_json::json!({"root":p.root, "config":p.config}))
        }
        Some(Action::Doctor) => emit(compiler::doctor()),
        Some(Action::Snippets) => {
            let settings = texstudio_local::settings::Settings::load()?;
            emit(
                serde_json::json!({"enabled":settings.snippets_enabled,"catalog":texstudio_local::snippets::load(&settings)}),
            )
        }
        None | Some(Action::Open) => {
            #[cfg(feature = "desktop")]
            {
                ui::launch(args.project);
                Ok(())
            }
            #[cfg(not(feature = "desktop"))]
            {
                anyhow::bail!(
                    "Desktop support is disabled. Build with default features to open the editor."
                )
            }
        }
        Some(action) => {
            let p = Project::open(&args.project)?;
            match action {
                Action::Info => {
                    let status = std::fs::read(p.internal_dir()?.join("status.json"))
                        .ok()
                        .and_then(|s| serde_json::from_slice::<serde_json::Value>(&s).ok());
                    emit(serde_json::json!({"root":p.root, "config":p.config, "build":status}))
                }
                Action::List => emit(p.entries()?),
                Action::Read { path } => {
                    emit(serde_json::json!({"path":path, "content":p.read(&path)?}))
                }
                Action::Write { path, create } => {
                    let mut content = String::new();
                    io::stdin()
                        .read_to_string(&mut content)
                        .context("Provide UTF-8 content on stdin")?;
                    if create {
                        p.create_file(&path, &content)?;
                    } else {
                        p.write(&path, &content)?;
                    }
                    emit(serde_json::json!({"ok":true, "path":path, "bytes":content.len()}))
                }
                Action::Import {
                    source,
                    destination,
                } => {
                    p.import(&source, &destination)?;
                    emit(serde_json::json!({"ok":true,"path":destination}))
                }
                Action::Mkdir { path } => {
                    p.mkdir(&path)?;
                    emit(serde_json::json!({"ok":true,"path":path}))
                }
                Action::Move {
                    source,
                    destination,
                } => {
                    p.move_file(&source, &destination)?;
                    emit(serde_json::json!({"ok":true,"path":destination}))
                }
                Action::Delete { path } => {
                    let trash = p.trash(&path)?;
                    emit(serde_json::json!({"ok":true,"trash":trash}))
                }
                Action::Build => {
                    let report = compiler::build(&p)?;
                    emit(&report)?;
                    compiler::successful(&report)
                }
                Action::Watch => compiler::watch(&p),
                _ => unreachable!(),
            }
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!(
            "{}",
            serde_json::json!({"ok":false, "error":format!("{error:#}")})
        );
        std::process::exit(1);
    }
}
