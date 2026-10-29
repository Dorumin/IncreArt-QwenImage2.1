//! Crop / mark…: draws an [`ImageEdits`] onto an image (DESIGN.md, Crop
//! and mark).
//!
//! Lines are drawn with soft edges: each mark's coverage is worked out per
//! pixel (the most of its segments, so a line crossing itself isn't
//! darker), then blended over the image once.

use image::{Rgba, RgbaImage};

use crate::model::{ImageEdits, Mark, Shape};

/// Points around an ellipse or rectangle outline, closed.
pub fn outline(shape: &Shape) -> Vec<[f32; 2]> {
    match shape {
        Shape::Pencil(points) => points.clone(),
        Shape::Rect(a, b) => vec![[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]], [a[0], a[1]]],
        Shape::Ellipse(a, b) => {
            let (cx, cy) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
            let (rx, ry) = ((b[0] - a[0]).abs() / 2.0, (b[1] - a[1]).abs() / 2.0);
            // about one point per 4 px of outline
            let n = ((rx + ry) * std::f32::consts::PI / 4.0).clamp(24.0, 720.0) as usize;
            (0..=n)
                .map(|i| {
                    let t = i as f32 / n as f32 * std::f32::consts::TAU;
                    [cx + rx * t.cos(), cy + ry * t.sin()]
                })
                .collect()
        }
    }
}

/// Distance from `p` to the segment `a`–`b`.
pub fn distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) };
    let (x, y) = (a[0] + t * dx - p[0], a[1] + t * dy - p[1]);
    (x * x + y * y).sqrt()
}

fn draw(img: &mut RgbaImage, mark: &Mark) {
    let points = outline(&mark.shape);
    let Some(&first) = points.first() else { return };
    let (w, h) = img.dimensions();
    let r = mark.width.max(0.5) / 2.0;
    let mut cover = vec![0f32; (w * h) as usize];
    let segments: Vec<_> =
        if points.len() == 1 { vec![(first, first)] } else { points.windows(2).map(|s| (s[0], s[1])).collect() };
    for (a, b) in segments {
        let x0 = (a[0].min(b[0]) - r - 1.0).floor().max(0.0) as u32;
        let y0 = (a[1].min(b[1]) - r - 1.0).floor().max(0.0) as u32;
        let x1 = ((a[0].max(b[0]) + r + 1.0).ceil().max(0.0) as u32).min(w);
        let y1 = ((a[1].max(b[1]) + r + 1.0).ceil().max(0.0) as u32).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = distance([x as f32 + 0.5, y as f32 + 0.5], a, b);
                let c = (r + 0.5 - d).clamp(0.0, 1.0);
                let i = (y * w + x) as usize;
                cover[i] = cover[i].max(c);
            }
        }
    }
    let [cr, cg, cb, ca] = mark.color;
    let alpha = ca as f32 / 255.0;
    for (i, c) in cover.into_iter().enumerate() {
        if c <= 0.0 {
            continue;
        }
        let p = img.get_pixel_mut(i as u32 % w, i as u32 / w);
        let a = c * alpha;
        // "over", unpremultiplied: a transparent image gets the line on top
        let below = p[3] as f32 / 255.0;
        let out = a + below * (1.0 - a);
        let mix = |top: u8, bottom: u8| {
            ((top as f32 * a + bottom as f32 * below * (1.0 - a)) / out.max(1e-6)).round().clamp(0.0, 255.0) as u8
        };
        *p = Rgba([mix(cr, p[0]), mix(cg, p[1]), mix(cb, p[2]), (out * 255.0).round() as u8]);
    }
}

/// The crop, clamped to an image of `w` × `h` (at least 1 × 1).
pub fn clamp_crop(crop: [u32; 4], w: u32, h: u32) -> [u32; 4] {
    let x = crop[0].min(w.saturating_sub(1));
    let y = crop[1].min(h.saturating_sub(1));
    [x, y, crop[2].clamp(1, w - x), crop[3].clamp(1, h - y)]
}

/// The image with its marks drawn, then cropped.
pub fn render(img: &RgbaImage, edits: &ImageEdits) -> RgbaImage {
    let mut out = img.clone();
    for m in &edits.marks {
        draw(&mut out, m);
    }
    match edits.crop {
        Some(c) => {
            let [x, y, w, h] = clamp_crop(c, out.width(), out.height());
            image::imageops::crop_imm(&out, x, y, w, h).to_image()
        }
        None => out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [u8; 4] = [255, 0, 0, 255];

    fn grey(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([128, 128, 128, 255]))
    }

    #[test]
    fn crops_after_drawing() {
        let edits = ImageEdits {
            crop: Some([10, 20, 30, 40]),
            marks: vec![Mark { shape: Shape::Pencil(vec![[0.0, 25.0], [100.0, 25.0]]), color: RED, width: 4.0 }],
        };
        let out = render(&grey(100, 100), &edits);
        assert_eq!(out.dimensions(), (30, 40));
        // the line is at y 25, so row 5 of the crop
        assert_eq!(*out.get_pixel(15, 5), Rgba(RED));
        assert_eq!(*out.get_pixel(15, 30), Rgba([128, 128, 128, 255]));
    }

    #[test]
    fn a_crop_past_the_edge_is_clamped() {
        let out = render(&grey(50, 40), &ImageEdits { crop: Some([40, 30, 100, 100]), marks: vec![] });
        assert_eq!(out.dimensions(), (10, 10));
    }

    #[test]
    fn shapes_draw_their_outline_only() {
        for shape in [Shape::Rect([10.0, 10.0], [90.0, 90.0]), Shape::Ellipse([10.0, 10.0], [90.0, 90.0])] {
            let edits = ImageEdits { crop: None, marks: vec![Mark { shape, color: RED, width: 3.0 }] };
            let out = render(&grey(100, 100), &edits);
            assert_eq!(*out.get_pixel(50, 50), Rgba([128, 128, 128, 255]), "inside stays");
            assert_eq!(*out.get_pixel(50, 10), Rgba(RED), "top edge drawn");
        }
    }

    #[test]
    fn lines_go_over_transparency() {
        let clear = RgbaImage::from_pixel(20, 20, Rgba([0, 0, 0, 0]));
        let mark = Mark { shape: Shape::Pencil(vec![[10.0, 10.0]]), color: RED, width: 6.0 };
        let out = render(&clear, &ImageEdits { crop: None, marks: vec![mark] });
        assert_eq!(*out.get_pixel(10, 10), Rgba(RED));
        assert_eq!(out.get_pixel(0, 0)[3], 0);
    }
}
