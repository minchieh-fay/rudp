use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time;

use crate::error::RudpError;
use crate::stats::{ConnectionStats, ConnectionStatus, RttStats, ConnectionState};
use crate::protocol::{PacketType, RawPacket, PingPacket, DataAckPacket, DataNackPacket, MAX_BUFFER_SIZE};
use crate::security::SecurityCode;
use crate::buffer_pool::{SharedBufferPool, PooledBuffer, DEFAULT_INITIAL_CAPACITY};

/// 接收数据结构
pub struct ReceivedData {
    /// Data source address
    pub from: SocketAddr,
    /// Reception result
    pub result: Result<PooledBuffer, RudpError>,
}

/// Pending packet structure for retransmission
#[derive(Debug, Clone)]
struct PendingPacket {
    /// Packet data
    data: Vec<u8>,
    /// Send timestamp
    send_time: Instant,
    /// Retry count
    retry_count: u8,
    /// Current RTO for this packet
    rto: Duration,
}

impl PendingPacket {
    fn new(data: Vec<u8>, rto: Duration) -> Self {
        Self {
            data,
            send_time: Instant::now(),
            retry_count: 0,
            rto,
        }
    }

    fn should_retry(&self, now: Instant) -> bool {
        now.duration_since(self.send_time) >= self.rto
    }

    fn retry(&mut self, rto: Duration) {
        self.retry_count += 1;
        self.send_time = Instant::now();
        self.rto = rto;
    }
}

/// Main Rudpbase structure
/// 
/// Note: This structure is NOT thread-safe. It should be used within a single thread
/// or protected by appropriate synchronization mechanisms (e.g., Mutex, RwLock).
/// For multi-threaded usage, consider wrapping in Arc<Mutex<Rudpbase>>.
pub struct Rudpbase {
    /// UDP socket
    socket: UdpSocket,
    /// Send buffer: [target_addr][seq] -> (buffer, send_time, retry_count)
    send_buffer: HashMap<SocketAddr, HashMap<u32, PendingPacket>>,
    /// Receive buffer: [source_addr] -> received seq set
    recv_acks: HashMap<SocketAddr, HashSet<u32>>,
    /// Next sequence number for each target
    next_seq: HashMap<SocketAddr, u32>,
    /// RTT statistics for each connection
    rtt_stats: HashMap<SocketAddr, RttStats>,
    /// Connection statistics
    connection_stats: HashMap<SocketAddr, ConnectionStats>,
    /// Connection states
    connection_states: HashMap<SocketAddr, ConnectionState>,
    /// Pending ACKs to be sent
    pending_acks: HashMap<SocketAddr, Vec<u32>>,
    /// Last cleanup time
    last_cleanup: Instant,
    /// Shared buffer pool for memory management
    buffer_pool: SharedBufferPool,
}

impl Rudpbase {
    /// Create a new Rudpbase instance
    pub async fn new(local_addr: SocketAddr) -> Result<Self, RudpError> {
        let socket = UdpSocket::bind(local_addr).await?;
        
        // 创建内存池并自动预热
        let buffer_pool = SharedBufferPool::default();
        // 预热内存池，预分配一些buffer以提高性能
        buffer_pool.warmup(DEFAULT_INITIAL_CAPACITY)?;
        
        Ok(Self {
            socket,
            send_buffer: HashMap::new(),
            recv_acks: HashMap::new(),
            next_seq: HashMap::new(),
            rtt_stats: HashMap::new(),
            connection_stats: HashMap::new(),
            connection_states: HashMap::new(),
            pending_acks: HashMap::new(),
            last_cleanup: Instant::now(),
            buffer_pool,
        })
    }

    /// Close the Rudpbase instance and clean up all resources
    pub async fn close(&mut self) {
        // Send close packets to all active connections
        let connections: Vec<SocketAddr> = self.connection_states.keys().cloned().collect();
        
        for addr in connections {
            let _ = self.send_close_packet(addr).await;
        }

        // Clear all internal state
        self.send_buffer.clear();
        self.recv_acks.clear();
        self.next_seq.clear();
        self.rtt_stats.clear();
        self.connection_stats.clear();
        self.connection_states.clear();
        self.pending_acks.clear();
    }

    /// 获取一个用于写入的buffer
    /// 
    /// 从内存池中获取一个预分配的buffer，用户可以直接写入数据区域
    /// 协议头空间已预留，用户无需关心
    /// 
    /// # 返回
    /// - `Ok(PooledBuffer)`: 可用的buffer，用户可以写入数据区
    /// - `Err(RudpError)`: 获取失败
    /// 
    /// # 使用示例
    /// ```rust,no_run
    /// use rudpbase::Rudpbase;
    /// 
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut rudp = Rudpbase::new("127.0.0.1:8080".parse().unwrap()).await?;
    ///     let mut buffer = rudp.get_buffer()?;
    ///     
    ///     // 写入用户数据
    ///     let data = b"Hello, world!";
    ///     buffer.data_mut()[..data.len()].copy_from_slice(data);
    ///     buffer.set_data_len(data.len())?;
    ///     
    ///     // 发送数据
    ///     let target_addr = "127.0.0.1:8081".parse().unwrap();
    ///     rudp.send(buffer, target_addr).await?;
    ///     Ok(())
    /// }
    /// ```
    pub fn get_buffer(&self) -> Result<PooledBuffer, RudpError> {
        self.buffer_pool.get_write_buffer()
    }

    /// 发送数据
    /// 
    /// 这是零拷贝的发送方法，直接使用预分配的buffer
    /// 
    /// # 参数
    /// - `buffer`: 包含数据的内存池buffer
    /// - `target`: 目标地址
    /// 
    /// # 返回
    /// - `Ok(())`: 发送成功
    /// - `Err(RudpError)`: 发送失败
    /// 
    /// # 使用示例
    /// ```rust,no_run
    /// use rudpbase::Rudpbase;
    /// 
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut rudp = Rudpbase::new("127.0.0.1:8080".parse().unwrap()).await?;
    ///     let mut buffer = rudp.get_buffer()?;
    ///     
    ///     // 填充数据
    ///     let data = b"Hello, world!";
    ///     buffer.data_mut()[..data.len()].copy_from_slice(data);
    ///     buffer.set_data_len(data.len())?;
    ///     
    ///     // 发送
    ///     let target_addr = "127.0.0.1:8081".parse().unwrap();
    ///     rudp.send(buffer, target_addr).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn send(&mut self, mut buffer: PooledBuffer, target: SocketAddr) -> Result<(), RudpError> {
        let seq = self.get_next_seq(target);
        
        // Fill protocol header
        buffer.fill_protocol_header(PacketType::Data, seq)?;
        
        // Store for retransmission
        let rto = self.rtt_stats.get(&target)
            .map(|stats| stats.rto)
            .unwrap_or(Duration::from_millis(200));
        
        let pending_packet = PendingPacket::new(buffer.full_data().to_vec(), rto);
        self.send_buffer.entry(target).or_insert_with(HashMap::new).insert(seq, pending_packet);
        
        // Send packet
        self.socket.send_to(buffer.full_data(), target).await?;
        
        // Update statistics
        self.connection_stats.entry(target).or_insert_with(ConnectionStats::new).record_packet_sent();
        
        // Update connection state
        self.connection_states.entry(target).or_insert_with(ConnectionState::new).update_activity();
        
        Ok(())
    }

    /// 获取内存池统计信息
    pub fn get_buffer_pool_stats(&self) -> Result<crate::buffer_pool::PoolStats, RudpError> {
        self.buffer_pool.stats()
    }

    /// 接收数据
    /// 
    /// 从内存池获取buffer来存储接收的数据，实现零拷贝接收
    /// 
    /// **重要说明**：
    /// - 只返回Data包（PacketType::Data = 2）给上层应用
    /// - 控制包（ACK、NACK、PING等）在库内部自动处理，不会返回给上层
    /// - 库会自动处理重传、心跳、连接管理等逻辑
    /// 
    /// # 返回
    /// - `Some(ReceivedData)`: 接收到用户数据包，包含发送方地址和池化buffer
    /// - `None`: 没有用户数据或超时（控制包已在内部处理）
    /// 
    /// # 使用示例
    /// ```rust,no_run
    /// use rudpbase::Rudpbase;
    /// 
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut rudp = Rudpbase::new("127.0.0.1:8080".parse().unwrap()).await?;
    ///     
    ///     loop {
    ///         // 维护连接（处理重传、心跳等）
    ///         rudp.tick().await;
    ///         
    ///         // 接收用户数据
    ///         if let Some(received) = rudp.recv().await {
    ///             match received.result {
    ///                 Ok(buffer) => {
    ///                     println!("Received user data from {}: {:?}", 
    ///                         received.from, buffer.data());
    ///                     // buffer会在离开作用域时自动归还到内存池
    ///                 }
    ///                 Err(e) => {
    ///                     println!("Receive error: {}", e);
    ///                 }
    ///             }
    ///         }
    ///     }
    /// }
    /// ```
    pub async fn recv(&mut self) -> Option<ReceivedData> {
        let mut buf = [0u8; MAX_BUFFER_SIZE + 64]; // Extra space for headers
        
        match time::timeout(Duration::from_millis(1), self.socket.recv_from(&mut buf)).await {
            Ok(Ok((len, from))) => {
                let packet_data = &buf[..len];
                match self.handle_received_packet(packet_data, from).await {
                    Ok(Some(received)) => Some(received),
                    Ok(None) => None, // Control packet, no data to return
                    Err(e) => Some(ReceivedData {
                        from,
                        result: Err(e),
                    }),
                }
            }
            Ok(Err(e)) => Some(ReceivedData {
                from: "0.0.0.0:0".parse().unwrap(),
                result: Err(RudpError::Io(e)),
            }),
            Err(_) => None, // Timeout, no data received
        }
    }

    /// Maintenance function - handle retransmissions, timeouts, ACKs, etc.
    pub async fn tick(&mut self) {
        let now = Instant::now();

        // Handle retransmissions
        self.handle_retransmissions(now).await;

        // Send pending ACKs
        self.send_pending_acks().await;

        // Check connection health
        self.check_connection_health(now).await;

        // Periodic cleanup
        if now.duration_since(self.last_cleanup) > Duration::from_secs(60) {
            self.periodic_cleanup(now);
            self.last_cleanup = now;
        }
    }

    /// Get connection status
    pub fn connection_status(&self, addr: SocketAddr) -> ConnectionStatus {
        self.connection_states.get(&addr)
            .map(|state| state.status.clone())
            .unwrap_or(ConnectionStatus::Dead)
    }

    /// Get connection statistics
    pub fn get_stats(&self, addr: SocketAddr) -> Option<ConnectionStats> {
        self.connection_stats.get(&addr).cloned()
    }

    // Private helper methods

    /// 获取下一个序列号
    /// 
    /// 为每个目标地址维护独立的序列号计数器
    /// 序列号从0开始，每次调用递增1
    /// 
    /// 序列号溢出处理：
    /// - 使用 wrapping_add 安全处理 u32::MAX + 1 = 0 的情况
    /// - 序列号是连续的：65534, 65535, 0, 1... 不需要特殊处理
    fn get_next_seq(&mut self, addr: SocketAddr) -> u32 {
        // 获取或创建该地址的序列号计数器，初始值为0
        let seq = self.next_seq.entry(addr).or_insert(0);
        let current = *seq;  // 保存当前值，这是要返回的序列号
        *seq = seq.wrapping_add(1);  // 安全递增，处理溢出：u32::MAX + 1 = 0

        current  // 返回使用的序列号
    }

    /// 处理接收到的包
    /// 
    /// 内部处理所有控制包（ACK、NACK、PING等），只有Data包会返回给上层
    async fn handle_received_packet(&mut self, packet_data: &[u8], from: SocketAddr) -> Result<Option<ReceivedData>, RudpError> {
        let packet = RawPacket::parse(packet_data)?;

        // Verify security code
        if !SecurityCode::verify(packet.packet_type, packet.seq, &packet.data, packet.security_code) {
            return Err(RudpError::Security);
        }

        // Update connection activity
        if let Some(state) = self.connection_states.get_mut(&from) {
            state.update_activity();
        }

        match packet.packet_type {
            // 只有Data包返回给上层应用
            PacketType::Data => self.handle_data_packet(packet, from).await,
            
            // 以下都是控制包，在库内部处理，不暴露给上层
            PacketType::DataAck => {
                self.handle_data_ack_packet(packet, from).await;
                Ok(None) // 不返回给上层
            }
            PacketType::DataNack => {
                self.handle_data_nack_packet(packet, from).await;
                Ok(None) // 不返回给上层
            }
            PacketType::Ping => {
                self.handle_ping_packet(packet, from).await;
                Ok(None) // 不返回给上层
            }
            PacketType::PingAck => {
                self.handle_ping_ack_packet(packet, from).await;
                Ok(None) // 不返回给上层
            }
            PacketType::Close => {
                self.handle_close_packet(packet, from).await;
                Ok(None) // 不返回给上层
            }
            PacketType::CloseAck => {
                self.handle_close_ack_packet(packet, from).await;
                Ok(None) // 不返回给上层
            }
        }
    }

    /// 处理数据包
    async fn handle_data_packet(&mut self, packet: RawPacket, from: SocketAddr) -> Result<Option<ReceivedData>, RudpError> {
        let received_seqs = self.recv_acks.entry(from).or_insert_with(HashSet::new);
        
        if received_seqs.contains(&packet.seq) {
            // Duplicate packet, resend ACK
            self.send_ack(from, packet.seq).await;
            return Ok(None);
        }

        // New packet, process data
        received_seqs.insert(packet.seq);
        self.send_ack(from, packet.seq).await;

        // Update statistics
        self.connection_stats.entry(from).or_insert_with(ConnectionStats::new).record_packet_received();

        // 从内存池获取buffer并拷贝数据
        let mut buffer = self.buffer_pool.get_write_buffer()?;
        if packet.data.len() > buffer.data_mut().len() {
            return Err(RudpError::BufferTooLarge { 
                size: packet.data.len(), 
                max: buffer.data_mut().len() 
            });
        }
        
        // 将接收到的数据拷贝到内存池buffer中
        buffer.data_mut()[..packet.data.len()].copy_from_slice(&packet.data);
        buffer.set_data_len(packet.data.len())?;

        Ok(Some(ReceivedData {
            from,
            result: Ok(buffer),
        }))
    }

    async fn handle_data_ack_packet(&mut self, packet: RawPacket, from: SocketAddr) {
        if let Some(ack_packet) = DataAckPacket::deserialize(&packet.data) {
            for ack_seq in ack_packet.ack_seqs {
                if let Some(pending_packets) = self.send_buffer.get_mut(&from) {
                    if let Some(pending_packet) = pending_packets.remove(&ack_seq) {
                        // Calculate RTT and update statistics
                        let rtt = Instant::now().duration_since(pending_packet.send_time);
                        self.rtt_stats.entry(from).or_insert_with(RttStats::new).update(rtt);
                        self.connection_stats.entry(from).or_insert_with(ConnectionStats::new).update_rtt(rtt);
                    }
                }
            }
        }
    }

    async fn handle_data_nack_packet(&mut self, packet: RawPacket, from: SocketAddr) {
        if let Some(nack_packet) = DataNackPacket::deserialize(&packet.data) {
            for nack_seq in nack_packet.nack_seqs {
                if let Some(pending_packets) = self.send_buffer.get_mut(&from) {
                    if let Some(pending_packet) = pending_packets.get_mut(&nack_seq) {
                        // Immediate retransmission for NACK
                        let _ = self.socket.send_to(&pending_packet.data, from).await;
                        pending_packet.retry_count += 1;
                        pending_packet.send_time = Instant::now();
                        
                        // Update statistics
                        self.connection_stats.entry(from).or_insert_with(ConnectionStats::new).record_retransmission();
                    }
                }
            }
        }
    }

    async fn handle_ping_packet(&mut self, packet: RawPacket, from: SocketAddr) {
        // Send ping acknowledgment
        let security_code = SecurityCode::calculate(PacketType::PingAck, packet.seq, &packet.data);
        let ping_ack = RawPacket {
            packet_type: PacketType::PingAck,
            security_code,
            seq: packet.seq,
            data: packet.data, // Echo back the timestamp
        };

        let _ = self.socket.send_to(&ping_ack.serialize(), from).await;
    }

    async fn handle_ping_ack_packet(&mut self, packet: RawPacket, from: SocketAddr) {
        if let Some(ping_packet) = PingPacket::deserialize(&packet.data) {
            // Calculate RTT
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;
            if now > ping_packet.timestamp {
                let rtt = Duration::from_nanos(now - ping_packet.timestamp);
                self.rtt_stats.entry(from).or_insert_with(RttStats::new).update(rtt);
                self.connection_stats.entry(from).or_insert_with(ConnectionStats::new).update_rtt(rtt);
            }
        }

        // Mark ping as received
        if let Some(state) = self.connection_states.get_mut(&from) {
            state.mark_ping_received();
        }
    }

    async fn handle_close_packet(&mut self, packet: RawPacket, from: SocketAddr) {
        // Send close acknowledgment
        let security_code = SecurityCode::calculate(PacketType::CloseAck, packet.seq, &packet.data);
        let close_ack = RawPacket {
            packet_type: PacketType::CloseAck,
            security_code,
            seq: packet.seq,
            data: vec![],
        };

        let _ = self.socket.send_to(&close_ack.serialize(), from).await;

        // Clean up connection
        self.cleanup_connection(from);
    }

    async fn handle_close_ack_packet(&mut self, _packet: RawPacket, from: SocketAddr) {
        // Clean up connection
        self.cleanup_connection(from);
    }

    async fn send_ack(&mut self, target: SocketAddr, seq: u32) {
        self.pending_acks.entry(target).or_insert_with(Vec::new).push(seq);
    }

    async fn send_pending_acks(&mut self) {
        let targets: Vec<SocketAddr> = self.pending_acks.keys().cloned().collect();
        
        for target in targets {
            if let Some(ack_seqs) = self.pending_acks.remove(&target) {
                if !ack_seqs.is_empty() {
                    let ack_packet = DataAckPacket::new(ack_seqs);
                    let seq = self.get_next_seq(target);
                    let security_code = SecurityCode::calculate(PacketType::DataAck, seq, &ack_packet.serialize());
                    
                    let packet = RawPacket {
                        packet_type: PacketType::DataAck,
                        security_code,
                        seq,
                        data: ack_packet.serialize(),
                    };

                    let _ = self.socket.send_to(&packet.serialize(), target).await;
                }
            }
        }
    }

    async fn send_close_packet(&mut self, target: SocketAddr) -> Result<(), RudpError> {
        let seq = self.get_next_seq(target);
        let security_code = SecurityCode::calculate(PacketType::Close, seq, &[]);
        
        let packet = RawPacket {
            packet_type: PacketType::Close,
            security_code,
            seq,
            data: vec![],
        };

        self.socket.send_to(&packet.serialize(), target).await?;
        Ok(())
    }

    async fn handle_retransmissions(&mut self, now: Instant) {
        let mut to_remove = Vec::new();

        for (addr, packets) in &mut self.send_buffer {
            let mut addr_to_remove = Vec::new();
            
            for (seq, pending_packet) in packets.iter_mut() {
                if pending_packet.should_retry(now) {
                    if pending_packet.retry_count >= 5 {
                        // Max retries reached, mark for removal
                        addr_to_remove.push(*seq);
                    } else {
                        // Retry with exponential backoff
                        let new_rto = pending_packet.rto * 2;
                        pending_packet.retry(new_rto);
                        
                        let _ = self.socket.send_to(&pending_packet.data, *addr).await;
                        
                        // Update statistics
                        self.connection_stats.entry(*addr).or_insert_with(ConnectionStats::new).record_retransmission();
                    }
                }
            }
            
            // Remove failed packets
            for seq in addr_to_remove {
                packets.remove(&seq);
            }
            
            // If no packets left for this address, mark for removal
            if packets.is_empty() {
                to_remove.push(*addr);
            }
        }

        // Remove empty entries
        for addr in to_remove {
            self.send_buffer.remove(&addr);
        }
        
        // Update connection states for packet loss
        for (addr, _) in &self.send_buffer {
            if let Some(state) = self.connection_states.get_mut(addr) {
                state.mark_packet_lost();
            }
            self.connection_stats.entry(*addr).or_insert_with(ConnectionStats::new).record_packet_lost();
        }
    }

    async fn check_connection_health(&mut self, now: Instant) {
        let mut connections_to_ping = Vec::new();
        let mut connections_to_close = Vec::new();

        for (addr, state) in &self.connection_states {
            if state.should_ping(now) {
                connections_to_ping.push(*addr);
            } else if state.should_close(now) {
                connections_to_close.push(*addr);
            }
        }

        // Send ping packets
        for addr in connections_to_ping {
            let ping_packet = PingPacket::new();
            let seq = self.get_next_seq(addr);
            let security_code = SecurityCode::calculate(PacketType::Ping, seq, &ping_packet.serialize());
            
            let packet = RawPacket {
                packet_type: PacketType::Ping,
                security_code,
                seq,
                data: ping_packet.serialize(),
            };

            let _ = self.socket.send_to(&packet.serialize(), addr).await;
            
            if let Some(state) = self.connection_states.get_mut(&addr) {
                state.mark_ping_sent();
            }
        }

        // Close dead connections
        for addr in connections_to_close {
            self.cleanup_connection(addr);
        }
    }

    fn cleanup_connection(&mut self, addr: SocketAddr) {
        self.send_buffer.remove(&addr);
        self.recv_acks.remove(&addr);
        self.next_seq.remove(&addr);
        self.rtt_stats.remove(&addr);
        self.connection_stats.remove(&addr);
        self.connection_states.remove(&addr);
        self.pending_acks.remove(&addr);
    }

    fn periodic_cleanup(&mut self, now: Instant) {
        // Clean up old received sequence numbers (older than 1 hour)
        let cleanup_threshold = now - Duration::from_secs(3600);
        
        for (addr, seqs) in &mut self.recv_acks {
            // 当序列号从u32::MAX溢出回到0时，清理1小时前的ACK缓存
            // 这样可以避免新的seq=0与旧的seq=0冲突，防止新包被误认为重复包
            if let Some(&current_seq) = self.next_seq.get(addr) {
                if current_seq == 0 {
                    // 序列号刚刚溢出，清理旧的缓存
                    seqs.clear();
                }
            }
        }

        // Remove empty entries
        self.recv_acks.retain(|_, seqs| !seqs.is_empty());
    }
} 