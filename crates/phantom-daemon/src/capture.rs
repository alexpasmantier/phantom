use anyhow::Result;
use libghostty_vt::style::{RgbColor, Underline};
use phantom_core::types::{CellData, RowContent, ScreenContent, ScreenFormat};

use crate::session::Session;

pub fn capture_screen(
    session: &mut Session,
    format: &ScreenFormat,
    region: Option<(u16, u16, u16, u16)>,
) -> Result<ScreenContent> {
    let cursor = session.cursor_info();
    let title = session.terminal.title().ok().map(|s| s.to_string());

    let snapshot = session.render_state.update(&session.terminal)?;

    let mut rows = Vec::new();
    let mut row_it = session.row_iter.update(&snapshot)?;
    let mut row_idx: u16 = 0;
    let mut col_idx: u16;

    while let Some(row) = row_it.next() {
        // Region filter: skip rows outside the region
        if let Some((top, _, bottom, _)) = region {
            if row_idx < top || row_idx > bottom {
                row_idx += 1;
                continue;
            }
        }

        let mut text = String::new();
        let mut cells = Vec::new();
        let mut cell_it = session.cell_iter.update(row)?;
        col_idx = 0;

        while let Some(cell) = cell_it.next() {
            let in_region = match region {
                Some((_, left, _, right)) => col_idx >= left && col_idx <= right,
                None => true,
            };

            let graphemes = cell.graphemes()?;
            let grapheme_str = if graphemes.is_empty() {
                " ".to_string()
            } else {
                graphemes.iter().collect()
            };

            if in_region {
                text.push_str(&grapheme_str);

                if matches!(format, ScreenFormat::Json) {
                    let style = cell.style()?;
                    let fg = cell.fg_color()?.map(|c| rgb_to_hex(&c));
                    let bg = cell.bg_color()?.map(|c| rgb_to_hex(&c));

                    cells.push(CellData {
                        grapheme: grapheme_str,
                        fg,
                        bg,
                        bold: style.bold,
                        italic: style.italic,
                        underline: !matches!(style.underline, Underline::None),
                        strikethrough: style.strikethrough,
                        inverse: style.inverse,
                        faint: style.faint,
                    });
                }
            }

            col_idx += 1;
        }

        rows.push(RowContent {
            row: row_idx,
            text,
            cells,
        });
        row_idx += 1;
    }

    Ok(ScreenContent {
        cols: session.cols,
        rows: session.rows,
        cursor,
        title,
        screen: rows,
    })
}

fn rgb_to_hex(color: &RgbColor) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}
