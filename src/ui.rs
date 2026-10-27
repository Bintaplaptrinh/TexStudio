use base64::{Engine, engine::general_purpose::STANDARD};
use dioxus::{
    desktop::{Config, LogicalSize, WindowBuilder, wry::http::Response},
    prelude::*,
};
use std::{
    borrow::Cow,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use texstudio_local::{
    compiler,
    completion::ProjectIndex,
    project::{Entry, Project, text_file},
    settings::Settings,
    snippets::{self, Catalog},
    syntax,
};

#[derive(Clone, PartialEq)]
struct Session {
    project: Option<Project>,
    entries: Vec<Entry>,
    active: String,
    content: String,
    baseline: String,
    edited: Option<Instant>,
    conflict: bool,
    status: String,
    log: String,
    pdf: Arc<str>,
    pdf_revision: u64,
    elapsed_ms: u128,
    auto: bool,
    settings: Settings,
    catalog: Arc<Catalog>,
    catalog_revision: u64,
    project_revision: u64,
    completion_index: Arc<ProjectIndex>,
    completion_revision: u64,
    document_revision: u64,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            project: None,
            entries: vec![],
            active: String::new(),
            content: String::new(),
            baseline: String::new(),
            edited: None,
            conflict: false,
            status: "Open a project to begin".into(),
            log: String::new(),
            pdf: Arc::from(""),
            pdf_revision: 0,
            elapsed_ms: 0,
            auto: true,
            settings: Settings::default(),
            catalog: Arc::new(Catalog::default()),
            catalog_revision: 0,
            project_revision: 0,
            completion_index: Arc::new(ProjectIndex::default()),
            completion_revision: 0,
            document_revision: 0,
        }
    }
}
type Shared = Arc<Mutex<Session>>;

fn load(session: &mut Session, project: Project) -> anyhow::Result<()> {
    flush(session)?;
    let content = project.read(&project.config.main)?;
    let completion_index = Arc::new(ProjectIndex::build(&project)?);
    let pdf = fs::read(project.internal_dir()?.join("preview.pdf"))
        .map(|b| STANDARD.encode(b))
        .unwrap_or_default();
    let mut settings = session.settings.clone();
    settings.remember(&project);
    settings.save()?;
    *session = Session {
        entries: project.entries()?,
        active: project.config.main.clone(),
        baseline: content.clone(),
        content,
        project: Some(project),
        pdf: pdf.into(),
        pdf_revision: session.pdf_revision + 1,
        status: "Waiting for changes".into(),
        settings,
        catalog: session.catalog.clone(),
        catalog_revision: session.catalog_revision,
        project_revision: session.project_revision + 1,
        completion_index,
        completion_revision: session.completion_revision + 1,
        document_revision: session.document_revision + 1,
        ..Session::default()
    };
    Ok(())
}

fn flush(session: &mut Session) -> anyhow::Result<()> {
    anyhow::ensure!(
        !session.conflict,
        "Resolve the current file conflict before switching projects"
    );
    if session.edited.is_some()
        && let Some(project) = &session.project
    {
        anyhow::ensure!(
            project.read(&session.active)? == session.baseline,
            "The file changed on disk. Resolve the conflict before switching projects"
        );
        project.write(&session.active, &session.content)?;
        session.baseline = session.content.clone();
        session.edited = None;
    }
    Ok(())
}

fn rename_project(shared: &Shared, name: &str) {
    let result = (|| -> anyhow::Result<()> {
        let mut session = shared.lock().unwrap();
        flush(&mut session)?;
        let Some(mut project) = session.project.clone() else {
            return Ok(());
        };
        if project.config.name == name.trim() {
            return Ok(());
        }
        project.rename(name)?;
        session.settings.refresh_name(&project);
        session.settings.save()?;
        if session.active == "texstudio.json" {
            session.content = project.read("texstudio.json")?;
            session.baseline = session.content.clone();
            session.document_revision += 1;
        }
        session.project = Some(project);
        Ok(())
    })();
    if let Err(error) = result {
        shared.lock().unwrap().status = error.to_string();
    }
}

fn update_settings(shared: &Shared, change: impl FnOnce(&mut Settings)) {
    let mut session = shared.lock().unwrap();
    let mut settings = session.settings.clone();
    change(&mut settings);
    match settings.save() {
        Ok(()) => {
            session.catalog = Arc::new(snippets::load(&settings));
            session.catalog_revision += 1;
            session.settings = settings;
        }
        Err(error) => session.status = format!("Could not save settings: {error}"),
    }
}

fn worker(shared: Shared) {
    thread::spawn(move || {
        let mut identity = PathBuf::new();
        let mut project_revision = 0;
        let mut observed = None;
        let mut built = None;
        let mut changed = Instant::now();
        let mut running: Option<(
            PathBuf,
            Option<u64>,
            mpsc::Receiver<anyhow::Result<compiler::BuildReport>>,
        )> = None;
        loop {
            thread::sleep(Duration::from_millis(120));
            let mut session = shared.lock().unwrap();
            let Some(mut project) = session.project.clone() else {
                continue;
            };
            if identity != project.root || project_revision != session.project_revision {
                identity = project.root.clone();
                project_revision = session.project_revision;
                observed = None;
                built = None;
            }
            if let Some((root, fingerprint, rx)) = &running
                && let Ok(report) = rx.try_recv()
            {
                if root == &project.root {
                    built = *fingerprint;
                    match report {
                        Ok(report) => {
                            session.elapsed_ms = report.elapsed_ms;
                            session.status = if report.success {
                                "Preview is up to date"
                            } else {
                                "LaTeX needs attention · last preview kept"
                            }
                            .into();
                            session.log = report.log;
                            if report.success
                                && let Some(bytes) = report.pdf.and_then(|p| fs::read(p).ok())
                            {
                                session.pdf = STANDARD.encode(bytes).into();
                                session.pdf_revision += 1;
                            }
                        }
                        Err(error) => {
                            session.status = format!("{error:#}");
                            session.log = format!("{error:#}");
                        }
                    }
                }
                running = None;
            }
            if session
                .edited
                .is_some_and(|t| t.elapsed() >= Duration::from_millis(250))
                && !session.conflict
            {
                let disk = project.read(&session.active);
                if disk.as_ref().is_ok_and(|v| v == &session.baseline) {
                    match project.write(&session.active, &session.content) {
                        Ok(()) => {
                            session.baseline = session.content.clone();
                            session.edited = None;
                        }
                        Err(error) => {
                            session.status = format!("Save failed: {error}");
                            continue;
                        }
                    }
                } else {
                    session.conflict = true;
                    session.status =
                        "File changed on disk. Keep your text or reload the disk version.".into();
                }
            }
            if let Ok(current) = Project::open(&project.root) {
                if current.config.name != project.config.name {
                    session.settings.refresh_name(&current);
                    if let Err(e) = session.settings.save() {
                        session.status = format!("Could not save project name: {e}");
                    }
                }
                project = current;
                session.project = Some(project.clone());
            } else {
                session.status = "Check texstudio.json and the main document path".into();
                continue;
            }
            let fingerprint = match project.fingerprint() {
                Ok(v) => v,
                Err(e) => {
                    session.status = format!("Could not read project: {e}");
                    continue;
                }
            };
            if observed != Some(fingerprint) {
                observed = Some(fingerprint);
                changed = Instant::now();
                session.entries = project.entries().unwrap_or_default();
                if let Ok(index) = ProjectIndex::build(&project)
                    && index != *session.completion_index
                {
                    session.completion_index = Arc::new(index);
                    session.completion_revision += 1;
                }
                if session.edited.is_none() && !session.conflict {
                    if let Ok(content) = project.read(&session.active) {
                        if content != session.baseline {
                            session.content = content.clone();
                            session.baseline = content;
                            session.document_revision += 1;
                        }
                    } else {
                        session.active = project.config.main.clone();
                        session.content = project.read(&session.active).unwrap_or_default();
                        session.baseline = session.content.clone();
                        session.document_revision += 1;
                    }
                }
            }
            if !session.auto
                || session.conflict
                || session.edited.is_some()
                || built == observed
                || running.is_some()
            {
                continue;
            }
            if changed.elapsed() < Duration::from_millis(project.config.debounce_ms) {
                session.status = "Waiting for a pause…".into();
                continue;
            }
            if (session.active.ends_with(".tex") && !syntax::ready(&session.content))
                || !project.ready().unwrap_or(false)
            {
                session.status = "Waiting for a complete expression…".into();
                continue;
            }
            session.status = "Updating preview…".into();
            if compiler::busy(&project).unwrap_or(true) {
                session.status = "Another build is running…".into();
                continue;
            }
            let (tx, rx) = mpsc::channel();
            running = Some((project.root.clone(), observed, rx));
            thread::spawn(move || {
                let _ = tx.send(compiler::build(&project));
            });
        }
    });
}

pub fn launch(path: PathBuf) {
    let mut session = Session::default();
    match Settings::load() {
        Ok(settings) => session.settings = settings,
        Err(error) => session.status = format!("{error:#}"),
    }
    session.catalog = Arc::new(snippets::load(&session.settings));
    let project = Project::open(&path).or_else(|error| {
        if path == std::path::Path::new(".")
            && let Some(last) = &session.settings.last_project
        {
            return Project::open(last);
        }
        Err(error)
    });
    match project {
        Ok(project) => {
            if let Err(e) = load(&mut session, project) {
                session.status = e.to_string();
            }
        }
        Err(_) => {
            session.status = "Choose a project folder or create your first document".into();
        }
    }
    let shared = Arc::new(Mutex::new(session));
    worker(shared.clone());
    let close_shared = shared.clone();
    #[cfg(windows)]
    let asset_base = "http://texasset.localhost";
    #[cfg(not(windows))]
    let asset_base = "texasset://localhost";
    let head = format!(
        "<title>TexStudio - LaTex workspace</title><link rel=\"icon\" href=\"{asset_base}/logo.png\"><style>{}</style><script type=\"module\">const ASSET_BASE = '{}';\n{}</script>",
        include_str!("../assets/editor.css"),
        asset_base,
        include_str!("../assets/editor.js")
    );
    let config = Config::new()
        .with_window(
            WindowBuilder::new()
                .with_title("TexStudio - LaTex workspace")
                .with_window_icon(window_icon())
                .with_inner_size(LogicalSize::new(1440., 940.))
                .with_min_inner_size(LogicalSize::new(1000., 680.))
                .with_always_on_top(false),
        )
        .with_menu(None)
        .with_custom_event_handler(move |event, _| {
            if matches!(
                event,
                dioxus::desktop::tao::event::Event::WindowEvent {
                    event: dioxus::desktop::tao::event::WindowEvent::CloseRequested,
                    ..
                }
            ) {
                let s = close_shared.lock().unwrap();
                if s.edited.is_some()
                    && let Some(p) = &s.project
                {
                    let saved = !s.conflict
                        && p.read(&s.active).is_ok_and(|text| text == s.baseline)
                        && p.write(&s.active, &s.content).is_ok();
                    if !saved && let Ok(dir) = p.internal_dir() {
                        let _ = fs::write(dir.join("recovered-editor.txt"), &s.content);
                    }
                }
            }
        })
        .with_custom_head(head)
        .with_custom_protocol("texasset", |_, req| {
            let (bytes, mime): (&'static [u8], &str) = match req.uri().path() {
                "/pdf.mjs" => (include_bytes!("../assets/pdfjs/pdf.mjs"), "text/javascript"),
                "/pdf.worker.mjs" => (
                    include_bytes!("../assets/pdfjs/pdf.worker.mjs"),
                    "text/javascript",
                ),
                "/logo.png" => (include_bytes!("../assets/logo-mark.png"), "image/png"),
                "/snippets.mjs" => (include_bytes!("../assets/snippets.mjs"), "text/javascript"),
                "/snippet-worker.js" => (
                    include_bytes!("../assets/snippet-worker.js"),
                    "text/javascript",
                ),
                "/latex-model.mjs" => (
                    include_bytes!("../assets/latex-model.mjs"),
                    "text/javascript",
                ),
                "/editor-assistance.mjs" => (
                    include_bytes!("../assets/editor-assistance.mjs"),
                    "text/javascript",
                ),
                "/source-buffer.mjs" => (
                    include_bytes!("../assets/source-buffer.mjs"),
                    "text/javascript",
                ),
                "/completion-aerijo.json" => (
                    include_bytes!("../assets/completions/aerijo.json"),
                    "application/json",
                ),
                "/completion-macros.json" => (
                    include_bytes!("../assets/completions/macros.json"),
                    "application/json",
                ),
                "/completion-environments.json" => (
                    include_bytes!("../assets/completions/environments.json"),
                    "application/json",
                ),
                "/completion-packagenames.json" => (
                    include_bytes!("../assets/completions/packagenames.json"),
                    "application/json",
                ),
                "/completion-classnames.json" => (
                    include_bytes!("../assets/completions/classnames.json"),
                    "application/json",
                ),
                _ => {
                    return Response::builder()
                        .status(404)
                        .body(Cow::Borrowed(&b"Not found"[..]))
                        .unwrap();
                }
            };
            Response::builder()
                .header("Content-Type", mime)
                .header("Access-Control-Allow-Origin", "*")
                .body(Cow::Borrowed(bytes))
                .unwrap()
        });
    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(shared)
        .launch(App);
}

fn window_icon() -> Option<dioxus::desktop::tao::window::Icon> {
    use image::ImageDecoder;
    // A PNG-only decoder keeps unrelated image codecs out of the executable.
    let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(include_bytes!(
        "../assets/logo-mark.png"
    )))
    .ok()?;
    let (width, height) = decoder.dimensions();
    if decoder.color_type() != image::ColorType::Rgba8 {
        return None;
    }
    let mut rgba = vec![0; decoder.total_bytes() as usize];
    decoder.read_image(&mut rgba).ok()?;
    dioxus::desktop::tao::window::Icon::from_rgba(rgba, width, height).ok()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn highlight(source: &str) -> String {
    let mut html = String::new();
    for line in source.split('\n') {
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%' {
                html.push_str(&format!(
                    "<span class='comment'>{}</span>",
                    escape(&format!("%{}", chars.collect::<String>()))
                ));
                break;
            } else if c == '\\' {
                let mut command = String::from("\\");
                while chars
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == '*')
                {
                    command.push(chars.next().unwrap());
                }
                if command.len() == 1
                    && let Some(c) = chars.next()
                {
                    command.push(c);
                }
                html.push_str(&format!(
                    "<span class='command'>{}</span>",
                    escape(&command)
                ));
            } else if "{}$[]".contains(c) {
                html.push_str(&format!("<span class='delimiter'>{c}</span>"));
            } else {
                html.push_str(&escape(&c.to_string()));
            }
        }
        html.push('\n');
    }
    html
}

#[component]
fn App() -> Element {
    let shared = use_context::<Shared>();
    let init_shared = shared.clone();
    let mut view = use_signal(move || init_shared.lock().unwrap().clone());
    let mut tools = use_signal(|| false);
    let mut logs = use_signal(|| false);
    let mut new_path = use_signal(String::new);
    let mut operation = use_signal(|| "file".to_string());
    let mut operation_open = use_signal(|| false);
    let mut filter = use_signal(String::new);
    let mut last_pdf = use_signal(|| 0u64);
    let mut project_menu = use_signal(|| false);
    let mut renaming = use_signal(|| false);
    let mut rename_text = use_signal(String::new);
    let mut rename_cancelled = use_signal(|| false);
    let mut snippet_library = use_signal(|| false);
    let mut snippet_filter = use_signal(String::new);
    let mut last_snippet_config = use_signal(String::new);
    let mut last_editing_config = use_signal(String::new);
    let mut last_source_revision = use_signal(|| None::<u64>);
    let poll = shared.clone();
    use_future(move || {
        let poll = poll.clone();
        async move {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let next = poll.lock().unwrap();
                let changed = *view.peek() != *next;
                if changed {
                    view.set(next.clone());
                }
            }
        }
    });
    use_effect(move || {
        let state = view.read();
        if let Some(project) = &state.project
            && *last_source_revision.peek() != Some(state.document_revision)
        {
            last_source_revision.set(Some(state.document_revision));
            let identity = format!("{}:{}", project.root.display(), state.active);
            let source = serde_json::json!({"identity":identity,"revision":state.document_revision,"text":state.content});
            document::eval(&format!(
                "(async()=>{{for(let i=0;i<100&&!window.texstudioSyncSource;i++)await new Promise(r=>setTimeout(r,20));window.texstudioSyncSource?.({source});}})()"
            ));
        }
    });
    use_effect(move || {
        let current = view.read();
        if current.pdf_revision != *last_pdf.peek() {
            last_pdf.set(current.pdf_revision);
            if current.pdf.is_empty() {
                document::eval("window.texstudioClearPreview?.()");
                return;
            }
            let data = serde_json::to_string(current.pdf.as_ref()).unwrap();
            document::eval(&format!(
                "(async()=>{{for(let i=0;i<100&&!window.texstudioRenderWhenReady;i++)await new Promise(r=>setTimeout(r,50));window.texstudioRenderWhenReady?.({data});}})()"
            ));
        }
    });
    use_effect(move || {
        if *renaming.read() {
            document::eval(
                "setTimeout(()=>{const input=document.getElementById('project-name-input');input?.focus();input?.select();},0)",
            );
        }
    });
    use_effect(move || {
        let state = view.read();
        let identity = format!(
            "{}:{}",
            state
                .project
                .as_ref()
                .map(|p| p.root.to_string_lossy().into_owned())
                .unwrap_or_default(),
            state.active
        );
        let enabled = state.settings.snippets_enabled
            && [".tex", ".sty", ".cls"]
                .iter()
                .any(|extension| state.active.ends_with(extension));
        let key = format!("{identity}:{}:{enabled}", state.catalog_revision);
        if key != *last_snippet_config.peek() {
            last_snippet_config.set(key);
            let config = serde_json::json!({"identity":identity,"enabled":enabled,"catalog":state.catalog.as_ref()});
            document::eval(&format!(
                "(async()=>{{for(let i=0;i<100&&!window.texstudioConfigureSnippets;i++)await new Promise(r=>setTimeout(r,50));window.texstudioConfigureSnippets?.({config});}})()"
            ));
        }
    });
    use_effect(move || {
        let state = view.read();
        let identity = format!(
            "{}:{}",
            state
                .project
                .as_ref()
                .map(|p| p.root.to_string_lossy().into_owned())
                .unwrap_or_default(),
            state.active
        );
        let editable = [".tex", ".sty", ".cls"]
            .iter()
            .any(|extension| state.active.ends_with(extension));
        let key = format!(
            "{identity}:{}:{}:{}",
            state.completion_revision,
            state.settings.autocomplete_enabled,
            state.settings.auto_closing_enabled
        );
        if key != *last_editing_config.peek() {
            last_editing_config.set(key);
            let config = serde_json::json!({"identity":identity,"editable":editable,"active":state.active,"autocomplete":state.settings.autocomplete_enabled,"autoClosing":state.settings.auto_closing_enabled,"index":state.completion_index.as_ref()});
            document::eval(&format!(
                "(async()=>{{for(let i=0;i<100&&!window.texstudioConfigureEditing;i++)await new Promise(r=>setTimeout(r,50));window.texstudioConfigureEditing?.({config});}})()"
            ));
        }
    });
    let state = view.read().clone();
    let project = state.project.clone();
    let name = project
        .as_ref()
        .map(|p| p.config.name.clone())
        .unwrap_or_else(|| "Your workspace".into());
    let has_project = project.is_some();
    let name_for_rename = name.clone();
    let open_shared = shared.clone();
    let create_shared = shared.clone();
    let edit_shared = shared.clone();
    let auto_shared = shared.clone();
    let operation_shared = shared.clone();
    let keep_shared = shared.clone();
    let reload_shared = shared.clone();
    let engine_shared = shared.clone();
    let rename_shared = shared.clone();
    let snippet_toggle_shared = shared.clone();
    let workshop_shared = shared.clone();
    let hypersnips_shared = shared.clone();
    let snippet_path_shared = shared.clone();
    let reload_snippets_shared = shared.clone();
    let autocomplete_shared = shared.clone();
    let auto_closing_shared = shared.clone();
    let dirty = state.edited.is_some();
    let line_numbers = (1..=state.content.lines().count().max(1)
        + usize::from(state.content.ends_with('\n')))
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let engine_name = project
        .as_ref()
        .map(|p| p.config.engine.as_str())
        .unwrap_or("pdftex")
        .to_string();
    let status_class = if state.status.contains("attention")
        || state.status.contains("failed")
        || state.conflict
    {
        "status error"
    } else {
        "status"
    };
    let workshop_count = state
        .catalog
        .snippets
        .iter()
        .filter(|s| s.source == "LaTeX Workshop")
        .count();
    let hypersnips_count = state
        .catalog
        .snippets
        .iter()
        .filter(|s| s.source == "HyperSnips")
        .count();
    #[cfg(windows)]
    let logo_url = "http://texasset.localhost/logo.png";
    #[cfg(not(windows))]
    let logo_url = "texasset://localhost/logo.png";
    rsx! {
        div { class: "app",
            header { class: "titlebar",
                div { class: "brand", img { class: "brand-logo", src: "{logo_url}", alt: "TexStudio logo" } span { "TexStudio" } }
                div { class: "project-title", "{name}" }
                div { class: "title-actions", button { class: "icon-button", title: "Settings", onclick: move |_| tools.toggle(), "⚙" } }
            }
            div { class: "workspace",
                nav { class: "activity", button { class: "activity-button active", title: "Project files", "▤" } button { class: "activity-button", title: "Search files", onclick: move |_| { document::eval("document.getElementById('file-search')?.focus()"); }, "⌕" } button { class: "activity-button snippet-activity", title: "Snippet library", onclick: move |_| snippet_library.toggle(), "{{}}" } div { class: "activity-bottom", button { class: "activity-button", title: "Agent CLI guide and settings", onclick: move |_| tools.toggle(), ">_" } } }
                aside { class: "sidebar",
                    div { class: "sidebar-heading", span { "EXPLORER" } span { class: "sidebar-actions",
                        button { title: "New file", disabled: !has_project, onclick: move |_| { operation.set("file".into()); operation_open.set(true); }, "+" }
                        button { title: "Import project", onclick: move |_| {
                            if let Some(path) = rfd::FileDialog::new().set_title("Import LaTeX project folder").pick_folder() {
                                let result = Project::open(&path).and_then(|p| load(&mut open_shared.lock().unwrap(), p));
                                if let Err(e) = result { open_shared.lock().unwrap().status = format!("{e:#}"); }
                            }
                        }, "↗" }
                    } }
                    div { class: "project-switcher",
                        div { class: "folder-title",
                            if *renaming.read() {
                                input { id: "project-name-input", class: "project-name-input", aria_label: "Project name", value: "{rename_text}", oninput: move |e|rename_text.set(e.value()),
                                    onkeydown: move |e| { if e.key() == Key::Escape { rename_cancelled.set(true); renaming.set(false); } else if e.key() == Key::Enter { document::eval("document.getElementById('project-name-input')?.blur()"); } },
                                    onblur: move |_| { if !*rename_cancelled.read() { rename_project(&rename_shared, &rename_text.read()); } renaming.set(false); }
                                }
                            } else {
                                button { class: "project-name", title: "Rename project", disabled: !has_project, onclick: move |_| { rename_text.set(name_for_rename.clone()); rename_cancelled.set(false); renaming.set(true); }, "{name}" }
                            }
                            button { class: "project-chevron", title: "Switch project", aria_expanded: "{project_menu}", onclick: move |_| project_menu.toggle(), "⌄" }
                        }
                        if *project_menu.read() {
                            div { class: "project-menu",
                                div { class: "project-menu-heading", "PROJECTS" }
                                if state.settings.projects.is_empty() { p { class: "muted", "Import a project to add it here." } }
                                for known in &state.settings.projects {
                                    { let root = known.root.clone(); let switch_shared = shared.clone(); let selected = project.as_ref().is_some_and(|p|p.root == root);
                                    rsx! { button { class: if selected { "project-option current" } else { "project-option" }, title: "{known.root.display()}", onclick: move |_| {
                                        let result = Project::open(&root).and_then(|p|load(&mut switch_shared.lock().unwrap(), p));
                                        if let Err(e) = result { switch_shared.lock().unwrap().status = format!("{e:#}"); } else { project_menu.set(false); }
                                    }, span { "{known.name}" } small { "{known.root.file_name().unwrap_or_default().to_string_lossy()}" } } }
                                    }
                                }
                                button { class: "project-import", onclick: { let import_shared = shared.clone(); move |_| {
                                    if let Some(path) = rfd::FileDialog::new().set_title("Import LaTeX project folder").pick_folder() {
                                        let result = Project::open(&path).and_then(|p|load(&mut import_shared.lock().unwrap(), p));
                                        if let Err(e) = result { import_shared.lock().unwrap().status = format!("{e:#}"); } else { project_menu.set(false); }
                                    }
                                } }, "+ Import project…" }
                            }
                        }
                    }
                    input { id: "file-search", class: "file-search", placeholder: "Find a file…", value: "{filter}", oninput: move |e| filter.set(e.value()) }
                    div { class: "file-tree",
                        for entry in state.entries.iter().filter(|e|e.path.to_lowercase().contains(&filter.read().to_lowercase())) {
                            { let path = entry.path.clone(); let click_shared = shared.clone(); let directory = entry.directory; let active = state.active == path; let label = path.rsplit('/').next().unwrap_or(&path).to_string(); let depth = path.matches('/').count();
                            rsx! { button { class: if active { "file-row selected" } else { "file-row" }, style: "padding-left: {14 + depth * 14}px", title: "{path}", onclick: move |_| {
                                let mut s = click_shared.lock().unwrap();
                                if directory { return; }
                                if s.edited.is_some() || s.conflict { s.status = "Save or resolve the current file before switching files".into(); return; }
                                if !text_file(&path) { s.status = "Binary asset · available to your LaTeX document".into(); return; }
                                if let Some(p) = s.project.clone() { match p.read(&path) { Ok(text) => { s.active = path.clone(); s.baseline = text.clone(); s.content = text; s.document_revision += 1; }, Err(e) => s.status = e.to_string() } }
                            }, span { class: if directory { "file-icon folder" } else { "file-icon" }, if directory { "▱" } else if entry.path.ends_with(".tex") { "TᴇX" } else if entry.path.ends_with(".bib") { "❞" } else { "◇" } } span { "{label}" } } }
                            }
                        }
                    }
                    div { class: "sidebar-footer",
                        button { class: "secondary-button", onclick: move |_| {
                            if let Some(path) = rfd::FileDialog::new().set_title("Choose a new project folder name").set_file_name("My paper").save_file() {
                                let result = Project::create(&path).and_then(|p|load(&mut create_shared.lock().unwrap(), p));
                                if let Err(e) = result { create_shared.lock().unwrap().status = format!("{e:#}"); }
                            }
                        }, "+ New project" }
                        div { class: "local-note", "Your files. Your machine." }
                    }
                }
                main { class: "editor-pane",
                    div { class: "pane-tabs", div { class: "editor-tab", span { class: "file-icon", "TᴇX" } "{state.active}" if dirty { span { class: "dirty-dot", "●" } } } span { class: "pane-label", "SOURCE" } }
                    div { class: "editor-toolbar", span { class: "breadcrumb", "{name}" span { " / " } "{state.active}" } div { class: "format-tools", button { title: "Insert a section", disabled: !has_project, onclick: { let s = shared.clone(); move |_| insert(&s, "\n\\section{New section}\n") }, "H₁" } button { title: "Insert equation environment", disabled: !has_project, onclick: { let s = shared.clone(); move |_| insert(&s, "\n\\begin{equation}\n  E = mc^2\n\\end{equation}\n") }, "∑" } button { title: "File operations", disabled: !has_project, onclick: move |_| { operation.set("file".into()); operation_open.set(true); }, "···" } } }
                    if state.conflict {
                        div { class: "conflict", "This file changed on disk. Your text is preserved."
                            button { onclick: move |_| { let mut s = keep_shared.lock().unwrap(); if let Some(p) = s.project.clone() { if let Err(e) = p.write(&s.active, &s.content) { s.status = e.to_string(); return; } s.baseline = s.content.clone(); s.edited = None; s.conflict = false; } }, "Keep my text" }
                            button { onclick: move |_| { let mut s = reload_shared.lock().unwrap(); if let Some(p) = s.project.clone() { match p.read(&s.active) { Ok(t) => { s.content = t.clone(); s.baseline = t; s.edited = None; s.conflict = false; s.document_revision += 1; }, Err(e) => s.status = e.to_string() } } }, "Reload disk" }
                        }
                    }
                    if has_project {
                        div { class: "code-area", id: "code-area",
                            pre { class: "line-numbers", id: "line-numbers", "{line_numbers}" }
                            div { class: "code-stack",
                                pre { id: "highlight", class: "highlight", aria_hidden: "true", dangerous_inner_html: highlight(&state.content) }
                                textarea { id: "source", class: "source-input", spellcheck: "false", autocomplete: "off", autocapitalize: "off", aria_label: "LaTeX source editor", oninput: move |e| { let text = e.value(); let mut s = edit_shared.lock().unwrap(); s.content = text; s.edited = Some(Instant::now()); view.set(s.clone()); } }
                            }
                        }
                    } else { div { class: "welcome", img { class: "welcome-logo", src: "{logo_url}", alt: "TexStudio logo" } h1 { "A little space to think." } p { "Import a LaTeX project or create your first paper." } } }
                    div { class: "editor-bottom", span { if dirty { "Saving…" } else { "All changes saved" } } span { "UTF-8" } span { "LaTeX" } span { "{state.content.split_whitespace().count()} words" } }
                }
                section { class: "preview-pane",
                    div { class: "preview-heading", div { class: "preview-title", "Preview" } label { class: "auto-toggle", input { r#type: "checkbox", checked: state.auto, onchange: move |e| auto_shared.lock().unwrap().auto = e.checked() } "Auto preview" } }
                    div { class: "preview-toolbar", span { id: "page-count", "PDF preview" } div { class: "zoom-controls", button { title: "Zoom out", onclick: move |_| { document::eval("window.texstudioZoom(-0.1)"); }, "−" } span { id: "zoom-level", "100%" } button { title: "Zoom in", onclick: move |_| { document::eval("window.texstudioZoom(0.1)"); }, "+" } button { title: "Fit page width", onclick: move |_| { document::eval("window.texstudioFit()"); }, "↔" } } }
                    div { class: "pdf-container", id: "pdf-container", div { id: "pdf-pages" } div { id: "pdf-placeholder", class: "pdf-placeholder", span { "▧" } h2 { "Your ideas, on paper." } p { "The PDF appears here automatically." } p { class: "muted", "Finish an expression. We’ll take it from there." } } }
                    div { class: "preview-bottom", span { class: "dot" } "MiKTeX" span { "{engine_name}" } if state.elapsed_ms > 0 { span { class: "build-time", "Last build {state.elapsed_ms} ms" } } }
                }
            }
            footer { class: "statusbar", div { class: "{status_class}", span { class: "dot" } "{state.status}" } button { class: "log-button", onclick: move |_| logs.toggle(), if *logs.read() { "Hide build log" } else { "Build log ↗" } } }
            if *logs.read() { div { class: "log-panel", div { class: "panel-heading", "BUILD OUTPUT" button { onclick: move |_| logs.set(false), "×" } } pre { "{state.log}" } } }
            if *tools.read() {
                div { class: "modal-backdrop", div { class: "modal guide",
                    div { class: "modal-heading", h2 { img { class: "settings-logo", src: "{logo_url}", alt: "" } "Settings" } button { onclick: move |_| tools.set(false), "×" } }
                    h3 { "Editing" }
                    label { class: "setting-toggle", span { "Suggestions" small { "On by default. Show the completion list automatically as you type. Tab accepts a suggestion; Enter starts a new line unless you explicitly select one." } } input { r#type: "checkbox", checked: state.settings.autocomplete_enabled, onchange: move |e|update_settings(&autocomplete_shared, |s|s.autocomplete_enabled=e.checked()) } }
                    label { class: "setting-toggle", span { "Auto closing" small { "Off by default. Optionally pair environments, braces, brackets, and math delimiters." } } input { r#type: "checkbox", checked: state.settings.auto_closing_enabled, onchange: move |e|update_settings(&auto_closing_shared, |s|s.auto_closing_enabled=e.checked()) } }
                    h3 { "Snippets" }
                    label { class: "setting-toggle", span { "Enable snippets" small { "Off by default. Expand with Tab; automatic HyperSnips rules expand as you type." } } input { r#type: "checkbox", checked: state.settings.snippets_enabled, onchange: move |e|update_settings(&snippet_toggle_shared, |s|s.snippets_enabled = e.checked()) } }
                    label { class: "setting-toggle", span { "LaTeX Workshop" small { "{workshop_count} templates" } } input { r#type: "checkbox", checked: state.settings.workshop_enabled, onchange: move |e|update_settings(&workshop_shared, |s|s.workshop_enabled = e.checked()) } }
                    label { class: "setting-toggle", span { "HyperSnips" small { "{hypersnips_count} rules · math contexts, scripts, and tab stops" } } input { r#type: "checkbox", checked: state.settings.hsnips_enabled, onchange: move |e|update_settings(&hypersnips_shared, |s|s.hsnips_enabled = e.checked()) } }
                    div { class: "snippet-source", span { title: "{state.settings.hsnips_path.display()}", "{state.settings.hsnips_path.file_name().unwrap_or_default().to_string_lossy()}" }
                        button { class: "small-button", onclick: move |_| { if let Some(path) = rfd::FileDialog::new().add_filter("HyperSnips", &["hsnips"]).set_title("Choose LaTeX HyperSnips source").pick_file() { update_settings(&snippet_path_shared, |s|s.hsnips_path = path); } }, "Choose .hsnips…" }
                        button { class: "small-button", onclick: move |_|update_settings(&reload_snippets_shared, |_|{}), "Reload" }
                    }
                    for warning in &state.catalog.warnings { p { class: "snippet-warning", "{warning}" } }
                    p { class: "muted", "Tab and Shift+Tab move through fields. Esc finishes a snippet. HyperSnips scripts run in a separate worker; package suggestions do not modify your project." }
                    button { class: "secondary-button", onclick: move |_| { tools.set(false); snippet_library.set(true); }, "Browse snippet library" }
                    p { "TexStudio watches your local project. Agents and the editor work with the same files, with no background server." }
                    h3 { "Agent CLI" }
                    pre { "texstudio --project ./paper list\ntexstudio --project ./paper read main.tex\ntexstudio --project ./paper write main.tex < draft.tex\ntexstudio --project ./paper import figure.png figures/figure.png\ntexstudio --project ./paper move draft.tex sections/draft.tex\ntexstudio --project ./paper delete notes.tex" }
                    p { "Commands emit JSON. Delete moves files into .texstudio/trash. Edits made by an agent update this window automatically." }
                    h3 { "TeX engine" }
                    select { value: "{engine_name}", disabled: !has_project, onchange: move |e| { let mut s = engine_shared.lock().unwrap(); if let Some(mut p) = s.project.clone() { p.config.engine = e.value(); let result = serde_json::to_string_pretty(&p.config).map_err(anyhow::Error::from).and_then(|json|p.write("texstudio.json", &json)); if let Err(e) = result { s.status = e.to_string(); } } }, option { value: "pdftex", "pdfLaTeX" } option { value: "xetex", "XeLaTeX" } option { value: "luatex", "LuaLaTeX" } }
                    p { class: "muted", "MiKTeX and document packages must be installed locally. Builds do not download packages. Set the main file and debounce_ms in texstudio.json." }
                } }
            }
            if *snippet_library.read() {
                div { class: "modal-backdrop", div { class: "modal snippet-library",
                    div { class: "modal-heading", h2 { "Snippet library" } button { onclick: move |_| snippet_library.set(false), "×" } }
                    if !state.settings.snippets_enabled { p { class: "snippet-notice", "Snippets are off. Enable them in Settings to expand or insert templates." } button { class: "secondary-button", onclick: move |_| { snippet_library.set(false); tools.set(true); }, "Open Settings" } }
                    input { class: "snippet-search", aria_label: "Search snippets", placeholder: "Search by name, trigger, or source…", value: "{snippet_filter}", oninput: move |e|snippet_filter.set(e.value()) }
                    div { id: "snippet-hint", class: "snippet-hint", "Type a trigger and press Tab, or insert a template below." }
                    div { class: "snippet-list",
                        for snippet in state.catalog.snippets.iter().filter(|s|format!("{} {} {}",s.description,s.trigger,s.source).to_lowercase().contains(&snippet_filter.read().to_lowercase())) {
                            { let id = serde_json::to_string(&snippet.id).unwrap(); rsx! {
                                button { class: "snippet-row", disabled: !state.settings.snippets_enabled || !has_project, title: "{snippet.description}", onclick: move |_| { snippet_library.set(false); document::eval(&format!("window.texstudioInsertSnippet?.({id})")); },
                                    div { strong { "{snippet.description}" } span { "{snippet.source}" if snippet.automatic { " · automatic" } } }
                                    code { "{snippet.trigger}" }
                                }
                            } }
                        }
                    }
                } }
            }
            if *operation_open.read() {
                div { class: "modal-backdrop", div { class: "modal",
                    div { class: "modal-heading", h2 { "Project files" } button { onclick: move |_| operation_open.set(false), "×" } }
                    select { value: "{operation}", onchange: move |e|operation.set(e.value()), option { value: "file", "New file" } option { value: "folder", "New folder" } option { value: "rename", "Move current file" } option { value: "delete", "Delete current file (recoverable)" } option { value: "import", "Import a file" } }
                    label { "Project path" input { placeholder: "sections/introduction.tex", value: "{new_path}", oninput: move |e|new_path.set(e.value()) } }
                    p { class: "muted", "Use a relative path. Move and delete apply to the current file. Main documents are protected." }
                    button { class: "primary-button", onclick: move |_| {
                        let mut s = operation_shared.lock().unwrap();
                        let Some(p) = s.project.clone() else { return; };
                        if s.edited.is_some() || s.conflict { s.status = "Save or resolve your current file first".into(); return; }
                        let path = new_path.read().clone();
                        let result = match operation.read().as_str() {
                            "file" => p.create_file(&path, ""),
                            "folder" => p.mkdir(&path),
                            "rename" => p.move_file(&s.active, &path),
                            "delete" => p.trash(&s.active).map(|_|()),
                            "import" => if let Some(source) = rfd::FileDialog::new().pick_file() { p.import(&source, &path) } else { return; },
                            _ => Ok(()),
                        };
                        match result { Ok(()) => { s.status = "Project updated".into(); operation_open.set(false); new_path.set(String::new()); }, Err(e) => { s.status = format!("{e:#}"); } }
                    }, "Apply" }
                } }
            }
        }
    }
}

fn insert(shared: &Shared, text: &str) {
    let mut s = shared.lock().unwrap();
    let position = s
        .content
        .rfind("\\end{document}")
        .unwrap_or(s.content.len());
    s.content.insert_str(position, text);
    s.document_revision += 1;
    s.edited = Some(Instant::now());
}
