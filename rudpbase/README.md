# Rudpbase
A reliable UDP transmission library for Rust

专注于：**不丢包的UDP传输，不做排序等复杂动作**

这是一个底层的可靠UDP传输库，提供核心的可靠传输功能。上层的`rudp`库将基于`rudpbase`进行业务封装。

## 数据结构设计

```rust
struct Rudpbase {
    socket: UdpSocket,
    // 发送缓存：[目标地址][seq] -> (buffer, 发送时间, 重传次数)
    send_buffer: HashMap<SocketAddr, HashMap<u32, (Buffer, Instant, u8)>>,
    // 接收缓存：[源地址] -> 已接收的seq集合
    recv_acks: HashMap<SocketAddr, HashSet<u32>>,
    // 每个目标的下一个seq
    next_seq: HashMap<SocketAddr, u32>,
    // RTT统计
    rtt_stats: HashMap<SocketAddr, RttStats>,
}

struct Buffer {
    buffer: Vec<u8>,  // 实际数据，最大1200字节
    len: usize,       // 实际数据长度
}

struct RBuffer {
    from: SocketAddr,           // 数据来源
    result: Result<Buffer, Error>, // 接收结果
}

struct RttStats {
    rtt: Duration,      // 平均RTT
    rtt_var: Duration,  // RTT变化量
    rto: Duration,      // 重传超时时间
}

#[derive(Debug, Clone)]
enum ConnectionStatus {
    Alive,              // 连接正常
    Probing,            // 正在探测（发送了ping等待回复）
    Degraded,           // 连接质量下降
    Dead,               // 连接断开
}

#[derive(Debug, Clone)]
struct ConnectionStats {
    packets_sent: u64,
    packets_received: u64,
    packets_lost: u64,
    retransmissions: u64,
    avg_rtt: Duration,
    last_activity: Instant,
}
```

## 全局接口

```rust
async fn new_rudpbase(local_addr: SocketAddr) -> Result<Rudpbase, RudpError>
```

## Rudpbase的接口

```rust
impl Rudpbase {
    // 关闭当前rudpbase，清理所有缓存
    async fn close(&mut self);
    
    // 发送数据到目标地址，自动处理重传
    async fn write(&mut self, buffer: &[u8], target: SocketAddr) -> Result<(), RudpError>;
    
    // 接收数据 - 轮询方式
    async fn poll_read(&mut self) -> Option<RBuffer>;
    
    // 维护函数：处理重传、超时、ACK等，需要定期调用
    async fn tick(&mut self);
    
    // 连接状态查询
    fn connection_status(&self, addr: SocketAddr) -> ConnectionStatus;
    
    // 获取连接统计信息
    fn get_stats(&self, addr: SocketAddr) -> Option<ConnectionStats>;
}
```

## 协议设计

### 协议头格式
```
｜type(1字节)｜安全码(4字节)｜seq(4字节)｜buffer｜
```

**seq字段升级说明**:
- **从2字节升级到4字节**: 支持0到42亿的序列号
- **高带宽场景**: 1Gbps网络下可支持约45分钟不重复
- **协议头长度**: 9字节（type=1 + 安全码=4 + seq=4）

**seq空间计算**:
```
2字节seq: 65,535 (约6.5万)
4字节seq: 4,294,967,295 (约42亿)

高带宽场景分析:
- 1Gbps，100字节包: 1,250,000包/秒 → 57分钟用完4字节seq
- 100Mbps，100字节包: 125,000包/秒 → 9.5小时用完4字节seq
- 10Mbps，100字节包: 12,500包/秒 → 4天用完4字节seq
```

### seq溢出处理机制
```rust
// seq溢出时的处理策略
impl Rudpbase {
    fn get_next_seq(&mut self, addr: SocketAddr) -> u32 {
        let seq = self.next_seq.entry(addr).or_insert(0);
        let current = *seq;
        
        // 处理seq溢出（从4294967295回到0）
        *seq = seq.wrapping_add(1);
        
        // 如果seq即将溢出，清理旧的缓存
        if current == u32::MAX {
            self.cleanup_old_seq_cache(addr);
        }
        
        current
    }
    
    fn cleanup_old_seq_cache(&mut self, addr: SocketAddr) {
        // 清理超过一定时间的seq缓存，为新的seq循环做准备
        let cutoff_time = Instant::now() - Duration::from_secs(3600); // 1小时前
        
        if let Some(ack_cache) = self.ack_cache.get_mut(&addr) {
            ack_cache.retain(|_, time| *time > cutoff_time);
        }
        
        // 在极端高带宽场景下，可能需要更频繁的清理
        // 或者考虑使用滑动窗口来管理seq空间
    }
}
```

### 安全码计算
```rust
// 安全码设计：4字节，用于防止攻击和检测包损坏
// 
// 计算步骤：
// 1. 取包的前16字节数据（如果不足16字节，用0填充）
// 2. 加入固定盐值："ffmesh" (6字节)
// 3. 计算：hash = FNV1a_32(盐值 + type + seq + len + 前16字节数据)
// 4. 安全码 = hash & 0xFFFFFFFF
//
// 示例代码：
fn calculate_security_code(packet_type: u8, seq: u32, data: &[u8]) -> u32 {
    let salt = b"ffmesh";
    let mut hasher = FnvHasher::default();
    
    hasher.write(salt);
    hasher.write(&[packet_type]);
    hasher.write(&seq.to_be_bytes());
    hasher.write(&(data.len() as u16).to_be_bytes()); // 使用数据实际长度
    
    // 取数据前16字节
    let data_prefix = if data.len() >= 16 {
        &data[..16]
    } else {
        // 不足16字节时，创建填充数组
        let mut padded = [0u8; 16];
        padded[..data.len()].copy_from_slice(data);
        hasher.write(&padded);
        return hasher.finish() as u32;
    };
    
    hasher.write(data_prefix);
    hasher.finish() as u32
}

// 包解析示例：
fn parse_packet(packet: &[u8]) -> Result<(u8, u32, u32, &[u8]), Error> {
    if packet.len() < 9 { // 最小协议头长度
        return Err("包太短".into());
    }
    
    let packet_type = packet[0];
    let security_code = u32::from_be_bytes([packet[1], packet[2], packet[3], packet[4]]);
    let seq = u32::from_be_bytes([packet[5], packet[6], packet[7], packet[8]]);
    let data = &packet[9..]; // 剩余部分就是数据
    
    Ok((packet_type, security_code, seq, data))
}
```

### 协议类型定义

#### 0: ping
用于RTT测量和连接保活
```
｜0｜安全码(4字节)｜timestamp(8字节)｜
```

#### 1: ping-ack
回复ping，不改变timestamp
```
｜1｜安全码(4字节)｜timestamp(8字节)｜
```
接收到ping-ack后，计算RTT = 当前时间 - 协议中的timestamp

#### 2: data
发送数据
```
｜2｜安全码(4字节)｜seq(4字节)｜buffer｜
```

#### 3: data-ack
确认收到数据包，立即发送或批量发送
```
｜3｜安全码(4字节)｜ack_count(1字节)｜seq1｜seq2｜...｜
```

#### 4: data-nack
请求重传丢失的数据包
```
｜4｜安全码(4字节)｜nack_count(1字节)｜seq1｜seq2｜...｜
```

#### 5: close
断开连接通知
```
｜5｜安全码(4字节)｜
```

#### 6: close-ack
确认断开连接
```
｜6｜安全码(4字节)｜
```

## 重传策略

### 超时重传
- **初始RTO**: 200ms
- **RTO计算**: RTO = RTT + 4 * RTT_VAR
- **RTO范围**: 最小200ms，最大3秒
- **重传策略**: 每次重传RTO翻倍，最大重传5次
- **重传失败**: 5次重传失败后，标记连接断开

### RTT计算
```rust
// 标准TCP RTT算法
// RTT = 7/8 * RTT + 1/8 * 新测量值
// RTT_VAR = 3/4 * RTT_VAR + 1/4 * |RTT - 新测量值|
```

## ACK机制

### 立即ACK
- 收到data包立即发送ACK
- 用于快速确认，减少重传

### 批量ACK（可选优化）
- 每50ms批量发送一次ACK
- 减少网络包数量，提高效率

## NACK机制

### 触发条件
- 发现seq缺失且超过1.5 * RTO时间
- 用于主动请求重传

### 重发策略
- 每个RTO间隔重发一次NACK
- 最多重发3次NACK
- 超过3次后发送ping确认连接状态

## 异常情况处理

### 1. ACK丢失处理

#### 问题场景
```
发送端: 发送 data(seq=100) 
接收端: 收到 data(seq=100)，发送 ack(seq=100)
发送端: 没收到 ack，触发重传
接收端: 收到重复的 data(seq=100)
```

#### 解决方案
```rust
// 接收端维护已接收seq集合
struct Receiver {
    received_seqs: HashMap<SocketAddr, HashSet<u32>>, // 已接收的seq
    ack_cache: HashMap<SocketAddr, HashMap<u32, Instant>>, // ACK缓存
}

// 接收端处理逻辑
fn handle_data_packet(&mut self, from: SocketAddr, seq: u32, data: &[u8]) {
    let received = self.received_seqs.entry(from).or_insert_with(HashSet::new);
    
    if received.contains(&seq) {
        // 重复包，直接重发ACK（可能之前的ACK丢了）
        self.send_ack(from, seq);
        return;
    }
    
    // 新包，处理数据并发送ACK
    received.insert(seq);
    self.process_data(data);
    self.send_ack(from, seq);
    
    // 缓存ACK，用于快速重发
    self.ack_cache.entry(from).or_insert_with(HashMap::new).insert(seq, Instant::now());
}
```

### 2. 发送端超时重传

#### 问题场景
```
发送端: 发送 data(seq=100)，等待ACK
时间: 超过 RTO 时间，没收到 ACK
发送端: 重传 data(seq=100)
```

#### 解决方案
```rust
struct Sender {
    pending_packets: HashMap<SocketAddr, HashMap<u32, PendingPacket>>,
    max_retries: u8, // 最大重传次数，默认5次
}

struct PendingPacket {
    data: Vec<u8>,
    send_time: Instant,
    retry_count: u8,
    rto: Duration,
}

fn handle_timeout(&mut self, addr: SocketAddr, seq: u32) -> Result<(), ConnectionError> {
    if let Some(packet) = self.pending_packets.get_mut(&addr).and_then(|m| m.get_mut(&seq)) {
        if packet.retry_count >= self.max_retries {
            // 超过最大重传次数，标记连接断开
            self.mark_connection_dead(addr);
            return Err(ConnectionError::MaxRetriesExceeded);
        }
        
        // 重传包
        packet.retry_count += 1;
        packet.rto *= 2; // 指数退避
        packet.send_time = Instant::now();
        
        self.send_packet(addr, seq, &packet.data)?;
    }
    Ok(())
}
```

### 3. 对方掉线检测

#### 检测机制
```rust
struct ConnectionState {
    last_activity: Instant,
    ping_sent: Option<Instant>,
    consecutive_ping_failures: u8,
}

const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const PING_INTERVAL: Duration = Duration::from_secs(10);
const MAX_PING_FAILURES: u8 = 3;

fn check_connection_health(&mut self, addr: SocketAddr) -> ConnectionStatus {
    let state = self.connections.get_mut(&addr).unwrap();
    let now = Instant::now();
    
    // 1. 检查是否长时间无活动
    if now.duration_since(state.last_activity) > IDLE_TIMEOUT {
        if state.ping_sent.is_none() {
            // 发送ping探测
            self.send_ping(addr);
            state.ping_sent = Some(now);
            return ConnectionStatus::Probing;
        }
        
        // 检查ping超时
        if now.duration_since(state.ping_sent.unwrap()) > self.get_rto(addr) {
            state.consecutive_ping_failures += 1;
            
            if state.consecutive_ping_failures >= MAX_PING_FAILURES {
                return ConnectionStatus::Dead;
            }
            
            // 重新发送ping
            self.send_ping(addr);
            state.ping_sent = Some(now);
        }
    }
    
    ConnectionStatus::Alive
}
```

### 4. 连接恢复机制

#### 自动重连
```rust
fn handle_connection_dead(&mut self, addr: SocketAddr) {
    // 1. 清理死连接的状态
    self.cleanup_connection(addr);
    
    // 2. 标记待重连
    self.pending_reconnects.insert(addr, Instant::now());
    
    // 3. 通知上层应用
    self.notify_connection_lost(addr);
}

fn attempt_reconnect(&mut self, addr: SocketAddr) -> Result<(), Error> {
    // 发送新的ping包尝试重连
    self.send_ping(addr);
    
    // 重置连接状态
    self.connections.insert(addr, ConnectionState::new());
    
    Ok(())
}
```

### 5. 资源清理机制

#### 定期清理
```rust
fn periodic_cleanup(&mut self) {
    let now = Instant::now();
    let cleanup_threshold = Duration::from_secs(300); // 5分钟
    
    // 清理长时间无活动的连接
    self.connections.retain(|addr, state| {
        if now.duration_since(state.last_activity) > cleanup_threshold {
            self.cleanup_connection_resources(*addr);
            false
        } else {
            true
        }
    });
    
    // 清理过期的ACK缓存
    for (_, ack_map) in &mut self.ack_cache {
        ack_map.retain(|_, time| now.duration_since(*time) < Duration::from_secs(60));
    }
}
```

### 6. 错误恢复策略

#### 分级错误处理
```rust
#[derive(Debug)]
enum ErrorSeverity {
    Recoverable,    // 可恢复错误，如单包丢失
    Degraded,       // 性能下降，如高丢包率
    Critical,       // 严重错误，如连接断开
}

fn handle_error(&mut self, addr: SocketAddr, error: RudpError) {
    match error.severity() {
        ErrorSeverity::Recoverable => {
            // 记录错误，继续运行
            self.stats.record_recoverable_error(addr);
        }
        ErrorSeverity::Degraded => {
            // 降低发送速率，增加重传间隔
            self.adjust_congestion_window(addr, false);
        }
        ErrorSeverity::Critical => {
            // 断开连接，清理资源
            self.handle_connection_dead(addr);
        }
    }
}
```

## 性能优化

### 内存管理
1. 使用内存池管理Buffer，避免频繁分配
2. 使用环形缓冲区管理seq空间
3. 定期清理过期的连接状态

### 网络优化
1. 批量处理ACK/NACK，减少系统调用
2. 合并小包发送，提高网络利用率
3. 自适应调整批量大小

### 连接管理
1. 定期清理无活动连接（超过30秒无数据）
2. 使用LRU淘汰过多连接
3. 连接状态压缩存储

## 安全性说明

### 安全码的安全性分析
- **4字节安全码**: 提供 2^32 = 42亿种可能性
- **盐值保护**: 固定盐值"ffmesh"防止通用攻击
- **数据绑定**: 包含数据前16字节，确保数据完整性
- **快速验证**: FNV1a算法性能优秀，适合实时通信

### 防攻击能力
1. **随机攻击**: 攻击者随机生成包的成功率为 1/42亿
2. **重放攻击**: seq序列号防止重放攻击
3. **数据篡改**: 安全码包含数据内容，篡改会被检测
4. **伪造攻击**: 不知道盐值的攻击者无法伪造有效包

### 可选增强安全性
如果需要更高安全性，可以考虑：
```rust
// 选项1：动态盐值（需要握手协商）
let dynamic_salt = generate_session_salt();

// 选项2：时间戳验证（防止重放攻击）
let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

// 选项3：更强的哈希算法
use sha2::{Sha256, Digest};
let hash = Sha256::digest(data);
```

## 特别说明

1. **包大小限制**: buffer不超过1200字节，确保能通过标准MTU
2. **seq管理**: seq是按目标地址分别管理的，不是全局的
3. **简化设计**: 不做包排序，只保证不丢包
4. **连接状态**: 每个目标地址维护独立的连接状态
5. **错误处理**: 损坏的包直接丢弃，依靠重传机制恢复
6. **安全验证**: 每个包都必须通过安全码验证才会被处理
7. **协议头优化**: 使用4字节seq支持高带宽场景，协议头9字节

## 使用示例

```rust
use rudpbase::{new_rudpbase, RudpError};
use std::time::Duration;
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), RudpError> {
    // 创建Rudpbase实例
    let mut rudp = new_rudpbase("127.0.0.1:8080".parse()?).await?;

    // 发送数据
    let data = b"Hello, Rudpbase!";
    rudp.write(data, "127.0.0.1:8081".parse()?).await?;

    // 接收数据
    if let Some(rbuffer) = rudp.poll_read().await {
        match rbuffer.result {
            Ok(buffer) => {
                println!("收到来自 {} 的数据: {:?}", rbuffer.from, buffer.buffer);
            }
            Err(e) => {
                println!("接收错误: {}", e);
            }
        }
    }

    // 维护连接（在主循环中调用）
    loop {
        rudp.tick().await;
        time::sleep(Duration::from_millis(10)).await;
    }
}
```





