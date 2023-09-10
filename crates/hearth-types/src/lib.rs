// Copyright (c) 2023 the Hearth contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// This file is part of Hearth.
//
// Hearth is free software: you can redistribute it and/or modify it under the
// terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Hearth is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more
// details.
//
// You should have received a copy of the GNU Affero General Public License
// along with Hearth. If not, see <https://www.gnu.org/licenses/>.

use std::fmt::{Display, Formatter, Result as FmtResult};

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// Filesystem native service protocol.
pub mod fs;

/// Paneling-related protocols and utilities.
pub mod panels;

/// Network/IPC protocol definitions.
pub mod protocol;

/// Registry protocol.
pub mod registry;

/// Terminal protocol.
pub mod terminal;

/// WebAssembly process protocols and utilities.
pub mod wasm;

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct ProcessId(pub u32);

/// Identifier for a lump (digest of BLAKE3 cryptographic hash).
#[repr(C)]
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Deserialize, Serialize, Pod, Zeroable)]
pub struct LumpId(pub [u8; 32]);

impl Display for LumpId {
    fn fmt(&self, fmt: &mut Formatter) -> FmtResult {
        for byte in self.0.iter() {
            write!(fmt, "{:02x}", byte)?;
        }

        Ok(())
    }
}

bitflags::bitflags! {
    /// The permission flags of a capability.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
    pub struct Flags: u32 {
        const SEND = 1 << 0;
        const KILL = 1 << 1;
        const LINK = 1 << 2;
        const REGISTER = 1 << 3;
        const TRUSTED = 1 << 4;
    }
}

/// The severity level for a log message emitted by a process.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub enum ProcessLogLevel {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
}

impl TryFrom<u32> for ProcessLogLevel {
    type Error = ();

    fn try_from(other: u32) -> Result<Self, ()> {
        use ProcessLogLevel::*;
        match other {
            0 => Ok(Trace),
            1 => Ok(Debug),
            2 => Ok(Info),
            3 => Ok(Warning),
            4 => Ok(Error),
            _ => Err(()),
        }
    }
}

impl From<ProcessLogLevel> for u32 {
    fn from(val: ProcessLogLevel) -> Self {
        use ProcessLogLevel::*;
        match val {
            Trace => 0,
            Debug => 1,
            Info => 2,
            Warning => 3,
            Error => 4,
        }
    }
}

/// A kind of guest-side signal.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub enum SignalKind {
    Message,
    Unlink,
}

impl TryFrom<u32> for SignalKind {
    type Error = ();

    fn try_from(other: u32) -> Result<Self, ()> {
        use SignalKind::*;
        match other {
            0 => Ok(Message),
            1 => Ok(Unlink),
            _ => Err(()),
        }
    }
}

impl From<SignalKind> for u32 {
    fn from(val: SignalKind) -> Self {
        use SignalKind::*;
        match val {
            Message => 0,
            Unlink => 1,
        }
    }
}

#[macro_export]
macro_rules! impl_serialize_json_display {
    ($ty: ident) => {
        impl ::std::fmt::Display for $ty {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                let string = ::serde_json::to_string(self).map_err(|_| ::std::fmt::Error)?;
                f.write_str(&string)
            }
        }
    };
}
