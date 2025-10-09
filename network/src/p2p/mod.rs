//! Behaviour of the node in the network.
//!
//! The communication has two main behaviors:
//! - It can be modeled as a request-response protocol where one party asks the other for certain
//!   information to continue the computation. For example a party may request a share to the other
//!   party, and the last may answer with a set of bits or an abort signal.
//! - It can be modeled as a broadcast channel in which a party sends a message to other parties.
//!   For this, we use the gossipsub protocol.
//!
//! The interactions with the swarm are done using channels in order to avoid deadlocks. The public
//! API methods that affect the [`Swarm`] like `listen()`, `dial()`, and `broadcast()` send a
//! [`SwarmCommand`] to a never-ending loop that takes care of them. Each [`SwarmCommand`] has a
//! [`tokio::sync::oneshot::Sender`] channel in which the calling API method (`listen()`, `dial()`, and
//! `broadcast()`) get the response back

#![allow(missing_docs)]

use crate::netio::NetIoStats;
use crate::p2p::Error::Transport;
use libp2p::core::transport::ListenerId;
use libp2p::futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use libp2p::gossipsub::{
    ConfigBuilder, ConfigBuilderError, IdentTopic, Message, MessageAuthenticity, PublishError,
    SubscriptionError, ValidationMode,
};
use libp2p::identity::Keypair;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{DialError, NetworkBehaviour, SwarmEvent};
use libp2p::{
    futures, gossipsub, noise, tcp, yamux, Multiaddr, PeerId, Stream, StreamProtocol, Swarm,
    SwarmBuilder, TransportError,
};
use libp2p_stream as stream;
use libp2p_stream::{AlreadyRegistered, OpenStreamError};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{Sender, UnboundedSender};
use tokio::sync::oneshot::error::RecvError;
use tokio::sync::{oneshot, Mutex};
use tokio::task::{JoinError, JoinHandle};
use tokio::time::Instant;
use tracing::{error, info, warn};

/// Prefix for the Ajax protocol for communication.
pub const AJAX_PROTOCOL_PREFIX: &str = "/ajax";

/// Default topic for the gossipsub protocol
pub const DEFAULT_TOPIC_GOSSIPSUB: &str = "ajax";

/// Error type for the P2P network.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// The behavior was not attached correctly to the swarm.
    #[error("infallible error: {0:?}")]
    Infallible(#[from] std::convert::Infallible),
    /// There was an error in the transport.
    #[error("transport error: {0:?}")]
    Transport(#[from] TransportError<std::io::Error>),
    /// The broadcast message was sent incorrectly to the caller function.
    #[error("error sending broadcast: {0:?}")]
    SendBroadcast(Box<Message>),
    /// The dialing process failed.
    #[error("error dialing the peer: {0:?}")]
    Dial(#[from] DialError),
    /// The stream protocol was already registered.
    #[error("stream protocol already registered: {0:?}")]
    AlreadyRegistered(#[from] AlreadyRegistered),
    /// The stream could not be opened.
    #[error("stream could not be opened: {0:?}")]
    OpenStreamError(#[from] OpenStreamError),
    /// The stream is not connected yet
    #[error("the stream is not connected yet with peer {0:?}")]
    StreamNotConnected(usize),
    /// The message was not sent correctly to the given peer.
    #[error("error while sending data to {receiver_id:?}: {error:?}")]
    SendError {
        /// Intended receiver of the message.
        receiver_id: PeerId,
        /// Error description.
        error: futures::io::Error,
    },
    /// The message was not received correctly from the given peer.
    #[error("error while receiving data from {sender_id:?}: {error:?}")]
    RecvError {
        /// Sender of the message.
        sender_id: usize,
        /// Error description.
        error: futures::io::Error,
    },
    /// Error when subscribing to the default topic.
    #[error("error while subscribing to the default topic: {0:?}")]
    DefaultSubscriptionError(#[from] SubscriptionError),
    /// Error during gossipsub configuration.
    #[error("error while configuring gossipsub: {0:?}")]
    GossipsubConfigError(#[from] ConfigBuilderError),
    /// Broadcast error using gossipsub.
    #[error("error while broadcasting the message: {0:?}")]
    BroadcastError(#[from] PublishError),
    /// Error when flushing the channel
    #[error("error flushing the channel: {0:?}")]
    FlushError(#[from] futures::io::Error),
    /// Error joining the tasks
    #[error("error joining the tasks: {0:?}")]
    JoinError(#[from] JoinError),
    /// Undesired swarm event.
    #[error("unexpected event in the swarm: {0:?}")]
    UnexpectedSwarmEvent(String),
    /// Error while configuring the TCP connections
    #[error("error while configuring the Noise: {0:?}")]
    NoiseConfigError(#[from] noise::Error),
    /// Error while sending a command to the swarm
    #[error("error while sending the command to the swarm: {0:?}")]
    SendSwarmCommandError(#[from] SendError<SwarmCommand>),
    /// Error while sending a command to the swarm
    #[error("error while receiving the response to the command to the swarm: {0:?}")]
    RecvSwarmCommandResponseError(#[from] RecvError),
}

/// Result type for the P2P network.
pub type Result<T> = std::result::Result<T, Error>;

/// Behaviour for the node inside the network.
#[derive(NetworkBehaviour)]
pub struct Behaviour {
    /// Behavior for the point-to-point channels.
    pub(crate) stream: stream::Behaviour,
    /// Behavior of the node in the gossipsub protocol.
    pub(crate) gossipsub: gossipsub::Behaviour,
}

/// Configuration of the node.
pub struct NodeConfig {
    /// Secret key to use for secure communication.
    pub listen_addresses: Vec<Multiaddr>,
    /// Key pair for private communication using QUIC.
    pub keypair: Keypair,
}

impl NodeConfig {
    /// Creates a new configuration for the node.
    ///
    /// The `listen_addresses` are the addresses that the node will use to listen for incomming
    /// communication, and the `keypair` is the public/private keys to secure the communication.
    pub fn new(listen_addresses: Vec<Multiaddr>, keypair: Keypair) -> Self {
        Self {
            listen_addresses,
            keypair,
        }
    }
}

/// A node in the network.
pub struct P2pNet {
    /// ID of the node in the network.
    id: usize,
    /// Addresses used by this node to listen to new connections.
    listen_addresses: Vec<Multiaddr>,
    /// Current stablished connections.
    streams: Arc<Mutex<HashMap<PeerId, Stream>>>,
    /// Map of integer IDs to network IDs
    peer_ids: Arc<Mutex<HashMap<usize, PeerId>>>,
    /// Stats for the network.
    stats: Arc<NetIoStats>,
    /// Sender for commands to the swarm.
    sender_swarm_commands: UnboundedSender<SwarmCommand>,
}

/// Commands that will be sent to the swarm by the other tasks.
pub enum SwarmCommand {
    /// Indicates the swarm to listen
    Listen {
        address: Multiaddr,
        response: oneshot::Sender<Result<ListenerId>>,
    },
    Dial {
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
        response: oneshot::Sender<Result<()>>,
    },
    OpenStream {
        peer_id: PeerId,
        protocol: StreamProtocol,
        response: oneshot::Sender<Result<()>>,
    },
    Broadcast {
        data: Vec<u8>,
        response: oneshot::Sender<Result<()>>,
    },
    ShutDown,
}

impl P2pNet {
    /// Returns the current stats of the network.
    pub fn stats(&self) -> &NetIoStats {
        &self.stats
    }

    /// Creates a new node.
    pub async fn new(
        party_idx: usize,
        config: NodeConfig,
        received_broadcasts: Sender<Message>,
    ) -> Result<Self> {
        let peer_id = PeerId::from(config.keypair.public());

        let (sender_swarm_commands, mut receiver_swarm_commands) =
            tokio::sync::mpsc::unbounded_channel::<SwarmCommand>();

        let mut peer_ids = HashMap::new();
        peer_ids.insert(party_idx, peer_id);

        let gossipsub_config = ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(20))
            .validation_mode(ValidationMode::Strict)
            .build()?;

        let mut swarm = SwarmBuilder::with_existing_identity(config.keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default().nodelay(true),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|keypair| Behaviour {
                gossipsub: gossipsub::Behaviour::new(
                    MessageAuthenticity::Signed(keypair.clone()),
                    gossipsub_config,
                )
                .unwrap(),
                stream: stream::Behaviour::new(),
            })?
            .with_swarm_config(|config| {
                config.with_idle_connection_timeout(Duration::from_secs(10))
            })
            .build();

        let subscribed_first_time = swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&IdentTopic::new(DEFAULT_TOPIC_GOSSIPSUB))?;
        if !subscribed_first_time {
            warn!("The peer was already subscribed to the topic");
        }

        let mut incoming_streams_handler = swarm
            .behaviour()
            .stream
            .new_control()
            .accept(StreamProtocol::new(AJAX_PROTOCOL_PREFIX))?;

        let streams = Arc::new(Mutex::new(HashMap::new()));
        let streams_for_listener = Arc::clone(&streams);
        let streams_for_swarm_tasks = Arc::clone(&streams);
        let broadcast_channel_for_swarm_tasks = received_broadcasts.clone();

        // Start a loop to listen for commands to the swarm.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(command) = receiver_swarm_commands.recv() => {
                        match command {
                            SwarmCommand::Listen { address, response } => {
                                match  swarm.listen_on(address) {
                                Ok(listen_addr) => {
                                        let _ = response.send(Ok(ListenerId::from(listen_addr)));
                                    }
                                    Err(err) => {
                                        let _ = response.send(Err(Transport(err)));
                                    }
                                }
                            }
                            SwarmCommand::Dial { peer_id, addresses, response } => {
                                let result =
                                    swarm.dial(DialOpts::peer_id(peer_id).addresses(addresses).condition(PeerCondition::DisconnectedAndNotDialing).build());
                                match result {
                                    Ok(_) => {let _ = response.send(Ok(())); },
                                    Err(err) => {let _ = response.send(Err(Error::Dial(err))); },
                                }
                            }
                            SwarmCommand::OpenStream { peer_id, protocol, response } => {
                                let mut control = swarm.behaviour().stream.new_control();
                                let result = control.open_stream(peer_id, protocol).await;
                                match result {
                                    Ok(stream) => {
                                        streams_for_listener.lock().await.insert(peer_id, stream);
                                        let _ = response.send(Ok(()));
                                    },
                                    Err(err) => { let _ = response.send(Err(Error::OpenStreamError(err))); },
                                }
                            }
                            SwarmCommand::Broadcast {
                                data,
                                response,
                            } => {
                                let result = swarm.behaviour_mut().gossipsub.publish(IdentTopic::new(DEFAULT_TOPIC_GOSSIPSUB), data);
                                match result {
                                    Ok(_) => {
                                        let _ = response.send(Ok(()));
                                    }
                                    Err(err) =>{
                                        let _ = response.send(Err(Error::BroadcastError(err)));
                                    }
                                }
                            }
                            SwarmCommand::ShutDown => {
                                break;
                            }
                        }
                    }
                    event = swarm.select_next_some() => {
                        info!("Peer {party_idx} received an incomming event to handle");
                        Self::handle_swarm_event(party_idx, event, broadcast_channel_for_swarm_tasks.clone(), Arc::clone(&streams_for_swarm_tasks) ).await.expect("The event should be handled correctly");
                    }
                    Some((peer, stream)) = incoming_streams_handler.next() => {
                        info!("Peer {party_idx} received an incomming stream in the main loop");
                        streams_for_listener.lock().await.insert(peer, stream);
                    }
                }
            }
        });

        Ok(Self {
            id: party_idx,
            peer_ids: Arc::new(Mutex::new(peer_ids)),
            listen_addresses: config.listen_addresses,
            streams,
            stats: Arc::new(NetIoStats::default()),
            sender_swarm_commands,
        })
    }

    pub async fn listen(&self) -> Result<()> {
        for address in &self.listen_addresses {
            let (tx, rx) = oneshot::channel();
            self.sender_swarm_commands.send(SwarmCommand::Listen {
                address: address.clone(),
                response: tx,
            })?;
            rx.await??;
            info!("Node {} listening on {}", self.id, address);
        }
        Ok(())
    }

    pub async fn dial(&self, addresses: Vec<(PeerId, usize, Vec<Multiaddr>)>) -> Result<()> {
        let mut peer_ids = self.peer_ids.lock().await;
        for (peer_id_encoded, peer_id, addresses) in addresses {
            let (tx, rx) = oneshot::channel();
            self.sender_swarm_commands.send(SwarmCommand::Dial {
                peer_id: peer_id_encoded,
                addresses,
                response: tx,
            })?;
            rx.await??;
            peer_ids.insert(peer_id, peer_id_encoded);
        }
        Ok(())
    }

    /// Handles an event in the network.
    async fn handle_swarm_event(
        own_id: usize,
        event: SwarmEvent<BehaviourEvent>,
        received_broadcasts: Sender<Message>,
        streams: Arc<Mutex<HashMap<PeerId, Stream>>>,
    ) -> Result<()> {
        match event {
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Subscribed {
                peer_id,
                topic,
            })) => {
                info!("Gossipsub: The peer ID {peer_id} subscribed to the topic \"{topic}\"");
            }
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                message,
                propagation_source,
                ..
            })) => {
                match received_broadcasts.send(message).await {
                    Ok(()) => info!(
                        "Peer {own_id} received a broadcasted message from {propagation_source}"
                    ),
                    Err(error) => return Err(Error::SendBroadcast(Box::new(error.0))),
                };
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Peer with ID {own_id} listening on {address:?}");
            }
            SwarmEvent::Dialing {
                peer_id: Some(peer_id),
                ..
            } => {
                info!("Peer {own_id} is dialing peer {peer_id:?}");
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                info!("Peer {own_id} established a connection with peer {peer_id:?}");
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                warn!("Connection closed with peer {peer_id:?}");
                streams.lock().await.remove(&peer_id);
            }
            SwarmEvent::IncomingConnection {
                local_addr,
                send_back_addr,
                connection_id,
            } => {
                info!("Incoming connection from {send_back_addr:?} to peer {local_addr:?} with connection ID {connection_id}");
            }
            // The following errors are not inherently bad, but need attention.
            event @ SwarmEvent::NewExternalAddrCandidate { .. }
            | event @ SwarmEvent::NewExternalAddrOfPeer { .. }
            | event @ SwarmEvent::ExternalAddrConfirmed { .. } => {
                warn!("Received event: {event:?}");
            }
            unexpected_event => {
                return Err(Error::UnexpectedSwarmEvent(format!("{unexpected_event:?}")))
            }
        };

        Ok(())
    }

    /// Returns the peer ID of the current node.
    pub fn id(&self) -> usize {
        self.id
    }

    /// Send a message to the peer.
    pub async fn send(&self, peer_id: usize, data: &[u8]) -> Result<usize> {
        let start = Instant::now();
        let peer_ids_db = self.peer_ids.lock().await;
        let peer_id_encoded = peer_ids_db
            .get(&peer_id)
            .ok_or(Error::StreamNotConnected(peer_id))?
            .clone();
        drop(peer_ids_db);
        let bytes = self
            .streams
            .lock()
            .await
            .get_mut(&peer_id_encoded)
            .ok_or(Error::StreamNotConnected(peer_id))?
            .write(data)
            .await
            .map_err(|error| Error::SendError {
                receiver_id: peer_id_encoded,
                error,
            })?;
        self.stats.update_send(bytes, start.elapsed());
        Ok(bytes)
    }

    /// Receive a message from the peer.
    pub async fn recv(&self, peer_id: usize, buffer: &mut [u8]) -> Result<usize> {
        self.flush_all().await?;

        let own_id = self.id;
        info!("Peer {own_id} is receiving a message from {peer_id}");
        let start = Instant::now();
        let peer_ids_db = self.peer_ids.lock().await;
        let peer_id_encoded = peer_ids_db
            .get(&peer_id)
            .ok_or(Error::StreamNotConnected(peer_id))?;
        let bytes_read = self
            .streams
            .lock()
            .await
            .get_mut(peer_id_encoded)
            .ok_or(Error::StreamNotConnected(peer_id))?
            .read(buffer)
            .await
            .map_err(|error| Error::RecvError {
                sender_id: peer_id,
                error,
            })?;
        self.stats.update_recv(bytes_read, start.elapsed());
        Ok(bytes_read)
    }

    /// Broadcast a message to all parties.
    pub async fn broadcast(&self, data: &[u8]) -> Result<()> {
        let own_id = self.id;
        info!("Broadcasting message from {own_id} using gossipsub");
        let init_time = Instant::now();
        let (cmd_sender, cmd_receiver) = tokio::sync::oneshot::channel();
        self.sender_swarm_commands.send(SwarmCommand::Broadcast {
            data: data.to_vec(),
            response: cmd_sender,
        })?;
        cmd_receiver.await??;
        self.stats.update_send(data.len(), init_time.elapsed());
        Ok(())
    }

    /// Broadcast a message by sending the message to all the parties using the raw streams.
    pub async fn raw_broadcast(&mut self, data: &[u8]) -> Result<()> {
        let own_id = self.id;
        info!("Broadcasting message from {own_id} using raw broadcasting");
        let mut streams = self.streams.lock().await;
        for (peer_id, stream) in streams.iter_mut() {
            let data = data.to_vec();
            let stats = self.stats.clone();
            let peer_id = peer_id.clone();
            let init_time = Instant::now();
            let bytes_sent = stream
                .write(&data)
                .await
                .map_err(|error| Error::SendError {
                    receiver_id: peer_id,
                    error,
                })?;
            stats.update_send(bytes_sent, init_time.elapsed());
        }
        Ok(())
    }

    /// Flush the stream for the given peer ID.
    pub async fn flush(&self, peer_id: usize) -> Result<()> {
        let peer_ids_db = self.peer_ids.lock().await;
        let peer_id_encoded = peer_ids_db
            .get(&peer_id)
            .ok_or(Error::StreamNotConnected(peer_id))?;
        self.streams
            .lock()
            .await
            .get_mut(peer_id_encoded)
            .ok_or(Error::StreamNotConnected(peer_id))?
            .flush()
            .await
            .map_err(Error::FlushError)?;
        Ok(())
    }

    /// Flush all the streams in the network.
    pub async fn flush_all(&self) -> Result<()> {
        let mut streams = self.streams.lock().await;
        for stream in streams.values_mut() {
            stream.flush().await.map_err(Error::FlushError)?;
        }
        Ok(())
    }
}
