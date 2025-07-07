use fnv::FnvHasher;
use std::hash::Hasher;
use crate::protocol::PacketType;

/// Security code calculator
pub struct SecurityCode;

impl SecurityCode {
    /// Salt value for security code calculation
    const SALT: &'static [u8] = b"ffmesh";

    /// Calculate security code for a packet
    /// 
    /// Algorithm:
    /// 1. Take first 16 bytes of data (pad with zeros if less than 16 bytes)
    /// 2. Add fixed salt: "ffmesh" (6 bytes)
    /// 3. Calculate: hash = FNV1a_32(salt + type + seq + data_len + first_16_bytes_data)
    /// 4. Security code = hash & 0xFFFFFFFF
    pub fn calculate(packet_type: PacketType, seq: u32, data: &[u8]) -> u32 {
        let mut hasher = FnvHasher::default();
        
        // Add salt
        hasher.write(Self::SALT);
        
        // Add packet type
        hasher.write(&[packet_type as u8]);
        
        // Add sequence number
        hasher.write(&seq.to_be_bytes());
        
        // Add data length
        hasher.write(&(data.len() as u16).to_be_bytes());
        
        // Add first 16 bytes of data (pad with zeros if necessary)
        if data.len() >= 16 {
            hasher.write(&data[..16]);
        } else {
            let mut padded = [0u8; 16];
            padded[..data.len()].copy_from_slice(data);
            hasher.write(&padded);
        }
        
        hasher.finish() as u32
    }

    /// Verify security code for a packet
    pub fn verify(packet_type: PacketType, seq: u32, data: &[u8], expected_code: u32) -> bool {
        let calculated_code = Self::calculate(packet_type, seq, data);
        calculated_code == expected_code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_code_calculation() {
        let data = b"Hello, World!";
        let code = SecurityCode::calculate(PacketType::Data, 100, data);
        
        // Verify the same calculation produces the same result
        let code2 = SecurityCode::calculate(PacketType::Data, 100, data);
        assert_eq!(code, code2);
    }

    #[test]
    fn test_security_code_verification() {
        let data = b"Test data";
        let code = SecurityCode::calculate(PacketType::Data, 42, data);
        
        // Correct verification should pass
        assert!(SecurityCode::verify(PacketType::Data, 42, data, code));
        
        // Wrong code should fail
        assert!(!SecurityCode::verify(PacketType::Data, 42, data, code + 1));
        
        // Wrong seq should fail
        assert!(!SecurityCode::verify(PacketType::Data, 43, data, code));
        
        // Wrong packet type should fail
        assert!(!SecurityCode::verify(PacketType::Ping, 42, data, code));
    }

    #[test]
    fn test_security_code_with_short_data() {
        let data = b"Hi"; // Less than 16 bytes
        let code = SecurityCode::calculate(PacketType::Data, 1, data);
        
        // Should still work with short data
        assert!(SecurityCode::verify(PacketType::Data, 1, data, code));
    }

    #[test]
    fn test_security_code_with_empty_data() {
        let data = b""; // Empty data
        let code = SecurityCode::calculate(PacketType::Ping, 0, data);
        
        // Should work with empty data
        assert!(SecurityCode::verify(PacketType::Ping, 0, data, code));
    }

    #[test]
    fn test_security_code_with_long_data() {
        let data = b"This is a very long message that is definitely more than 16 bytes";
        let code = SecurityCode::calculate(PacketType::Data, 999, data);
        
        // Should work with long data
        assert!(SecurityCode::verify(PacketType::Data, 999, data, code));
    }

    #[test]
    fn test_different_data_produces_different_codes() {
        let data1 = b"Hello";
        let data2 = b"World";
        
        let code1 = SecurityCode::calculate(PacketType::Data, 1, data1);
        let code2 = SecurityCode::calculate(PacketType::Data, 1, data2);
        
        // Different data should produce different codes
        assert_ne!(code1, code2);
    }
} 