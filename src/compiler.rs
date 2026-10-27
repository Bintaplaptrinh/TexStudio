use crate::project::Project;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BuildReport {
    pub success: bool,
    pub elapsed_ms: u128,
    pub pdf: Option<PathBuf>,
    pub log: String,
}

fn lock_file(project: &Project) -> Result<fs::File> {
    Ok(fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(project.internal_dir()?.join("build.lock"))?)
}

pub fn busy(project: &Project) -> Result<bool> {
    let file = lock_file(project)?;
    match file.try_lock() {
        Ok(()) => Ok(false),
        Err(fs::TryLockError::WouldBlock) => Ok(true),
        Err(fs::TryLockError::Error(error)) => Err(error.into()),
    }
}

pub fn hidden_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}

pub fn doctor() -> serde_json::Value {
    let miktex = hidden_command("miktex").arg("--version").output();
    let texify = hidden_command("texify").arg("--version").output();
    serde_json::json!({
        "miktex": miktex.as_ref().is_ok_and(|r| r.status.success()),
        "texify": texify.as_ref().is_ok_and(|r| r.status.success()),
        "version": miktex.map(|r| String::from_utf8_lossy(&r.stdout).lines().next().unwrap_or("").to_owned()).unwrap_or_default(),
        "hint": "Install MiKTeX and required packages locally. Restart TexStudio after changing PATH. Automatic package installation is disabled during builds."
    })
}

pub fn build(project: &Project) -> Result<BuildReport> {
    let internal = project.internal_dir()?;
    let mut lock = lock_file(project)?;
    lock.try_lock().context("A build is already running")?;
    lock.set_len(0)?;
    writeln!(lock, "{}", std::process::id())?;
    // Mirror sources into an isolated, persistent workspace. Unchanged files and
    // auxiliary files survive between runs, so texify can reuse its work.
    let work = internal.join(format!("work-{}", project.config.engine));
    if work.exists() {
        ensure!(
            !crate::project::is_link(&fs::symlink_metadata(&work)?)
                && work.canonicalize()?.starts_with(&internal),
            "Build workspace must stay inside the project"
        );
    }
    fs::create_dir_all(&work)?;
    let source_list = internal.join(format!("sources-{}.json", project.config.engine));
    let previous: Vec<String> = fs::read(&source_list)
        .ok()
        .and_then(|s| serde_json::from_slice(&s).ok())
        .unwrap_or_default();
    let mut sources = Vec::new();
    for entry in project.entries()? {
        if entry.directory {
            continue;
        }
        let source = project.resolve(&entry.path)?;
        let target = work.join(&entry.path);
        fs::create_dir_all(target.parent().unwrap())?;
        let content = fs::read(source)?;
        if fs::read(&target).ok().as_ref() != Some(&content) {
            fs::write(&target, content)?;
        }
        sources.push(entry.path);
    }
    for stale in previous.iter().filter(|p| !sources.contains(p)) {
        project.resolve(stale)?;
        let target = work.join(stale);
        if target.is_file() {
            fs::remove_file(target)?;
        }
    }
    fs::write(source_list, serde_json::to_vec(&sources)?)?;
    let transcript = internal.join("compiler-output.txt");
    let output = fs::File::create(&transcript)?;
    let start = Instant::now();
    let mut cmd = hidden_command("texify");
    cmd.current_dir(&work)
        .args(["--pdf", "--batch", "--quiet", "--max-iterations=5"])
        .arg(format!("--engine={}", project.config.engine))
        .arg("--tex-option=--disable-installer")
        .arg("--tex-option=--disable-write18")
        .arg("--tex-option=--interaction=nonstopmode")
        .arg("--tex-option=--halt-on-error")
        .arg("--tex-option=--file-line-error")
        .arg("--tex-option=--synctex=1")
        .arg("--")
        .arg(&project.config.main)
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output);
    let mut child = cmd
        .spawn()
        .context("MiKTeX texify was not found. Install MiKTeX and add its bin folder to PATH")?;
    #[cfg(windows)]
    let _job = crate::process_job::ProcessJob::attach(&mut child)?;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(45) {
            timed_out = true;
            #[cfg(windows)]
            {
                let _ = hidden_command("taskkill")
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .output();
            }
            let _ = child.kill();
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(30));
    };
    let stem = Path::new(&project.config.main)
        .file_stem()
        .context("Missing main file name")?
        .to_string_lossy();
    let mut log = fs::read_to_string(&transcript).unwrap_or_default();
    if let Ok(tex_log) = fs::read_to_string(work.join(format!("{stem}.log"))) {
        log.push('\n');
        log.push_str(&tex_log);
    }
    if timed_out {
        log.push_str(
            "\nBuild stopped after 45 seconds. Check the document and installed MiKTeX packages.",
        );
    }
    let generated = work.join(format!("{stem}.pdf"));
    let success = status.success() && generated.is_file() && !timed_out;
    let preview = internal.join("preview.pdf");
    if success {
        // NamedTempFile::persist replaces atomically on Windows and Unix.
        let mut copy = tempfile::NamedTempFile::new_in(&internal)?;
        let mut input = fs::File::open(generated)?;
        std::io::copy(&mut input, copy.as_file_mut())?;
        copy.persist(&preview)
            .map_err(|e| anyhow::anyhow!(e.error))?;
    }
    if log.len() > 160_000 {
        let mut offset = log.len() - 160_000;
        while !log.is_char_boundary(offset) {
            offset += 1;
        }
        log = log[offset..].to_owned();
    }
    let report = BuildReport {
        success,
        elapsed_ms: start.elapsed().as_millis(),
        pdf: preview.is_file().then_some(preview),
        log,
    };
    let mut saved = tempfile::NamedTempFile::new_in(&internal)?;
    saved.write_all(&serde_json::to_vec_pretty(&report)?)?;
    saved
        .persist(internal.join("status.json"))
        .map_err(|e| anyhow::anyhow!(e.error))?;
    Ok(report)
}

pub fn watch(project: &Project) -> Result<()> {
    let mut last = None;
    let mut changed = Instant::now();
    let mut built = None;
    loop {
        let fingerprint = project.fingerprint()?;
        if last != Some(fingerprint) {
            last = Some(fingerprint);
            changed = Instant::now();
        }
        if built != last
            && changed.elapsed() >= Duration::from_millis(project.config.debounce_ms)
            && project.ready()?
        {
            let current = Project::open(&project.root)?;
            let report = build(&current)?;
            println!("{}", serde_json::to_string(&report)?);
            built = last;
        }
        thread::sleep(Duration::from_millis(150));
    }
}

pub fn successful(report: &BuildReport) -> Result<()> {
    ensure!(
        report.success,
        "MiKTeX compilation failed; see the build report log"
    );
    Ok(())
}
