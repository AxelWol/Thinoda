//! Developer tasks: `cargo xtask icons` (generate app icons) and `cargo xtask package` (build installers).

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, bail};

mod web_pwa;

const USAGE: &str = "cargo xtask <command>

commands:
    icons                     regenerate assets/icon-*.png and assets/icon.ico from the built-in SVG
    web-pwa                   generate the offline shell in Trunk's staging directory
    package [--target T] [--formats F,..]
                              build a release binary and create installers with cargo-packager
                              (nsis on Windows, app+dmg on macOS, deb+appimage on Linux)
    ci                        fmt --check, clippy -D warnings and the full test suite
";

/// Thinoda logo variants. The mark is a "T" grown from mind-map nodes: a central
/// thought (hub) branching to two ideas, with the stem resolving into a solid
/// shape — "Give your thoughts shape."
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconStyle {
    /// Full-bleed artwork for Windows, Linux and in-app use (48 px and up).
    Standard,
    /// Heavier strokes and no hub cut-out so 16–40 px stays legible.
    Small,
    /// Apple icon grid: 824 px body on a 1024 canvas with a soft drop shadow.
    MacOs,
    /// Apple grid with the small-size artwork (16/32 px slots of the `.icns`).
    MacOsSmall,
}

struct Geometry {
    inset: f32,
    radius: f32,
    stroke: f32,
    side_x: f32,
    side_y: f32,
    side_r: f32,
    hub_y: f32,
    hub_r: f32,
    hole_r: f32,
    shape: f32,
    shape_cy: f32,
}

const STANDARD: Geometry = Geometry {
    inset: 40.0,
    radius: 216.0,
    stroke: 60.0,
    side_x: 232.0,
    side_y: 372.0,
    side_r: 82.0,
    hub_y: 300.0,
    hub_r: 118.0,
    hole_r: 52.0,
    shape: 200.0,
    shape_cy: 760.0,
};

const SMALL: Geometry = Geometry {
    inset: 16.0,
    radius: 200.0,
    stroke: 104.0,
    side_x: 214.0,
    side_y: 400.0,
    side_r: 118.0,
    hub_y: 300.0,
    hub_r: 160.0,
    hole_r: 0.0,
    shape: 250.0,
    shape_cy: 750.0,
};

/// Returns the SVG source of the logo in the requested style.
pub fn icon_svg(style: IconStyle) -> String {
    let g = match style {
        IconStyle::Standard | IconStyle::MacOs => &STANDARD,
        IconStyle::Small | IconStyle::MacOsSmall => &SMALL,
    };
    let mac = matches!(style, IconStyle::MacOs | IconStyle::MacOsSmall);
    let body = 1024.0 - 2.0 * g.inset;
    let (r, sw, sx, sy, sr, hy, hr) = (
        g.radius, g.stroke, g.side_x, g.side_y, g.side_r, g.hub_y, g.hub_r,
    );
    let rx = 1024.0 - sx;
    let half = g.shape / 2.0;
    let (sx0, sy0, scy) = (512.0 - half, g.shape_cy - half, g.shape_cy);
    let srx = g.shape * 0.28;
    let hole = if g.hole_r > 0.0 {
        format!(
            r##"<circle cx="512" cy="{hy}" r="{}" fill="#4F63F2"/>"##,
            g.hole_r
        )
    } else {
        String::new()
    };
    let art = format!(
        r##"<rect x="{i}" y="{i}" width="{body}" height="{body}" rx="{r}" fill="url(#bg)"/>
  <rect x="{i}" y="{i}" width="{body}" height="{body}" rx="{r}" fill="url(#hl)"/>
  <g stroke="#FFFFFF" stroke-width="{sw}" stroke-linecap="round" fill="none">
    <path d="M{sx} {sy} C {a} {b} {c} {hy} 512 {hy} C {d} {hy} {e} {b} {rx} {sy}"/>
    <path d="M512 {hy} L512 {scy}"/>
  </g>
  <g fill="#FFFFFF">
    <circle cx="{sx}" cy="{sy}" r="{sr}"/><circle cx="{rx}" cy="{sy}" r="{sr}"/>
    <circle cx="512" cy="{hy}" r="{hr}"/>
    <rect x="{sx0}" y="{sy0}" width="{w}" height="{w}" rx="{srx}" transform="rotate(45 512 {scy})"/>
  </g>
  {hole}"##,
        i = g.inset,
        a = sx + 98.0,
        b = sy - 72.0,
        c = 420.0,
        d = 604.0,
        e = rx - 98.0,
        w = g.shape,
    );
    let defs = r##"<defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#6A5CFF"/><stop offset="0.55" stop-color="#4466F0"/><stop offset="1" stop-color="#14B8A6"/>
    </linearGradient>
    <radialGradient id="hl" cx="0.3" cy="0.2" r="0.8">
      <stop offset="0" stop-color="#FFFFFF" stop-opacity="0.22"/><stop offset="1" stop-color="#FFFFFF" stop-opacity="0"/>
    </radialGradient>
    <filter id="sh" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="14"/></filter>
  </defs>"##;
    let content = if mac {
        let k = 824.0 / body;
        let t = 100.0 - g.inset * k;
        format!(
            r##"<rect x="100" y="112" width="824" height="824" rx="{mr}" fill="#000000" opacity="0.3" filter="url(#sh)"/>
  <g transform="translate({t} {t}) scale({k})">
  {art}
  </g>"##,
            mr = r * k,
        )
    } else {
        art
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1024 1024\">\n  {defs}\n  {content}\n</svg>\n"
    )
}

/// PNG sizes written to `assets/icon-{size}.png` (Linux hicolor theme + in-app use).
pub const ICON_SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];
/// Frames embedded in the Windows `.ico` (covers 100–300 % display scaling).
pub const ICO_SIZES: [u32; 10] = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256];
/// `.icns` entries: OSType and pixel size (1x and @2x slots from 16 pt to 512 pt).
pub const ICNS_ENTRIES: [(&[u8; 4], u32); 10] = [
    (b"icp4", 16),
    (b"icp5", 32),
    (b"ic11", 32),
    (b"icp6", 64),
    (b"ic12", 64),
    (b"ic07", 128),
    (b"ic08", 256),
    (b"ic13", 256),
    (b"ic09", 512),
    (b"ic14", 512),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Picks the artwork that stays legible at `size` pixels.
fn style_for(size: u32, mac: bool) -> IconStyle {
    match (size <= 40, mac) {
        (true, false) => IconStyle::Small,
        (false, false) => IconStyle::Standard,
        (true, true) => IconStyle::MacOsSmall,
        (false, true) => IconStyle::MacOs,
    }
}

/// Rasterizes the logo at `size`×`size`, choosing the size-appropriate style.
pub fn render_icon(size: u32) -> anyhow::Result<image::RgbaImage> {
    render_style(style_for(size, false), size)
}

/// Rasterizes the logo in `style` at `size`×`size`.
pub fn render_style(style: IconStyle, size: u32) -> anyhow::Result<image::RgbaImage> {
    let svg = icon_svg(style);
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())?;
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

fn png_bytes(img: &image::RgbaImage) -> anyhow::Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}

/// Builds an Apple `.icns` container with PNG payloads for every Retina slot.
pub fn build_icns() -> anyhow::Result<Vec<u8>> {
    let mut body = Vec::new();
    for (ty, size) in ICNS_ENTRIES {
        let png = png_bytes(&render_style(style_for(size, true), size)?)?;
        body.extend_from_slice(ty);
        body.extend_from_slice(&(png.len() as u32 + 8).to_be_bytes());
        body.extend_from_slice(&png);
    }
    // 1024 px (512 pt @2x) slot.
    let png = png_bytes(&render_style(IconStyle::MacOs, 1024)?)?;
    body.extend_from_slice(b"ic10");
    body.extend_from_slice(&(png.len() as u32 + 8).to_be_bytes());
    body.extend_from_slice(&png);
    let mut icns = b"icns".to_vec();
    icns.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    icns.extend_from_slice(&body);
    Ok(icns)
}

fn icons() -> anyhow::Result<()> {
    let dir = root().join("assets");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("icon.svg"), icon_svg(IconStyle::Standard))?;
    std::fs::write(dir.join("icon-small.svg"), icon_svg(IconStyle::Small))?;
    std::fs::write(dir.join("icon-macos.svg"), icon_svg(IconStyle::MacOs))?;
    for s in ICON_SIZES {
        render_icon(s)?.save(dir.join(format!("icon-{s}.png")))?;
    }
    render_icon(1024)?.save(dir.join("icon.png"))?;
    render_style(IconStyle::MacOs, 512)?.save(dir.join("icon-macos-512.png"))?;
    let pwa = dir.join("pwa-icons");
    std::fs::create_dir_all(&pwa)?;
    for size in [192, 512] {
        render_icon(size)?.save(pwa.join(format!("icon-{size}.png")))?;
    }
    web_pwa::opaque_icon(180)?.save(pwa.join("apple-touch-icon.png"))?;
    for size in [152, 167] {
        web_pwa::opaque_icon(size)?.save(pwa.join(format!("apple-touch-icon-{size}.png")))?;
    }
    web_pwa::opaque_icon(512)?.save(pwa.join("maskable-512.png"))?;
    let mut ico = std::fs::File::create(dir.join("icon.ico"))?;
    let frames: Vec<_> = ICO_SIZES
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
    std::fs::write(dir.join("icon.icns"), build_icns()?)?;
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
    build.args([
        "build",
        "--release",
        "-p",
        "thinoda-app",
        "--bin",
        "thinoda",
    ]);
    if let Some(t) = &target {
        build.args(["--target", t]);
    }
    run(&mut build)?;
    let mut pack = Command::new("cargo-packager");
    pack.args(["--release", "-p", "thinoda-app"]);
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
    run(cargo().args([
        "test",
        "-p",
        "thinoda-core",
        "--example",
        "layout_benchmark",
    ]))?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("icons") => icons(),
        Some("web-pwa") => web_pwa::build(),
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

    #[test]
    fn every_style_renders() {
        for style in [
            super::IconStyle::Standard,
            super::IconStyle::Small,
            super::IconStyle::MacOs,
            super::IconStyle::MacOsSmall,
        ] {
            let img = super::render_style(style, 128).unwrap();
            assert_eq!(img.get_pixel(64, 40).0[3], 255, "{style:?}");
        }
    }

    #[test]
    fn mac_icon_follows_apple_grid() {
        let img = super::render_style(super::IconStyle::MacOs, 1024).unwrap();
        // Transparent margin around the 824 px body (shadow is faint), opaque body.
        assert!(img.get_pixel(40, 512).0[3] < 40);
        assert_eq!(img.get_pixel(120, 512).0[3], 255);
    }

    #[test]
    fn icns_has_all_retina_slots() {
        let icns = super::build_icns().unwrap();
        assert_eq!(&icns[..4], b"icns");
        assert_eq!(
            u32::from_be_bytes(icns[4..8].try_into().unwrap()) as usize,
            icns.len()
        );
        let mut types = Vec::new();
        let mut i = 8;
        while i < icns.len() {
            types.push(String::from_utf8_lossy(&icns[i..i + 4]).into_owned());
            assert_eq!(&icns[i + 9..i + 12], b"PNG");
            i += u32::from_be_bytes(icns[i + 4..i + 8].try_into().unwrap()) as usize;
        }
        assert_eq!(i, icns.len());
        for t in [
            "ic11", "ic12", "ic07", "ic13", "ic08", "ic14", "ic09", "ic10",
        ] {
            assert!(types.iter().any(|x| x == t), "missing {t}");
        }
    }
}
