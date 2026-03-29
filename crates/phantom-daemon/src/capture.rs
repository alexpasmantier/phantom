use anyhow::Result;
use libghostty_vt::style::{RgbColor, Underline};
use phantom_core::types::{CellData, RowContent, ScreenContent, ScreenFormat};

use crate::session::Session;

pub fn capture_screen(session: &mut Session, format: &ScreenFormat) -> Result<ScreenContent> {
    let cursor = session.cursor_info();
    let title = session.terminal.title().ok().map(|s| s.to_string());

    let snapshot = session.render_state.update(&session.terminal)?;

    let mut rows = Vec::new();
    let mut row_it = session.row_iter.update(&snapshot)?;
    let mut row_idx: u16 = 0;

    while let Some(row) = row_it.next() {
        let mut text = String::new();
        let mut cells = Vec::new();
        let mut cell_it = session.cell_iter.update(row)?;

        while let Some(cell) = cell_it.next() {
            let graphemes = cell.graphemes()?;
            // Empty cells (no text) represent spaces
            let grapheme_str = if graphemes.is_empty() {
                " ".to_string()
            } else {
                graphemes.iter().collect()
            };
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
