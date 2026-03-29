use serde::{Deserialize, Serialize};

use crate::types::{
    CursorInfo, InputAction, ScreenContent, ScreenFormat, SessionInfo, WaitCondition,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    CreateSession {
        name: String,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        scrollback: u32,
    },
    SendInput {
        session: String,
        action: InputAction,
    },
    Screenshot {
        session: String,
        format: ScreenFormat,
    },
    Wait {
        session: String,
        conditions: Vec<WaitCondition>,
        timeout_ms: u64,
        poll_ms: u64,
    },
    GetCursor {
        session: String,
    },
    GetScrollback {
        session: String,
        lines: Option<u32>,
        format: ScreenFormat,
    },
    Resize {
        session: String,
        cols: u16,
        rows: u16,
    },
    GetStatus {
        session: String,
    },
    ListSessions,
    KillSession {
        session: String,
        signal: Option<i32>,
    },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Ok {
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<ResponseData>,
    },
    Error {
        code: i32,
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseData {
    Session(SessionInfo),
    Sessions(Vec<SessionInfo>),
    Screen(ScreenContent),
    Cursor(CursorInfo),
    Text(String),
}

impl Response {
    pub fn ok() -> Self {
        Self::Ok { data: None }
    }

    pub fn ok_with(data: ResponseData) -> Self {
        Self::Ok { data: Some(data) }
    }

    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self::Error {
            code,
            message: message.into(),
        }
    }

    pub fn session_not_found(name: &str) -> Self {
        Self::error(
            crate::exit_codes::SESSION_NOT_FOUND,
            format!("No session named '{name}'"),
        )
    }
}
