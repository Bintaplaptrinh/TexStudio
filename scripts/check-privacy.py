"""Check Git's index (or candidate worktree files) without printing secret values."""

import argparse
import pathlib
import re
import subprocess
import sys


BLOCKED_DIRS = {
    ".texstudio", ".ssh", ".aws", ".azure", ".gcloud", ".codex", ".claude",
    ".idea", ".vscode", ".vs", "node_modules", "__pycache__",
}
BLOCKED_ROOTS = {
    "target", "dist", "artifacts", "repo", "papers", "projects", "workspace",
    "private", "local", "screenshots", "recordings",
}
BLOCKED_NAMES = {
    "settings.json", "credentials.json", "credentials.toml", ".netrc", ".npmrc",
}
BLOCKED_EXTENSIONS = {
    ".exe", ".dll", ".pdb", ".msi", ".msix", ".zip", ".pem", ".key",
    ".p12", ".pfx", ".jks", ".keystore", ".pdf", ".aux", ".log", ".pyc",
}
PATTERNS = [
    ("private key", re.compile(r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY-----")),
    ("GitHub token", re.compile(r"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{50,})\b")),
    ("AWS access key", re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b")),
    ("Slack token", re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{20,}\b")),
    ("credential in URL", re.compile(r"https?://[^\s/:]+:[^\s/@]+@")),
    ("personal filesystem path", re.compile(r"(?i)(?:[a-z]:[\\/](?:users|projects?)[\\/][\w .-]+[\\/]|/(?:Users|home)/[\w.-]+/)")),
    ("literal credential", re.compile(r'''(?im)^\s*["']?(?:api[_-]?key|client[_-]?secret|password|access[_-]?token)["']?\s*[:=]\s*["'][A-Za-z0-9/+_.=-]{12,}["']''')),
]


def git(*args):
    return subprocess.check_output(["git", *args])


def findings(name, data):
    path = pathlib.PurePosixPath(name.lower())
    if (
        set(path.parts) & BLOCKED_DIRS
        or path.parts[0] in BLOCKED_ROOTS
        or path.name in BLOCKED_NAMES
        or path.suffix in BLOCKED_EXTENSIONS
        or (path.name.startswith(".env") and path.name not in {".env.example", ".env.sample"})
        or (path.parent.name == ".cargo" and path.name in {"config", "config.toml", "credentials"})
    ):
        yield "private/generated file type"
    # Binary assets are reviewed separately; do not decode them as source text.
    if b"\0" in data:
        return
    source = data.decode("utf-8", errors="replace")
    for label, pattern in PATTERNS:
        if pattern.search(source):
            yield label


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worktree", action="store_true", help="include untracked, non-ignored candidates")
    args = parser.parse_args()
    root = pathlib.Path(git("rev-parse", "--show-toplevel").decode().strip())
    command = ["ls-files", "-z", "--cached"]
    if args.worktree:
        command += ["--others", "--exclude-standard"]
    names = sorted(set(n.decode("utf-8") for n in git(*command).split(b"\0") if n))
    problems = []
    for name in names:
        if args.worktree:
            file = root / name
            if not file.exists():
                continue
            if file.is_symlink():
                problems.append((name, "symlink requires manual review"))
                continue
            data = file.read_bytes()
        else:
            data = git("show", f":{name}")
        problems.extend((name, reason) for reason in findings(name, data))
    for name, reason in problems:
        print(f"FAIL {name}: {reason}")
    if problems:
        print("Privacy check failed. Values were withheld; review the listed files.")
        return 1
    print(f"Privacy check passed for {len(names)} files. Review personal documents and commit identity separately.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
