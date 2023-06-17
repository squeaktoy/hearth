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

use hearth_types::Flags;
use serde::{Deserialize, Serialize};

/// A reason for the revocation or unlinking of a process.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum UnlinkReason {
    /// The process is no longer alive.
    Dead,

    /// The process is no longer accessible.
    Inaccessible,

    /// Access to the process has been revoked.
    AccessRevoked,
}

/// Types of messages relating to low-level capability operations between two peers.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum CapOperation {
    Local(LocalCapOperation),
    Remote(RemoteCapOperation),
}

/// Operations on local capabilities.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum LocalCapOperation {
    /// Declares a capability and its identifier.
    DeclareCap { id: u32, flags: Flags },

    /// Revokes a capability.
    ///
    /// All operations on this capability become invalid when this operation
    /// is sent, but the capability ID will not be reused until
    /// [RemoteCapOperation::AcknowledgeRevocation] is received.
    RevokeCap { id: u32, reason: UnlinkReason },

    /// Responds to [RemoteCapOperation::ListServicesResponse].
    ListServicesResponse {
        /// The identifier of the request being responded to.
        req_id: u32,

        /// The list of all services.
        ///
        /// This may change between messages, so do not assume that the list
        /// is valid even at the time of receiving.
        services: Vec<String>,
    },

    /// Responds to [RemoteCapOperation::GetServiceRequest].
    GetServiceResponse {
        /// The identifier of the request being responded to.
        req_id: u32,

        /// The service's capability, if available.
        service_cap: Option<u32>,
    },
}

/// Operations on remote capabilities.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum RemoteCapOperation {
    /// Acknowledges that a capability has been revoked, freeing the ID for
    /// reuse.
    AcknowledgeRevocation { id: u32 },

    /// Communicates that a capability is no longer being used.
    ///
    /// Local cap operations may still may still be received using this
    /// capability and the sender of this operation must assume that the ID of
    /// this cap will stay in use until it is revoked.
    FreeCap { id: u32 },

    /// Requests a list of all services.
    ListServicesRequest {
        /// The identifier to use for the response.
        req_id: u32,
    },

    /// Requests a capability for a service.
    GetServiceRequest {
        /// The identifier to use for the response.
        req_id: u32,

        /// The service being requested.
        name: String,
    },

    /// Sends a message to a remote capability.
    ///
    /// Ignored if the capability does not have [Flags::SEND] set.
    Send {
        /// The remote capability to send a message to.
        ///
        /// Ignored if invalid or revoked.
        id: u32,

        /// The contents of the message.
        data: Vec<u8>,

        /// The local capabilities transferred in this message.
        caps: Vec<u32>,
    },

    /// Kills a remote capability.
    ///
    /// Ignored if the capability does not have [Flags::KILL] set.
    Kill {
        /// The remote capability to kill.
        ///
        /// Ignored if invalid or revoked.
        id: u32,
    },
}
