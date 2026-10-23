use image::{imageops, Rgb, RgbImage};

/// Cut `img` into `cols` panels of ratio `rw:rh`. The image is centered on a
/// padded canvas sized so no source pixel is cropped or resampled.
pub fn split(img: &RgbImage, cols: u32, (rw, rh): (u32, u32), pad: Rgb<u8>) -> Vec<RgbImage> {
    let (w, h) = img.dimensions();
    let cols = cols.max(1) as u64;
    let (rw, rh) = (rw.max(1) as u64, rh.max(1) as u64);
    // Canvas ratio is cols*rw : rh. Grow whichever side is short.
    let (cw, ch) = if w as u64 * rh >= h as u64 * cols * rw {
        (w as u64, (w as u64 * rh).div_ceil(cols * rw))
    } else {
        ((h as u64 * cols * rw).div_ceil(rh), h as u64)
    };
    let mut canvas = RgbImage::from_pixel(cw as u32, ch as u32, pad);
    imageops::replace(&mut canvas, img, ((cw - w as u64) / 2) as i64, ((ch - h as u64) / 2) as i64);

    // Integer boundaries so the panels butt-join exactly; widths differ by at most 1px.
    (0..cols)
        .map(|i| {
            let x0 = (i * cw / cols) as u32;
            let x1 = ((i + 1) * cw / cols) as u32;
            imageops::crop_imm(&canvas, x0, 0, x1 - x0, ch as u32).to_image()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]))
    }

    fn stitch(panels: &[RgbImage]) -> RgbImage {
        let w = panels.iter().map(|p| p.width()).sum();
        let mut out = RgbImage::new(w, panels[0].height());
        let mut x = 0;
        for p in panels {
            imageops::replace(&mut out, p, x, 0);
            x += p.width() as i64;
        }
        out
    }

    #[test]
    fn two_by_one_into_two_squares_is_lossless() {
        let src = gradient(2000, 1000);
        let panels = split(&src, 2, (1, 1), Rgb([255, 255, 255]));
        assert_eq!(panels.len(), 2);
        assert!(panels.iter().all(|p| p.dimensions() == (1000, 1000)));
        assert_eq!(stitch(&panels), src);
    }

    #[test]
    fn short_image_pads_top_and_bottom() {
        // 3000x1000 into 2 squares: canvas 3000x1500, 250px pad top and bottom.
        let panels = split(&gradient(3000, 1000), 2, (1, 1), Rgb([0, 0, 0]));
        assert!(panels.iter().all(|p| p.dimensions() == (1500, 1500)));
        assert_eq!(*panels[0].get_pixel(10, 10), Rgb([0, 0, 0]));
        assert_eq!(*panels[0].get_pixel(10, 250), Rgb([10, 0, 10]));
    }

    #[test]
    fn tall_image_pads_sides_for_portrait_panels() {
        // 1000x1000 into 2 panels of 4:5: canvas 1600x1000, 300px pad each side.
        let panels = split(&gradient(1000, 1000), 2, (4, 5), Rgb([0, 0, 0]));
        assert!(panels.iter().all(|p| p.dimensions() == (800, 1000)));
        assert_eq!(*panels[0].get_pixel(299, 0), Rgb([0, 0, 0]));
        assert_eq!(*panels[0].get_pixel(301, 5), Rgb([1, 5, 6]));
    }

    #[test]
    fn odd_width_panels_still_cover_every_pixel() {
        let src = gradient(1001, 333);
        let panels = split(&src, 3, (1, 1), Rgb([9, 9, 9]));
        assert_eq!(stitch(&panels).width(), panels.iter().map(|p| p.width()).sum::<u32>());
        assert_eq!(stitch(&panels).dimensions(), (1001, 334));
    }
}
