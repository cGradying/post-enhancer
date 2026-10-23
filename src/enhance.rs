use image::RgbImage;

/// One-click auto enhance: stretch luminance from its 0.5th to 99.5th
/// percentile, add a gentle S-curve, then sharpen lightly. The same curve is
/// applied to all three channels, so hues don't shift.
pub fn auto(img: &mut RgbImage) {
    let mut hist = [0u64; 256];
    for p in img.pixels() {
        hist[luma(p.0) as usize] += 1;
    }
    let total = img.width() as u64 * img.height() as u64;
    let pct = |q: u64| {
        let mut acc = 0;
        (0..256).find(|&i| {
            acc += hist[i];
            acc * 1000 >= total * q
        }).unwrap_or(255) as f32
    };
    let (lo, hi) = (pct(5), pct(995).max(pct(5) + 1.0));

    let lut: [u8; 256] = std::array::from_fn(|i| {
        let t = ((i as f32 - lo) / (hi - lo)).clamp(0.0, 1.0);
        let s = t * t * (3.0 - 2.0 * t); // smoothstep
        ((t * 0.75 + s * 0.25) * 255.0).round() as u8
    });
    for p in img.pixels_mut() {
        p.0 = p.0.map(|c| lut[c as usize]);
    }
    *img = image::imageops::unsharpen(img, 0.8, 2);
}

fn luma([r, g, b]: [u8; 3]) -> u8 {
    ((r as u32 * 54 + g as u32 * 183 + b as u32 * 19) >> 8) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widens_a_flat_histogram_and_keeps_size() {
        let mut img = RgbImage::from_fn(100, 40, |x, _| {
            let v = 100 + (x % 50) as u8;
            image::Rgb([v, v, v])
        });
        auto(&mut img);
        assert_eq!(img.dimensions(), (100, 40));
        let (min, max) = img.pixels().fold((255, 0), |(a, b), p| (a.min(p.0[0]), b.max(p.0[0])));
        assert!(min < 20 && max > 235, "range {min}..{max}");
    }
}
