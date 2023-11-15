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

use serde::{Deserialize, Serialize};

/// A user input event.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum InputEvent {
    /// The panel received a Unicode character.
    ReceivedCharacter(char),
}

/// An event sent to a panel.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum PanelEvent {
    /// The panel has gained focus (`true`) or lost focus (`false`).
    Focus(bool),

    /// The panel received an [InputEvent].
    Input(InputEvent),
}

crate::impl_serialize_json_display!(PanelEvent);

/// A message sent to the panel control service to control the panel store.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum PanelCommand {
    /// Focuses a panel.
    Focus(u32),
}
