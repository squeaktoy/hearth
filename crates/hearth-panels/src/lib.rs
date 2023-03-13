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

use hearth_rpc::remoc::rtc::async_trait;
use hearth_core::runtime::{Plugin, Runtime, RuntimeBuilder};

pub struct PanelsPlugin {}

#[async_trait]
impl Plugin for PanelsPlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {}

    async fn run(&mut self, runtime: Arc<Runtime>) {}
}

impl PanelsPlugin {
    pub fn new() -> Self {
        Self {}
    }
}
