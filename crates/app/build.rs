//! On Windows, embeds the app icon and version info into magpie.exe, so
//! Explorer, the taskbar and Start show the Magpie icon.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/magpie.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/magpie.ico")
            .set("ProductName", "Magpie")
            .set("FileDescription", "Magpie")
            .set("CompanyName", "Ahaan Shah")
            .set("LegalCopyright", "Copyright (c) 2026 Ahaan Shah. MIT License.")
            .set("OriginalFilename", "magpie.exe")
            .set("InternalName", "magpie");
        res.compile().expect("embed Windows resources");
    }
}
