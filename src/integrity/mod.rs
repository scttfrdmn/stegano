//! Data integrity verification

use blake3::Hash;

pub struct IntegrityChecker;

impl IntegrityChecker {
    pub fn new() -> Self {
        Self
    }

    /// Verify data integrity using BLAKE3 hash
    pub fn verify_data_integrity(&self, data: &[u8], expected_hash: &Hash) -> bool {
        let actual_hash = blake3::hash(data);
        actual_hash == *expected_hash
    }

    /// Calculate BLAKE3 hash for data
    pub fn calculate_data_hash(&self, data: &[u8]) -> Hash {
        blake3::hash(data)
    }

    /// Verify checksum using simple XOR
    pub fn verify_xor_checksum(&self, data: &[u8], expected_checksum: u8) -> bool {
        let actual_checksum = self.calculate_xor_checksum(data);
        actual_checksum == expected_checksum
    }

    /// Calculate XOR checksum
    pub fn calculate_xor_checksum(&self, data: &[u8]) -> u8 {
        data.iter().fold(0u8, |acc, &byte| acc ^ byte)
    }

    /// Verify CRC32 checksum
    pub fn verify_crc32(&self, data: &[u8], expected_crc: u32) -> bool {
        let actual_crc = self.calculate_crc32(data);
        actual_crc == expected_crc
    }

    /// Calculate CRC32 checksum
    pub fn calculate_crc32(&self, data: &[u8]) -> u32 {
        // Simple CRC32 implementation for demonstration
        let mut crc = 0xFFFFFFFFu32;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB88320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    /// Check if embedded data has valid header
    pub fn verify_header_integrity(&self, data: &[u8], expected_magic: &[u8]) -> bool {
        if data.len() < expected_magic.len() {
            return false;
        }

        data[..expected_magic.len()] == *expected_magic
    }
}

impl Default for IntegrityChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integrity_checker_creation() {
        let checker = IntegrityChecker::new();
        let data = b"test data";
        let hash = checker.calculate_data_hash(data);
        assert!(checker.verify_data_integrity(data, &hash));
    }

    #[test]
    fn test_integrity_checker_default() {
        let checker = IntegrityChecker;
        let checksum = checker.calculate_xor_checksum(b"test");
        assert!(checker.verify_xor_checksum(b"test", checksum));
    }

    #[test]
    fn test_blake3_hash_verification() {
        let checker = IntegrityChecker::new();
        let original_data = b"hash verification test";
        let hash = checker.calculate_data_hash(original_data);

        // Same data should verify
        assert!(checker.verify_data_integrity(original_data, &hash));

        // Different data should not verify
        let modified_data = b"hash verification test!";
        assert!(!checker.verify_data_integrity(modified_data, &hash));
    }

    #[test]
    fn test_xor_checksum() {
        let checker = IntegrityChecker::new();
        let data = b"checksum test data";
        let checksum = checker.calculate_xor_checksum(data);

        // Verify correct checksum
        assert!(checker.verify_xor_checksum(data, checksum));

        // Wrong checksum should fail
        assert!(!checker.verify_xor_checksum(data, checksum ^ 1));

        // Empty data should have checksum 0
        assert_eq!(checker.calculate_xor_checksum(b""), 0);
        assert!(checker.verify_xor_checksum(b"", 0));
    }

    #[test]
    fn test_crc32_checksum() {
        let checker = IntegrityChecker::new();
        let data = b"CRC32 test data";
        let crc = checker.calculate_crc32(data);

        // Verify correct CRC
        assert!(checker.verify_crc32(data, crc));

        // Wrong CRC should fail
        assert!(!checker.verify_crc32(data, crc ^ 1));

        // Different data should have different CRC
        let other_data = b"CRC32 test data!";
        let other_crc = checker.calculate_crc32(other_data);
        assert_ne!(crc, other_crc);
    }

    #[test]
    fn test_header_integrity() {
        let checker = IntegrityChecker::new();
        let magic = b"STEG";
        let valid_data = b"STEGvalid embedded data";
        let invalid_data = b"INVAinvalid data";
        let short_data = b"ST";

        // Valid header should verify
        assert!(checker.verify_header_integrity(valid_data, magic));

        // Invalid header should fail
        assert!(!checker.verify_header_integrity(invalid_data, magic));

        // Short data should fail
        assert!(!checker.verify_header_integrity(short_data, magic));
    }

    #[test]
    fn test_hash_consistency() {
        let checker = IntegrityChecker::new();
        let data = b"consistency test";

        // Same data should always produce same hash
        let hash1 = checker.calculate_data_hash(data);
        let hash2 = checker.calculate_data_hash(data);
        assert_eq!(hash1, hash2);

        // Different data should produce different hashes
        let other_data = b"consistency test!";
        let other_hash = checker.calculate_data_hash(other_data);
        assert_ne!(hash1, other_hash);
    }

    #[test]
    fn test_checksum_properties() {
        let checker = IntegrityChecker::new();

        // XOR checksum is commutative
        let data1 = b"abc";
        let data2 = b"cba";
        let checksum1 = checker.calculate_xor_checksum(data1);
        let checksum2 = checker.calculate_xor_checksum(data2);
        assert_eq!(checksum1, checksum2); // Same bytes, different order

        // Single byte
        let single = b"A";
        let single_checksum = checker.calculate_xor_checksum(single);
        assert_eq!(single_checksum, b'A');
    }

    #[test]
    fn test_large_data_integrity() {
        let checker = IntegrityChecker::new();
        let large_data = vec![0x42u8; 10000];

        let hash = checker.calculate_data_hash(&large_data);
        let xor_checksum = checker.calculate_xor_checksum(&large_data);
        let crc32 = checker.calculate_crc32(&large_data);

        // All verification methods should work with large data
        assert!(checker.verify_data_integrity(&large_data, &hash));
        assert!(checker.verify_xor_checksum(&large_data, xor_checksum));
        assert!(checker.verify_crc32(&large_data, crc32));
    }
}
