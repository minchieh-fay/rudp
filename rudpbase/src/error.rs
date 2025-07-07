use thiserror::Error;
use std::net::SocketAddr;

/// Main error type for Rudpbase operations
#[derive(Error, Debug)]
pub enum RudpError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Connection error: {0}")]
    Connection(#[from] ConnectionError),
    
    #[error("Protocol error: {message}")]
    Protocol { message: String },
    
    #[error("Security error: invalid security code")]
    Security,
    
    #[error("Buffer too large: {size} bytes (max: {max})")]
    BufferTooLarge { size: usize, max: usize },
    
    #[error("Internal error")]
    InternalError,
    
    #[error("Packet too small: {size} bytes (min: {min})")]
    PacketTooSmall { size: usize, min: usize },
    
    #[error("Timeout error")]
    Timeout,
}

/// Connection-specific errors
#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("Connection to {addr} is dead")]
    Dead { addr: SocketAddr },
    
    #[error("Connection to {addr} timed out")]
    Timeout { addr: SocketAddr },
    
    #[error("Maximum retries exceeded for {addr}")]
    MaxRetriesExceeded { addr: SocketAddr },
    
    #[error("Connection to {addr} is degraded")]
    Degraded { addr: SocketAddr },
    
    #[error("Connection reset by peer {addr}")]
    Reset { addr: SocketAddr },
}

/// Error severity levels for handling different types of errors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorSeverity {
    /// Recoverable errors like single packet loss
    Recoverable,
    /// Performance degradation like high packet loss rate
    Degraded,
    /// Critical errors like connection failure
    Critical,
}

impl RudpError {
    /// Get the severity level of this error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            RudpError::Io(_) => ErrorSeverity::Recoverable,
            RudpError::Connection(conn_err) => conn_err.severity(),
            RudpError::Protocol { .. } => ErrorSeverity::Recoverable,
            RudpError::Security => ErrorSeverity::Critical,
            RudpError::BufferTooLarge { .. } => ErrorSeverity::Recoverable,
            RudpError::InternalError => ErrorSeverity::Critical,
            RudpError::PacketTooSmall { .. } => ErrorSeverity::Recoverable,
            RudpError::Timeout => ErrorSeverity::Degraded,
        }
    }
}

impl ConnectionError {
    /// Get the severity level of this connection error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            ConnectionError::Dead { .. } => ErrorSeverity::Critical,
            ConnectionError::Timeout { .. } => ErrorSeverity::Degraded,
            ConnectionError::MaxRetriesExceeded { .. } => ErrorSeverity::Critical,
            ConnectionError::Degraded { .. } => ErrorSeverity::Degraded,
            ConnectionError::Reset { .. } => ErrorSeverity::Critical,
        }
    }
} 