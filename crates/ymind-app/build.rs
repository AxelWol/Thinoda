fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Embed the icon and version info so Explorer, the taskbar and shortcuts show the yMind icon.
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/icon.ico")
        .set("ProductName", "yMind")
        .set("FileDescription", "yMind")
        .set("CompanyName", "yMind contributors")
        .set("LegalCopyright", "Copyright © yMind contributors")
        .set("OriginalFilename", "ymind.exe")
        .set("InternalName", "ymind");
    if let Err(e) = res.compile() {
        panic!("failed to embed Windows resources: {e}");
    }
}
