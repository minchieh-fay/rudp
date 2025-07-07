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

/// RTT (Round Trip Time) statistics using standard TCP algorithm
#[derive(Debug, Clone)]
pub struct RttStats {
    /// Smoothed RTT
    pub rtt: Duration,
    /// RTT variation
    pub rtt_var: Duration,
    /// Retransmission timeout
    pub rto: Duration,
}

impl RttStats {
    pub fn new() -> Self {
        Self {
            rtt: Duration::from_millis(200),     // Initial RTT estimate
            rtt_var: Duration::from_millis(100), // Initial RTT variation
            rto: Duration::from_millis(200),     // Initial RTO
        }
    }

    /// Update RTT statistics with a new measurement
    /// Uses standard TCP RTT algorithm:
    /// RTT = 7/8 * RTT + 1/8 * new_measurement
    /// RTT_VAR = 3/4 * RTT_VAR + 1/4 * |RTT - new_measurement|
    /// RTO = RTT + 4 * RTT_VAR
    pub fn update(&mut self, measurement: Duration) {
        let measurement_ns = measurement.as_nanos() as u64;
        let rtt_ns = self.rtt.as_nanos() as u64;
        let rtt_var_ns = self.rtt_var.as_nanos() as u64;

        if self.rtt == Duration::from_millis(200) {
            // First measurement
            self.rtt = measurement;
            self.rtt_var = Duration::from_nanos(measurement_ns / 2);
        } else {
            // Update RTT: RTT = 7/8 * RTT + 1/8 * measurement
            let new_rtt_ns = (rtt_ns * 7 + measurement_ns) / 8;
            self.rtt = Duration::from_nanos(new_rtt_ns);

            // Update RTT_VAR: RTT_VAR = 3/4 * RTT_VAR + 1/4 * |RTT - measurement|
            let diff = if measurement_ns > new_rtt_ns {
                measurement_ns - new_rtt_ns
            } else {
                new_rtt_ns - measurement_ns
            };
            let new_rtt_var_ns = (rtt_var_ns * 3 + diff) / 4;
            self.rtt_var = Duration::from_nanos(new_rtt_var_ns);
        }

        // Update RTO: RTO = RTT + 4 * RTT_VAR
        let rto_ns = self.rtt.as_nanos() as u64 + 4 * self.rtt_var.as_nanos() as u64;
        self.rto = Duration::from_nanos(rto_ns);

        // Clamp RTO to reasonable bounds
        if self.rto < Duration::from_millis(200) {
            self.rto = Duration::from_millis(200);
        } else if self.rto > Duration::from_secs(3) {
            self.rto = Duration::from_secs(3);
        }
    }

    /// Get current RTO with exponential backoff
    pub fn get_rto(&self, retry_count: u8) -> Duration {
        let base_rto = self.rto.as_millis() as u64;
        let backoff_rto = base_rto * (1u64 << retry_count.min(5)); // Cap at 2^5 = 32x
        Duration::from_millis(backoff_rto.min(3000)) // Max 3 seconds
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

// Constants for connection management
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
pub const PING_INTERVAL: Duration = Duration::from_secs(10);
pub const MAX_PING_FAILURES: u8 = 3;
pub const MAX_RETRIES: u8 = 5;
pub const CLEANUP_THRESHOLD: Duration = Duration::from_secs(300); // 5 minutes 