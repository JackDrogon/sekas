// Copyright 2023-present The Sekas Authors.
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

use std::sync::Arc;
use std::time::Duration;
use std::vec;

use log::{debug, info, warn};
use sekas_api::server::v1::node_server::NodeServer;
use sekas_api::server::v1::root_server::RootServer;
use sekas_api::server::v1::*;
use sekas_client::RootClient;
use sekas_runtime::{Executor, Shutdown};

use crate::constants::*;
use crate::engine::{Engines, StateEngine};
use crate::node::Node;
use crate::root::Root;
use crate::serverpb::v1::raft_server::RaftServer;
use crate::serverpb::v1::NodeIdent;
use crate::service::ProxyServer;
use crate::transport::TransportManager;
use crate::{Config, Error, Result, Server};

/// The main entrance of sekas server.
pub fn run(config: Config, executor: Executor, shutdown: Shutdown) -> Result<()> {
    executor.block_on(async { run_in_async(config, shutdown).await })
}

async fn run_in_async(config: Config, shutdown: Shutdown) -> Result<()> {
    let engines = Engines::open(&config.root_dir, &config.db)?;

    let root_list = if config.init { vec![config.addr.clone()] } else { config.join_list.clone() };
    let transport_manager = TransportManager::new(root_list, engines.state()).await;
    let address_resolver = transport_manager.address_resolver();
    let node = Node::new(config.clone(), engines, transport_manager.clone()).await?;

    let ident = bootstrap_or_join_cluster(&config, &node, transport_manager.root_client()).await?;
    node.bootstrap(&ident).await?;
    let root = Root::new(transport_manager.clone(), &ident, config.clone());
    let initial_node_descs = root.bootstrap(&node).await?;
    address_resolver.set_initial_nodes(initial_node_descs);

    info!("node {} starts serving requests", ident.node_id);

    let server = Server { node: Arc::new(node), root, address_resolver };

    let proxy_server =
        if config.enable_proxy_service { Some(ProxyServer::new(&transport_manager)) } else { None };
    bootstrap_services(&config.addr, server, proxy_server, shutdown).await
}

/// Listen and serve incoming rpc requests.
async fn bootstrap_services(
    addr: &str,
    server: Server,
    _proxy_server: Option<ProxyServer>,
    shutdown: Shutdown,
) -> Result<()> {
    use sekas_runtime::TcpIncoming;
    use tokio::net::TcpListener;
    use tonic::transport::Server;

    use crate::service::admin::make_admin_service;

    let listener = TcpListener::bind(addr).await?;
    let incoming = TcpIncoming::from_listener(listener, true);

    let builder = Server::builder()
        .accept_http1(true) // Support http1 for admin service.
        .add_service(NodeServer::new(server.clone()))
        .add_service(RaftServer::new(server.clone()))
        .add_service(RootServer::new(server.clone()))
        .add_service(make_admin_service(server.clone()));

    #[cfg(feature = "layer_etcd")]
    let builder = {
        builder
            .add_service(sekas_etcd_proxy::make_etcd_kv_service())
            .add_service(sekas_etcd_proxy::make_etcd_watch_service())
            .add_service(sekas_etcd_proxy::make_etcd_lease_service())
    };

    let server = builder.serve_with_incoming(incoming);

    sekas_runtime::select! {
        res = server => { res? }
        _ = shutdown => {}
    };

    Ok(())
}

async fn bootstrap_or_join_cluster(
    config: &Config,
    node: &Node,
    root_client: &RootClient,
) -> Result<NodeIdent> {
    let state_engine = node.state_engine();
    if let Some(node_ident) = state_engine.read_ident().await? {
        info!("both cluster and node are initialized, node id {}", node_ident.node_id);
        node.reload_root_from_engine().await?;
        return Ok(node_ident);
    }

    Ok(if config.init {
        bootstrap_cluster(node, &config.addr).await?
    } else {
        try_join_cluster(node, &config.addr, config.join_list.clone(), config.cpu_nums, root_client)
            .await?
    })
}

async fn try_join_cluster(
    node: &Node,
    local_addr: &str,
    join_list: Vec<String>,
    cpu_nums: u32,
    root_client: &RootClient,
) -> Result<NodeIdent> {
    info!("try join a bootstrapted cluster");

    let join_list = join_list.into_iter().filter(|addr| *addr != local_addr).collect::<Vec<_>>();
    if join_list.is_empty() {
        return Err(Error::InvalidArgument("the filtered join list is empty".into()));
    }

    let capacity = NodeCapacity { cpu_nums: cpu_nums as f64, ..Default::default() };

    let req = JoinNodeRequest { addr: local_addr.to_owned(), capacity: Some(capacity) };

    let mut backoff: u64 = 1;
    loop {
        info!("try send request to root server");
        match root_client.join_node(req.clone()).await {
            Ok(res) => {
                debug!("issue join request to root server success");
                let node_ident =
                    save_node_ident(node.state_engine(), res.cluster_id, res.node_id).await;
                node.update_root(res.root.unwrap_or_default()).await?;
                return node_ident;
            }
            Err(e) => {
                warn!("failed to join cluster: {e:?}. join_list={join_list:?}");
            }
        }
        std::thread::sleep(Duration::from_secs(backoff));
        backoff = std::cmp::min(backoff * 2, 120);
    }
}

pub(crate) async fn bootstrap_cluster(node: &Node, addr: &str) -> Result<NodeIdent> {
    info!("'--init' is specified, try bootstrap cluster");

    // TODO(walter) clean staled data in db.
    write_initial_cluster_data(node, addr).await?;

    let state_engine = node.state_engine();
    let cluster_id = vec![];

    let ident = save_node_ident(state_engine, cluster_id.to_owned(), FIRST_NODE_ID).await?;

    info!("bootstrap cluster successfully");

    Ok(ident)
}

async fn save_node_ident(
    state_engine: &StateEngine,
    cluster_id: Vec<u8>,
    node_id: u64,
) -> Result<NodeIdent> {
    let node_ident = NodeIdent { cluster_id, node_id };
    state_engine.save_ident(&node_ident).await?;

    info!("save node ident, node id {}", node_id);

    Ok(node_ident)
}

async fn write_initial_cluster_data(node: &Node, addr: &str) -> Result<()> {
    // Create the first raft group of cluster, this node is the only member of the
    // raft group.
    node.create_replica(FIRST_REPLICA_ID, sekas_schema::system::root_group()).await?;

    // Create another group with empty shard to prepare user usage.
    node.create_replica(INIT_USER_REPLICA_ID, sekas_schema::system::init_group()).await?;

    let root_node = NodeDesc { id: FIRST_NODE_ID, addr: addr.to_owned(), ..Default::default() };
    let root_desc = RootDesc { epoch: INITIAL_EPOCH, root_nodes: vec![root_node] };
    node.update_root(root_desc).await?;

    Ok(())
}

#[cfg(test)]
pub(crate) fn open_engine_with_default_config<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<crate::engine::RawDb> {
    crate::engine::open_raw_db(&crate::DbConfig::default(), path)
}
