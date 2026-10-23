use image::{imageops::FilterType, DynamicImage, ImageDecoder, ImageReader, RgbImage};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
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
    Preset { id: "ig_landscape", folder: "instagram", size: Size::Exact(1080, 566) },
    Preset { id: "ig_story", folder: "instagram", size: Size::Exact(1080, 1920) },
    Preset { id: "fb_feed", folder: "facebook", size: Size::LongSide(2048) },
    Preset { id: "fb_story", folder: "facebook", size: Size::Exact(1080, 1920) },
];

pub struct Source {
    pub img: RgbImage,
    pub icc: Option<Vec<u8>>,
}

pub fn decode(bytes: &[u8]) -> image::ImageResult<Source> {
    let mut decoder = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?.into_decoder()?;
    let icc = decoder.icc_profile().ok().flatten();
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(Source { img: img.to_rgb8(), icc })
}

/// Center-crop to the target ratio, then downscale. Never upscales: a small
/// source comes out at its own size in the right ratio.
pub fn fit(img: &RgbImage, size: &Size) -> RgbImage {
    let (w, h) = img.dimensions();
    let (tw, th) = match *size {
        Size::Exact(tw, th) => (tw, th),
        Size::LongSide(l) if w >= h => (l, (l as u64 * h as u64 / w as u64) as u32),
        Size::LongSide(l) => ((l as u64 * w as u64 / h as u64) as u32, l),
    };
    let (cw, ch) = if w as u64 * th as u64 > h as u64 * tw as u64 {
        ((h as u64 * tw as u64 / th as u64) as u32, h)
    } else {
        (w, (w as u64 * th as u64 / tw as u64) as u32)
    };
    let crop = image::imageops::crop_imm(img, (w - cw) / 2, (h - ch) / 2, cw, ch).to_image();
    if cw <= tw {
        return crop;
    }
    image::imageops::resize(&crop, tw, th, FilterType::Lanczos3)
}

/// Quality 95 with full-resolution chroma (4:4:4) keeps red/blue edges sharp
/// through the platform's own re-encode. Source ICC profile is carried over.
pub fn encode_jpeg(img: &RgbImage, icc: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = Encoder::new(&mut out, 95);
    enc.set_sampling_factor(SamplingFactor::R_4_4_4);
    enc.set_progressive(true);
    if let Some(icc) = icc {
        let _ = enc.add_icc_profile(icc);
    }
    enc.encode(img.as_raw(), img.width() as u16, img.height() as u16, ColorType::Rgb)
        .expect("output sizes fit in u16");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_dimensions() {
        let big = RgbImage::new(4000, 3000);
        let dims = |id: &str| fit(&big, &PRESETS.iter().find(|p| p.id == id).unwrap().size).dimensions();
        assert_eq!(dims("ig_portrait"), (1080, 1350));
        assert_eq!(dims("ig_story"), (1080, 1920));
        assert_eq!(dims("fb_feed"), (2048, 1536));
    }

    #[test]
    fn small_source_is_not_upscaled() {
        let out = fit(&RgbImage::new(800, 800), &Size::Exact(1080, 1350));
        assert_eq!(out.dimensions(), (640, 800));
    }

    #[test]
    fn jpeg_round_trips() {
        let jpg = encode_jpeg(&RgbImage::new(64, 32), None);
        assert_eq!(decode(&jpg).unwrap().img.dimensions(), (64, 32));
    }
}
