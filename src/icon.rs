//! The SilkScroll icon, drawn in code so the repository has no binary assets.
//!
//! This file is also compiled into `build.rs`, which turns the same drawing
//! into the `.ico` resource embedded in the exe.

/// Draws the icon at `size` x `size` pixels as straight (not premultiplied)
/// RGBA. `active` picks the colorful version; otherwise it is greyed out, which
/// the tray uses while smooth scrolling is turned off.
pub fn rgba(size: u32, active: bool) -> Vec<u8> {
    let s = size as f32;
    let (top, bottom) =
        if active { ([0x63, 0x66, 0xF1], [0x06, 0xB6, 0xD4]) } else { ([0xA1, 0xA1, 0xAA], [0x71, 0x71, 0x7A]) };
    let radius = s * 0.24;
    let stroke = (s * 0.09).max(1.4);

    let mut px = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let bg = coverage(rounded_square(fx, fy, s, radius));

            // Two downward chevrons: the "scroll" mark.
            let mut fg = 0.0f32;
            for (cy, alpha) in [(0.33f32, 0.6f32), (0.53, 1.0)] {
                let a = (0.29 * s, cy * s);
                let m = (0.5 * s, (cy + 0.15) * s);
                let b = (0.71 * s, cy * s);
                let d = segment(fx, fy, a, m).min(segment(fx, fy, m, b)) - stroke / 2.0;
                fg = fg.max(coverage(d) * alpha);
            }

            let t = fy / s;
            for c in 0..3 {
                let base = top[c] as f32 + (bottom[c] as f32 - top[c] as f32) * t;
                px.push((base + (255.0 - base) * fg).round() as u8);
            }
            px.push((bg * 255.0).round() as u8);
        }
    }
    px
}

/// The icon as a `BITMAPINFOHEADER` + pixels + mask block: the format both
/// `.ico` files and `CreateIconFromResourceEx` expect.
pub fn dib(size: u32, active: bool) -> Vec<u8> {
    let rgba = rgba(size, active);
    let mask_row = size.div_ceil(32) * 4;
    let image_bytes = size * size * 4 + mask_row * size;

    let mut out = Vec::with_capacity(40 + image_bytes as usize);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&(size as i32 * 2).to_le_bytes()); // color + mask
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&image_bytes.to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);

    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    // Transparency comes from the alpha channel, so the AND mask stays empty.
    out.resize(out.len() + (mask_row * size) as usize, 0);
    out
}

/// A complete `.ico` file containing every size in `sizes`.
#[allow(dead_code)] // used by build.rs
pub fn ico_file(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| dib(s, true)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());

    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&size, image) in sizes.iter().zip(&images) {
        let dim = if size >= 256 { 0 } else { size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(image.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in images {
        out.extend_from_slice(&image);
    }
    out
}

fn coverage(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

fn rounded_square(x: f32, y: f32, size: f32, radius: f32) -> f32 {
    let c = size / 2.0;
    let half = c - 0.5;
    let qx = (x - c).abs() - (half - radius);
    let qy = (y - c).abs() - (half - radius);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}

fn segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (px, py) = (x - a.0, y - a.1);
    let (bx, by) = (b.0 - a.0, b.1 - a.1);
    let h = ((px * bx + py * by) / (bx * bx + by * by)).clamp(0.0, 1.0);
    ((px - bx * h).powi(2) + (py - by * h).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_are_transparent_and_center_is_opaque() {
        let size = 32;
        let px = rgba(size, true);
        assert_eq!(px[3], 0, "top-left corner");
        let center = ((size / 2 * size + size / 2) * 4 + 3) as usize;
        assert_eq!(px[center], 255);
    }

    #[test]
    fn ico_header_matches_its_images() {
        let ico = ico_file(&[16, 256]);
        assert_eq!(&ico[0..6], &[0, 0, 1, 0, 2, 0]);
        assert_eq!(ico[6], 16);
        assert_eq!(ico[6 + 16], 0, "256 px is stored as 0");
        let first_len = u32::from_le_bytes(ico[14..18].try_into().unwrap()) as usize;
        assert_eq!(first_len, dib(16, true).len());
    }
}
