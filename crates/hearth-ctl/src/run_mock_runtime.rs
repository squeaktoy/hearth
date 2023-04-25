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

use clap::{Parser, Result};
use hearth_rpc::{
    mocks::*,
    remoc::rtc::{LocalRwLock, ServerSharedMut},
    *,
};
use hearth_types::PeerId;
use std::sync::Arc;
use yacexits::EX_PROTOCOL;

use crate::{CommandError, ToCommandError};

/// Runs a mock daemon on a dedicated IPC socket. Only useful for testing purposes.
#[derive(Debug, Parser)]
pub struct RunMockRuntime {}

impl RunMockRuntime {
    pub async fn run(self) -> Result<(), CommandError> {
        let daemon_listener = hearth_ipc::Listener::new()
            .await
            .to_command_error("creating ipc listener", EX_PROTOCOL)?;

        let (peer_provider_server, peer_provider) =
            PeerProviderServerSharedMut::<_, remoc::codec::Default>::new(
                Arc::new(LocalRwLock::new(MockPeerProvider::new())),
                1024,
            );

        tokio::spawn(async move {
            peer_provider_server.serve(true).await;
        });

        let (process_factory_server, process_factory) =
            ProcessFactoryServerSharedMut::<_, remoc::codec::Default>::new(
                Arc::new(LocalRwLock::new(MockProcessFactory {})),
                1024,
            );

        tokio::spawn(async move {
            process_factory_server.serve(true).await;
        });

        let daemon_offer = DaemonOffer {
            peer_provider,
            peer_id: PeerId(0),
            process_factory,
        };

        hearth_ipc::listen(daemon_listener, daemon_offer);

        eprintln!("Waiting for interrupt signal");
        tokio::signal::ctrl_c()
            .await
            .to_command_error("interrupt await error", EX_PROTOCOL)?;
        eprintln!("interrupt signal received");
        Ok(())
    }
}
