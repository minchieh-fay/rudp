//! # Rudpbase
//! 
//! A reliable UDP transmission library like TCP, focused on preventing packet loss without complex ordering.
//! 
//! ## Features
//! 
//! - **Reliable transmission**: Ensures no packet loss through acknowledgment and retransmission
//! - **High performance**: 9-byte protocol header, no encryption overhead
//! - **Simple design**: Focus on reliability without packet ordering
//! - **Security**: 4-byte security code with salt protection
//! - **High bandwidth support**: 4-byte sequence numbers support up to 4.2 billion packets
//! - **Memory pool**: Zero-copy buffer management for optimal performance
//! 
//! ## Usage
//! 
//! ### 推荐使用方式（内存池，零拷贝）
//! 
//! ```rust
//! use rudpbase::{Rudpbase, Buffer, RBuffer};
//! use std::net::SocketAddr;
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut rudp = Rudpbase::new("127.0.0.1:8080".parse()?).await?;
//!     
//!     // 发送数据（推荐方式：使用内存池）
//!     let mut buffer = rudp.get_buffer()?;
//!     let data = b"Hello, Rudpbase!";
//!     buffer.data_mut()[..data.len()].copy_from_slice(data);
//!     buffer.set_data_len(data.len())?;
//!     rudp.write(buffer, "127.0.0.1:8081".parse()?).await?;
//!     
//!     // 接收数据
//!     if let Some(rbuffer) = rudp.poll_read().await {
//!         match rbuffer.result {
//!             Ok(buffer) => {
//!                 println!("Received from {}: {:?}", rbuffer.from, buffer.buffer);
//!             }
//!             Err(e) => {
//!                 println!("Receive error: {}", e);
//!             }
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//! 
//! ### 传统使用方式（兼容性API）
//! 
//! ```rust
//! use rudpbase::{Rudpbase, Buffer, RBuffer};
//! use std::net::SocketAddr;
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut rudp = Rudpbase::new("127.0.0.1:8080".parse()?).await?;
//!     
//!     // 发送数据（传统方式，需要数据拷贝）
//!     let data = b"Hello, Rudpbase!";
//!     rudp.write_bytes(data, "127.0.0.1:8081".parse()?).await?;
//!     
//!     Ok(())
//! }
//! ```

use std::net::SocketAddr;

pub mod core;
pub mod protocol;
pub mod error;
pub mod stats;
pub mod security;
pub mod buffer_pool;

pub use core::{Rudpbase, Buffer, RBuffer, RPooledBuffer};
pub use error::{RudpError, ConnectionError};
pub use stats::{ConnectionStatus, ConnectionStats, RttStats};
pub use protocol::{PacketType, PROTOCOL_HEADER_SIZE};
pub use security::SecurityCode;
pub use buffer_pool::*;

/// Create a new Rudpbase instance
/// 
/// # Arguments
/// 
/// * `local_addr` - Local address to bind to
/// 
/// # Returns
/// 
/// Returns a Result containing the Rudpbase instance or an error
pub async fn new_rudpbase(local_addr: SocketAddr) -> Result<Rudpbase, RudpError> {
    Rudpbase::new(local_addr).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_functionality() {
        // Basic tests will be implemented here
        assert!(true);
    }
} 