#![allow(missing_docs)]

//! Behavior of the node in the network.
//!
//! The communication has two main behaviors:
//! - The party can use raw streams to communicate with other parties in a point-to-point fashion.
//! - The party can use gossipsub to broadcast a message to the other parties.
//!
//! The interactions with the swarm are done using channels to avoid deadlocks. The public
//! API methods that affect the [`Swarm`] like `listen()`, `dial()`, and `broadcast()` send a
//! [`SwarmCommand`] to a never-ending loop that takes care of them. Each [`SwarmCommand`] has a
//! [`oneshot::Sender`] channel in which the calling API method (`listen()`, `dial()`,
//! and `broadcast()`) get the response back from the Swarm. In that way, those methods get an
//! answer to give back to the caller.
//!
//! The rationale behind this design is to avoid deadlocks and reordering of commands. This is
//! achieved by letting the swarm be controlled by just one task at a time, namely, the task present
//! in the [`P2pNet::new`] function. Making the swarm to be controlled by multiple tasks may not
//! work well with the implementation. For that reason, we extensively use channels to send the
//! commands to the swarm to the controlling never-ending loop in the [`P2pNet::new`] function.
//!
//! To guarantee that the peers are connected, we implemented a stream opening with retry.

use crate::netio::NetIoStats;
use crate::p2p::Error::Transport;
use dashmap::DashMap;
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
    futures, gossipsub, noise, tcp, yamux, Multiaddr, PeerId, Stream, StreamProtocol, SwarmBuilder,
    TransportError,
};
use libp2p_stream as stream;
use libp2p_stream::{AlreadyRegistered, OpenStreamError};
use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::error::RecvError;
use tokio::sync::{mpsc, oneshot, Mutex, Notify};
use tokio::task::JoinError;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

/// Prefix for the Ajax protocol for communication.
pub const AJAX_PROTOCOL_PREFIX: &str = "/ajax";

/// Default topic for the gossipsub protocol
pub const DEFAULT_TOPIC_GOSSIPSUB: &str = "ajax";

/// Maximum number of attempts to open a stream with a peer.
const MAX_CONNECTION_ATTEMPTS: usize = 100;

/// Maximum time to wait for a stream to be opened with a peer.
const MAXIMUM_IDLE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(1000);

/// Duration between two attempts to open a stream with a peer.
const WAITING_TIME_BETWEEN_ITERATIONS: Duration = Duration::from_millis(300);

/// Byte used to signal that a party received a opening stream request and that the party has added
/// the stream to its internal database.
const ACK_BYTE: u8 = 0x06;

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
    /// The stream is not in the internal database.
    #[error("Stream was not found in the internal database of the node for party {0}")]
    StreamNotInDatabase(usize),
    /// The peer ID was not found in the internal database.
    #[error("Peer ID was not found in the internal database of the node for party {0}")]
    PeerIdNotInDatabase(usize),
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
    /// Error when flushing the channel.
    #[error("error flushing the channel: {0:?}")]
    FlushError(#[from] futures::io::Error),
    /// Error joining the tasks.
    #[error("error joining the tasks: {0:?}")]
    JoinError(#[from] JoinError),
    /// Undesired swarm event.
    #[error("unexpected event in the swarm: {0:?}")]
    UnexpectedSwarmEvent(String),
    /// Error while configuring the TCP connections.
    #[error("error while configuring the Noise: {0:?}")]
    NoiseConfigError(#[from] noise::Error),
    /// Error while sending a command to the swarm.
    #[error("error while sending the command to the swarm: {0:?}")]
    SendSwarmCommandError(#[from] SendError<SwarmCommand>),
    /// Error while sending a command to the swarm.
    #[error("error while receiving the response to the command to the swarm: {0:?}")]
    RecvSwarmCommandResponseError(#[from] RecvError),
    /// The dial was retried multiple times but not successful.
    #[error("Dial timeout")]
    DialTimeout,
    /// Error when receiving stream connection requests.
    ///
    /// This error occurs when the listener does not add the opened stream to the internal database
    /// successfully.
    #[error("Error receiving stream connection requests")]
    StreamHandshakeError(tokio::io::Error),
    /// The ACK signal received from the other side of the stream does not match the ACK signal sent
    /// by the listener.
    #[error("The signal received is not an ACK signal")]
    InvalidAckSignal,
}

/// Result type for the P2P network.
pub type Result<T> = std::result::Result<T, Error>;

/// Behavior for the node inside the network.
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

/// Commands that will be sent to the swarm by the other tasks.
pub enum SwarmCommand {
    /// Indicates the swarm to listen on the provided address.
    Listen {
        /// Address to listen on.
        address: Multiaddr,
        /// Response channel for the command.
        ///
        /// The channel will store the final result of the `Listen` command.
        response: oneshot::Sender<Result<ListenerId>>,
    },
    /// Indicates the swarm to dial the provided set of addresses.
    Dial {
        /// ID of the peer to dial.
        peer_id: PeerId,
        /// Addresses to dial the peer with.
        addresses: Vec<Multiaddr>,
        /// Response channel for the command.
        ///
        /// The channel will store the final result of the `Dial` command.
        response: oneshot::Sender<Result<()>>,
    },
    /// Indicates the swarm to open a stream with the provided peer.
    OpenStream {
        /// ID of the peer to open a stream with.
        peer_id: PeerId,
        /// Response channel for the command.
        ///
        /// The channel will store the final result of the `OpenStream` command.
        response: oneshot::Sender<Result<()>>,
    },
    /// Indicates the swarm to broadcast the provided message.
    Broadcast {
        /// Message to broadcast.
        data: Vec<u8>,
        /// Response channel for the command.
        ///
        /// The channel will store the final result of the `OpenStream` command.
        response: oneshot::Sender<Result<()>>,
    },
    /// Indicates the swarm to shut down.
    ShutDown,
}

/// A node in the network.
///
/// The node is implemented as a manager to the [`libp2p::swarm::Swarm`]. In this case, this struct
/// acts as a receiver of commands to modify the swarm. You may consider the swarm as a resource
/// that needs to be managed asynchronously. Hence, other tasks will send commands to the swarm to
/// perform actions like: send and receive messages, dial a party, listen to an address, broadcast
/// a message, etc. Those actions may be executed asynchronously by the caller, and this struct
/// manages all of these actions and passes them into the swarm. Once the action is performed, the
/// node returns a response telling whether the command was executed successfully or not.
pub struct P2pNet {
    /// ID of the node in the network.
    id: usize,
    /// Addresses used by this node to listen to new connections.
    listen_addresses: Vec<Multiaddr>,
    /// Current stablished connections.
    streams: Arc<DashMap<PeerId, Arc<Mutex<Stream>>>>,
    /// Map of integer IDs to network IDs
    peer_ids: Arc<DashMap<usize, PeerId>>,
    /// Stats for the network.
    stats: Arc<NetIoStats>,
    /// Sender for commands to the swarm.
    sender_swarm_commands: Sender<SwarmCommand>,
    /// Keypair of the node
    key_pair: Keypair,
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
        addresses: Vec<(PeerId, usize, Vec<Multiaddr>)>,
        received_broadcasts: Sender<Message>,
        network_ready: Arc<Notify>,
        n_nodes: usize,
    ) -> Result<Self> {
        let peer_id = PeerId::from(config.keypair.public());

        let (sender_swarm_commands, mut receiver_swarm_commands) =
            tokio::sync::mpsc::channel::<SwarmCommand>(64);

        let peer_ids = DashMap::new();
        peer_ids.insert(party_idx, peer_id);

        let peer_ids = Arc::new(peer_ids);

        let gossipsub_config = ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(20))
            .validation_mode(ValidationMode::Strict)
            .build()?;

        let mut swarm = SwarmBuilder::with_existing_identity(config.keypair.clone())
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
                config.with_idle_connection_timeout(MAXIMUM_IDLE_CONNECTION_TIMEOUT)
            })
            .build();

        let subscribed_first_time = swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&IdentTopic::new(DEFAULT_TOPIC_GOSSIPSUB))?;
        if !subscribed_first_time {
            warn!(
                local_id = party_idx,
                topic = DEFAULT_TOPIC_GOSSIPSUB,
                "The peer was already subscribed to the topic"
            );
        }

        let streams = Arc::new(DashMap::new());
        let streams_for_listener = Arc::clone(&streams);
        let streams_for_swarm_tasks = Arc::clone(&streams);
        let broadcast_channel_for_swarm_tasks = received_broadcasts.clone();
        let peer_ids_for_event_handler = peer_ids.clone();

        // Handle incoming stream connections.
        let mut incoming_streams_handler = swarm
            .behaviour()
            .stream
            .new_control()
            .accept(StreamProtocol::new(AJAX_PROTOCOL_PREFIX))?;

        let (tx_connected_parties, mut rx_connected_parties) = mpsc::channel(100);
        let tx_connected_parties_listener = tx_connected_parties.clone();
        tokio::spawn(async move {
            info!(local_id = party_idx, "Listening for incoming streams");
            while let Some((peer, stream)) = incoming_streams_handler.next().await {
                let streams_clone = streams_for_listener.clone();
                let tx_connected_parties_listener_clone = tx_connected_parties_listener.clone();

                // Spawn a new task to handle the new stream ACK response.
                tokio::spawn(async move {
                    let stream_ref = Arc::new(Mutex::new(stream));
                    let stream_for_ack = stream_ref.clone();

                    // We first add the stream to the database, and then we send an ACK message
                    // to tell the other side of the channel that this party has already added
                    // the stream to its internal database.

                    // Add the new stream to the database.
                    {
                        info!(
                            local_id = party_idx,
                            "Storing stream in the database with remote peer {peer}"
                        );
                        if streams_clone.insert(peer, stream_ref).is_some() {
                            error!(local_id = party_idx, "The stream was already present in the database. The old stream will be dropped.");
                        }
                    }

                    // Send an ACK signal to confirm that the stream was added to the database.
                    {
                        info!(
                            local_id = party_idx,
                            "Trying to send ACK message for peer {peer}"
                        );
                        let mut stream_mutex = stream_for_ack.lock().await;

                        match stream_mutex.write_all(&[ACK_BYTE]).await {
                            Ok(()) => {
                                info!(
                                    local_id = party_idx,
                                    "ACK signal sent successfully to peer {peer}"
                                );
                            }
                            Err(e) => {
                                error!(local_id = party_idx, "Error sending ACK signal: {e:?}");
                                return;
                            }
                        }
                        match stream_mutex.flush().await {
                            Ok(()) => info!(
                                local_id = party_idx,
                                "ACK signal flushed successfully to peer {peer}"
                            ),
                            Err(e) => {
                                error!(local_id = party_idx, "Error flushing ACK signal: {e:?}");
                                return;
                            }
                        }

                        if let Err(e) = tx_connected_parties_listener_clone.send(peer).await {
                            error!(
                                local_id = party_idx,
                                "Error sending successful connection signal: {e:?}"
                            );
                        }
                    }
                });
            }
            error!(
                local_id = party_idx,
                "The stream handler was closed unexpectedly"
            );
        });

        // Start a loop to listen for commands to the swarm.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(command) = receiver_swarm_commands.recv() => {
                        match command {
                            SwarmCommand::Listen { address, response } => {
                                match swarm.listen_on(address) {
                                    Ok(listen_addr) => {
                                        let _ = response.send(Ok(listen_addr));
                                    }
                                    Err(err) => {
                                        let _ = response.send(Err(Transport(err)));
                                    }
                                }
                            }
                            SwarmCommand::Dial { peer_id, addresses, response } => {
                                let result = swarm.dial(
                                    DialOpts::peer_id(peer_id)
                                        .addresses(addresses.clone())
                                        .condition(PeerCondition::DisconnectedAndNotDialing)
                                        .build()
                                );
                                match result {
                                    Ok(_) => { let _ = response.send(Ok(())); }
                                    Err(e) => { let _ = response.send(Err(Error::Dial(e))); }
                                };
                            }
                            SwarmCommand::OpenStream { peer_id, response } => {
                                let mut control = swarm.behaviour().stream.new_control();
                                let streams_clone = streams_for_swarm_tasks.clone();
                                let tx_connected_parties_requester = tx_connected_parties.clone();
                                tokio::spawn(
                                    async move {
                                        let protocol = StreamProtocol::new(AJAX_PROTOCOL_PREFIX);
                                        let mut connection_attempts = 0;
                                        loop {
                                            match control.open_stream(peer_id, protocol.clone()).await {
                                                Ok(mut stream) => {
                                                    // Wait to receive the ACK signal from the other side of the channel.
                                                    // The ACK signal confirms that the party on the other side of the
                                                    // stream has added this stream to its internal database.
                                                    let mut ack_buffer = [0u8; 1];
                                                    info!(local_id = party_idx, "Waiting for ACK signal from peer {peer_id} to open a stream.");
                                                    if let Err(e) = stream.read_exact(&mut ack_buffer).await {
                                                        if connection_attempts >= MAX_CONNECTION_ATTEMPTS {
                                                            error!(local_id = party_idx, "Error receiving ACK signal from peer {peer_id}: {e:?}");
                                                            let _ = response.send(Err(Error::StreamHandshakeError(e)));
                                                            return;
                                                        } else {
                                                            warn!(local_id = party_idx, "Error receiving ACK signal from peer {peer_id}: {e:?}. Retrying... ({connection_attempts}/{MAX_CONNECTION_ATTEMPTS})");
                                                            connection_attempts += 1;
                                                        }
                                                    } else if ack_buffer[0] != ACK_BYTE {
                                                        if connection_attempts >= MAX_CONNECTION_ATTEMPTS {
                                                            let _ = response.send(Err(Error::InvalidAckSignal));
                                                            error!(local_id = party_idx, "Received different byte to the ACK signal. Retrying... ({connection_attempts}/{MAX_CONNECTION_ATTEMPTS})");
                                                            return;
                                                        } else {
                                                            warn!(local_id = party_idx, "Received different byte to the ACK signal");
                                                            connection_attempts += 1;
                                                        }
                                                    } else {
                                                        // Show a message in case that the stream is already there.
                                                        if streams_clone.insert(peer_id, Arc::new(Mutex::new(stream))).is_some() {
                                                            error!(local_id = party_idx, "A stream with {peer_id} was already present in the database. The old stream will be dropped.")
                                                        }
                                                        info!(local_id = party_idx, "Correct ACK received. Peer successfully opened a stream with peer {peer_id}.");
                                                        let _ = response.send(Ok(()));
                                                        let _ = tx_connected_parties_requester.send(peer_id).await;
                                                        return;
                                                    }
                                                }
                                                Err(error) => {
                                                    if connection_attempts >= MAX_CONNECTION_ATTEMPTS {
                                                        error!(local_id = party_idx, "Error opening a stream with peer {peer_id}: {error:?}.");
                                                        let _ = response.send(Err(Error::OpenStreamError(error)));
                                                        return;
                                                    } else {
                                                        connection_attempts += 1;
                                                        warn!(local_id = party_idx, "Error opening a stream with peer {peer_id}: {error:?}. Retrying... ({connection_attempts}/{MAX_CONNECTION_ATTEMPTS})");
                                                    }
                                                }
                                            }
                                            tokio::time::sleep(WAITING_TIME_BETWEEN_ITERATIONS).await;
                                        }
                                    }
                                );
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
                        debug!(local_id = party_idx, "Peer received an incomming event to handle: {event:?}");
                        // Spawn the event handler in a new task in case that handling the event is
                        // a very heavy task.
                        tokio::spawn(
                            Self::handle_swarm_event(
                                party_idx,
                                event,
                                broadcast_channel_for_swarm_tasks.clone(),
                                Arc::clone(&streams_for_swarm_tasks),
                                Arc::clone(&peer_ids_for_event_handler),
                            )
                        );
                    }
                }
            }
        });

        // Creates the node.
        let node = Self {
            id: party_idx,
            peer_ids,
            listen_addresses: config.listen_addresses,
            streams,
            stats: Arc::new(NetIoStats::default()),
            sender_swarm_commands,
            key_pair: config.keypair,
        };

        // We instruct the node to listen to the given listen addresses.
        node.listen().await?;

        // The node dials other nodes in the network and tries to connect to them.
        node.dial(addresses.clone()).await?;

        // Wait that everyone is dialed correctly.

        // Now that we are connected using the dial, we open streams between the parties.
        for (peer_id, _, _) in addresses {
            if node.encoded_peer_id() < peer_id {
                node.open_stream(peer_id).await?
            }
        }

        // This task checks that all the parties are connected. Once the parties are connected, it
        // notifies the external tasks that the network is ready to be used. Basically, this mechanism
        // pauses the execution of the node until all parties are connected and ready to communicate.
        tokio::spawn(async move {
            let mut parties_ready = HashSet::new();
            while let Some(party_id) = rx_connected_parties.recv().await {
                parties_ready.insert(party_id);

                // Check if all parties are ready
                if parties_ready.len() == n_nodes - 1 {
                    info!(
                        local_id = party_idx,
                        "All parties are connected and ready to communicate"
                    );
                    network_ready.notify_one();
                    return;
                }
            }
        });

        Ok(node)
    }

    /// Listen for connections on the given set of addresses.
    pub async fn listen(&self) -> Result<()> {
        for address in &self.listen_addresses {
            let (tx, rx) = oneshot::channel();
            self.sender_swarm_commands
                .send(SwarmCommand::Listen {
                    address: address.clone(),
                    response: tx,
                })
                .await?;
            rx.await??;
            info!("Node {} listening on {}", self.id, address);
        }
        Ok(())
    }

    /// Dials the parties given in the `addresses` vector.
    ///
    /// # Warning
    ///
    /// Dialing another party does NOT open a raw stream, it just opens a connection.
    /// **You must spawn a stream manually** using the [`Self::open_stream`] function once
    /// you have connected successfully using this method.
    pub async fn dial(&self, addresses: Vec<(PeerId, usize, Vec<Multiaddr>)>) -> Result<()> {
        let own_id = self.id;
        for (peer_id_encoded, peer_id, addresses) in addresses {
            if own_id < peer_id {
                let (tx, rx) = oneshot::channel();
                self.sender_swarm_commands
                    .send(SwarmCommand::Dial {
                        peer_id: peer_id_encoded,
                        addresses,
                        response: tx,
                    })
                    .await?;
                rx.await??;
                info!("Peer {own_id} dialed {peer_id} successfully");
            }
            self.peer_ids.insert(peer_id, peer_id_encoded);
        }
        Ok(())
    }

    /// Opens a stream with a remote party.
    ///
    /// # Warning
    ///
    /// This method should be executed once the party is connected using [`Self::dial`].
    pub async fn open_stream(&self, peer_id: PeerId) -> Result<()> {
        let (command_sender, command_receiver) = oneshot::channel();
        self.sender_swarm_commands
            .send(SwarmCommand::OpenStream {
                peer_id,
                response: command_sender,
            })
            .await?;
        command_receiver.await??;
        Ok(())
    }

    /// Handles a swarm event in the network.
    async fn handle_swarm_event(
        own_id: usize,
        event: SwarmEvent<BehaviourEvent>,
        received_broadcasts: Sender<Message>,
        streams: Arc<DashMap<PeerId, Arc<Mutex<Stream>>>>,
        peer_ids: Arc<DashMap<usize, PeerId>>,
    ) -> Result<()> {
        match event {
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Subscribed {
                peer_id,
                topic,
            })) => {
                info!("Gossipsub: The peer ID {peer_id} successfully subscribed to the topic \"{topic}\"");
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
                info!("Peer {own_id} established a connection with peer {peer_id}");
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                warn!("Connection closed with peer {peer_id:?}");
                streams.remove(&peer_id);
                let keys: Vec<_> = peer_ids
                    .iter()
                    .filter(|entry| entry.value().eq(&peer_id))
                    .map(|entry| entry.key().clone())
                    .collect();
                for key in keys {
                    peer_ids.remove(&key);
                }
            }
            SwarmEvent::IncomingConnection {
                local_addr,
                send_back_addr,
                connection_id,
            } => {
                info!("Incoming connection from {send_back_addr} to peer {local_addr} with connection ID {connection_id}");
            }
            // The following errors are not inherently bad but need attention.
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

        let peer_id_encoded = {
            *self
                .peer_ids
                .get(&peer_id)
                .ok_or(Error::PeerIdNotInDatabase(peer_id))?
        };

        let bytes = {
            let individual_stream_mutex = {
                self.streams
                    .get_mut(&peer_id_encoded)
                    .ok_or(Error::StreamNotInDatabase(peer_id))?
                    .clone()
            };

            let mut stream = individual_stream_mutex.lock().await;

            stream.write(data).await.map_err(|error| Error::SendError {
                receiver_id: peer_id_encoded,
                error,
            })?
        };
        self.stats.update_send(bytes, start.elapsed());
        Ok(bytes)
    }

    /// Receive a message from the peer.
    pub async fn recv(&self, peer_id: usize, buffer: &mut [u8]) -> Result<usize> {
        let own_id = self.id;
        info!("Peer {own_id} is receiving a message from {peer_id}");
        let start = Instant::now();
        let peer_id_encoded = {
            *self
                .peer_ids
                .get(&peer_id)
                .ok_or(Error::PeerIdNotInDatabase(peer_id))?
        };

        let bytes_read = {
            let individual_stream_mutex = {
                self.streams
                    .get_mut(&peer_id_encoded)
                    .ok_or(Error::StreamNotInDatabase(peer_id))?
                    .clone()
            };

            let mut stream = individual_stream_mutex.lock().await;
            stream
                .read(buffer)
                .await
                .map_err(|error| Error::RecvError {
                    sender_id: peer_id,
                    error,
                })?
        };

        self.stats.update_recv(bytes_read, start.elapsed());
        Ok(bytes_read)
    }

    /// Broadcast a message to all parties using the gossipsub protocol.
    pub async fn broadcast(&self, data: &[u8]) -> Result<()> {
        let own_id = self.id;
        info!("Broadcasting message from {own_id} using gossipsub");
        let init_time = Instant::now();
        let (cmd_sender, cmd_receiver) = tokio::sync::oneshot::channel();
        self.sender_swarm_commands
            .send(SwarmCommand::Broadcast {
                data: data.to_vec(),
                response: cmd_sender,
            })
            .await?;
        cmd_receiver.await??;
        self.stats.update_send(data.len(), init_time.elapsed());
        Ok(())
    }

    /// Broadcast a message by sending the message to all the parties using the raw streams.
    pub async fn raw_broadcast(&mut self, data: &[u8]) -> Result<()> {
        let own_id = self.id;
        info!("Broadcasting message from {own_id} using raw broadcasting");
        for mut entry in self.streams.iter_mut() {
            let data = data.to_vec();
            let peer_id = entry.key().clone();
            let stats = self.stats.clone();
            let init_time = Instant::now();
            let bytes_sent =
                entry
                    .value_mut()
                    .lock()
                    .await
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
        let peer_id_encoded = {
            *self
                .peer_ids
                .get(&peer_id)
                .ok_or(Error::StreamNotInDatabase(peer_id))?
        };

        let individual_stream_mutex = {
            self.streams
                .get_mut(&peer_id_encoded)
                .ok_or(Error::StreamNotInDatabase(peer_id))?
                .clone()
        };

        let mut individual_stream_mutex = individual_stream_mutex.lock().await;
        individual_stream_mutex.flush().await?;
        Ok(())
    }

    /// Flush all the streams in the network.
    pub async fn flush_all(&self) -> Result<()> {
        let streams: Vec<Arc<Mutex<Stream>>> = {
            self.streams
                .iter()
                .map(|entry| entry.value().clone())
                .collect()
        };
        for stream in streams {
            let mut stream_mutex = stream.lock().await;
            stream_mutex.flush().await.map_err(Error::FlushError)?;
        }
        Ok(())
    }

    /// Returns the ID of the peer in the context of [`libp2p`].
    ///
    /// In this case, a node in the network has two ID versions: one is for the MPC protocol
    /// represented as an [`usize`] and the other is for the [`libp2p`] library represented as
    /// a [`PeerId`]. These two ID should match all the time during the execution of the protocol.
    pub fn encoded_peer_id(&self) -> PeerId {
        PeerId::from(self.key_pair.public())
    }
}
