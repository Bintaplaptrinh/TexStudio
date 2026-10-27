use anyhow::Result;
use std::fs;
use texstudio_local::{compiler, project::Project};

#[test]
#[ignore = "requires a locally installed MiKTeX distribution"]
fn preserves_preview_on_error_and_resolves_includes() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let p = Project::create(&temp.path().join("paper with spaces"))?;
    p.write(
        "main.tex",
        "\\documentclass{article}\n\\begin{document}\n\\input{sections/intro}\n\\end{document}\n",
    )?;
    p.write("sections/intro.tex", "First build.")?;
    let first = compiler::build(&p)?;
    assert!(first.success, "{}", first.log);
    let pdf_path = first.pdf.unwrap();
    let good = fs::read(&pdf_path)?;
    assert!(good.starts_with(b"%PDF"));
    p.write("sections/intro.tex", "\\undefinedTexStudioCommand")?;
    let failed = compiler::build(&p)?;
    assert!(!failed.success);
    assert_eq!(good, fs::read(&pdf_path)?);
    assert!(!p.root.join("main.aux").exists());
    p.write("sections/intro.tex", "Second build with changed content.")?;
    let recovered = compiler::build(&p)?;
    assert!(recovered.success, "{}", recovered.log);
    assert_ne!(good, fs::read(&pdf_path)?);
    assert!(!compiler::busy(&p)?);
    Ok(())
}
