use std::io::IsTerminal;

#[derive(Debug, Clone, Copy)]
pub enum OutputMode {
    Human,
    Json,
}

impl OutputMode {
    pub fn detect(force_json: bool, force_human: bool) -> Self {
        if force_human || (!force_json && std::io::stdout().is_terminal()) {
            Self::Human
        } else {
            Self::Json
        }
    }

    pub fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}
