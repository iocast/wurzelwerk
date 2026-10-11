//! Embeds the app icon as a Windows resource: the taskbar, Alt+Tab and Explorer take the
//! icon from the exe (winit only sets the small title-bar icon at runtime).

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .expect("embed Windows icon resource");
    }
}
