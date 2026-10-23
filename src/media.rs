use crate::upscale::{self, Mode};
use fast_image_resize::{ResizeOptions, Resizer};
use image::{DynamicImage, ImageDecoder, ImageReader, RgbImage};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
use serde::Deserialize;
use std::io::Cursor;

pub enum Size {
    Exact(u32, u32),
    LongSide(u32),
}

pub struct Preset {
    pub id: &'static str,
    pub folder: &'static str,
    pub size: Size,
}

// Sizes the platforms serve without re-scaling, so their recompression stays light.
pub const PRESETS: &[Preset] = &[
    Preset { id: "ig_square", folder: "instagram", size: Size::Exact(1080, 1080) },
    Preset { id: "ig_portrait", folder: "instagram", size: Size::Exact(1080, 1350) },
    Preset { id: "ig_tall", folder: "instagram", size: Size::Exact(1080, 1440) },
    Preset { id: "ig_landscape", folder: "instagram", size: Size::Exact(1080, 566) },
    Preset { id: "ig_story", folder: "instagram", size: Size::Exact(1080, 1920) },
    Preset { id: "fb_feed", folder: "facebook", size: Size::LongSide(2048) },
    Preset { id: "fb_story", folder: "facebook", size: Size::Exact(1080, 1920) },
];

pub const SOFTWARE: &str = "Post Prep by cgradying";

pub struct Source {
    pub img: RgbImage,
    pub icc: Option<Vec<u8>>,
}

/// Decode with the `image` crate; on macOS fall back to ImageIO via `sips`
/// for formats it can't read (HEIC, AVIF, camera RAW).
pub fn decode(bytes: &[u8], name: &str) -> Result<Source, String> {
    match decode_native(bytes) {
        Ok(s) => Ok(s),
        #[cfg(target_os = "macos")]
        Err(e) => decode_sips(bytes, name).map_err(|e2| format!("{e}; macOS fallback: {e2}")),
        #[cfg(not(target_os = "macos"))]
        Err(e) => Err(format!("{e} ({name}: HEIC, AVIF and RAW need the Mac app)")),
    }
}

fn decode_native(bytes: &[u8]) -> Result<Source, String> {
    let mut decoder = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let icc = decoder.icc_profile().ok().flatten();
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    img.apply_orientation(orientation);
    Ok(Source { img: img.to_rgb8(), icc })
}

#[cfg(target_os = "macos")]
fn decode_sips(bytes: &[u8], name: &str) -> Result<Source, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let ext = name.rsplit_once('.').map_or("bin", |(_, e)| e).chars().filter(char::is_ascii_alphanumeric).collect::<String>();
    let base = std::env::temp_dir().join(format!("postprep-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let (src, dst) = (base.with_extension(ext), base.with_extension("png"));
    std::fs::write(&src, bytes).map_err(|e| e.to_string())?;
    let out = std::process::Command::new("sips").args(["-s", "format", "png"]).arg(&src).arg("--out").arg(&dst).output();
    let result = match out {
        Ok(o) if o.status.success() => std::fs::read(&dst).map_err(|e| e.to_string()).and_then(|png| decode_native(&png)),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    };
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&dst);
    result
}

/// Lanczos3 resize of the `crop` rect of `img` to `w`×`h` (SIMD).
pub fn resize_rect(img: &RgbImage, (x, y, cw, ch): (u32, u32, u32, u32), w: u32, h: u32) -> RgbImage {
    let mut dst = RgbImage::new(w.max(1), h.max(1));
    let opts = ResizeOptions::new().crop(x as f64, y as f64, cw as f64, ch as f64);
    Resizer::new().resize(img, &mut dst, &opts).expect("rgb8 to rgb8 resize");
    dst
}

pub fn resize(img: &RgbImage, w: u32, h: u32) -> RgbImage {
    resize_rect(img, (0, 0, img.width(), img.height()), w, h)
}

pub fn target_size(w: u32, h: u32, size: &Size) -> (u32, u32) {
    match *size {
        Size::Exact(tw, th) => (tw, th),
        Size::LongSide(l) if w >= h => (l, (l as u64 * h as u64 / w as u64) as u32),
        Size::LongSide(l) => ((l as u64 * w as u64 / h as u64) as u32, l),
    }
}

/// Center rect of `w`×`h` with ratio `tw`:`th`.
pub fn cover_rect(w: u32, h: u32, tw: u32, th: u32) -> (u32, u32, u32, u32) {
    let (cw, ch) = if w as u64 * th as u64 > h as u64 * tw as u64 {
        ((h as u64 * tw as u64 / th as u64) as u32, h)
    } else {
        (w, (w as u64 * th as u64 / tw as u64) as u32)
    };
    ((w - cw) / 2, (h - ch) / 2, cw.max(1), ch.max(1))
}

/// Center-crop to the target ratio, then resize. A source smaller than the
/// target is upscaled with `up`, or kept at its own size when `up` is Off.
pub fn fit(img: &RgbImage, size: &Size, up: Mode) -> RgbImage {
    let (tw, th) = target_size(img.width(), img.height(), size);
    let rect = cover_rect(img.width(), img.height(), tw, th);
    if rect.2 >= tw {
        return resize_rect(img, rect, tw, th);
    }
    let crop = image::imageops::crop_imm(img, rect.0, rect.1, rect.2, rect.3).to_image();
    match up {
        Mode::Off => crop,
        _ => upscale::to_size(&crop, tw, th, up),
    }
}

#[derive(Deserialize, Clone, Copy, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Jpeg,
    Png,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Png => "png",
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct Meta {
    pub artist: String,
    pub copyright: String,
}

impl Default for Meta {
    fn default() -> Self {
        Meta { artist: "cgradying".into(), copyright: "© cgradying".into() }
    }
}

/// JPEG: quality `q` with full-resolution chroma (4:4:4) so red/blue edges
/// survive the platform's re-encode. PNG: lossless. Both keep the source ICC
/// profile and carry artist/copyright/software metadata.
pub fn encode(img: &RgbImage, format: Format, q: u8, icc: Option<&[u8]>, meta: &Meta) -> Vec<u8> {
    let mut out = Vec::new();
    match format {
        Format::Jpeg => {
            let mut enc = Encoder::new(&mut out, q.clamp(60, 100));
            enc.set_sampling_factor(SamplingFactor::R_4_4_4);
            enc.set_progressive(true);
            let _ = enc.add_exif_metadata(&exif_tiff(meta));
            if let Some(icc) = icc {
                let _ = enc.add_icc_profile(icc);
            }
            enc.encode(img.as_raw(), img.width() as u16, img.height() as u16, ColorType::Rgb)
                .expect("output sizes fit in u16");
        }
        Format::Png => {
            let mut info = png::Info::with_size(img.width(), img.height());
            info.color_type = png::ColorType::Rgb;
            info.bit_depth = png::BitDepth::Eight;
            info.icc_profile = icc.map(|i| i.to_vec().into());
            let mut enc = png::Encoder::with_info(&mut out, info).expect("png info");
            enc.set_compression(png::Compression::Fast);
            for (k, v) in [("Author", meta.artist.as_str()), ("Copyright", meta.copyright.as_str()), ("Software", SOFTWARE)] {
                let _ = enc.add_text_chunk(k.into(), v.into());
            }
            let mut w = enc.write_header().expect("png header");
            w.write_image_data(img.as_raw()).expect("png data");
        }
    }
    out
}

/// Minimal little-endian TIFF with IFD0 tags Software, Artist, Copyright.
pub fn exif_tiff(meta: &Meta) -> Vec<u8> {
    let tags: [(u16, &str); 3] = [(0x0131, SOFTWARE), (0x013B, &meta.artist), (0x8298, &meta.copyright)];
    let ifd_len = 2 + tags.len() * 12 + 4;
    let mut head = b"II*\0".to_vec();
    head.extend_from_slice(&8u32.to_le_bytes());
    head.extend_from_slice(&(tags.len() as u16).to_le_bytes());
    let mut data = Vec::new();
    for (tag, text) in tags {
        let mut bytes: Vec<u8> = text.bytes().filter(|b| *b != 0).collect();
        bytes.push(0);
        head.extend_from_slice(&tag.to_le_bytes());
        head.extend_from_slice(&2u16.to_le_bytes()); // ASCII
        head.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        if bytes.len() <= 4 {
            bytes.resize(4, 0);
            head.extend_from_slice(&bytes);
        } else {
            head.extend_from_slice(&((8 + ifd_len + data.len()) as u32).to_le_bytes());
            data.extend_from_slice(&bytes);
            if data.len() % 2 == 1 {
                data.push(0);
            }
        }
    }
    head.extend_from_slice(&0u32.to_le_bytes());
    head.extend_from_slice(&data);
    head
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(id: &str) -> &'static Size {
        &PRESETS.iter().find(|p| p.id == id).unwrap().size
    }

    #[test]
    fn preset_dimensions() {
        let big = RgbImage::new(4000, 3000);
        assert_eq!(fit(&big, preset("ig_portrait"), Mode::Off).dimensions(), (1080, 1350));
        assert_eq!(fit(&big, preset("ig_tall"), Mode::Off).dimensions(), (1080, 1440));
        assert_eq!(fit(&big, preset("fb_feed"), Mode::Off).dimensions(), (2048, 1536));
    }

    #[test]
    fn small_source_kept_or_upscaled() {
        let small = RgbImage::new(800, 800);
        assert_eq!(fit(&small, &Size::Exact(1080, 1350), Mode::Off).dimensions(), (640, 800));
        assert_eq!(fit(&small, &Size::Exact(1080, 1350), Mode::Lanczos).dimensions(), (1080, 1350));
    }

    #[test]
    fn jpeg_and_png_round_trip_with_metadata() {
        let img = RgbImage::new(64, 32);
        for f in [Format::Jpeg, Format::Png] {
            let bytes = encode(&img, f, 95, None, &Meta::default());
            assert_eq!(decode(&bytes, "x").unwrap().img.dimensions(), (64, 32));
            assert!(bytes.windows(9).any(|w| w == b"cgradying"));
        }
    }

    #[test]
    fn exif_has_tiff_header_and_offsets_in_range() {
        let t = exif_tiff(&Meta::default());
        assert_eq!(&t[..4], b"II*\0");
        let count = u16::from_le_bytes([t[8], t[9]]) as usize;
        assert_eq!(count, 3);
        for i in 0..count {
            let e = 10 + i * 12;
            let len = u32::from_le_bytes(t[e + 4..e + 8].try_into().unwrap()) as usize;
            if len > 4 {
                let off = u32::from_le_bytes(t[e + 8..e + 12].try_into().unwrap()) as usize;
                assert_eq!(t[off + len - 1], 0, "string {i} is null-terminated in range");
            }
        }
    }
}
