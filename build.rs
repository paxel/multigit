//! Builds the app icon into the Windows programs, so the Start menu entry
//! and the taskbar show it. Elsewhere, and when cross-compiling from another
//! system (no resource compiler there), it does nothing.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=packaging/windows/multigit.ico");
    let target_windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    if target_windows && cfg!(windows) {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("packaging/windows/multigit.ico");
        resource.compile()?;
    }
    Ok(())
}
