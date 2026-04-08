use phantom_core::protocol::Response;

#[derive(Debug)]
pub enum PhantomError {
    /// Engine failed to start.
    EngineStartFailed(String),
    /// Engine did not respond within timeout.
    EngineTimeout,
    /// Wait condition timed out.
    WaitTimeout,
    /// Session not found.
    SessionNotFound(String),
    /// Session name already exists.
    SessionCollision(String),
    /// Process has exited.
    ProcessExited,
    /// Engine returned an error.
    Engine { code: i32, message: String },
    /// Internal error.
    Internal(anyhow::Error),
}

impl std::fmt::Display for PhantomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EngineStartFailed(msg) => write!(f, "engine failed to start: {msg}"),
            Self::EngineTimeout => write!(f, "engine did not respond"),
            Self::WaitTimeout => write!(f, "wait condition timed out"),
            Self::SessionNotFound(name) => write!(f, "session '{name}' not found"),
            Self::SessionCollision(name) => write!(f, "session '{name}' already exists"),
            Self::ProcessExited => write!(f, "process has exited"),
            Self::Engine { code, message } => write!(f, "engine error ({code}): {message}"),
            Self::Internal(e) => write!(f, "internal error: {e}"),
        }
    }
}

impl std::error::Error for PhantomError {}

impl From<anyhow::Error> for PhantomError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e)
    }
}

pub type Result<T> = std::result::Result<T, PhantomError>;

pub(crate) fn response_to_result(
    resp: Response,
) -> Result<Option<phantom_core::protocol::ResponseData>> {
    match resp {
        Response::Ok { data } => Ok(data),
        Response::Error { code, message } => Err(match code {
            phantom_core::exit_codes::SESSION_NOT_FOUND => PhantomError::SessionNotFound(message),
            phantom_core::exit_codes::SESSION_COLLISION => PhantomError::SessionCollision(message),
            phantom_core::exit_codes::WAIT_TIMEOUT => PhantomError::WaitTimeout,
            phantom_core::exit_codes::PROCESS_EXITED => PhantomError::ProcessExited,
            _ => PhantomError::Engine { code, message },
        }),
    }
}
