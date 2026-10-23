use crate::media;
use image::RgbImage;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// How to grow a source that is smaller than its target.
#[derive(Deserialize, Clone, Copy, Default, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Off,
    Lanczos,
    Ai,
}

/// Resize `img` up to exactly `w`×`h`. AI runs Real-ESRGAN ×4 first and then
/// Lanczos settles the exact size; if AI is unavailable or fails it falls
/// back to Lanczos plus a light unsharp mask.
pub fn to_size(img: &RgbImage, w: u32, h: u32, mode: Mode) -> RgbImage {
    if mode == Mode::Ai {
        match ai_x4(img) {
            Ok(big) => return media::resize(&big, w, h),
            Err(e) => eprintln!("AI upscale failed, using Lanczos: {e}"),
        }
    }
    image::imageops::unsharpen(&media::resize(img, w, h), 0.8, 2)
}

pub fn by_factor(img: &RgbImage, factor: u32, mode: Mode) -> RgbImage {
    let f = factor.clamp(2, 4);
    let mode = if mode == Mode::Off { Mode::Lanczos } else { mode };
    to_size(img, img.width() * f, img.height() * f, mode)
}

pub fn ai_bin() -> Option<&'static PathBuf> {
    static BIN: OnceLock<Option<PathBuf>> = OnceLock::new();
    BIN.get_or_init(|| {
        let bundled = std::env::current_exe().ok()?.parent()?.join("realesrgan/realesrgan-ncnn-vulkan");
        std::env::var_os("REALESRGAN_BIN").map(PathBuf::from).into_iter().chain([bundled]).find(|p| p.is_file())
    })
    .as_ref()
}

fn ai_x4(img: &RgbImage) -> Result<RgbImage, String> {
    // ponytail: one GPU job at a time; a queue per GPU if several Macs share a server.
    static GPU: Mutex<()> = Mutex::new(());
    let bin = ai_bin().ok_or("Real-ESRGAN is not installed")?;
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let base = std::env::temp_dir().join(format!("postprep-ai-{}-{:?}", std::process::id(), std::thread::current().id()));
    let (src, dst) = (base.with_extension("in.png"), base.with_extension("out.png"));
    img.save(&src).map_err(|e| e.to_string())?;
    let out = std::process::Command::new(bin)
        .arg("-i").arg(&src).arg("-o").arg(&dst)
        .args(["-s", "4", "-n", "realesrgan-x4plus", "-f", "png"])
        .arg("-m").arg(bin.parent().unwrap().join("models"))
        .output()
        .map_err(|e| e.to_string());
    let result = out.and_then(|o| {
        if !o.status.success() {
            return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
        }
        image::open(&dst).map(|i| i.to_rgb8()).map_err(|e| e.to_string())
    });
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&dst);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanczos_doubles() {
        let img = RgbImage::from_fn(50, 30, |x, y| image::Rgb([x as u8, y as u8, 0]));
        assert_eq!(by_factor(&img, 2, Mode::Off).dimensions(), (100, 60));
        assert_eq!(by_factor(&img, 4, Mode::Lanczos).dimensions(), (200, 120));
    }
}
