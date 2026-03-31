use phantom_core::types::ScreenContent;

/// A captured terminal screen with convenience methods.
pub struct Screen {
    inner: ScreenContent,
    text: String,
}

impl Screen {
    pub(crate) fn new(inner: ScreenContent) -> Self {
        let text = inner
            .screen
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Self { inner, text }
    }

    /// Full screen content as a single string (rows joined by newlines).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Check if the screen contains the given text.
    pub fn contains(&self, needle: &str) -> bool {
        self.text.contains(needle)
    }

    /// Get the text of a specific row (0-indexed).
    pub fn row_text(&self, row: u16) -> Option<&str> {
        self.inner
            .screen
            .iter()
            .find(|r| r.row == row)
            .map(|r| r.text.as_str())
    }

    /// The cursor position and style at capture time.
    pub fn cursor(&self) -> &phantom_core::types::CursorInfo {
        &self.inner.cursor
    }

    /// Terminal dimensions.
    pub fn size(&self) -> (u16, u16) {
        (self.inner.cols, self.inner.rows)
    }

    /// Window title, if set by the application.
    pub fn title(&self) -> Option<&str> {
        self.inner.title.as_deref()
    }

    /// Access the underlying `ScreenContent` for full cell-level data.
    pub fn raw(&self) -> &ScreenContent {
        &self.inner
    }
}

impl std::fmt::Display for Screen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl std::fmt::Debug for Screen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Screen({} x {}):\n{}", self.inner.cols, self.inner.rows, self.text)
    }
}
