//! Developer tasks: `cargo xtask icons` (generate app icons) and `cargo xtask package` (build installers).

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, bail};

const USAGE: &str = "cargo xtask <command>

commands:
    icons                     regenerate assets/icon-*.png and assets/icon.ico from the built-in SVG
    package [--target T] [--formats F,..]
                              build a release binary and create installers with cargo-packager
                              (nsis on Windows, app+dmg on macOS, deb+appimage on Linux)
    ci                        fmt --check, clippy -D warnings and the full test suite
";

/// The yMind logo: a central topic with four branches on an accent gradient.
pub const ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="#4C8DFF"/><stop offset="1" stop-color="#7B4CFF"/></linearGradient></defs>
  <rect x="40" y="40" width="944" height="944" rx="210" fill="url(#g)"/>
  <g stroke="#FFFFFF" stroke-width="44" stroke-linecap="round" fill="none" opacity="0.95">
    <path d="M512 512 C 400 512 380 300 250 300"/><path d="M512 512 C 400 512 380 724 250 724"/>
    <path d="M512 512 C 624 512 644 300 774 300"/><path d="M512 512 C 624 512 644 724 774 724"/>
  </g>
  <rect x="352" y="432" width="320" height="160" rx="70" fill="#FFFFFF"/>
  <g fill="#FFFFFF"><circle cx="230" cy="300" r="62"/><circle cx="230" cy="724" r="62"/>
    <circle cx="794" cy="300" r="62"/><circle cx="794" cy="724" r="62"/></g>
  <path d="M452 470 L512 540 L572 470 M512 540 L512 572" stroke="#5B6BFF" stroke-width="30" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
</svg>"##;

pub const ICON_SIZES: [u32; 7] = [16, 32, 48, 64, 128, 256, 512];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Rasterizes the logo at `size`×`size`.
pub fn render_icon(size: u32) -> anyhow::Result<image::RgbaImage> {
    let tree = resvg::usvg::Tree::from_str(ICON_SVG, &resvg::usvg::Options::default())?;
    let mut pix = resvg::tiny_skia::Pixmap::new(size, size).context("pixmap")?;
    let s = size as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(s, s),
        &mut pix.as_mut(),
    );
    // tiny-skia stores premultiplied RGBA.
    let mut data = pix.take();
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    image::RgbaImage::from_raw(size, size, data).context("image")
}

fn icons() -> anyhow::Result<()> {
    let dir = root().join("assets");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("icon.svg"), ICON_SVG)?;
    for s in ICON_SIZES {
        render_icon(s)?.save(dir.join(format!("icon-{s}.png")))?;
    }
    render_icon(1024)?.save(dir.join("icon.png"))?;
    let mut ico = std::fs::File::create(dir.join("icon.ico"))?;
    let frames: Vec<_> = [16, 32, 48, 64, 128, 256]
        .into_iter()
        .map(|s| {
            let img = render_icon(s)?;
            Ok(image::codecs::ico::IcoFrame::as_png(
                img.as_raw(),
                s,
                s,
                image::ExtendedColorType::Rgba8,
            )?)
        })
        .collect::<anyhow::Result<_>>()?;
    image::codecs::ico::IcoEncoder::new(&mut ico).encode_images(&frames)?;
    println!("icons written to {}", dir.display());
    Ok(())
}

fn run(cmd: &mut Command) -> anyhow::Result<()> {
    eprintln!("> {cmd:?}");
    let st = cmd
        .current_dir(root())
        .status()
        .with_context(|| format!("failed to run {cmd:?}"))?;
    if !st.success() {
        bail!("command failed: {cmd:?}");
    }
    Ok(())
}

fn cargo() -> Command {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

fn package(args: &[String]) -> anyhow::Result<()> {
    let mut target = None;
    let mut formats = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--target" => target = it.next().cloned(),
            "--formats" => formats = it.next().cloned(),
            other => bail!("unknown option {other}\n\n{USAGE}"),
        }
    }
    if !root().join("assets/icon.ico").exists() {
        icons()?;
    }
    let has_packager = Command::new("cargo-packager")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !has_packager {
        run(cargo().args(["install", "cargo-packager", "--locked"]))?;
    }
    let mut build = cargo();
    build.args(["build", "--release", "-p", "ymind-app", "--bin", "ymind"]);
    if let Some(t) = &target {
        build.args(["--target", t]);
    }
    run(&mut build)?;
    let mut pack = Command::new("cargo-packager");
    pack.args(["--release", "-p", "ymind-app"]);
    if let Some(t) = &target {
        pack.args(["--target", t]);
    }
    if let Some(f) = &formats {
        pack.args(["--formats", f]);
    }
    run(&mut pack)?;
    println!(
        "packages written to {}",
        root().join("target/packages").display()
    );
    Ok(())
}

fn ci() -> anyhow::Result<()> {
    run(cargo().args(["fmt", "--all", "--check"]))?;
    run(cargo().args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ]))?;
    run(cargo().args(["test", "--workspace"]))?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("icons") => icons(),
        Some("package") => package(&args[1..]),
        Some("ci") => ci(),
        _ => {
            print!("{USAGE}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_renders_opaque_center() {
        let img = super::render_icon(64).unwrap();
        assert_eq!(img.dimensions(), (64, 64));
        assert_eq!(img.get_pixel(32, 32).0[3], 255);
        assert_eq!(img.get_pixel(0, 0).0[3], 0);
    }
}
