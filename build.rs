fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=assets/studio.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/studio.ico")
            .set("ProductName", "Fuji Recipe Studio")
            .set("FileDescription", "Fuji Recipe Studio")
            .set("OriginalFilename", "FujiRecipeStudio.exe")
            .compile_for("FujiRecipeStudio")?;
    }
    Ok(())
}
