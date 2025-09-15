/// Behaviour of the node in the network.
///
/// The communication has two main behaviors:
/// - It can be modeled as a request-response protocol where one party asks the other for certain
///   information to continue the computation. For example a party may request a share to the other
///   party, and the last may answer with a set of bits or an abort signal.
/// - It can be modeled as a broadcast channel in which a party sends a message to other parties.
///   For this, we use the gossipsub protocol.
use libp2p::futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use libp2p::gossipsub::{
    ConfigBuilder, ConfigBuilderError, IdentTopic, Message, MessageAuthenticity, PublishError,
    SubscriptionError, ValidationMode,
};
use libp2p::identity::Keypair;
use libp2p::swarm::dial_opts::DialOpts;
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
use tracing::{info, warn};

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
    StreamNotConnected(PeerId),
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
        sender_id: PeerId,
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
pub(crate) struct Behaviour {
    /// Behavior for the point-to-point channels.
    pub(crate) stream: stream::Behaviour,
    /// Behavior of the node in the gossipsub protocol.
    pub(crate) gossipsub: gossipsub::Behaviour,
}

/// Configuration of the node.
pub struct NodeConfig {
    /// Secret key to use for secure communication.
    pub listen_addresses: Vec<Multiaddr>,

    /// Key pair for secret communication using QUIC.
    pub keypair: Keypair,
}

/// A node in the network.
pub struct Node {
    /// ID of the node in the network.
    pub id: PeerId,
    /// Swarm to which the node is connected.
    swarm: Swarm<Behaviour>,
    /// Addresses used by this node to listen to new connections.
    listen_addresses: Vec<Multiaddr>,
    /// Channel of received broadcasts.
    received_broadcasts: Sender<gossipsub::Message>,
    /// Current stablished connections.
    pub streams: Arc<Mutex<HashMap<PeerId, Arc<Mutex<Stream>>>>>,
}

impl Node {
    /// Creates a new node.
    pub fn new(
        config: NodeConfig,
        received_broadcasts: Sender<gossipsub::Message>,
    ) -> Result<Self> {
        let peer_id = PeerId::from(config.keypair.public());

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
            .build();

        let subscribed = swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&IdentTopic::new(DEFAULT_TOPIC_GOSSIPSUB))?;
        if subscribed {
            warn!("the peer was already subscribed to the topic");
        }

        info!("creating node with peer ID {peer_id}");

        Ok(Self {
            id: peer_id,
            swarm,
            listen_addresses: config.listen_addresses,
            received_broadcasts,
            streams: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Listens for new connection on the provided listening addresses.
    pub fn listen(&mut self) -> Result<()> {
        for address in &self.listen_addresses {
            info!("listening on address {address}");
            self.swarm.listen_on(address.clone())?;
        }
        Ok(())
    }

    /// Handles new incoming streams.
    ///
    /// New incoming streams are stored in a [`HashMap`] with pairs `(peer, stream)`.
    pub async fn handle_incoming_stream(&mut self) -> Result<()> {
        let mut incoming_streams_handler = self
            .swarm
            .behaviour()
            .stream
            .new_control()
            .accept(StreamProtocol::new(AJAX_PROTOCOL_PREFIX))?;
        while let Some((peer, stream)) = incoming_streams_handler.next().await {
            self.streams
                .lock()
                .await
                .insert(peer, Arc::new(Mutex::new(stream)));
        }
        Ok(())
    }

    /// Dials to remote peers to establish a new connection.
    pub async fn dial(&mut self, remote_peers: Vec<(PeerId, Vec<Multiaddr>)>) -> Result<()> {
        for (peer_id, addresses) in remote_peers {
            self.swarm
                .dial(DialOpts::peer_id(peer_id).addresses(addresses).build())?;
            let mut connection_control = self.swarm.behaviour().stream.new_control();
            let stream = connection_control
                .open_stream(peer_id, StreamProtocol::new(AJAX_PROTOCOL_PREFIX))
                .await?;
            self.streams
                .lock()
                .await
                .insert(peer_id, Arc::new(Mutex::new(stream)));
        }
        Ok(())
    }

    /// Handles an event in the network.
    async fn handle_event(&self, event: SwarmEvent<BehaviourEvent>) -> Result<()> {
        match event {
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                message,
                ..
            })) => {
                match self.received_broadcasts.send(message).await {
                    Ok(()) => {}
                    Err(error) => return Err(Error::SendBroadcast(Box::new(error.0))),
                };
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                let listen_address = address.with_p2p(*self.swarm.local_peer_id()).unwrap();
                info!("listening on {listen_address:?}");
            }
            SwarmEvent::Dialing {
                peer_id: Some(peer_id),
                ..
            } => {
                info!("dialing peer {peer_id:?}");
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                info!("connection established with peer {peer_id:?}");
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                warn!("connection closed with peer {peer_id:?}");
            }
            // The following errors are not inherently bad, but need attention.
            event @ SwarmEvent::IncomingConnection { .. }
            | event @ SwarmEvent::NewExternalAddrCandidate { .. }
            | event @ SwarmEvent::NewExternalAddrOfPeer { .. }
            | event @ SwarmEvent::ExternalAddrConfirmed { .. } => {
                warn!("received event: {event:?}");
            }
            unexpected_event => {
                return Err(Error::UnexpectedSwarmEvent(format!("{unexpected_event:?}")))
            }
        };

        Ok(())
    }

    /// Returns the peer ID of the current node.
    pub fn id(&self) -> &PeerId {
        &self.id
    }

    /// Executes the node in a loop and listens for new events to be processed.
    pub async fn run(&mut self) -> Result<()> {
        info!("P2P node is running...");
        loop {
            let event = self.swarm.select_next_some().await;
            self.handle_event(event).await?;
        }
    }

    /// Send a message to the peer.
    pub async fn send(&self, peer_id: PeerId, data: &[u8]) -> Result<()> {
        self.streams
            .lock()
            .await
            .get(&peer_id)
            .ok_or(Error::StreamNotConnected(peer_id))?
            .lock()
            .await
            .write_all(data)
            .await
            .map_err(|error| Error::SendError {
                receiver_id: peer_id,
                error,
            })?;
        Ok(())
    }

    /// Receive a message from the peer.
    pub async fn recv(&self, peer_id: PeerId, buffer: &mut [u8]) -> Result<usize> {
        let bytes_read = self
            .streams
            .lock()
            .await
            .get(&peer_id)
            .ok_or(Error::StreamNotConnected(peer_id))?
            .lock()
            .await
            .read(buffer)
            .await
            .map_err(|error| Error::RecvError {
                sender_id: peer_id,
                error,
            })?;
        Ok(bytes_read)
    }

    /// Broadcast a message to all parties.
    pub async fn broadcast(&mut self, data: &[u8]) -> Result<()> {
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(IdentTopic::new(DEFAULT_TOPIC_GOSSIPSUB), data)?;
        Ok(())
    }

    /// Broadcast a message by sending the message to all the parties using the raw streams.
    pub async fn raw_broadcast(&mut self, data: &[u8]) -> Result<()> {
        let mut handles = Vec::new();
        let mut streams = self.streams.lock().await;
        for (peer_id, stream) in streams.iter_mut() {
            let stream = stream.clone();
            let data = data.to_vec();
            let peer_id = *peer_id;
            let handle: JoinHandle<Result<()>> = tokio::spawn(async move {
                stream
                    .lock()
                    .await
                    .write_all(&data)
                    .await
                    .map_err(|error| Error::SendError {
                        receiver_id: peer_id,
                        error,
                    })?;
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
    pub async fn flush(&self, peer_id: PeerId) -> Result<()> {
        self.streams
            .lock()
            .await
            .get(&peer_id)
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
        let mut handles = Vec::new();
        let streams = self.streams.lock().await;
        for stream in streams.values() {
            let stream = stream.clone();
            let handle: JoinHandle<Result<()>> = tokio::spawn(async move {
                stream
                    .lock()
                    .await
                    .flush()
                    .await
                    .map_err(Error::FlushError)?;
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
}
