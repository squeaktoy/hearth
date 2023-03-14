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

use std::sync::Arc;

use hearth_core::process::{Process, ProcessContext};
use hearth_core::pubsub::PublisherProcess;
use hearth_core::runtime::{Plugin, Runtime, RuntimeBuilder};
use hearth_core::{tokio, tracing};
use hearth_rpc::hearth_types::panels::*;
use hearth_rpc::remoc::rtc::async_trait;
use hearth_rpc::ProcessInfo;
use slab::Slab;
use tokio::sync::{mpsc, RwLock};

/// Plugin to add paneling support to a Hearth runtime.
///
/// Adds the [PanelControlService] and [AmbientPanelService] services.
pub struct PanelsPlugin {
    store: Arc<RwLock<PanelStore>>,
}

#[async_trait]
impl Plugin for PanelsPlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {
        builder.add_service(
            "hearth.panels.PanelControlService".into(),
            PanelControlService::new(self.store.to_owned()),
        );

        /*builder.add_service(
            "hearth.panels.AmbientPanelService".into(),
            AmbientPanelService::new(),
        );*/
    }

    async fn run(&mut self, runtime: Arc<Runtime>) {}
}

impl PanelsPlugin {
    pub fn new() -> Self {
        let store = PanelStore::new();
        let store = RwLock::new(store);
        let store = Arc::new(store);
        Self { store }
    }
}

/// `hearth.panels.PanelControlService`: Receives [PanelCommands][PanelCommand]
/// and executes them.
pub struct PanelControlService {
    store: Arc<RwLock<PanelStore>>,
}

#[async_trait]
impl Process for PanelControlService {
    fn get_info(&self) -> ProcessInfo {
        ProcessInfo {}
    }

    async fn run(&mut self, mut ctx: ProcessContext) {
        while let Some(message) = ctx.recv().await {
            let cmd: PanelCommand = match serde_json::from_slice(&message.data) {
                Ok(cmd) => cmd,
                Err(err) => {
                    // TODO use process log instead for better debugging
                    tracing::error!("Error parsing PanelCommand: {:?}", err);
                    continue;
                }
            };

            let mut store = self.store.write().await;
            match cmd {
                PanelCommand::Focus(panel) => {
                    store.focus(panel as _);
                }
            }
        }
    }
}

impl PanelControlService {
    pub fn new(store: Arc<RwLock<PanelStore>>) -> Self {
        Self { store }
    }
}

/// `hearth.panels.AmbientPanelService`: Publisher for panel events addressed
/// to the ambient panel.
pub type AmbientPanelService = PublisherProcess<PanelEvent>;

/// Core data structure for panel management.
pub struct PanelStore {
    /// The pool of [PanelWrappers][PanelWrapper] to active panels.
    pub panels: Slab<PanelWrapper>,

    /// The currently focused panel.
    pub focused: Option<usize>,
}

impl PanelStore {
    /// Creates an empty [PanelStore].
    pub fn new() -> Self {
        Self {
            panels: Default::default(),
            focused: None,
        }
    }

    /// Adds a new panel to the store. Returns its ID.
    pub fn add_panel(&mut self, wrapper: PanelWrapper) -> usize {
        self.panels.insert(wrapper)
    }

    /// Changes panel focus to a new panel.
    ///
    /// Returns the ID of the last focused panel. Does nothing and returns
    /// `None` if the new panel ID is invalid.
    pub fn focus(&mut self, panel: usize) -> Option<usize> {
        if !self.panel_send(panel, PanelEvent::Focus(true)) {
            return None;
        }

        if let Some(last) = self.focused.replace(panel) {
            // self.focused should never point to an invalid panel
            self.expect_send(last, PanelEvent::Focus(false));
            Some(last)
        } else {
            None
        }
    }

    /// Safely sends a panel event to a panel, returning true if successful.
    fn panel_send(&mut self, panel: usize, message: PanelEvent) -> bool {
        if let Some(wrapper) = self.panels.get(panel) {
            let result = wrapper.event_tx.send(message);
            if result.is_err() {
                self.panels.remove(panel);
                false // remote hung up
            } else {
                true // successfully sent the message
            }
        } else {
            false // no panel was found
        }
    }

    /// Gets a panel wrapper but panic if the ID was not found.
    fn expect_panel(&self, panel: usize) -> &PanelWrapper {
        match self.panels.get(panel) {
            Some(wrapper) => wrapper,
            None => panic!("Expected panel ID {} to be valid", panel),
        }
    }

    /// Sends a message to a panel, returning true if successful, but panics if
    /// the ID was not found.
    fn expect_send(&mut self, panel: usize, message: PanelEvent) -> bool {
        let result = self.expect_panel(panel).event_tx.send(message);
        if result.is_err() {
            self.panels.remove(panel);
            false // remote hung up
        } else {
            true // successfully sent the message
        }
    }
}

/// Handle to a remote panel implementation.
pub struct PanelWrapper {
    /// The sender to send messages.
    pub event_tx: mpsc::UnboundedSender<PanelEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_passthru(store: &mut PanelStore) -> (usize, mpsc::UnboundedReceiver<PanelEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let wrapper = PanelWrapper { event_tx };
        let id = store.add_panel(wrapper);
        (id, event_rx)
    }

    #[test]
    fn focus() {
        let mut store = PanelStore::new();
        let (p1_id, mut p1_rx) = add_passthru(&mut store);
        let (p2_id, mut p2_rx) = add_passthru(&mut store);
        assert_eq!(store.focus(42), None);
        assert_eq!(store.focus(p1_id), None);
        assert_eq!(p1_rx.try_recv().unwrap(), PanelEvent::Focus(true));
        assert_eq!(store.focus(p2_id), Some(p1_id));
        assert_eq!(p1_rx.try_recv().unwrap(), PanelEvent::Focus(false));
        assert_eq!(p2_rx.try_recv().unwrap(), PanelEvent::Focus(true));
    }
}
