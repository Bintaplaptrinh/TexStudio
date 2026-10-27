"""Exercise leak detection without storing real credentials in the fixtures."""

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "privacy", Path(__file__).resolve().parents[1] / "scripts" / "check-privacy.py"
)
privacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(privacy)


class PrivacyTests(unittest.TestCase):
    def test_private_files_are_rejected_even_without_secret_content(self):
        for name in [".env", "papers/main.tex", "nested/.texstudio/status.json", "settings.json", "key.pfx", "target/app.exe"]:
            with self.subTest(name=name):
                self.assertIn("private/generated file type", privacy.findings(name, b"{}"))

    def test_reviewed_source_and_templates_are_allowed(self):
        for name in ["examples/first-paper/main.tex", "Cargo.lock", ".env.example", ".github/workflows/release.yml"]:
            with self.subTest(name=name):
                self.assertEqual(list(privacy.findings(name, b"example source")), [])

    def test_credentials_and_user_paths_are_detected(self):
        samples = [
            ("gh" + "p_" + "x" * 36, "GitHub token"),
            ("AK" + "IA" + "A" * 16, "AWS access key"),
            ("-----BEGIN " + "RSA PRIVATE KEY-----", "private key"),
            ("https://user:" + "secret@example.invalid", "credential in URL"),
            ("C:" + "/Users/" + "Someone/source/app.rs", "personal filesystem path"),
            ('password = "' + 'x' * 20 + '"', "literal credential"),
        ]
        for source, category in samples:
            with self.subTest(category=category):
                self.assertIn(category, privacy.findings("src/example.txt", source.encode()))

    def test_binary_assets_and_public_notice_emails_are_allowed(self):
        self.assertEqual(list(privacy.findings("assets/logo.png", b"\x89PNG\0")), [])
        self.assertEqual(list(privacy.findings("LICENSE", b"Author <author@example.org>")), [])

    def test_index_scan_catches_staged_secret_after_worktree_is_cleaned(self):
        script = Path(privacy.__file__).resolve()
        with tempfile.TemporaryDirectory() as folder:
            subprocess.run(["git", "init", "--quiet", folder], check=True)
            file = Path(folder) / "source.txt"
            token = "gh" + "p_" + "x" * 36
            file.write_text(token)
            subprocess.run(["git", "add", "source.txt"], cwd=folder, check=True)
            file.write_text("sanitized source")
            staged = subprocess.run([sys.executable, str(script)], cwd=folder, capture_output=True, text=True)
            self.assertEqual(staged.returncode, 1)
            self.assertIn("GitHub token", staged.stdout)
            self.assertNotIn(token, staged.stdout + staged.stderr)
            worktree = subprocess.run([sys.executable, str(script), "--worktree"], cwd=folder, capture_output=True, text=True)
            self.assertEqual(worktree.returncode, 0)


if __name__ == "__main__":
    unittest.main()
