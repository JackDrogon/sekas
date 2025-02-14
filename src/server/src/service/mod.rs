// Copyright 2022 The Engula Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
pub mod admin;
mod metrics;
pub mod node;
pub mod raft;
pub mod root;

use std::sync::Arc;
use std::time::Duration;

use sekas_client::{ClientOptions, SekasClient};

use crate::node::Node;
use crate::root::Root;
use crate::transport::{AddressResolver, TransportManager};

#[derive(Clone)]
pub struct Server {
    pub node: Arc<Node>,
    pub root: Root,
    pub address_resolver: Arc<AddressResolver>,
}

#[derive(Clone)]
pub struct ProxyServer {
    pub client: SekasClient,
}

impl ProxyServer {
    pub(crate) fn new(transport_manager: &TransportManager) -> Self {
        let opts =
            ClientOptions { connect_timeout: Some(Duration::from_millis(250)), timeout: None };
        ProxyServer { client: transport_manager.build_client(opts) }
    }
}
