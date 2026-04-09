//! Rasterize a `ScreenContent` into a PNG so vision-capable models can see
//! the terminal screen as an image. Uses fontdue for glyph rasterization and
//! the `image` crate for PNG encoding.
//!
//! Font: JetBrains Mono Regular (SIL OFL 1.1) — vendored under `assets/`.

use std::io::Cursor;
use std::sync::OnceLock;

use anyhow::Result;
use fontdue::{Font, FontSettings};
use image::{ImageBuffer, ImageFormat, Rgba, RgbaImage};
use phantom_core::types::{CellData, ScreenContent};

const FONT_BYTES: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");
const FONT_SIZE: f32 = 16.0;
const PADDING: u32 = 8;

/// Default ANSI-ish dark theme.
const BG_DEFAULT: Rgba<u8> = Rgba([30, 30, 30, 255]);
const FG_DEFAULT: Rgba<u8> = Rgba([220, 220, 220, 255]);

/// Region (top, left, bottom, right), 0-indexed inclusive — same convention
/// as `phantom_core::protocol::Request::Screenshot::region`.
pub type Region = (u16, u16, u16, u16);

struct Metrics {
    cell_w: u32,
    cell_h: u32,
    /// Distance from the top of the cell to the glyph baseline.
    baseline: f32,
}

fn font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        Font::from_bytes(FONT_BYTES, FontSettings::default()).expect("vendored font is invalid")
    })
}

fn metrics() -> &'static Metrics {
    static M: OnceLock<Metrics> = OnceLock::new();
    M.get_or_init(|| {
        let f = font();
        // 'M' is a representative wide glyph for monospace advance width.
        let m = f.metrics('M', FONT_SIZE);
        let lm = f
            .horizontal_line_metrics(FONT_SIZE)
            .expect("font has horizontal metrics");
        let line_h = lm.new_line_size.ceil().max(1.0) as u32;
        Metrics {
            cell_w: m.advance_width.round().max(1.0) as u32,
            cell_h: line_h,
            baseline: lm.ascent,
        }
    })
}

/// Render a `ScreenContent` to a PNG. If `region` is supplied, the image is
/// sized to the region (not the full screen) and rows are positioned relative
/// to the region's top edge.
pub fn render_png(screen: &ScreenContent, region: Option<Region>) -> Result<Vec<u8>> {
    let m = metrics();

    // Compute output dimensions and the row offset to subtract.
    // For region screenshots, capture.rs preserves the absolute row index in
    // RowContent.row but only includes in-region columns in cells/text. So we
    // subtract `top` from each row's index but use 0-based indexing for cells.
    let (out_cols, out_rows, top_offset) = match region {
        Some((top, left, bottom, right)) => (
            (right.saturating_sub(left) + 1) as u32,
            (bottom.saturating_sub(top) + 1) as u32,
            top as u32,
        ),
        None => (screen.cols as u32, screen.rows as u32, 0),
    };

    let width = out_cols * m.cell_w + 2 * PADDING;
    let height = out_rows * m.cell_h + 2 * PADDING;

    let mut img: RgbaImage = ImageBuffer::from_pixel(width, height, BG_DEFAULT);
    let f = font();

    for row in &screen.screen {
        let local_row = (row.row as u32).saturating_sub(top_offset);
        let row_y = PADDING + local_row * m.cell_h;

        // Prefer the styled `cells` (populated by JSON screenshots). Fall back to
        // plain `text` for sessions captured in text mode.
        if !row.cells.is_empty() {
            for (col_idx, cell) in row.cells.iter().enumerate() {
                let col_x = PADDING + (col_idx as u32) * m.cell_w;
                draw_cell(&mut img, f, m, col_x, row_y, cell);
            }
        } else {
            for (col_idx, ch) in row.text.chars().enumerate() {
                let col_x = PADDING + (col_idx as u32) * m.cell_w;
                fill_rect(&mut img, col_x, row_y, m.cell_w, m.cell_h, BG_DEFAULT);
                draw_glyph(&mut img, f, col_x, row_y, ch, FG_DEFAULT, m);
            }
        }
    }

    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)?;
    Ok(buf)
}

fn draw_cell(img: &mut RgbaImage, f: &Font, m: &Metrics, x: u32, y: u32, cell: &CellData) {
    let raw_fg = parse_color(cell.fg.as_deref()).unwrap_or(FG_DEFAULT);
    let raw_bg = parse_color(cell.bg.as_deref()).unwrap_or(BG_DEFAULT);
    let (fg, bg) = if cell.inverse {
        (raw_bg, raw_fg)
    } else {
        (raw_fg, raw_bg)
    };
    let fg = if cell.faint {
        blend_color(fg, BG_DEFAULT, 0.5)
    } else {
        fg
    };

    fill_rect(img, x, y, m.cell_w, m.cell_h, bg);

    let ch = cell.grapheme.chars().next().unwrap_or(' ');
    if ch != ' ' && ch != '\0' {
        draw_glyph(img, f, x, y, ch, fg, m);
    }
    if cell.underline {
        draw_underline(img, x, y, m, fg);
    }
    if cell.strikethrough {
        draw_strikethrough(img, x, y, m, fg);
    }
}

fn draw_glyph(img: &mut RgbaImage, f: &Font, x: u32, y: u32, ch: char, color: Rgba<u8>, m: &Metrics) {
    let (gm, bitmap) = f.rasterize(ch, FONT_SIZE);
    if gm.width == 0 || gm.height == 0 {
        return;
    }
    // Glyph origin is at the baseline. Top of bitmap = baseline - height - ymin.
    let baseline_y = y as i32 + m.baseline.round() as i32;
    let glyph_x = x as i32 + gm.xmin;
    let glyph_y = baseline_y - gm.height as i32 - gm.ymin;

    for row in 0..gm.height {
        for col in 0..gm.width {
            let alpha = bitmap[row * gm.width + col];
            if alpha == 0 {
                continue;
            }
            let px = glyph_x + col as i32;
            let py = glyph_y + row as i32;
            if px < 0 || py < 0 {
                continue;
            }
            let (px, py) = (px as u32, py as u32);
            if px >= img.width() || py >= img.height() {
                continue;
            }
            let bg = *img.get_pixel(px, py);
            let a = alpha as f32 / 255.0;
            img.put_pixel(px, py, blend_color(color, bg, a));
        }
    }
}

fn draw_underline(img: &mut RgbaImage, x: u32, y: u32, m: &Metrics, color: Rgba<u8>) {
    let line_y = y + m.cell_h.saturating_sub(2);
    fill_rect(img, x, line_y, m.cell_w, 1, color);
}

fn draw_strikethrough(img: &mut RgbaImage, x: u32, y: u32, m: &Metrics, color: Rgba<u8>) {
    let line_y = y + m.cell_h / 2;
    fill_rect(img, x, line_y, m.cell_w, 1, color);
}

fn fill_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    let xmax = (x + w).min(img.width());
    let ymax = (y + h).min(img.height());
    for py in y..ymax {
        for px in x..xmax {
            img.put_pixel(px, py, color);
        }
    }
}

fn blend_color(fg: Rgba<u8>, bg: Rgba<u8>, alpha: f32) -> Rgba<u8> {
    let a = alpha.clamp(0.0, 1.0);
    Rgba([
        (fg[0] as f32 * a + bg[0] as f32 * (1.0 - a)) as u8,
        (fg[1] as f32 * a + bg[1] as f32 * (1.0 - a)) as u8,
        (fg[2] as f32 * a + bg[2] as f32 * (1.0 - a)) as u8,
        255,
    ])
}

/// Parse a color string from `phantom_core::types::CellData`.
/// Recognized formats: `#rrggbb` (hex RGB) and `palette:N` (xterm 256-color).
fn parse_color(s: Option<&str>) -> Option<Rgba<u8>> {
    let s = s?;
    if let Some(hex) = s.strip_prefix('#')
        && hex.len() == 6
    {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        return Some(Rgba([r, g, b, 255]));
    }
    if let Some(idx_str) = s.strip_prefix("palette:") {
        let idx: u8 = idx_str.parse().ok()?;
        return Some(palette_to_rgb(idx));
    }
    None
}

/// xterm-256 color palette:
/// - 0..16: standard ANSI + bright variants
/// - 16..232: 6×6×6 RGB cube
/// - 232..256: 24-step grayscale
fn palette_to_rgb(idx: u8) -> Rgba<u8> {
    // Standard 16-color palette (ANSI + bright). Values match the typical
    // xterm/VGA defaults.
    const ANSI_16: [(u8, u8, u8); 16] = [
        (0, 0, 0),       // 0  black
        (170, 0, 0),     // 1  red
        (0, 170, 0),     // 2  green
        (170, 85, 0),    // 3  yellow (brown)
        (0, 0, 170),     // 4  blue
        (170, 0, 170),   // 5  magenta
        (0, 170, 170),   // 6  cyan
        (170, 170, 170), // 7  white (light gray)
        (85, 85, 85),    // 8  bright black (dark gray)
        (255, 85, 85),   // 9  bright red
        (85, 255, 85),   // 10 bright green
        (255, 255, 85),  // 11 bright yellow
        (85, 85, 255),   // 12 bright blue
        (255, 85, 255),  // 13 bright magenta
        (85, 255, 255),  // 14 bright cyan
        (255, 255, 255), // 15 bright white
    ];

    if idx < 16 {
        let (r, g, b) = ANSI_16[idx as usize];
        return Rgba([r, g, b, 255]);
    }
    if idx < 232 {
        // 6×6×6 RGB cube. Each component step is one of: 0, 95, 135, 175, 215, 255.
        const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];
        let n = idx - 16;
        let r = CUBE[(n / 36) as usize];
        let g = CUBE[((n / 6) % 6) as usize];
        let b = CUBE[(n % 6) as usize];
        return Rgba([r, g, b, 255]);
    }
    // Grayscale 232..255: 8 + 10*i for i in 0..24
    let v = 8 + (idx - 232) * 10;
    Rgba([v, v, v, 255])
}

#[cfg(test)]
mod tests {
    use super::*;
    use phantom_core::types::{CursorInfo, CursorStyle, RowContent};

    fn make_screen(cols: u16, rows: u16, lines: &[&str]) -> ScreenContent {
        ScreenContent {
            cols,
            rows,
            cursor: CursorInfo {
                x: 0,
                y: 0,
                visible: true,
                style: CursorStyle::Block,
            },
            title: None,
            screen: lines
                .iter()
                .enumerate()
                .map(|(i, &line)| RowContent {
                    row: i as u16,
                    text: line.into(),
                    cells: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn parse_hex_color() {
        assert_eq!(parse_color(Some("#ff0000")), Some(Rgba([255, 0, 0, 255])));
        assert_eq!(parse_color(Some("#00ff00")), Some(Rgba([0, 255, 0, 255])));
        assert_eq!(parse_color(Some("#1e2c3d")), Some(Rgba([0x1e, 0x2c, 0x3d, 255])));
    }

    #[test]
    fn parse_palette_color_ansi_16() {
        // 0 = black, 1 = red, 15 = bright white
        assert_eq!(parse_color(Some("palette:0")), Some(Rgba([0, 0, 0, 255])));
        assert_eq!(parse_color(Some("palette:1")), Some(Rgba([170, 0, 0, 255])));
        assert_eq!(
            parse_color(Some("palette:15")),
            Some(Rgba([255, 255, 255, 255]))
        );
    }

    #[test]
    fn parse_palette_color_cube() {
        // 16 is the cube origin (0,0,0); 231 is its corner (255,255,255).
        assert_eq!(parse_color(Some("palette:16")), Some(Rgba([0, 0, 0, 255])));
        assert_eq!(
            parse_color(Some("palette:231")),
            Some(Rgba([255, 255, 255, 255]))
        );
    }

    #[test]
    fn parse_palette_color_grayscale() {
        // 232..256 is grayscale: 8, 18, 28, ..., 238
        assert_eq!(parse_color(Some("palette:232")), Some(Rgba([8, 8, 8, 255])));
        assert_eq!(
            parse_color(Some("palette:255")),
            Some(Rgba([238, 238, 238, 255]))
        );
    }

    #[test]
    fn parse_invalid_color() {
        assert_eq!(parse_color(None), None);
        assert_eq!(parse_color(Some("not-a-color")), None);
        assert_eq!(parse_color(Some("#fff")), None); // 3-digit hex unsupported
        assert_eq!(parse_color(Some("palette:foo")), None);
    }

    #[test]
    fn render_png_full_screen_produces_valid_png() {
        let screen = make_screen(5, 2, &["hello", "world"]);
        let png = render_png(&screen, None).expect("render");
        // PNG magic: 89 50 4E 47 0D 0A 1A 0A
        assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
        // Decode and confirm dimensions match expectations.
        let img = image::load_from_memory(&png).expect("decode");
        let m = metrics();
        assert_eq!(img.width(), 5 * m.cell_w + 2 * PADDING);
        assert_eq!(img.height(), 2 * m.cell_h + 2 * PADDING);
    }

    #[test]
    fn render_png_region_produces_correctly_sized_png() {
        // 80×24 screen, capture rows 5..=10, cols 10..=20 → 11 cols × 6 rows
        let mut screen = make_screen(80, 24, &[]);
        // Build rows that mimic what capture.rs would produce for a region:
        // row indices are absolute, text only contains in-region characters.
        for absolute_row in 5..=10u16 {
            screen.screen.push(RowContent {
                row: absolute_row,
                text: "abcdefghijk".into(),
                cells: vec![],
            });
        }
        let png = render_png(&screen, Some((5, 10, 10, 20))).expect("render");
        let img = image::load_from_memory(&png).expect("decode");
        let m = metrics();
        assert_eq!(img.width(), 11 * m.cell_w + 2 * PADDING);
        assert_eq!(img.height(), 6 * m.cell_h + 2 * PADDING);
    }
}
