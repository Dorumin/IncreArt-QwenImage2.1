//! Review's Tile…: picked images on a grid of cells, composed into one image
//! (DESIGN.md, Tiling).

use std::path::Path;
use std::sync::Arc;

use ab_glyph::{Font, FontRef, PxScale, PxScaleFont, ScaleFont};
use image::{Pixel, Rgba, RgbaImage, imageops};
use increart_core::ImageRef;
use increart_core::config::{TextSize, TileSettings};

/// What a tile shows.
#[derive(Clone, Debug, PartialEq)]
pub enum TileContent {
    Image(ImageRef),
    /// Wrapped, centred text (in the caption colour).
    Text {
        text: String,
        size: TextSize,
    },
}

/// One image or text block on the grid: its top-left cell and the cells it
/// spans.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub content: TileContent,
    pub col: u32,
    pub row: u32,
    pub w: u32,
    pub h: u32,
    pub caption: String,
}

/// Tiles on a grid `cols` wide; rows as many as they need. Cells no tile
/// covers stay empty. A title and subtitle (if any) go above the grid.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TileLayout {
    pub cols: u32,
    pub tiles: Vec<Tile>,
    pub title: String,
    pub subtitle: String,
}

/// Bigger than this (output pixels) is refused.
pub const MAX_PIXELS: u64 = 250_000_000;
pub const MAX_COLS: u32 = 32;

impl TileLayout {
    /// The images in order, left to right, `cols` per row.
    pub fn flow(images: Vec<ImageRef>, cols: u32) -> TileLayout {
        let cols = cols.clamp(1, MAX_COLS);
        let tiles = images
            .into_iter()
            .enumerate()
            .map(|(i, image)| Tile {
                content: TileContent::Image(image),
                col: i as u32 % cols,
                row: i as u32 / cols,
                w: 1,
                h: 1,
                caption: String::new(),
            })
            .collect();
        TileLayout { cols, tiles, ..TileLayout::default() }
    }

    /// About square: ⌈√n⌉ columns.
    pub fn square(images: Vec<ImageRef>) -> TileLayout {
        let cols = (images.len() as f64).sqrt().ceil() as u32;
        TileLayout::flow(images, cols)
    }

    /// The images at these (column, row) cells (the contact sheet's),
    /// shifted so the top-left one is at 0, 0.
    pub fn at(cells: Vec<(ImageRef, u32, u32)>) -> TileLayout {
        let c0 = cells.iter().map(|c| c.1).min().unwrap_or(0);
        let r0 = cells.iter().map(|c| c.2).min().unwrap_or(0);
        let cols = cells.iter().map(|c| c.1 - c0 + 1).max().unwrap_or(1).min(MAX_COLS);
        let tiles = cells
            .into_iter()
            .map(|(image, c, r)| Tile {
                content: TileContent::Image(image),
                col: c - c0,
                row: r - r0,
                w: 1,
                h: 1,
                caption: String::new(),
            })
            .collect();
        let mut l = TileLayout { cols, tiles, ..TileLayout::default() };
        l.settle(&[]);
        l
    }

    pub fn rows(&self) -> u32 {
        self.tiles.iter().map(|t| t.row + t.h).max().unwrap_or(1)
    }

    /// The tile covering a cell.
    pub fn tile_at(&self, col: u32, row: u32) -> Option<usize> {
        self.tiles.iter().position(|t| (t.col..t.col + t.w).contains(&col) && (t.row..t.row + t.h).contains(&row))
    }

    /// Moves a tile to a cell; the tile there (if any) takes its old place.
    pub fn move_tile(&mut self, i: usize, col: u32, row: u32) {
        let Some(t) = self.tiles.get(i) else { return };
        let old = (t.col, t.row);
        let other = self.tile_at(col, row).filter(|&j| j != i);
        if let Some(j) = other {
            (self.tiles[j].col, self.tiles[j].row) = old;
        }
        (self.tiles[i].col, self.tiles[i].row) = (col.min(self.cols - 1), row);
        self.settle(&[Some(i), other].into_iter().flatten().collect::<Vec<_>>());
    }

    /// How many cells a tile spans; the tiles in the way move.
    pub fn set_span(&mut self, i: usize, w: u32, h: u32) {
        let Some(t) = self.tiles.get_mut(i) else { return };
        (t.w, t.h) = (w.max(1), h.max(1));
        self.settle(&[i]);
    }

    /// Tiles past the new width move to the first free cells.
    pub fn set_cols(&mut self, cols: u32) {
        self.cols = cols.clamp(1, MAX_COLS);
        self.settle(&[]);
    }

    /// Packs the tiles in order, leaving no holes (as far as spans allow).
    pub fn reflow(&mut self) {
        for t in &mut self.tiles {
            t.col = u32::MAX;
        }
        self.settle(&[]);
    }

    /// A text block in a cell (or, if taken, the first free one).
    pub fn add_text(&mut self, col: u32, row: u32, text: String) -> usize {
        let content = TileContent::Text { text, size: TextSize::Auto };
        let t = Tile { content, col, row, w: 1, h: 1, caption: String::new() };
        self.tiles.push(t);
        self.settle(&[]);
        self.tiles.len() - 1
    }

    pub fn images(&self) -> impl Iterator<Item = &ImageRef> {
        self.tiles.iter().filter_map(Tile::image)
    }

    pub fn remove(&mut self, i: usize) {
        if i < self.tiles.len() {
            self.tiles.remove(i);
        }
    }

    /// Keeps each tile where it is if it fits there, else puts it in the
    /// first free place (row by row); `first` are placed before the rest.
    fn settle(&mut self, first: &[usize]) {
        let n = self.tiles.len();
        let order: Vec<usize> = first.iter().copied().chain((0..n).filter(|i| !first.contains(i))).collect();
        let mut placed: Vec<(u32, u32, u32, u32)> = vec![];
        let free = |placed: &[(u32, u32, u32, u32)], c: u32, r: u32, w: u32, h: u32| {
            placed.iter().all(|&(pc, pr, pw, ph)| c + w <= pc || pc + pw <= c || r + h <= pr || pr + ph <= r)
        };
        for i in order {
            let t = &mut self.tiles[i];
            t.w = t.w.clamp(1, self.cols);
            t.h = t.h.max(1);
            let fits = t.col.checked_add(t.w).is_some_and(|e| e <= self.cols);
            if !(fits && free(&placed, t.col, t.row, t.w, t.h)) {
                let (w, h, cols) = (t.w, t.h, self.cols);
                let spot = (0..)
                    .flat_map(|r| (0..=cols - w).map(move |c| (c, r)))
                    .find(|&(c, r)| free(&placed, c, r, w, h))
                    .expect("rows are unbounded");
                (t.col, t.row) = spot;
            }
            placed.push((t.col, t.row, t.w, t.h));
        }
    }
}

impl Tile {
    pub fn image(&self) -> Option<&ImageRef> {
        match &self.content {
            TileContent::Image(i) => Some(i),
            TileContent::Text { .. } => None,
        }
    }
}

/// A composed image, shared (it can be large).
#[derive(Clone)]
pub struct TileImage(pub Arc<RgbaImage>);

impl PartialEq for TileImage {
    fn eq(&self, other: &TileImage) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for TileImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TileImage({}×{})", self.0.width(), self.0.height())
    }
}

/// Where the cells, tiles and captions go, in output pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub cell: (u32, u32),
    /// The strip under each row for captions (0 for none).
    pub caption: u32,
    pub gap: u32,
    /// The title's and the subtitle's lines (0 for none), above the grid.
    pub title: u32,
    pub subtitle: u32,
    pub size: (u32, u32),
    /// Text heights: the captions' (before shrinking to fit) and the
    /// title's (the subtitle's is half; a text block's at most this).
    pub caption_px: u32,
    pub title_px: u32,
}

/// The auto sizes, in percent (`TextSize::Auto`).
const CAPTION_CELL: f64 = 5.0;
const TITLE_WIDTH: f64 = 4.0;
const TITLE_CELL: f64 = 10.0;
/// A caption too wide shrinks to this share of its size before it's cut.
const SHRINK: f32 = 0.6;
const MIN_PX: u32 = 6;

/// A size in px for cells `cell_h` high; `auto` for `TextSize::Auto`.
fn px(size: TextSize, cell_h: u32, auto: f64) -> u32 {
    let v = match size {
        TextSize::Auto => auto,
        TextSize::Cell(pc) => cell_h as f64 * pc as f64 / 100.0,
        TextSize::Px(n) => n as f64,
    };
    (v.round() as u32).max(MIN_PX)
}

impl Geometry {
    /// For images whose largest are `largest` (w, h) at 100%.
    pub fn new(layout: &TileLayout, s: &TileSettings, largest: (u32, u32)) -> Geometry {
        let scale = s.scale.clamp(1, 100) as f64 / 100.0;
        let cell = (((largest.0 as f64 * scale) as u32).max(1), ((largest.1 as f64 * scale) as u32).max(1));
        let cols = layout.cols;
        let width = cols * cell.0 + (cols + 1) * s.gap;
        let ch = cell.1 as f64;
        let caption_px = px(s.caption_text, cell.1, ch * CAPTION_CELL / 100.0);
        let title_auto = (width as f64 * TITLE_WIDTH / 100.0).max(ch * TITLE_CELL / 100.0);
        let title_px = px(s.title_text, cell.1, title_auto);
        let caption = if layout.tiles.iter().any(|t| !t.caption.trim().is_empty()) {
            (caption_px as f64 * 1.5).ceil() as u32
        } else {
            0
        };
        let line = |text: &str, size: u32| if text.trim().is_empty() { 0 } else { (size as f64 * 1.4).ceil() as u32 };
        let (title, subtitle) = (line(&layout.title, title_px), line(&layout.subtitle, title_px / 2));
        let mut g = Geometry { cell, caption, gap: s.gap, title, subtitle, size: (width, 0), caption_px, title_px };
        let rows = layout.rows();
        g.size.1 = g.top() + rows * (cell.1 + caption) + rows * s.gap;
        g
    }

    /// A text block's size: its own, or (auto) as large as fits its cells,
    /// up to the title's.
    pub fn text_px(&self, t: &Tile) -> u32 {
        let TileContent::Text { text, size } = &t.content else { return self.caption_px };
        if *size != TextSize::Auto {
            return px(*size, self.cell.1, 0.0);
        }
        let (_, _, w, h) = self.image_rect(t);
        let font = font();
        let fits = |p: u32| {
            let sf = font.as_scaled(PxScale::from(p as f32));
            let words = text.split_whitespace().all(|word| text_width(&sf, word) <= w as f32);
            let lines = wrap(&sf, text, w).len() as f32;
            words && lines * (sf.height() + sf.line_gap()) - sf.line_gap() <= h as f32
        };
        // the largest that fits (the size only grows the lines)
        let (mut lo, mut hi) = (MIN_PX, self.title_px.max(MIN_PX));
        if fits(hi) {
            return hi;
        }
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if fits(mid) { lo = mid } else { hi = mid }
        }
        lo
    }

    /// Where the grid starts: below the gap, and the titles and a gap.
    fn top(&self) -> u32 {
        let titles = self.title + self.subtitle;
        self.gap + if titles > 0 { titles + self.gap } else { 0 }
    }

    /// The title's and the subtitle's areas (x, y, w, h), full width.
    pub fn title_rects(&self) -> [(u32, u32, u32, u32); 2] {
        let (g, w) = (self.gap, self.size.0.saturating_sub(2 * self.gap));
        [(g, g, w, self.title), (g, g + self.title, w, self.subtitle)]
    }

    /// A cell (x, y, w, h), its caption included.
    pub fn cell_rect(&self, col: u32, row: u32) -> (u32, u32, u32, u32) {
        let (cw, ch, g, cap) = (self.cell.0, self.cell.1, self.gap, self.caption);
        (g + col * (cw + g), self.top() + row * (ch + cap + g), cw, ch + cap)
    }

    /// A tile's image area (x, y, w, h); its caption is the `caption`
    /// pixels below it.
    pub fn image_rect(&self, t: &Tile) -> (u32, u32, u32, u32) {
        let (cw, ch, g, cap) = (self.cell.0, self.cell.1, self.gap, self.caption);
        let (x, y, _, _) = self.cell_rect(t.col, t.row);
        (x, y, t.w * cw + (t.w - 1) * g, t.h * (ch + cap) + (t.h - 1) * g - cap)
    }
}

/// Cells when there are no images to size them by.
pub const NO_IMAGES: (u32, u32) = (1024, 1024);

/// Reads the tiles' images from the library and composes them.
pub fn compose(root: &Path, layout: &TileLayout, s: &TileSettings) -> Result<RgbaImage, String> {
    let images = layout
        .tiles
        .iter()
        .map(|t| {
            let Some(image) = t.image() else { return Ok(None) };
            let path = root.join(image.rel_path());
            image::open(&path).map(|i| Some(i.to_rgba8())).map_err(|e| format!("{}: {e}", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    compose_images(layout, &images, s)
}

/// Composes decoded images (one per tile, in order; none for text).
pub fn compose_images(
    layout: &TileLayout,
    images: &[Option<RgbaImage>],
    s: &TileSettings,
) -> Result<RgbaImage, String> {
    let largest = images
        .iter()
        .flatten()
        .fold(None, |a: Option<(u32, u32)>, i| {
            Some(a.map_or(i.dimensions(), |(w, h)| (w.max(i.width()), h.max(i.height()))))
        })
        .unwrap_or(NO_IMAGES);
    let g = Geometry::new(layout, s, largest);
    let pixels = g.size.0 as u64 * g.size.1 as u64;
    if pixels > MAX_PIXELS {
        return Err(format!(
            "{} × {} is too large (over {} MP): scale it down",
            g.size.0,
            g.size.1,
            MAX_PIXELS / 1_000_000
        ));
    }
    let mut out = RgbaImage::from_pixel(g.size.0, g.size.1, Rgba(s.background));
    let font = font();
    let [tr, sr] = g.title_rects();
    for (text, size, r) in [(&layout.title, g.title_px, tr), (&layout.subtitle, g.title_px / 2, sr)] {
        if r.3 > 0 {
            draw_text(&mut out, &font, text.trim(), size, r, s.caption_color);
        }
    }
    for (t, img) in layout.tiles.iter().zip(images) {
        let (x, y, w, h) = g.image_rect(t);
        let img = match (&t.content, img) {
            (TileContent::Text { text, .. }, _) => {
                draw_text(&mut out, &font, text, g.text_px(t), (x, y, w, h), s.caption_color);
                continue;
            }
            (TileContent::Image(_), Some(img)) => img,
            (TileContent::Image(_), None) => continue,
        };
        let (iw, ih) = (img.width() as f64, img.height() as f64);
        let k = if s.fill { (w as f64 / iw).max(h as f64 / ih) } else { (w as f64 / iw).min(h as f64 / ih) };
        let (sw, sh) = (((iw * k).round() as u32).max(1), ((ih * k).round() as u32).max(1));
        let scaled = if (sw, sh) == (img.width(), img.height()) {
            img.clone()
        } else {
            imageops::resize(img, sw, sh, imageops::FilterType::CatmullRom)
        };
        if s.fill {
            let (cx, cy) = (sw.saturating_sub(w) / 2, sh.saturating_sub(h) / 2);
            let part = imageops::crop_imm(&scaled, cx, cy, w.min(sw), h.min(sh)).to_image();
            imageops::overlay(&mut out, &part, x as i64, y as i64);
        } else {
            imageops::overlay(&mut out, &scaled, (x + (w - sw) / 2) as i64, (y + (h - sh) / 2) as i64);
        }
        if g.caption > 0 && !t.caption.trim().is_empty() {
            let (text, px) = caption_fit(t.caption.trim(), w, g.caption_px);
            let sf = font.as_scaled(PxScale::from(px as f32));
            let baseline = (y + h) as f32 + (g.caption as f32 - (sf.ascent() - sf.descent())) / 2.0 + sf.ascent();
            draw_line(&mut out, &font, px, &text, (x, w), baseline, s.caption_color);
        }
    }
    Ok(out)
}

/// A text's width in px at a scale.
fn text_width(sf: &PxScaleFont<&FontRef<'_>>, t: &str) -> f32 {
    let mut prev = None;
    t.chars()
        .map(|c| {
            let id = sf.glyph_id(c);
            let k = prev.map_or(0.0, |p| sf.kern(p, id));
            prev = Some(id);
            k + sf.h_advance(id)
        })
        .sum()
}

fn font() -> FontRef<'static> {
    FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).expect("built-in font")
}

/// A caption one line `w` wide: its size shrunk (down to `SHRINK`) to fit,
/// then cut short with "…" if it still doesn't.
pub fn caption_fit(text: &str, w: u32, px: u32) -> (String, u32) {
    let font = font();
    let full = text_width(&font.as_scaled(PxScale::from(px as f32)), text);
    let k = (w as f32 / full).clamp(SHRINK, 1.0);
    let px = ((px as f32 * k).floor() as u32).max(MIN_PX);
    let sf = font.as_scaled(PxScale::from(px as f32));
    let mut text = text.to_string();
    if text_width(&sf, &text) > w as f32 {
        while !text.is_empty() && text_width(&sf, &format!("{text}…")) > w as f32 {
            text.pop();
        }
        text.push('…');
    }
    (text, px)
}

/// The lines (as typed, then word-wrapped to `w` px) of text `px` high.
pub fn text_lines(text: &str, px: u32, w: u32) -> Vec<String> {
    let font = font();
    wrap(&font.as_scaled(PxScale::from(px as f32)), text, w)
}

fn wrap(sf: &PxScaleFont<&FontRef<'_>>, text: &str, w: u32) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    for para in text.trim().lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let longer = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && text_width(sf, &longer) > w as f32 {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = longer;
            }
        }
        lines.push(line);
    }
    lines
}

/// Lines (as typed, then word-wrapped to `w`), centred both ways in
/// `(x, y, w, h)`; lines below it are left out.
fn draw_text(
    out: &mut RgbaImage,
    font: &FontRef<'static>,
    text: &str,
    size: u32,
    (x, y, w, h): (u32, u32, u32, u32),
    color: [u8; 4],
) {
    let sf = font.as_scaled(PxScale::from(size as f32));
    let mut lines = wrap(&sf, text, w);
    let lh = sf.height() + sf.line_gap();
    let fit = ((h as f32 + sf.line_gap()) / lh).floor().max(1.0) as usize;
    lines.truncate(fit);
    let top = y as f32 + (h as f32 - (lines.len() as f32 * lh - sf.line_gap())).max(0.0) / 2.0;
    for (n, line) in lines.iter().enumerate() {
        draw_line(out, font, size, line, (x, w), top + n as f32 * lh + sf.ascent(), color);
    }
}

/// One line on `baseline`, centred in the span `(x, w)`.
fn draw_line(
    out: &mut RgbaImage,
    font: &FontRef<'static>,
    size: u32,
    text: &str,
    (x, w): (u32, u32),
    baseline: f32,
    color: [u8; 4],
) {
    let sf = font.as_scaled(PxScale::from(size as f32));
    let mut pen = x as f32 + (w as f32 - text_width(&sf, text)).max(0.0) / 2.0;
    let mut prev = None;
    for c in text.chars() {
        let id = sf.glyph_id(c);
        pen += prev.map_or(0.0, |p| sf.kern(p, id));
        prev = Some(id);
        let glyph = id.with_scale_and_position(sf.scale(), ab_glyph::point(pen, baseline));
        pen += sf.h_advance(id);
        let Some(outline) = font.outline_glyph(glyph) else { continue };
        let b = outline.px_bounds();
        outline.draw(|gx, gy, cov| {
            let (px, py) = (b.min.x as i64 + gx as i64, b.min.y as i64 + gy as i64);
            if px < 0 || py < 0 || px >= out.width() as i64 || py >= out.height() as i64 {
                return;
            }
            let [r, g, bl, a] = color;
            let a = (a as f32 * cov.clamp(0.0, 1.0)).round() as u8;
            out.get_pixel_mut(px as u32, py as u32).blend(&Rgba([r, g, bl, a]));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use increart_core::Id;

    fn img(n: u32) -> ImageRef {
        ImageRef::output(format!("2026.01.01-00.00-{n:04}").parse::<Id>().unwrap())
    }

    fn cells(l: &TileLayout) -> Vec<(u32, u32, u32, u32)> {
        l.tiles.iter().map(|t| (t.col, t.row, t.w, t.h)).collect()
    }

    #[test]
    fn flows_squares_and_keeps_sheet_cells() {
        let l = TileLayout::square((0..5).map(img).collect());
        assert_eq!(l.cols, 3);
        assert_eq!(l.rows(), 2);
        assert_eq!(cells(&l)[4], (1, 1, 1, 1));
        // the sheet's (column, row): shifted to the corner, the hole kept
        let l = TileLayout::at(vec![(img(0), 2, 1), (img(1), 4, 1), (img(2), 3, 2)]);
        assert_eq!((l.cols, l.rows()), (3, 2));
        assert_eq!(cells(&l), [(0, 0, 1, 1), (2, 0, 1, 1), (1, 1, 1, 1)]);
        assert_eq!(l.tile_at(1, 0), None, "a hole");
    }

    #[test]
    fn moves_swap_spans_push_and_reflow_packs() {
        let mut l = TileLayout::flow((0..4).map(img).collect(), 2);
        // onto another tile: they swap
        l.move_tile(0, 1, 1);
        assert_eq!(cells(&l), [(1, 1, 1, 1), (1, 0, 1, 1), (0, 1, 1, 1), (0, 0, 1, 1)]);
        // into an empty cell below: a hole is left
        l.move_tile(1, 0, 3);
        assert_eq!(l.rows(), 4);
        assert_eq!(l.tile_at(1, 0), None);
        // a 2×2 span pushes the tiles in its way to free cells
        l.reflow();
        assert_eq!(cells(&l), [(0, 0, 1, 1), (1, 0, 1, 1), (0, 1, 1, 1), (1, 1, 1, 1)]);
        l.set_span(0, 2, 2);
        assert_eq!(cells(&l), [(0, 0, 2, 2), (0, 2, 1, 1), (1, 2, 1, 1), (0, 3, 1, 1)]);
        // wider than the grid: clamped; fewer columns: tiles re-placed
        l.set_span(0, 5, 1);
        assert_eq!(cells(&l)[0], (0, 0, 2, 1));
        l.set_cols(1);
        assert_eq!(cells(&l), [(0, 0, 1, 1), (0, 2, 1, 1), (0, 1, 1, 1), (0, 3, 1, 1)]);
    }

    #[test]
    fn composes_with_gaps_fit_fill_and_captions() {
        let red = RgbaImage::from_pixel(40, 20, Rgba([255, 0, 0, 255]));
        let blue = RgbaImage::from_pixel(20, 20, Rgba([0, 0, 255, 255]));
        let mut l = TileLayout::flow(vec![img(0), img(1)], 2);
        let mut s = TileSettings { gap: 4, background: [0, 0, 0, 0], ..TileSettings::default() };
        let both = [Some(red.clone()), Some(blue.clone())];
        let out = compose_images(&l, &both, &s).unwrap();
        // cells 40×20 (the largest): 4 + 40 + 4 + 40 + 4
        assert_eq!(out.dimensions(), (92, 28));
        assert_eq!(out.get_pixel(1, 1).0, [0, 0, 0, 0], "the gap");
        assert_eq!(out.get_pixel(10, 10).0, [255, 0, 0, 255]);
        // the square fits in the middle of its cell, letterboxed
        assert_eq!(out.get_pixel(50, 10).0, [0, 0, 0, 0]);
        assert_eq!(out.get_pixel(66, 10).0, [0, 0, 255, 255]);
        s.fill = true;
        let out = compose_images(&l, &both, &s).unwrap();
        assert_eq!(out.get_pixel(50, 10).0, [0, 0, 255, 255], "filled");
        // half size; a caption adds a strip below every row
        s.scale = 50;
        l.tiles[1].caption = "blue".into();
        s.caption_text = TextSize::Px(10);
        let out = compose_images(&l, &both, &s).unwrap();
        assert_eq!(out.dimensions(), (52, 10 + 15 + 8));
        let strip = (44..52).flat_map(|x| (14..29).map(move |y| (x, y)));
        assert!(strip.clone().any(|(x, y)| out.get_pixel(x, y).0[3] > 0), "caption drawn");
    }

    #[test]
    fn text_blocks_fill_empty_cells() {
        let red = RgbaImage::from_pixel(40, 40, Rgba([255, 0, 0, 255]));
        let mut l = TileLayout::flow(vec![img(0)], 2);
        // the empty cell beside the image
        let i = l.add_text(1, 0, "Two words".into());
        assert_eq!(cells(&l)[i], (1, 0, 1, 1));
        assert_eq!(l.images().count(), 1);
        // an occupied cell: the first free one instead
        let j = l.add_text(0, 0, "x".into());
        assert_eq!(cells(&l)[j], (0, 1, 1, 1));
        l.remove(j);
        l.tiles[i].content = TileContent::Text { text: "Two words".into(), size: TextSize::Px(12) };
        let s = TileSettings { gap: 0, background: [0, 0, 0, 0], caption_color: [255; 4], ..TileSettings::default() };
        let out = compose_images(&l, &[Some(red), None], &s).unwrap();
        assert_eq!(out.dimensions(), (80, 40));
        let ink = |x0: u32| (x0..x0 + 40).flat_map(|x| (0..40).map(move |y| (x, y)));
        assert!(ink(40).clone().any(|(x, y)| out.get_pixel(x, y).0[3] > 128), "text drawn");
        // "Two words" doesn't fit 40 px at 12 px: wrapped onto two lines
        let rows: Vec<u32> = (0..40).filter(|&y| (40..80).any(|x| out.get_pixel(x, y).0[3] > 0)).collect();
        assert!(rows.windows(2).any(|w| w[1] - w[0] > 1), "two lines: {rows:?}");
        // only text: cells of the default size
        let mut only = TileLayout { cols: 1, tiles: vec![l.tiles[1].clone()], ..TileLayout::default() };
        let g = Geometry::new(&only, &s, NO_IMAGES);
        assert_eq!(g.size, NO_IMAGES);
        // a title and subtitle: a band above the grid, then a gap
        let s = TileSettings { gap: 10, title_text: TextSize::Px(20), ..s };
        only.title = "Title".into();
        only.subtitle = "sub".into();
        let g = Geometry::new(&only, &s, (40, 40));
        assert_eq!((g.title, g.subtitle), (28, 14));
        assert_eq!(g.cell_rect(0, 0).1, 10 + 28 + 14 + 10);
        assert_eq!(g.size, (60, 10 + 42 + 10 + 40 + 10));
        let out = compose_images(&only, &[None], &s).unwrap();
        // (no images: cells of the default size; the title centred above)
        let w = out.width();
        assert!((10..38).any(|y| (10..w - 10).any(|x| out.get_pixel(x, y).0[3] > 0)), "title drawn");
    }

    #[test]
    fn auto_sizes_follow_the_cells_and_the_width() {
        let s = TileSettings::default();
        let three = TileLayout { title: "T".into(), ..TileLayout::flow((0..3).map(img).collect(), 3) };
        let g = Geometry::new(&three, &s, (1024, 1024));
        // captions 5 % of the cell; the title 4 % of 3 × 1024 + 4 × 16
        assert_eq!((g.caption_px, g.title_px), (51, 125));
        // one column: the title at least 10 % of the cell
        let one = TileLayout { cols: 1, ..three.clone() };
        assert_eq!(Geometry::new(&one, &s, (1024, 1024)).title_px, 102);
        // a quarter the scale, a quarter the text: the same look
        let g = Geometry::new(&three, &TileSettings { scale: 25, gap: 4, ..s.clone() }, (1024, 1024));
        assert_eq!((g.caption_px, g.title_px), (13, 31));
        // set ones: a share of the cell, or pixels
        let set = TileSettings { caption_text: TextSize::Cell(10.0), title_text: TextSize::Px(40), ..s.clone() };
        let g = Geometry::new(&three, &set, (1024, 1024));
        assert_eq!((g.caption_px, g.title_px), (102, 40));

        // a text block: as large as fits, up to the title's size
        let mut l = three.clone();
        let short = l.add_text(0, 1, "Hi".into());
        let note = "a much longer note that has to wrap over several lines to fit ".repeat(5);
        let long = l.add_text(1, 1, note.clone());
        let g = Geometry::new(&l, &s, (1024, 1024));
        assert_eq!(g.text_px(&l.tiles[short]), g.title_px, "capped");
        let px = g.text_px(&l.tiles[long]);
        assert!(px < g.title_px && px > 40, "{px}");
        let lines = text_lines(&note, px, 1024);
        assert!(lines.len() > 1 && lines.len() as u32 * px <= 1024, "{lines:?}");

        // a caption too wide shrinks first, then is cut short
        let (text, px) = caption_fit("seed 5", 1000, 50);
        assert_eq!((text.as_str(), px), ("seed 5", 50));
        let wide = caption_fit("Claymation · seed 5", 300, 50);
        assert!(wide.1 < 50 && wide.1 >= 30 && !wide.0.ends_with('…'), "shrunk: {wide:?}");
        let cut = caption_fit("Claymation · cfg 3 · shift 3.1 · strength 0.6 · seed 5", 300, 50);
        assert_eq!(cut.1, 30);
        assert!(cut.0.ends_with('…'), "{cut:?}");
    }

    #[test]
    fn refuses_huge() {
        let l = TileLayout::flow((0..2).map(img).collect(), 2);
        let s = TileSettings::default();
        let big = Geometry::new(&l, &s, (20_000, 20_000));
        assert!(big.size.0 as u64 * big.size.1 as u64 > MAX_PIXELS);
    }
}
