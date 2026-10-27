use serde_json::Value;
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn cli(root: &std::path::Path, args: &[&str], stdin: Option<&str>) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_texstudio"))
        .arg("--project")
        .arg(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

#[test]
fn agent_file_operations_and_error_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("paper");
    assert!(
        cli(tmp.path(), &["new", root.to_str().unwrap()], None)
            .status
            .success()
    );
    let write = cli(
        &root,
        &["write", "sections/notes.tex", "--create"],
        Some("Tiếng Việt: α + β"),
    );
    assert!(write.status.success(), "{:?}", write);
    assert!(
        !cli(
            &root,
            &["write", "sections/notes.tex", "--create"],
            Some("overwrite")
        )
        .status
        .success()
    );
    let read = cli(&root, &["read", "sections/notes.tex"], None);
    let json: Value = serde_json::from_slice(&read.stdout).unwrap();
    assert_eq!(json["content"], "Tiếng Việt: α + β");
    assert!(
        cli(&root, &["move", "sections/notes.tex", "notes.tex"], None)
            .status
            .success()
    );
    let removed = cli(&root, &["delete", "notes.tex"], None);
    let json: Value = serde_json::from_slice(&removed.stdout).unwrap();
    assert!(std::path::Path::new(json["trash"].as_str().unwrap()).is_file());
    let escape = cli(&root, &["write", "../outside.tex"], Some("escape"));
    assert!(!escape.status.success());
    assert!(!tmp.path().join("outside.tex").exists());
    let error: Value = serde_json::from_slice(&escape.stderr).unwrap();
    assert_eq!(error["ok"], false);
}
