#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![deny(missing_docs)]
//! This crate provides network communication for MPC protocols.

pub mod error;
pub mod netio;

/// Implementation of the networking functionality using the crate [`libp2p`].
pub mod p2p;

const SEND_BUFFER_SIZE: usize = 1024 * 1024;
const RECV_BUFFER_SIZE: usize = 1024 * 1024;

/// Network IO trait
pub trait IO {
    /// Get the party id of the current party.
    fn party_id(&self) -> u32;

    /// Get the number of parties in the network.
    fn party_num(&self) -> u32;

    /// Send data to a party.
    fn send(&self, party_id: u32, data: &[u8]) -> Result<(), error::NetIoError>;

    /// Receive data from a party.
    fn recv(&self, party_id: u32, buf: &mut [u8]) -> Result<usize, error::NetIoError>;

    /// Broadcast data to all parties.
    fn broadcast(&self, data: &[u8]) -> Result<(), error::NetIoError>;

    /// Flush the send buffer.
    fn flush(&self, party_id: u32) -> Result<(), error::NetIoError>;

    /// Flush all send buffers.
    fn flush_all(&self) -> Result<(), error::NetIoError>;
}
