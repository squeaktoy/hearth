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

use std::collections::HashSet;
use std::fmt::Display;

use hearth_rpc::hearth_types::ProcessId;
use hearth_rpc::remoc::rtc::async_trait;
use hearth_rpc::ProcessInfo;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::error;

use crate::process::{Message, Process, ProcessContext};

/// A process that publishes events of type T to all subscribed processes.
///
/// To subscribe to this publisher, send a message containing `subscribe`. To
/// unsubscribe, send `unsubscribe`. Messages with new events of type T will be
/// formatted and sent to a subscriber until it unsubscribes.
pub struct PublisherProcess<T> {
    receiver: Receiver<T>,
    subscribers: HashSet<ProcessId>,
}

#[async_trait]
impl<T: Display + Send + Sync + 'static> Process for PublisherProcess<T> {
    fn get_info(&self) -> ProcessInfo {
        ProcessInfo {}
    }

    async fn run(&mut self, mut ctx: ProcessContext) {
        loop {
            tokio::select! {
                message = ctx.recv() => {
                    if let Some(message) = message {
                        self.on_message(message);
                    } else {
                        break; // process is dead
                    }
                },
                event = self.receiver.recv() => {
                    if let Some(event) = event {
                        self.on_event(&mut ctx, event).await;
                    } else {
                        break; // all senders are dropped; die
                    }
                }
            }
        }
    }
}

impl<T: Display + Send + Sync + 'static> PublisherProcess<T> {
    /// Creates a new [PublisherProcess] and a [Sender] to send events to it.
    ///
    /// When all senders are dropped, the process dies.
    pub fn new() -> (Sender<T>, Self) {
        let (sender, receiver) = channel(128);
        let publisher = Self {
            receiver,
            subscribers: HashSet::new(),
        };

        (sender, publisher)
    }

    /// Internal message handling.
    fn on_message(&mut self, message: Message) {
        match message.data.as_slice() {
            b"subscribe" => {
                self.subscribers.insert(message.sender);
            }
            b"unsubscribe" => {
                self.subscribers.remove(&message.sender);
            }
            _ => error!(
                "Expected 'subscribe' or 'unsubscribe' from PID {}; received {:?}",
                message.sender, message.data
            ),
        }
    }

    /// Internal event handling.
    async fn on_event(&mut self, ctx: &mut ProcessContext, event: T) {
        let event_data = event.to_string().into_bytes();
        for subscriber in self.subscribers.iter() {
            if let Err(err) = ctx.send_message(*subscriber, event_data.clone()).await {
                error!("Event notification sending error: {:?}", err);
            }
        }
    }
}
