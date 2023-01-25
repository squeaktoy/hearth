use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ProcessId(pub u64);

impl ProcessId {
    pub fn split(self) -> (PeerId, LocalProcessId) {
        let peer = self.0 as u32 >> 4;
        let pid = (self.0 & 0xffff) as u32;
        (PeerId(peer), LocalProcessId(pid))
    }

    pub fn from_peer_process(peer: PeerId, pid: LocalProcessId) -> Self {
        Self(((peer.0 as u64) << 4) | (pid.0 as u64))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PeerId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct LocalProcessId(pub u32);

/// Process-local identifiers for loaded assets.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct AssetId(pub u32);

/// Identifier for a lump (digest of BLAKE3 cryptographic hash).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct LumpId(pub [u8; 32]);
