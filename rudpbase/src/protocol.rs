use std::time::{SystemTime, UNIX_EPOCH};

/// Protocol header size in bytes
pub const PROTOCOL_HEADER_SIZE: usize = 9; // type(1) + security_code(4) + seq(4)

/// Maximum buffer size (to ensure it fits in standard MTU)
pub const MAX_BUFFER_SIZE: usize = 1200;

/// Packet types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PacketType {
    /// Ping packet for RTT measurement and keep-alive
    Ping = 0,
    /// Ping acknowledgment
    PingAck = 1,
    /// Data packet
    Data = 2,
    /// Data acknowledgment
    DataAck = 3,
    /// Negative acknowledgment (request retransmission)
    DataNack = 4,
    /// Close connection
    Close = 5,
    /// Close acknowledgment
    CloseAck = 6,
}

impl PacketType {
    /// Convert u8 to PacketType
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(PacketType::Ping),
            1 => Some(PacketType::PingAck),
            2 => Some(PacketType::Data),
            3 => Some(PacketType::DataAck),
            4 => Some(PacketType::DataNack),
            5 => Some(PacketType::Close),
            6 => Some(PacketType::CloseAck),
            _ => None,
        }
    }
}

/// Ping packet structure
#[derive(Debug, Clone)]
pub struct PingPacket {
    pub timestamp: u64, // 8 bytes timestamp
}

impl PingPacket {
    pub fn new() -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        self.timestamp.to_be_bytes().to_vec()
    }

    pub fn deserialize(data: &[u8]) -> Option<Self> {
        if data.len() >= 8 {
            let timestamp = u64::from_be_bytes([
                data[0], data[1], data[2], data[3],
                data[4], data[5], data[6], data[7],
            ]);
            Some(Self { timestamp })
        } else {
            None
        }
    }
}

/// Data acknowledgment packet structure
#[derive(Debug, Clone)]
pub struct DataAckPacket {
    pub ack_seqs: Vec<u32>,
}

impl DataAckPacket {
    pub fn new(seqs: Vec<u32>) -> Self {
        Self { ack_seqs: seqs }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(self.ack_seqs.len() as u8); // ack_count
        
        for seq in &self.ack_seqs {
            data.extend_from_slice(&seq.to_be_bytes());
        }
        
        data
    }

    pub fn deserialize(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        let ack_count = data[0] as usize;
        if data.len() < 1 + ack_count * 4 {
            return None;
        }

        let mut ack_seqs = Vec::with_capacity(ack_count);
        for i in 0..ack_count {
            let offset = 1 + i * 4;
            let seq = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            ack_seqs.push(seq);
        }

        Some(Self { ack_seqs })
    }
}

/// Data negative acknowledgment packet structure
#[derive(Debug, Clone)]
pub struct DataNackPacket {
    pub nack_seqs: Vec<u32>,
}

impl DataNackPacket {
    pub fn new(seqs: Vec<u32>) -> Self {
        Self { nack_seqs: seqs }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(self.nack_seqs.len() as u8); // nack_count
        
        for seq in &self.nack_seqs {
            data.extend_from_slice(&seq.to_be_bytes());
        }
        
        data
    }

    pub fn deserialize(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        let nack_count = data[0] as usize;
        if data.len() < 1 + nack_count * 4 {
            return None;
        }

        let mut nack_seqs = Vec::with_capacity(nack_count);
        for i in 0..nack_count {
            let offset = 1 + i * 4;
            let seq = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            nack_seqs.push(seq);
        }

        Some(Self { nack_seqs })
    }
}

/// Raw packet structure for parsing
#[derive(Debug, Clone)]
pub struct RawPacket {
    pub packet_type: PacketType,
    pub security_code: u32,
    pub seq: u32,
    pub data: Vec<u8>,
}

impl RawPacket {
    /// Parse a raw UDP packet into RawPacket structure
    pub fn parse(packet: &[u8]) -> Result<Self, crate::error::RudpError> {
        if packet.len() < PROTOCOL_HEADER_SIZE {
            return Err(crate::error::RudpError::PacketTooSmall {
                size: packet.len(),
                min: PROTOCOL_HEADER_SIZE,
            });
        }

        let packet_type = PacketType::from_u8(packet[0])
            .ok_or_else(|| crate::error::RudpError::Protocol {
                message: format!("Unknown packet type: {}", packet[0]),
            })?;

        let security_code = u32::from_be_bytes([
            packet[1], packet[2], packet[3], packet[4],
        ]);

        let seq = u32::from_be_bytes([
            packet[5], packet[6], packet[7], packet[8],
        ]);

        let data = packet[PROTOCOL_HEADER_SIZE..].to_vec();

        Ok(Self {
            packet_type,
            security_code,
            seq,
            data,
        })
    }

    /// Serialize the packet into bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut packet = Vec::with_capacity(PROTOCOL_HEADER_SIZE + self.data.len());
        
        packet.push(self.packet_type as u8);
        packet.extend_from_slice(&self.security_code.to_be_bytes());
        packet.extend_from_slice(&self.seq.to_be_bytes());
        packet.extend_from_slice(&self.data);
        
        packet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_type_conversion() {
        assert_eq!(PacketType::from_u8(0), Some(PacketType::Ping));
        assert_eq!(PacketType::from_u8(2), Some(PacketType::Data));
        assert_eq!(PacketType::from_u8(255), None);
    }

    #[test]
    fn test_ping_packet_serialization() {
        let ping = PingPacket::new();
        let serialized = ping.serialize();
        let deserialized = PingPacket::deserialize(&serialized).unwrap();
        assert_eq!(ping.timestamp, deserialized.timestamp);
    }

    #[test]
    fn test_data_ack_packet_serialization() {
        let ack = DataAckPacket::new(vec![1, 2, 3]);
        let serialized = ack.serialize();
        let deserialized = DataAckPacket::deserialize(&serialized).unwrap();
        assert_eq!(ack.ack_seqs, deserialized.ack_seqs);
    }

    #[test]
    fn test_raw_packet_parsing() {
        let mut packet = vec![2u8]; // Data packet
        packet.extend_from_slice(&0x12345678u32.to_be_bytes()); // Security code
        packet.extend_from_slice(&100u32.to_be_bytes()); // Seq
        packet.extend_from_slice(b"Hello"); // Data

        let parsed = RawPacket::parse(&packet).unwrap();
        assert_eq!(parsed.packet_type, PacketType::Data);
        assert_eq!(parsed.security_code, 0x12345678);
        assert_eq!(parsed.seq, 100);
        assert_eq!(parsed.data, b"Hello");
    }
} 