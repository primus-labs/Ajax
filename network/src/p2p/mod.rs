#![allow(missing_docs)]

//! Behaviour of the node in the network.
//!
//! The communication has two main behaviors:
//! - It can be modeled as a request-response protocol where one party asks the other for certain
//!   information to continue the computation. For example a party may request a share to the other
//!   party, and the last may answer with a set of bits or an abort signal.
//! - It can be modeled as a broadcast channel in which a party sends a message to other parties.
//!   For this, we use the gossipsub protocol.

use crate::netio::NetIoStats;
use libp2p::futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use libp2p::gossipsub::{
    ConfigBuilder, ConfigBuilderError, IdentTopic, Message, MessageAuthenticity, PublishError,
    SubscriptionError, ValidationMode,
};
use libp2p::identity::Keypair;
use libp2p::swarm::dial_opts::PeerCondition::DisconnectedAndNotDialing;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{DialError, NetworkBehaviour, SwarmEvent};
use libp2p::{
    futures, gossipsub, Multiaddr, PeerId, Stream, StreamProtocol, Swarm, SwarmBuilder,
    TransportError,
};
use libp2p_stream as stream;
use libp2p_stream::{AlreadyRegistered, OpenStreamError};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
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
    /// Swarm to which the node is connected.
    swarm: Arc<Mutex<Swarm<Behaviour>>>,
    /// Addresses used by this node to listen to new connections.
    listen_addresses: Vec<Multiaddr>,
    /// Channel of received broadcasts.
    received_broadcasts: Sender<Message>,
    /// Current stablished connections.
    streams: Arc<Mutex<HashMap<PeerId, Arc<Mutex<Stream>>>>>,
    /// Map of integer IDs to network IDs
    peer_ids: Arc<Mutex<HashMap<usize, PeerId>>>,
    /// Stats for the network.
    stats: Arc<NetIoStats>,
}

impl P2pNet {
    /// Returns the current stats of the network.
    pub fn stats(&self) -> &NetIoStats {
        &self.stats
    }

    /// Creates a new node.
    pub fn new(
        party_idx: usize,
        config: NodeConfig,
        received_broadcasts: Sender<Message>,
    ) -> Result<Self> {
        let peer_id = PeerId::from(config.keypair.public());

        let mut peer_ids = HashMap::new();
        peer_ids.insert(party_idx, peer_id);

        let gossipsub_config = ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(20))
            .validation_mode(ValidationMode::Strict)
            .build()?;

        let mut swarm = SwarmBuilder::with_existing_identity(config.keypair)
            .with_tokio()
            .with_quic()
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

        info!("Creating node with peer ID {peer_id}");

        Ok(Self {
            id: party_idx,
            peer_ids: Arc::new(Mutex::new(peer_ids)),
            swarm: Arc::new(Mutex::new(swarm)),
            listen_addresses: config.listen_addresses,
            received_broadcasts,
            streams: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(NetIoStats::default()),
        })
    }

    /// Listens for new connection on the provided listening addresses.
    pub async fn listen(&self) -> Result<()> {
        let own_id = self.id;
        for address in &self.listen_addresses {
            self.swarm.lock().await.listen_on(address.clone())?;
            info!("Peer {own_id} listening on address {address}");
        }
        Ok(())
    }

    /// Dials to remote peers to establish a new connection.
    pub async fn dial(&self, remote_peers: Vec<(PeerId, usize, Vec<Multiaddr>)>) -> Result<()> {
        let own_id = self.id;
        for (peer_id_encoded, peer_id, addresses) in remote_peers {
            info!("Peer {own_id} is dialing peer {peer_id} ({peer_id_encoded})");
            if let Err(DialError::DialPeerConditionFalse(DisconnectedAndNotDialing)) =
                self.swarm.lock().await.dial(
                    DialOpts::peer_id(peer_id_encoded)
                        .addresses(addresses)
                        .condition(PeerCondition::DisconnectedAndNotDialing)
                        .build(),
                )
            {
                warn!("Peer {own_id} is dialing {peer_id} ({peer_id_encoded}) but the peer is already connected or there is an ongoing dialing. Aborting the new dialing try.");
            }
            let mut connection_control = self.swarm.lock().await.behaviour().stream.new_control();
            let stream = connection_control
                .open_stream(peer_id_encoded, StreamProtocol::new(AJAX_PROTOCOL_PREFIX))
                .await?;
            self.streams
                .lock()
                .await
                .insert(peer_id_encoded, Arc::new(Mutex::new(stream)));
            self.peer_ids
                .lock()
                .await
                .insert(peer_id, peer_id_encoded.clone());
            info!("Dial to peer {peer_id_encoded} successful. The stream was added correctly.");
        }
        Ok(())
    }

    /// Executes the node in a loop and listens for new events to be processed. Also, the method
    /// handles the new incoming streams.
    pub async fn run(&self) -> Result<()> {
        let mut incoming_streams_handler = self
            .swarm
            .lock()
            .await
            .behaviour()
            .stream
            .new_control()
            .accept(StreamProtocol::new(AJAX_PROTOCOL_PREFIX))?;

        tokio::spawn({
            let streams = self.streams.clone();
            let own_id = self.id;
            async move {
                while let Some((peer, stream)) = incoming_streams_handler.next().await {
                    info!("New stream from peer {peer:?} to {own_id}");
                    streams
                        .lock()
                        .await
                        .insert(peer, Arc::new(Mutex::new(stream)));
                }
            }
        });

        info!("Node with ID {} waiting incoming events", self.id);
        loop {
            let event = self.swarm.lock().await.select_next_some().await;
            self.handle_event(event).await?;
        }
    }

    /// Handles an event in the network.
    async fn handle_event(&self, event: SwarmEvent<BehaviourEvent>) -> Result<()> {
        let own_id = self.id;
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
                match self.received_broadcasts.send(message).await {
                    Ok(()) => info!(
                        "Peer {own_id} received a broadcasted message from {propagation_source}"
                    ),
                    Err(error) => return Err(Error::SendBroadcast(Box::new(error.0))),
                };
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                let listen_address = address
                    .with_p2p(*self.swarm.lock().await.local_peer_id())
                    .unwrap();
                info!("Peer with ID {own_id} listening on {listen_address:?}");
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
                self.streams.lock().await.remove(&peer_id);
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

    /// Returns the swarm of the network.
    pub async fn swarm(&self) -> &Arc<Mutex<Swarm<Behaviour>>> {
        &self.swarm
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
            .get(&peer_id_encoded)
            .ok_or(Error::StreamNotConnected(peer_id))?
            .lock()
            .await
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
            .get(peer_id_encoded)
            .ok_or(Error::StreamNotConnected(peer_id))?
            .lock()
            .await
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
        self.swarm
            .lock()
            .await
            .behaviour_mut()
            .gossipsub
            .publish(IdentTopic::new(DEFAULT_TOPIC_GOSSIPSUB), data)?;
        self.stats.update_send(data.len(), init_time.elapsed());
        Ok(())
    }

    /// Broadcast a message by sending the message to all the parties using the raw streams.
    pub async fn raw_broadcast(&mut self, data: &[u8]) -> Result<()> {
        let own_id = self.id;
        info!("Broadcasting message from {own_id} using raw broadcasting");
        let mut handles = Vec::new();
        let mut streams = self.streams.lock().await;
        for (peer_id, stream) in streams.iter_mut() {
            let stream = stream.clone();
            let data = data.to_vec();
            let stats = self.stats.clone();
            let peer_id = peer_id.clone();
            let handle: JoinHandle<Result<()>> =
                tokio::spawn(async move {
                    let init_time = Instant::now();
                    let bytes_sent = stream.lock().await.write(&data).await.map_err(|error| {
                        Error::SendError {
                            receiver_id: peer_id,
                            error,
                        }
                    })?;
                    stats.update_send(bytes_sent, init_time.elapsed());
                    Ok(())
                });
            handles.push(handle);
        }
        let results = futures::future::join_all(handles).await;
        for result in results {
            result??;
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
            .get(peer_id_encoded)
            .ok_or(Error::StreamNotConnected(peer_id))?
            .lock()
            .await
            .flush()
            .await
            .map_err(Error::FlushError)?;
        Ok(())
    }

    /// Flush all the streams in the network.
    pub async fn flush_all(&self) -> Result<()> {
        let streams = self.streams.lock().await;
        for stream in streams.values() {
            stream
                .lock()
                .await
                .flush()
                .await
                .map_err(Error::FlushError)?;
        }
        Ok(())
    }
}
