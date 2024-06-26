use std::collections::HashMap;

use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::Color;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum FactoryError {
    /// The request has failed to parse.
    ParseError,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalState {
    pub position: Vec3,
    pub orientation: Quat,
    pub half_size: Vec2,
    pub opacity: f32,
    pub padding: Vec2,
    pub units_per_em: f32,
    pub colors: HashMap<usize, Color>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum TerminalUpdate {
    Quit,
    Input(String),
    State(TerminalState),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum FactoryRequest {
    CreateTerminal(TerminalState),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum FactorySuccess {
    /// The first returned capability is to the new terminal, which receives [TerminalUpdates][TerminalUpdate].
    Terminal,
}

pub type FactoryResponse = Result<FactorySuccess, FactoryError>;
