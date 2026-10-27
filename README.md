# TexStudio

A local LaTeX desktop workspace written in Rust, with MiKTeX compilation and a
JSON CLI for AI agents. No account, database, cloud backend, or HTTP server.

This first implementation provides a project explorer, UTF-8 source editor with
syntax highlighting and line numbers, autosave, an embedded PDF preview with
zoom, compiler logs, engine selection, and file operations. There is no Compile
or Recompile button. Changes made by an external editor or CLI appear in the
open window automatically.

## Run

Install Rust, MiKTeX (including `texify` on PATH), and the packages your document
uses. On Windows, the desktop UI uses the installed WebView2 runtime.

```powershell
cargo run -- doctor
cargo run -- new ./papers/my-paper
cargo run -- --project ./papers/my-paper open
```

Try the included example:

```powershell
cargo run -- --project ./examples/first-paper open
```

Opening a normal LaTeX folder also works when it contains `main.tex`. Configure
a different entry point using `texstudio.json`:

```json
{
  "name": "My paper",
  "main": "main.tex",
  "engine": "pdftex",
  "debounce_ms": 650
}
```

Supported engines are `pdftex`, `xetex`, and `luatex`. The engine selector writes
this configuration. The debounce can range from 150 to 10000 milliseconds.

## Projects and appearance

TexStudio uses a black, gray, and white theme. The supplied `assets/logo.png`
is the source for its header, welcome screen, settings, window/taskbar icon,
and Windows executable icon. Smaller monochrome variants are included for
these surfaces; the original artwork is unchanged.

**Import project** registers an existing LaTeX folder. Click the project name
under **Explorer** to rename it; press Enter or click away to save, or Escape
to cancel. Renaming updates the display name in `texstudio.json` without moving
the folder. The adjacent dropdown switches between imported projects. The last
project reopens when launching without a project path.

The project registry and app preferences are stored in
`%LOCALAPPDATA%/TexStudio/settings.json` on Windows. They stay on this machine.

## Automatic previews

1. The editor saves after 250 ms without an input event and flushes pending
   edits on normal window close.
2. A watcher checks for file changes every 120 ms and waits for the configured
   pause before scheduling a build.
3. A lexical heuristic waits for balanced braces, environments, and common math
   delimiters in the main document and active `.tex` buffer. It understands
   comments, escaped delimiters, `\verb`, and common verbatim environments.
4. A separate thread runs MiKTeX `texify` with bounded passes. Editing and
   autosave continue during compilation. New edits are coalesced into the next
   build.
5. Successful PDFs replace the preview atomically. Errors keep the last good
   PDF, expose the log, and wait for another edit.

Build workspaces and auxiliary files live under `.texstudio/`. Unchanged sources
are not recopied. Workspaces are separated by engine. The rendered PDF is
`.texstudio/preview.pdf`, and the last report is `.texstudio/status.json`.
PDF.js is bundled and served by an in-process asset protocol: no CDN or server.

Rust reduces application overhead; it does not make MiKTeX incremental at the
TeX macro level. Document complexity, bibliography passes, font generation,
and package availability still determine latency. No fixed compile-time or
installer-size promise has been measured across representative projects.

Automatic package downloading and shell escape are disabled. Install missing
packages with MiKTeX separately. Builds time out after 45 seconds. The scheduler
is a heuristic, not a full TeX parser; advanced catcode changes and macro-defined
environments may require adjustments. CLI `build` bypasses this heuristic.

An operating-system file lock prevents concurrent builds and releases on a
crash. On Windows, compiler processes run in a job object so they cannot
outlive the app.

## Autocomplete and auto closing

**Settings → Editing → Suggestions** is on by default. The completion list
opens automatically when you type `\` and filters as you continue typing;
Ctrl+Space is optional. Turn Suggestions off to hide the list. Auto closing
has a separate switch and is off by default. Both work independently of the
optional snippet catalogs.

The completion list appears at the caret while typing LaTeX commands, or inside
arguments for environments, packages, classes, citations, labels, and files.
It combines Aerijo/autocomplete-latex's MIT-licensed data with the Workshop
catalogs and an index of local project files. Commands such as `\frac` are
offered in math contexts. Comments and verbatim regions suppress suggestions.

- **Ctrl+Space:** request suggestions manually.
- **Up / Down:** select a suggestion.
- **Tab**, or a mouse click: insert it.
- **Enter:** insert a newline, or accept a suggestion after an explicit arrow-key
  selection or a manual Ctrl+Space request.
- **Escape:** dismiss the list.
- **Tab / Shift+Tab:** navigate the inserted argument fields.

Examples: type `\sec` for section commands, `$\fr` for a fraction,
`\begin{ali` for `align`, `\ref{sec:` for project labels, or `\cite{doe`
for bibliography keys. `@doe` can expand to `\cite{doe2026}`. Local
`\newcommand`, common `\def`, and `\newenvironment` definitions also appear;
the active buffer's definitions and labels are available before autosave.
Project symbols refresh after external edits. Source indexing is bounded to
256 text files and 16 MiB; large or advanced TeX macro systems need a future
language-service implementation.

Auto closing pairs `{}`, `[]`, `()`, inline/display math, and `\left`/`\right`
delimiters. Finishing `\begin{align}` adds `\end{align}` when there is no
matching end already. Enter between them creates an indented body. Typing an
existing closing character moves over it; Backspace removes an empty pair.
Selections can be wrapped in delimiters. These rules are suppressed inside
comments and verbatim content and preserve escaped currency dollars.

Completion data is bundled; neither Atom nor a network connection is needed.
Package and class catalogs list known names, not a scan of what MiKTeX has
installed. Package installation remains a separate MiKTeX operation. Atom's
magic-comment build settings are not imported; TexStudio uses `texstudio.json`.

## Snippets

Snippets are **off by default**. Open **Settings → Enable snippets** to turn
them on or off. Source checkboxes independently control the catalogs:

- **LaTeX Workshop:** 52 bundled templates, including equations, tables,
  figures, environments, and selection wrappers. Type a trigger such as `BEQ`
  and press Tab, or use the **{} Snippet library** in the left activity bar.
- **HyperSnips:** the local `repo/Hsnips-Snippets/for 0.2.7+/latex.hsnips` source
  supplies 137 rules. TexStudio finds it automatically in this checkout.
  **Choose .hsnips…** selects another local source and **Reload** refreshes it.
  Automatic rules respect math contexts; for example, `sq` inside math becomes
  a square-root template and `a;` becomes `\alpha`.

Tab and Shift+Tab move through numbered fields. Repeated fields remain linked,
selected text can populate defaults, and Escape ends a snippet. The library
supports searching by name, source, or trigger. Some regex rules need a matching
trigger in the source before they can be inserted from the library.

HyperSnips expressions and helper functions execute in a separate, time-bounded
Web Worker with a small editor compatibility layer. No Node.js or VS Code
extension host is loaded. Package suggestions and status-bar timers from the
original global script do not modify the project or the app. Complex nested
snippet sessions and VS Code extension APIs beyond that layer are not provided.

The HyperSnips file is read locally at runtime, not redistributed. The Workshop
templates are bundled with their MIT license. `texstudio snippets` returns the
configured catalog and whether snippets are enabled, for agent inspection.

## Agent CLI

After building, use `target/release/texstudio.exe` directly. Successful commands
return JSON on stdout; runtime errors return JSON on stderr with a nonzero exit code.
`watch` emits one JSON object per build. `write` consumes UTF-8 text on stdin.

```sh
texstudio new ./paper
texstudio --project ./paper info
texstudio --project ./paper list
texstudio --project ./paper read main.tex
texstudio --project ./paper write sections/intro.tex --create < intro.tex
texstudio --project ./paper write main.tex < revised-main.tex
texstudio --project ./paper mkdir figures
texstudio --project ./paper import diagram.png figures/diagram.png
texstudio --project ./paper move sections/intro.tex sections/introduction.tex
texstudio --project ./paper delete sections/old-draft.tex
texstudio --project ./paper build
texstudio --project ./paper watch
```

For PowerShell, pipe UTF-8 content instead of using `<` redirection:

```powershell
Get-Content -Raw -Encoding utf8 draft.tex | .\target\release\texstudio.exe --project ./paper write main.tex
```

Paths are slash-separated and relative to the project. Absolute paths,
traversal, hidden paths, and links leaving the project are rejected. Import and
move do not overwrite. Delete moves data to `.texstudio/trash`; restore it with
normal filesystem tools. The configured main document, its parent folders, and
project configuration are protected from move/delete.

External edits reload when the buffer is saved. If an external tool changes a
file with pending editor edits, choose **Keep my text** or **Reload disk**. On
close with a conflict, pending text is retained in
`.texstudio/recovered-editor.txt`.

## Build and verify

Windows CI checks every branch push and builds a downloadable `.exe` artifact.
Pushing a version tag such as `v0.1.0` from `main` publishes the executable,
portable ZIP, checksums, and automatically generated GitHub release notes.
See [CONTRIBUTING.md](CONTRIBUTING.md) for Git flow, release steps, and privacy
checks before committing.

```powershell
cargo fmt --check
python scripts/check-privacy.py --worktree
python -m unittest discover -s tests -p privacy_test.py
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --test compiler -- --ignored
cargo run -- snippets > target/snippet-catalog.json
node --test tests/snippets.test.mjs tests/editor-assistance.test.mjs
node --test tests/source-buffer.test.mjs
cargo build --release
```

The ignored integration test requires MiKTeX. It checks project paths with
spaces, included files, error recovery, and preservation of the last good PDF.
Other tests cover UTF-8 commands, file operations, recoverable deletion, path
confinement, and scheduling heuristics.

For a smaller CLI-only binary without the desktop UI or PDF.js:

```powershell
cargo build --release --no-default-features
```

The release profile strips symbols and uses size optimization and thin LTO.
MiKTeX remains a separate dependency. Cargo can build and launch the desktop;
the Dioxus CLI and Node.js are not runtime requirements.

## Scope and next steps

This is a desktop MVP, not full Overleaf feature parity. Editing currently uses
a highlighted textarea rather than a language-service editor. Next milestones:
a rope-backed editing core, language-server completion and structured
diagnostics, SyncTeX source/PDF navigation, Biber-aware bibliography recipes,
project archives, and native installer packaging. Search filters file paths.
Move does not rewrite `\input` references. PDF pages render to canvas without
text selection.

Windows is the primary validation target. Other desktop platforms need their
native dependencies and MiKTeX integration tested.

## References

- [MiKTeX](https://miktex.org) and [texify](https://docs.miktex.org/manual/texify.html)
- [Rust/UI](https://rust-ui.com/docs/components) for visual direction
- [Dioxus desktop](https://dioxuslabs.com/learn/0.7/getting_started/) for the Rust UI
- Overleaf under `../repo` as an architecture reference
- LaTeX Workshop templates and the local HyperSnips source for snippet integration
- [Aerijo/autocomplete-latex](https://github.com/Aerijo/autocomplete-latex) for completion data

This is an original implementation; no Overleaf source was copied. See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for bundled dependencies.
