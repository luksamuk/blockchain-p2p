use lazy_static::lazy_static;
use libp2p::floodsub::{Floodsub, FloodsubEvent, Topic};
use libp2p::identity::Keypair;
use libp2p::mdns::{Mdns, MdnsEvent};
use libp2p::swarm::NetworkBehaviourEventProcess;
use libp2p::swarm::Swarm;
use libp2p::NetworkBehaviour;
use libp2p::PeerId;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::sync::mpsc;

use crate::app::App;
use crate::block::Block;

lazy_static! {
    pub static ref KEYS: Keypair = Keypair::generate_ed25519();
    pub static ref PEER_ID: PeerId = PeerId::from(KEYS.public());
    pub static ref CHAIN_TOPIC: Topic = Topic::new("chains");
    pub static ref BLOCK_TOPIC: Topic = Topic::new("blocks");
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChainResponse {
    pub blocks: Vec<Block>,
    pub receiver: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalChainRequest {
    pub from_peer_id: String,
}

pub enum EventType {
    LocalChainResponse(ChainResponse),
    Input(String),
    Init,
}

#[derive(NetworkBehaviour)]
pub struct AppBehaviour {
    pub floodsub: Floodsub,
    pub mdns: Mdns,
    #[behaviour(ignore)]
    pub response_sender: mpsc::UnboundedSender<ChainResponse>,
    #[behaviour(ignore)]
    pub init_sender: mpsc::UnboundedSender<bool>,
    #[behaviour(ignore)]
    pub app: App,
}

impl AppBehaviour {
    pub async fn new(
        app: App,
        response_sender: mpsc::UnboundedSender<ChainResponse>,
        init_sender: mpsc::UnboundedSender<bool>,
    ) -> Self {
        let mut behaviour = Self {
            app,
            floodsub: Floodsub::new(*PEER_ID),
            mdns: Mdns::new(Default::default())
                .await
                .expect("Could not create MDNS"),
            response_sender,
            init_sender,
        };

        behaviour.floodsub.subscribe(CHAIN_TOPIC.clone());
        behaviour.floodsub.subscribe(BLOCK_TOPIC.clone());

        behaviour
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for AppBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(discovered_list) => {
                for (peer, _addr) in discovered_list {
                    self.floodsub.add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(expired_list) => {
                for (peer, _addr) in expired_list {
                    if !self.mdns.has_node(&peer) {
                        self.floodsub.remove_node_from_partial_view(&peer);
                    }
                }
            }
        }
    }
}

impl NetworkBehaviourEventProcess<FloodsubEvent> for AppBehaviour {
    fn inject_event(&mut self, event: FloodsubEvent) {
        if let FloodsubEvent::Message(msg) = event {
            if let Ok(resp) = serde_json::from_slice::<ChainResponse>(&msg.data) {
                if resp.receiver == PEER_ID.to_string() {
                    info!("Response from {}:", msg.source);
                    resp.blocks.iter().for_each(|r| info!("{:?}", r));

                    match self.app.choose_chain(self.app.blocks.clone(), resp.blocks) {
                        Ok(blocks) => self.app.blocks = blocks,
                        Err(msg) => error!("Could not pick best chain: {}", msg),
                    }
                }
            } else if let Ok(resp) = serde_json::from_slice::<LocalChainRequest>(&msg.data) {
                info!("Sending local chain to {}", msg.source.to_string());
                let peer_id = resp.from_peer_id;
                if PEER_ID.to_string() == peer_id {
                    if let Err(e) = self.response_sender.send(ChainResponse {
                        blocks: self.app.blocks.clone(),
                        receiver: msg.source.to_string(),
                    }) {
                        error!("Error sending response via channel: {}", e);
                    }
                }
            } else if let Ok(block) = serde_json::from_slice::<Block>(&msg.data) {
                info!("Received new block from {}", msg.source.to_string());
                if let Err(e) = self.app.add_block(block) {
                    error!("Error adding block: {}", e);
                }
            }
        }
    }
}

pub fn get_list_peers(swarm: &Swarm<AppBehaviour>) -> Vec<String> {
    let nodes = swarm.behaviour().mdns.discovered_nodes();
    let mut unique_peers = HashSet::new();
    for peer in nodes {
        unique_peers.insert(peer);
    }
    unique_peers.iter().map(|p| p.to_string()).collect()
}

pub fn handle_print_peers(swarm: &Swarm<AppBehaviour>) {
    let peers = get_list_peers(swarm);
    peers.iter().for_each(|p| println!("{}", p));
}

pub fn handle_print_chain(swarm: &Swarm<AppBehaviour>) {
    println!("Local Blockchain:");
    let pretty_json = serde_json::to_string_pretty(&swarm.behaviour().app.blocks)
        .expect("Could not convert block to json");
    println!("{}", pretty_json);
}

pub fn handle_create_block(cmd: &str, swarm: &mut Swarm<AppBehaviour>) {
    if let Some(data) = cmd.strip_prefix("create block") {
        let behaviour = swarm.behaviour_mut();
        let latest_block = behaviour
            .app
            .blocks
            .last()
            .expect("Expected at least one block");
        let block = Block::new(
            latest_block.id + 1,
            latest_block.hash.clone(),
            data.trim().to_owned(),
        );
        let json = serde_json::to_string(&block).expect("Could not convert request to json");
        let hash = block.hash.clone();
        behaviour.app.blocks.push(block);
        info!("Broadcasting new block {}", hash);
        behaviour
            .floodsub
            .publish(BLOCK_TOPIC.clone(), json.as_bytes());
    }
}
