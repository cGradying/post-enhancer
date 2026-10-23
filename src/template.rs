use crate::{media, upscale};
use image::{imageops, Rgb, RgbImage, Rgba, RgbaImage};
use serde::Deserialize;

/// A frame for one photo: canvas size, the slot the photo sits in, and its
/// styling. Text, logo and watermark arrive pre-rendered in the `over` layer.
#[derive(Deserialize, Clone)]
pub struct Layout {
    pub w: u32,
    pub h: u32,
    pub slot: Slot,
    #[serde(default)]
    pub cover: bool,
    #[serde(default)]
    pub border: u32,
    #[serde(default = "white")]
    pub border_color: String,
    #[serde(default)]
    pub radius: u32,
    #[serde(default)]
    pub shadow: bool,
    /// Blur the photo itself as the background instead of the `under` layer.
    #[serde(default)]
    pub blur_bg: bool,
    #[serde(default = "white")]
    pub bg_color: String,
}

#[derive(Deserialize, Clone, Copy)]
pub struct Slot {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

fn white() -> String {
    "#ffffff".into()
}

pub fn parse_hex(s: &str) -> Rgb<u8> {
    let s = s.trim_start_matches('#');
    let c = |i: usize| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(255);
    Rgb([c(0), c(2), c(4)])
}

pub fn compose(photo: &RgbImage, l: &Layout, under: Option<&RgbaImage>, over: Option<&RgbaImage>, up: upscale::Mode) -> RgbImage {
    let (w, h) = (l.w.clamp(16, 8000), l.h.clamp(16, 8000));
    let mut canvas = background(photo, l, under, w, h);

    // Photo size inside the slot.
    let s = l.slot;
    let (pw, ph) = photo.dimensions();
    let placed = if l.cover {
        let rect = media::cover_rect(pw, ph, s.w, s.h);
        if rect.2 >= s.w || up == upscale::Mode::Off {
            media::resize_rect(photo, rect, s.w, s.h)
        } else {
            upscale::to_size(&imageops::crop_imm(photo, rect.0, rect.1, rect.2, rect.3).to_image(), s.w, s.h, up)
        }
    } else {
        let scale = (s.w as f64 / pw as f64).min(s.h as f64 / ph as f64);
        let (dw, dh) = ((pw as f64 * scale).round().max(1.0) as u32, (ph as f64 * scale).round().max(1.0) as u32);
        match (scale > 1.0, up) {
            (true, upscale::Mode::Off) => photo.clone(),
            (true, _) => upscale::to_size(photo, dw, dh, up),
            (false, _) => media::resize(photo, dw, dh),
        }
    };
    let (dw, dh) = placed.dimensions();
    let (x, y) = (s.x as i64 + (s.w as i64 - dw as i64) / 2, s.y as i64 + (s.h as i64 - dh as i64) / 2);

    if l.shadow {
        shadow(&mut canvas, x, y, dw, dh);
    }
    if l.border > 0 {
        let b = l.border as i64;
        let c = parse_hex(&l.border_color);
        let (bw, bh) = (dw + 2 * l.border, dh + 2 * l.border);
        paint(&mut canvas, x - b, y - b, bw, bh, l.radius + l.border, |_, _| Rgba([c[0], c[1], c[2], 255]));
    }
    paint(&mut canvas, x, y, dw, dh, l.radius, |px, py| {
        let p = placed.get_pixel(px, py).0;
        Rgba([p[0], p[1], p[2], 255])
    });

    if let Some(over) = over {
        let fitted;
        let over = if over.dimensions() == (w, h) { over } else { fitted = imageops::resize(over, w, h, imageops::FilterType::Triangle); &fitted };
        imageops::overlay(&mut canvas, over, 0, 0);
    }
    image::DynamicImage::ImageRgba8(canvas).to_rgb8()
}

fn background(photo: &RgbImage, l: &Layout, under: Option<&RgbaImage>, w: u32, h: u32) -> RgbaImage {
    if l.blur_bg {
        // Blur at 1/8 size: same look, a fraction of the cost.
        let (sw, sh) = ((w / 8).max(8), (h / 8).max(8));
        let small = media::resize_rect(photo, media::cover_rect(photo.width(), photo.height(), sw, sh), sw, sh);
        let soft = imageops::blur(&small, 3.0);
        let big = media::resize(&soft, w, h);
        return RgbaImage::from_fn(w, h, |x, y| {
            let p = big.get_pixel(x, y).0;
            let d = |v: u8| (v as u16 * 3 / 4) as u8;
            Rgba([d(p[0]), d(p[1]), d(p[2]), 255])
        });
    }
    let c = parse_hex(&l.bg_color);
    let mut canvas = RgbaImage::from_pixel(w, h, Rgba([c[0], c[1], c[2], 255]));
    if let Some(under) = under {
        let fitted;
        let under = if under.dimensions() == (w, h) { under } else { fitted = imageops::resize(under, w, h, imageops::FilterType::Triangle); &fitted };
        imageops::overlay(&mut canvas, under, 0, 0);
    }
    canvas
}

/// Soft drop shadow: a blurred rectangle drawn at 1/4 scale and stretched up.
fn shadow(canvas: &mut RgbaImage, x: i64, y: i64, w: u32, h: u32) {
    let spread = (w.max(h) / 25).max(6) as i64;
    let q = 4;
    let (mw, mh) = ((w as i64 + 4 * spread) / q, (h as i64 + 4 * spread) / q);
    let mut mask = image::GrayImage::new(mw.max(1) as u32, mh.max(1) as u32);
    for my in 0..mask.height() {
        for mx in 0..mask.width() {
            let inside = (mx as i64) >= 2 * spread / q && (mx as i64) < (2 * spread + w as i64) / q && (my as i64) >= 2 * spread / q && (my as i64) < (2 * spread + h as i64) / q;
            mask.put_pixel(mx, my, image::Luma([if inside { 120 } else { 0 }]));
        }
    }
    let mask = imageops::blur(&mask, spread as f32 / q as f32 / 1.5);
    let mask = imageops::resize(&mask, (mw * q) as u32, (mh * q) as u32, imageops::FilterType::Triangle);
    let (ox, oy) = (x - 2 * spread, y - 2 * spread + spread / 2);
    for (mx, my, a) in mask.enumerate_pixels() {
        let (cx, cy) = (ox + mx as i64, oy + my as i64);
        if a.0[0] > 0 && cx >= 0 && cy >= 0 && (cx as u32) < canvas.width() && (cy as u32) < canvas.height() {
            let p = canvas.get_pixel_mut(cx as u32, cy as u32);
            let k = 255 - a.0[0] as u32;
            for c in 0..3 {
                p.0[c] = (p.0[c] as u32 * k / 255) as u8;
            }
        }
    }
}

/// Paint a `w`×`h` rounded rect at (`x`,`y`), anti-aliased at the corners.
fn paint(canvas: &mut RgbaImage, x: i64, y: i64, w: u32, h: u32, radius: u32, color: impl Fn(u32, u32) -> Rgba<u8>) {
    let r = radius.min(w / 2).min(h / 2) as f32;
    for py in 0..h {
        for px in 0..w {
            let (cx, cy) = (x + px as i64, y + py as i64);
            if cx < 0 || cy < 0 || cx as u32 >= canvas.width() || cy as u32 >= canvas.height() {
                continue;
            }
            let cov = if r > 0.0 { corner_coverage(px as f32 + 0.5, py as f32 + 0.5, w as f32, h as f32, r) } else { 1.0 };
            if cov <= 0.0 {
                continue;
            }
            let src = color(px, py);
            let dst = canvas.get_pixel_mut(cx as u32, cy as u32);
            for c in 0..3 {
                dst.0[c] = (src.0[c] as f32 * cov + dst.0[c] as f32 * (1.0 - cov)).round() as u8;
            }
        }
    }
}

fn corner_coverage(px: f32, py: f32, w: f32, h: f32, r: f32) -> f32 {
    let dx = (r - px).max(px - (w - r)).max(0.0);
    let dy = (r - py).max(py - (h - r)).max(0.0);
    if dx == 0.0 || dy == 0.0 {
        return 1.0;
    }
    (r + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout {
            w: 300, h: 400, slot: Slot { x: 30, y: 30, w: 240, h: 340 },
            cover: false, border: 0, border_color: white(), radius: 0, shadow: false, blur_bg: false, bg_color: "#102030".into(),
        }
    }

    #[test]
    fn landscape_photo_is_letterboxed_in_portrait_slot() {
        let photo = RgbImage::from_pixel(400, 300, Rgb([200, 10, 10]));
        let out = compose(&photo, &layout(), None, None, upscale::Mode::Off);
        assert_eq!(out.dimensions(), (300, 400));
        // 240 wide → 180 tall, centered: rows 110..290.
        assert_eq!(*out.get_pixel(150, 111), Rgb([200, 10, 10]));
        assert_eq!(*out.get_pixel(150, 105), Rgb([16, 32, 48]));
        assert_eq!(*out.get_pixel(150, 291), Rgb([16, 32, 48]));
    }

    #[test]
    fn cover_fills_slot_and_overlay_lands_on_top() {
        let photo = RgbImage::from_pixel(400, 300, Rgb([200, 10, 10]));
        let mut l = layout();
        l.cover = true;
        let mut over = RgbaImage::new(300, 400);
        over.put_pixel(5, 5, Rgba([0, 255, 0, 255]));
        let out = compose(&photo, &l, None, Some(&over), upscale::Mode::Off);
        assert_eq!(*out.get_pixel(31, 31), Rgb([200, 10, 10]));
        assert_eq!(*out.get_pixel(268, 368), Rgb([200, 10, 10]));
        assert_eq!(*out.get_pixel(5, 5), Rgb([0, 255, 0]));
    }

    #[test]
    fn rounded_corner_shows_background() {
        let photo = RgbImage::from_pixel(240, 340, Rgb([200, 10, 10]));
        let mut l = layout();
        l.radius = 40;
        let out = compose(&photo, &l, None, None, upscale::Mode::Off);
        assert_eq!(*out.get_pixel(31, 31), Rgb([16, 32, 48]));
        assert_eq!(*out.get_pixel(150, 200), Rgb([200, 10, 10]));
    }
}
