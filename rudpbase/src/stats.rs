use std::time::{Duration, Instant};

/// Connection status enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    /// Connection is alive and healthy
    Alive,
    /// Connection is being probed (ping sent, waiting for response)
    Probing,
    /// Connection quality is degraded (high packet loss, etc.)
    Degraded,
    /// Connection is dead (no response to multiple pings)
    Dead,
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    /// Total packets sent to this connection
    pub packets_sent: u64,
    /// Total packets received from this connection
    pub packets_received: u64,
    /// Total packets lost (estimated)
    pub packets_lost: u64,
    /// Total number of retransmissions
    pub retransmissions: u64,
    /// Average round-trip time
    pub avg_rtt: Duration,
    /// Last activity timestamp
    pub last_activity: Instant,
}

impl ConnectionStats {
    pub fn new() -> Self {
        Self {
            packets_sent: 0,
            packets_received: 0,
            packets_lost: 0,
            retransmissions: 0,
            avg_rtt: Duration::from_millis(200), // Initial RTT estimate
            last_activity: Instant::now(),
        }
    }

    pub fn record_packet_sent(&mut self) {
        self.packets_sent += 1;
        self.last_activity = Instant::now();
    }

    pub fn record_packet_received(&mut self) {
        self.packets_received += 1;
        self.last_activity = Instant::now();
    }

    pub fn record_packet_lost(&mut self) {
        self.packets_lost += 1;
    }

    pub fn record_retransmission(&mut self) {
        self.retransmissions += 1;
    }

    pub fn update_rtt(&mut self, rtt: Duration) {
        // Simple moving average for RTT
        self.avg_rtt = Duration::from_nanos(
            ((self.avg_rtt.as_nanos() as u64 * 7 + rtt.as_nanos() as u64) / 8) as u64
        );
    }

    pub fn packet_loss_rate(&self) -> f64 {
        if self.packets_sent == 0 {
            0.0
        } else {
            self.packets_lost as f64 / self.packets_sent as f64
        }
    }
}

/// RTT统计和拥塞控制
#[derive(Debug, Clone)]
pub struct RttStats {
    /// 平滑RTT
    pub srtt: Duration,
    /// RTT变化量
    pub rttvar: Duration,
    /// 重传超时时间
    pub rto: Duration,
    /// 拥塞窗口（以包为单位）
    pub cwnd: u32,
    /// 慢启动阈值
    pub ssthresh: u32,
    /// 当前飞行中的包数量
    pub in_flight: u32,
    /// 最后一次拥塞事件时间
    pub last_congestion: Option<Instant>,
    /// 拥塞控制状态
    pub congestion_state: CongestionState,
}

/// 拥塞控制状态
#[derive(Debug, Clone, PartialEq)]
pub enum CongestionState {
    /// 慢启动阶段
    SlowStart,
    /// 拥塞避免阶段
    CongestionAvoidance,
    /// 快速恢复阶段
    FastRecovery,
}

impl RttStats {
    pub fn new() -> Self {
        Self {
            srtt: Duration::from_millis(100),
            rttvar: Duration::from_millis(50),
            rto: Duration::from_millis(200),
            cwnd: 1,  // 初始拥塞窗口为1
            ssthresh: 65535,  // 初始慢启动阈值设为最大值
            in_flight: 0,
            last_congestion: None,
            congestion_state: CongestionState::SlowStart,
        }
    }

    /// 更新RTT统计
    pub fn update_rtt(&mut self, rtt_sample: Duration) {
        const ALPHA: f64 = 0.125;
        const BETA: f64 = 0.25;
        const K: u32 = 4;
        const G: Duration = Duration::from_millis(10);

        let rtt_sample_ms = rtt_sample.as_millis() as f64;
        let srtt_ms = self.srtt.as_millis() as f64;
        let rttvar_ms = self.rttvar.as_millis() as f64;

        // 更新RTTVAR
        let new_rttvar_ms = (1.0 - BETA) * rttvar_ms + BETA * (srtt_ms - rtt_sample_ms).abs();
        self.rttvar = Duration::from_millis(new_rttvar_ms as u64);

        // 更新SRTT
        let new_srtt_ms = (1.0 - ALPHA) * srtt_ms + ALPHA * rtt_sample_ms;
        self.srtt = Duration::from_millis(new_srtt_ms as u64);

        // 计算RTO
        let rto_ms = new_srtt_ms + (K as f64 * new_rttvar_ms).max(G.as_millis() as f64);
        self.rto = Duration::from_millis(rto_ms.min(60000.0).max(200.0) as u64);
    }

    /// 包发送时调用（增加飞行中包数量）
    pub fn on_packet_sent(&mut self) {
        self.in_flight += 1;
    }

    /// 收到ACK时调用（拥塞控制）
    pub fn on_ack_received(&mut self, acked_packets: u32) {
        self.in_flight = self.in_flight.saturating_sub(acked_packets);
        
        match self.congestion_state {
            CongestionState::SlowStart => {
                // 慢启动：每个ACK增加1个包
                self.cwnd += acked_packets;
                
                // 检查是否超过慢启动阈值
                if self.cwnd >= self.ssthresh {
                    self.congestion_state = CongestionState::CongestionAvoidance;
                }
            }
            CongestionState::CongestionAvoidance => {
                // 拥塞避免：每个RTT增加1个包
                // 近似实现：每收到cwnd个ACK增加1个包
                self.cwnd += acked_packets.max(1) / self.cwnd.max(1);
            }
            CongestionState::FastRecovery => {
                // 快速恢复阶段，暂时不调整窗口
            }
        }
        
        // 限制最大窗口大小
        self.cwnd = self.cwnd.min(1000);
    }

    /// 检测到丢包时调用
    pub fn on_packet_lost(&mut self) {
        let now = Instant::now();
        
        // 避免在短时间内多次触发拥塞控制
        if let Some(last) = self.last_congestion {
            if now.duration_since(last) < self.rto {
                return;
            }
        }
        
        self.last_congestion = Some(now);
        
        // 设置慢启动阈值为当前窗口的一半
        self.ssthresh = (self.cwnd / 2).max(2);
        
        match self.congestion_state {
            CongestionState::SlowStart | CongestionState::CongestionAvoidance => {
                // 进入快速恢复或直接降低窗口
                self.cwnd = self.ssthresh;
                self.congestion_state = CongestionState::CongestionAvoidance;
            }
            CongestionState::FastRecovery => {
                // 已经在快速恢复中，进一步降低窗口
                self.cwnd = (self.cwnd / 2).max(1);
            }
        }
    }

    /// 检查是否可以发送新包（窗口控制）
    pub fn can_send(&self) -> bool {
        self.in_flight < self.cwnd
    }

    /// 获取当前可发送的包数量
    pub fn available_window(&self) -> u32 {
        self.cwnd.saturating_sub(self.in_flight)
    }
}

/// Connection state for tracking connection health
#[derive(Debug)]
pub struct ConnectionState {
    /// Last activity timestamp
    pub last_activity: Instant,
    /// Timestamp when ping was sent (None if no ping pending)
    pub ping_sent: Option<Instant>,
    /// Number of consecutive ping failures
    pub consecutive_ping_failures: u8,
    /// Connection status
    pub status: ConnectionStatus,
}

impl ConnectionState {
    pub fn new() -> Self {
        Self {
            last_activity: Instant::now(),
            ping_sent: None,
            consecutive_ping_failures: 0,
            status: ConnectionStatus::Alive,
        }
    }

    pub fn update_activity(&mut self) {
        self.last_activity = Instant::now();
        self.consecutive_ping_failures = 0;
        self.status = ConnectionStatus::Alive;
    }

    pub fn mark_ping_sent(&mut self) {
        self.ping_sent = Some(Instant::now());
        self.status = ConnectionStatus::Probing;
    }

    pub fn mark_ping_received(&mut self) {
        self.ping_sent = None;
        self.consecutive_ping_failures = 0;
        self.status = ConnectionStatus::Alive;
        self.last_activity = Instant::now();
    }

    pub fn mark_ping_failed(&mut self) {
        self.ping_sent = None;
        self.consecutive_ping_failures += 1;
        
        if self.consecutive_ping_failures >= 3 {
            self.status = ConnectionStatus::Dead;
        } else {
            self.status = ConnectionStatus::Degraded;
        }
    }

    /// 检查是否应该发送ping
    pub fn should_ping(&self, now: Instant) -> bool {
        // 如果空闲时间超过30秒且没有待处理的ping
        now.duration_since(self.last_activity) > Duration::from_secs(30) && self.ping_sent.is_none()
    }

    /// 检查是否应该关闭连接
    pub fn should_close(&self, now: Instant) -> bool {
        // 如果有待处理的ping且已超时，或者连续ping失败次数过多
        if let Some(ping_time) = self.ping_sent {
            now.duration_since(ping_time) > Duration::from_secs(10) && self.consecutive_ping_failures >= 3
        } else {
            self.consecutive_ping_failures >= 3
        }
    }

    /// 标记包丢失
    pub fn mark_packet_lost(&mut self) {
        self.status = ConnectionStatus::Degraded;
    }
}

/// 拥塞控制信息
#[derive(Debug, Clone)]
pub struct CongestionInfo {
    /// 拥塞窗口大小（以包为单位）
    pub congestion_window: u32,
    /// 慢启动阈值
    pub slow_start_threshold: u32,
    /// 当前飞行中的包数量
    pub in_flight_packets: u32,
    /// 可用窗口大小
    pub available_window: u32,
    /// 拥塞控制状态
    pub congestion_state: CongestionState,
    /// 当前RTO
    pub current_rto: Duration,
}

// Constants for connection management
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
pub const PING_INTERVAL: Duration = Duration::from_secs(10);
pub const MAX_PING_FAILURES: u8 = 3;
pub const MAX_RETRIES: u8 = 5;
pub const CLEANUP_THRESHOLD: Duration = Duration::from_secs(300); // 5 minutes 