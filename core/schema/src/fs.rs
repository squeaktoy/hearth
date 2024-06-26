use serde::{Deserialize, Serialize};

use crate::LumpId;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Error {
    NotFound,
    PermissionDenied,
    IsADirectory,
    NotADirectory,
    DirectoryTraversal,
    InvalidTarget,
    InvalidRequest,
    Other(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum RequestKind {
    Get,
    List,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Request {
    pub target: String,
    pub kind: RequestKind,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FileInfo {
    pub name: String,
    // TODO more file properties like size or last modified?
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Success {
    Get(LumpId),
    List(Vec<FileInfo>),
}

pub type Response = Result<Success, Error>;
