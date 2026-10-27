# Contributing and releases

## Git flow

- `main`: reviewed, releasable code.
- `develop`: integration for the next release. Create it from `main` when needed.
- `feat/<topic>`, `fix/<topic>`, `docs/<topic>`: branch from `develop`, open a PR back to `develop`.
- `release/<version>`: branch from `develop`, update `Cargo.toml` and `Cargo.lock`, stabilize, then merge into `main` and back into `develop`.
- `hotfix/<topic>`: branch from `main`, merge into `main` and `develop` after validation.

Use Conventional Commits, for example `feat(editor): show suggestions as you type`,
`fix(editor): preserve the caret during autosave`, `docs(cli): explain agent file
operations`, and `ci(release): publish Windows binaries`. Put a short description
of each changed file in the commit body. Use `!` for a breaking change.

Label PRs with `feature`/`enhancement`, `bug`/`fix`, `documentation`/`docs`,
`ci`/`build`/`dependencies`/`chore`, or `breaking-change`. The release configuration
groups **merged PRs by labels**, not by commit prefixes. Unmatched PRs go into
Other changes; `skip-changelog` excludes a PR. Direct commits still appear in
the full comparison link but do not create individual merged-PR entries.

CI runs on branch pushes and PRs into integration/release branches. It checks
repository privacy, Rust formatting/lints/tests, the CLI-only feature set, and
JavaScript behavior, then builds a Windows x86_64 portable executable. The
`TexStudio-windows-x86_64` Actions artifact is retained for 14 days. MiKTeX is
not installed on CI; the explicitly ignored compiler integration test must be
run locally with `cargo test --test compiler -- --ignored` before releasing
changes to compilation behavior. The optional real HyperSnips catalog test also
requires the local reference checkout and a generated `target/snippet-catalog.json`;
it is skipped on clean CI checkouts.

## Publish a release

1. Update the package version in `Cargo.toml`; run `cargo check` to update
   `Cargo.lock`. Merge the release PR into `main` after CI passes.
2. Tag the release commit on `main` and push the tag:

   ```sh
   git switch main
   git pull --ff-only
   git tag -a v0.1.0 -m "Release v0.1.0"
   git push origin main
   git push origin v0.1.0
   ```

3. `Release Windows` verifies that the tag is on `main` and matches the Cargo
   package version, reruns the checks/build, and publishes a GitHub release
   with automatically generated notes. A tag such as `v0.2.0-rc.1` creates a
   prerelease when Cargo has the matching `0.2.0-rc.1` version.

Downloads include `TexStudio-<version>-windows-x86_64.exe`, a ZIP containing the
executable, README and license notices, and a SHA-256 checksum file. The `.exe`
is portable, not an installer. MiKTeX, its document packages, and WebView2 must
be installed separately. Binaries are currently unsigned. Packaging uses an
explicit allowlist and never archives the checkout, local preferences, private
papers, logs, or `.texstudio` state. Builder source paths are remapped.

The publisher creates a draft, attaches verified assets, and publishes it only
after uploads succeed. Rerun a failed workflow to finish a draft; it refuses to
overwrite an already published release. Only the publish job has write access,
using the built-in `GITHUB_TOKEN`; no personal access token is required.
Creating/pushing the version tag is the explicit release step; ordinary commits
do not publish a GitHub release.

GitHub's generated notes behavior is documented at
[Automatically generated release notes](https://docs.github.com/en/repositories/releasing-projects-on-github/automatically-generated-release-notes).
The workflow uses the GitHub CLI's
[`--generate-notes`](https://cli.github.com/manual/gh_release_create) option.

## Privacy before committing

Keep real projects in `papers/`, `projects/`, `workspace/`, `private/`, or outside
the repository. Only sanitized starter sources belong in `examples/first-paper/`.
Local reference checkouts belong in `repo/` (ignored). Git does not automatically
hide a private `.tex` file saved in an arbitrary source folder.

`.gitignore` excludes local settings, credentials, environment files, editor/
agent state, LaTeX outputs, build products and distribution archives. Ignore
rules do not remove files or identities already committed. Preserve upstream
copyright/license attribution; public upstream author addresses are not local
account credentials.

Before staging, run `python scripts/check-privacy.py --worktree`. After staging,
run `python scripts/check-privacy.py` to inspect the exact Git index and
`git diff --cached --stat` to review the file list. The guard reports file names
and detection categories without printing secret values. It catches common
credential formats and personal filesystem paths; it cannot recognize every
secret or personal detail inside prose, images, or documents. Review those
manually. Use GitHub secret scanning/push protection as another layer where
available.

Enable the included local pre-commit guard after cloning:

```sh
git config --local core.hooksPath .githooks
```

It runs the same index scan before every commit and requires Python 3. CI runs
the scan independently, even when a local hook is not installed.

Use a repository-local GitHub public identity before committing:

```sh
git config --local user.name YOUR_GITHUB_USERNAME
git config --local user.email YOUR_GITHUB_NOREPLY_EMAIL
```

Copy the noreply address from GitHub Settings → Emails. This affects future
commits; old commit authors remain in history. Removing an old identity needs
a coordinated history rewrite and does not erase copies already published.
