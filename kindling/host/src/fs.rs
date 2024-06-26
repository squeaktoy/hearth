use super::*;
use core::panic;

use hearth_guest::{fs::*, Lump, LumpId};

lazy_static::lazy_static! {
    static ref FILESYSTEM: RequestResponse<Request, Response> =
        RequestResponse::expect_service("hearth.fs.Filesystem");
}

/// Get a LumpId of a file from a path.
pub fn get_file(path: &str) -> Result<LumpId, Error> {
    let success = FILESYSTEM
        .request(
            Request {
                target: path.to_string(),
                kind: RequestKind::Get,
            },
            &[],
        )
        .0?;
    match success {
        Success::Get(lump) => Ok(lump),
        _ => panic!("expected Success::Get, got {:?}", success),
    }
}

/// Read the bytes of a file into a `Vec<u8>`.
pub fn read_file(path: &str) -> Result<Vec<u8>, Error> {
    let lump = get_file(path)?;
    let lump = Lump::load_by_id(&lump);
    Ok(lump.get_data())
}

/// List all files and directories inside of a path.
pub fn list_files(path: &str) -> Result<Vec<FileInfo>, Error> {
    let success = FILESYSTEM
        .request(
            Request {
                target: path.to_string(),
                kind: RequestKind::List,
            },
            &[],
        )
        .0?;
    match success {
        Success::List(files) => Ok(files),
        _ => panic!("expected Success::List, got {:?}", success),
    }
}
