use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::Path,
};

use anyhow::{Context, bail};

/// Opaque background with the complete logo inside the maskable safe circle.
pub fn opaque_icon(size: u32) -> anyhow::Result<image::RgbaImage> {
    let mut canvas = image::RgbaImage::from_pixel(size, size, image::Rgba([31, 36, 48, 255]));
    let mark_size = size * 2 / 3;
    let mark = crate::render_icon(mark_size)?;
    let offset = (size - mark_size) / 2;
    for (x, y, source) in mark.enumerate_pixels() {
        let target = canvas.get_pixel_mut(x + offset, y + offset);
        let alpha = u32::from(source[3]);
        for channel in 0..3 {
            target[channel] = ((u32::from(source[channel]) * alpha
                + u32::from(target[channel]) * (255 - alpha)
                + 127)
                / 255) as u8;
        }
    }
    Ok(canvas)
}

pub fn build() -> anyhow::Result<()> {
    let staging = std::env::var_os("TRUNK_STAGING_DIR")
        .context("web-pwa must run as a Trunk post_build hook (TRUNK_STAGING_DIR missing)")?;
    let worker = generate(Path::new(&staging))?;
    std::fs::write(Path::new(&staging).join("sw.js"), worker)?;
    Ok(())
}

fn generate(staging: &Path) -> anyhow::Result<String> {
    // Normalize our prior injection so repeated hook runs stay content-deterministic.
    let index = staging.join("index.html");
    let html = std::fs::read_to_string(&index)?;
    let html = if let Some((before, rest)) = html.split_once("<!-- thinoda-build -->") {
        let (_, after) = rest
            .split_once("<!-- /thinoda-build -->")
            .context("Incomplete Thinoda build metadata")?;
        format!("{before}{after}")
    } else {
        html
    };
    let mut paths = Vec::new();
    collect(staging, staging, &mut paths)?;
    paths.sort();
    if !paths.iter().any(|path| path == "index.html")
        || !paths.iter().any(|path| path.ends_with(".wasm"))
    {
        bail!("PWA shell requires index.html and the compiled WASM module");
    }
    let mut revision = DefaultHasher::new();
    let mut entries = Vec::new();
    for path in paths {
        path.hash(&mut revision);
        if path == "index.html" {
            html.as_bytes().hash(&mut revision);
        } else {
            std::fs::read(staging.join(&path))?.hash(&mut revision);
        }
        // Build artifacts have simple names; reject anything needing URL escaping.
        if !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
        {
            bail!("Unsupported PWA artifact path: {path}");
        }
        entries.push(format!("\"./{path}\""));
    }
    let revision = format!("{:016x}", revision.finish());
    let version = env!("CARGO_PKG_VERSION");
    if !html.contains("</head>") {
        bail!("PWA index requires a closing head element");
    }
    std::fs::write(&index, html.replacen("</head>", &format!(
        "<!-- thinoda-build --><meta name=\"thinoda-build\" content=\"{version}:{revision}\"><!-- /thinoda-build --></head>"
    ), 1))?;
    Ok(include_str!("../../crates/thinoda-web/sw.js")
        .replace("__THINODA_FILES__", &format!("[{}]", entries.join(",")))
        .replace("__THINODA_VERSION__", version)
        .replace("__THINODA_REVISION__", &revision))
}

fn collect(base: &Path, dir: &Path, paths: &mut Vec<String>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            collect(base, &entry.path(), paths)?;
        } else if ty.is_file()
            && matches!(
                entry.path().extension().and_then(|ext| ext.to_str()),
                Some("html" | "js" | "wasm" | "css" | "png" | "ico" | "svg" | "webmanifest")
            )
            && entry.file_name() != "sw.js"
        {
            paths.push(
                entry
                    .path()
                    .strip_prefix(base)?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_icons_preserve_the_safe_zone_and_fill_every_pixel() {
        for size in [152, 167, 180, 512] {
            let icon = opaque_icon(size).unwrap();
            assert_eq!(icon.dimensions(), (size, size));
            assert!(icon.pixels().all(|pixel| pixel[3] == 255));
            let background = image::Rgba([31, 36, 48, 255]);
            for (x, y, pixel) in icon.enumerate_pixels() {
                if *pixel != background {
                    let center = size as f32 / 2.0;
                    let distance = (x as f32 - center).hypot(y as f32 - center);
                    assert!(distance <= size as f32 * 0.4, "{size}: {x},{y}");
                }
            }
        }
    }

    #[test]
    fn shell_generation_is_complete_deterministic_and_content_versioned() {
        let dir = std::env::temp_dir().join(format!("thinoda-pwa-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("snippets")).unwrap();
        std::fs::write(dir.join("index.html"), "<html><head></head>").unwrap();
        assert!(generate(&dir).is_err());
        std::fs::write(dir.join("app.wasm"), b"wasm").unwrap();
        std::fs::write(dir.join("app.js"), "app").unwrap();
        std::fs::write(dir.join("snippets/bridge.js"), "bridge").unwrap();
        let first = generate(&dir).unwrap();
        assert!(
            std::fs::read_to_string(dir.join("index.html"))
                .unwrap()
                .contains(&format!("content=\"{}:", env!("CARGO_PKG_VERSION")))
        );
        assert!(first.contains("\"./snippets/bridge.js\""));
        assert!(first.contains("\"./app.wasm\""));
        assert!(!first.contains("__THINODA_"));
        std::fs::write(dir.join("sw.js"), "previous output").unwrap();
        assert_eq!(first, generate(&dir).unwrap());
        std::fs::write(dir.join("app.wasm"), b"new wasm").unwrap();
        assert_ne!(first, generate(&dir).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
