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

use hearth_guest::Capability;
use kindling_host::{prelude::*, registry::Registry};
use petgraph::{algo::toposort, prelude::DiGraph};
use serde::Deserialize;
hearth_guest::export_metadata!();

const SEARCH_DIR: &str = "init";

pub struct Service {
    /// A capability to this process, stays as `None` until this process is started.
    pub process: Option<Capability>,

    name: String,
    config: ServiceConfig,
}

impl Service {
    pub fn new(name: String, config: ServiceConfig) -> Self {
        Self {
            name,
            process: None,
            config,
        }
    }

    pub fn spawn(&mut self, registry: Option<Registry>) -> Capability {
        let lump = get_file(&format!("{}/{}/service.wasm", SEARCH_DIR, self.name))
            .expect("WASM module not found");
        let cap = spawn_mod(lump, registry.map(|x| x.as_ref().to_owned()));
        self.process = Some(cap.to_owned());
        cap
    }
}

#[no_mangle]
pub extern "C" fn run() {
    info!("Hello world!");
    let mut graph = DiGraph::<Service, ()>::new();
    let mut names_to_idxs = HashMap::new();
    for file in list_files(SEARCH_DIR).unwrap() {
        info!("file: {}", file.name);
        let Some(config) = get_config(&file.name) else {
            error!("Failed to get config");
            continue;
        };
        info!("config: {:?}", config);

        let name = file.name;
        let service = Service::new(name.clone(), config);
        let idx = graph.add_node(service);
        names_to_idxs.insert(name, idx);
    }

    for idx in graph.node_indices() {
        let node = graph.node_weight(idx).unwrap();
        let name = node.name.clone();
        info!("Collecting dependencies of service \'{name}\'");

        let mut remove = false;
        for dep in node.config.dependencies.need.clone() {
            match names_to_idxs.get(&dep.clone()) {
                Some(dep_idx) => {
                    graph.add_edge(idx, *dep_idx, ());
                }
                None => {
                    remove = true;
                    error!("Dependency \'{dep}\' not found");
                }
            };
        }

        if remove {
            info!("Service \'{name}\' will not be spawned");
            graph.remove_node(idx);
        }
    }

    let sorted_services = toposort(&graph, None).unwrap();

    for idx in sorted_services {
        let service = graph.node_weight_mut(idx).unwrap();
        service.spawn(None);
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Dependencies {
    #[serde(default)]
    pub need: Vec<String>,

    #[serde(default)]
    pub milestone: Vec<String>,

    #[serde(default)]
    pub waits_for: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct License {
    pub name: String,
    pub file: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServiceConfig {
    #[serde(default)]
    pub dependencies: Dependencies,

    pub description: Option<String>,

    #[serde(default)]
    pub license: Vec<License>,

    #[serde(default)]
    pub targets: Vec<String>,
}

fn get_config(name: &str) -> Option<ServiceConfig> {
    let config_path = format!("{}/{}/service.toml", SEARCH_DIR, name);
    let config_data = read_file(&config_path).ok()?;
    let config_str = String::from_utf8(config_data).unwrap();
    toml::from_str(&config_str).ok()
}
