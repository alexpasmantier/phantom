use std::time::Instant;

use crossbeam_channel::Sender;
use phantom_core::protocol::Response;
use phantom_core::types::WaitCondition;

use crate::session::Session;

pub struct PendingWait {
    pub session_name: String,
    pub conditions: Vec<WaitCondition>,
    pub deadline: Instant,
    pub poll_interval: std::time::Duration,
    pub last_check: Instant,
    pub reply: Sender<Response>,
}

impl PendingWait {
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    pub fn should_poll(&self) -> bool {
        self.last_check.elapsed() >= self.poll_interval
    }
}

/// Evaluate all wait conditions against the current session state.
/// Returns true if ALL conditions are satisfied.
pub fn evaluate_conditions(session: &mut Session, conditions: &[WaitCondition]) -> bool {
    conditions.iter().all(|cond| evaluate_one(session, cond))
}

fn evaluate_one(session: &mut Session, condition: &WaitCondition) -> bool {
    match condition {
        WaitCondition::TextPresent(text) => session.screen_text().contains(text.as_str()),
        WaitCondition::TextAbsent(text) => !session.screen_text().contains(text.as_str()),
        WaitCondition::Regex(re) => re.is_match(session.screen_text()),
        WaitCondition::ScreenStable { duration_ms } => {
            let hash = session.screen_hash();
            if hash != session.last_screen_hash {
                session.last_screen_hash = hash;
                session.screen_stable_since = Instant::now();
                false
            } else {
                session.screen_stable_since.elapsed()
                    >= std::time::Duration::from_millis(*duration_ms)
            }
        }
        WaitCondition::CursorAt { x, y } => {
            session.terminal.cursor_x().ok() == Some(*x)
                && session.terminal.cursor_y().ok() == Some(*y)
        }
        WaitCondition::CursorVisible(visible) => {
            session.terminal.is_cursor_visible().ok() == Some(*visible)
        }
        WaitCondition::ProcessExited { exit_code } => match session.check_exit() {
            Some(actual) => match exit_code {
                Some(expected) => actual == *expected,
                None => true,
            },
            None => false,
        },
        WaitCondition::ScreenChanged => {
            // Screen changed = hash differs from what it was when the wait was registered.
            // We use last_screen_hash which is set at registration time.
            let hash = session.screen_hash();
            hash != session.last_screen_hash
        }
    }
}
