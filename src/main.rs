pub mod app;
pub mod block;
pub mod hash_utils;
pub mod p2p;

use libp2p::core::upgrade;
use libp2p::futures::StreamExt;
use libp2p::mplex;
use libp2p::noise::{Keypair, NoiseConfig, X25519Spec};
use libp2p::swarm::{Swarm, SwarmBuilder};
use libp2p::tcp::TokioTcpConfig;
use libp2p::Transport;
use log::{error, info};
use pretty_env_logger;
use tokio::io::stdin;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio::time::Duration;
use tokio::{select, spawn};

#[tokio::main]
async fn main() {
    println!("Run with \"RUST_LOG=info cargo run\".\n\n\
    Possible commands:\n\
    ls peers\n\
    ls chain\n\
    create block <data>\n\n");

    pretty_env_logger::init();

    info!("Peer ID: {}", p2p::PEER_ID.clone());

    let (response_sender, mut response_recv) = mpsc::unbounded_channel();
    let (init_sender, mut init_recv) = mpsc::unbounded_channel();

    let auth_keys = Keypair::<X25519Spec>::new()
        .into_authentic(&p2p::KEYS)
        .expect("Could not create auth keys");
    let transp = TokioTcpConfig::new()
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::new())
        .boxed();

    let behaviour =
        p2p::AppBehaviour::new(app::App::new(), response_sender, init_sender.clone()).await;
    let mut swarm = SwarmBuilder::new(transp, behaviour, *p2p::PEER_ID)
        .executor(Box::new(|fut| {
            spawn(fut);
        }))
        .build();

    let mut stdin = BufReader::new(stdin()).lines();

    Swarm::listen_on(
        &mut swarm,
        "/ip4/0.0.0.0/tcp/0"
            .parse()
            .expect("Could not get a local socket"),
    )
    .expect("Swarm could not be started");

    spawn(async move {
        sleep(Duration::from_secs(1)).await;
        info!("Sending init event");
        init_sender.send(true).expect("Could not send init event");
    });

    loop {
        let event = {
            select! {
                line = stdin.next_line() => Some(p2p::EventType::Input(line.expect("Could not get line").expect("Could not get line from stdin"))),
                response = response_recv.recv() => {
                    Some(p2p::EventType::LocalChainResponse(response.expect("Response does not exist")))
                },
                _init = init_recv.recv() => {
                    Some(p2p::EventType::Init)
                },
                _event = swarm.select_next_some() => {
                    //info!("Unhandled Swarm event: {:?}", event);
                    None
                },
            }
        };

        if let Some(event) = event {
            match event {
                p2p::EventType::Init => {
                    let peers = p2p::get_list_peers(&swarm);
                    swarm.behaviour_mut().app.genesis();

                    info!("Connected nodes: {}", peers.len());

                    if !peers.is_empty() {
                        let req = p2p::LocalChainRequest {
                            from_peer_id: peers
                                .iter()
                                .last()
                                .expect("Expected at least one peer")
                                .to_string(),
                        };

                        let json = serde_json::to_string(&req)
                            .expect("Could not transform request to json");

                        swarm
                            .behaviour_mut()
                            .floodsub
                            .publish(p2p::CHAIN_TOPIC.clone(), json.as_bytes());
                    }
                }

                p2p::EventType::LocalChainResponse(resp) => {
                    let json =
                        serde_json::to_string(&resp).expect("Could not transform response to json");
                    
                    swarm.behaviour_mut()
                        .floodsub
                        .publish(p2p::CHAIN_TOPIC.clone(), json.as_bytes());
                }

                p2p::EventType::Input(line) => match line.as_str() {
                    "ls peers" => p2p::handle_print_peers(&swarm),
                    cmd if cmd.starts_with("ls chain") => p2p::handle_print_chain(&swarm),
                    cmd if cmd.starts_with("create block") => p2p::handle_create_block(cmd, &mut swarm),
                    _ => error!("Unknown command"),
                },
            }
        }
    }
}
