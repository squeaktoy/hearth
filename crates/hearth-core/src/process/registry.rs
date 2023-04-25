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

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use super::store::{Capability, ProcessStoreTrait};

pub struct Registry<Store: ProcessStoreTrait> {
    store: Arc<Store>,
    services: Mutex<HashMap<String, Capability>>,
}

impl<Store: ProcessStoreTrait> Drop for Registry<Store> {
    fn drop(&mut self) {
        for (_name, cap) in self.services.lock().drain() {
            cap.free(self.store.as_ref());
        }
    }
}

impl<Store: ProcessStoreTrait> Registry<Store> {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            services: Default::default(),
        }
    }

    pub fn get(&self, service: impl AsRef<str>) -> Option<Capability> {
        let cap = self
            .services
            .lock()
            .get(service.as_ref())?
            .clone(self.store.as_ref());

        Some(cap)
    }

    #[must_use = "capabilities must be freed before drop"]
    pub fn insert(&self, service: impl ToString, cap: &Capability) -> Option<Capability> {
        let cap = cap.clone(self.store.as_ref());
        self.services.lock().insert(service.to_string(), cap)
    }

    #[must_use = "capabilities must be freed before drop"]
    pub fn remove(&self, service: impl AsRef<str>) -> Option<Capability> {
        self.services.lock().remove(service.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::process::{
        store::{tests::*, ProcessStore},
        Flags,
    };

    fn make_registry() -> Registry<ProcessStore<MockProcessEntry>> {
        let store = make_store();
        Registry::new(Arc::new(store))
    }

    #[test]
    fn create_registry() {
        let _reg = make_registry();
    }

    #[test]
    fn insert() {
        let reg = make_registry();
        let handle = reg.store.insert_mock();
        let cap = Capability::new(handle, Flags);
    }
}
