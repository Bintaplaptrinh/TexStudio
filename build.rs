fn main() {
    println!("cargo:rerun-if-changed=assets/logo.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/logo.ico")
            .set("ProductName", "TexStudio")
            .set("FileDescription", "TexStudio - LaTex workspace")
            .set("InternalName", "TexStudio")
            .compile()
            .expect("Windows icon resource must compile");
    }
}
