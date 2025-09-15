//! Error types for network I/O operations.

use thiserror::Error;

#[derive(Debug, Error)]
/// Error types for network I/O operations.
pub enum NetIoError {
    /// An I/O error occurred.
    #[error("error in IO: {0}")]
    IoError(#[from] std::io::Error),
    /// Failed to acquire a mutex lock.
    #[error("error acquiring the mutex: {0}")]
    MutexLockFailed(String),
    /// The requested connection was not found.
    #[error("connection not found with peer {0}")]
    ConnectionNotFound(u32),
    /// A timeout occurred.
    #[error("a time out error occurred: {0}")]
    Timeout(String),
}

/// Type alias for network I/O results.
pub type NetIoResult<T> = std::result::Result<T, NetIoError>;
