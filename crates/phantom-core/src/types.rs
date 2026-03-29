use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub name: String,
    pub pid: u32,
    pub cols: u16,
    pub rows: u16,
    pub title: Option<String>,
    pub pwd: Option<String>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Exited { code: Option<i32> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorInfo {
    pub x: u16,
    pub y: u16,
    pub visible: bool,
    pub style: CursorStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenContent {
    pub cols: u16,
    pub rows: u16,
    pub cursor: CursorInfo,
    pub title: Option<String>,
    pub screen: Vec<RowContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowContent {
    pub row: u16,
    pub text: String,
    pub cells: Vec<CellData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellData {
    pub grapheme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strikethrough: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inverse: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub faint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitCondition {
    TextPresent(String),
    TextAbsent(String),
    #[serde(with = "serde_regex")]
    Regex(regex::Regex),
    ScreenStable {
        duration_ms: u64,
    },
    CursorAt {
        x: u16,
        y: u16,
    },
    CursorVisible(bool),
    ProcessExited {
        exit_code: Option<i32>,
    },
    ScreenChanged,
}

mod serde_regex {
    use regex::Regex;
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(re: &Regex, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(re.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Regex, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Regex::new(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenFormat {
    Text,
    Json,
    Html,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputAction {
    Type { text: String, delay_ms: Option<u64> },
    Key { keys: Vec<String> },
    Paste { text: String },
    Mouse { spec: String },
}
