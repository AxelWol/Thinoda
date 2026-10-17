fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Embed the icon and version info so Explorer, the taskbar and shortcuts show the Thinoda icon.
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/icon.ico")
        .set("ProductName", "Thinoda")
        .set("FileDescription", "Thinoda")
        .set("CompanyName", "Axel Wolters")
        .set("LegalCopyright", "Copyright © 2026 Axel Wolters (EUPL-1.2)")
        .set("OriginalFilename", "thinoda.exe")
        .set("InternalName", "thinoda");
    if let Err(e) = res.compile() {
        panic!("failed to embed Windows resources: {e}");
    }
}
