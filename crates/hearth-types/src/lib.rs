use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct ProcessId(pub u64);

impl ProcessId {
    pub fn split(self) -> (PeerId, LocalProcessId) {
        let peer = (self.0 >> 32) as u32;
        let pid = self.0 as u32;
        (PeerId(peer), LocalProcessId(pid))
    }

    pub fn from_peer_process(peer: PeerId, pid: LocalProcessId) -> Self {
        Self(((peer.0 as u64) << 32) | (pid.0 as u64))
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct PeerId(pub u32);

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct LocalProcessId(pub u32);

/// Process-local identifiers for loaded assets.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct AssetId(pub u32);

/// Identifier for a lump (digest of BLAKE3 cryptographic hash).
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct LumpId(pub [u8; 32]);

impl Display for LumpId {
    fn fmt(&self, fmt: &mut Formatter) -> FmtResult {
        for byte in self.0.iter() {
            write!(fmt, "{:02x}", byte)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_conversion() {
        let tests = &[(0, 0), (420, 69), (100000, 100000)];
        for (peer, pid) in tests.iter() {
            let peer = PeerId(*peer);
            let pid = LocalProcessId(*pid);
            assert_eq!((peer, pid), ProcessId::from_peer_process(peer, pid).split());
        }
    }
}
