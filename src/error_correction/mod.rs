//! Error correction and integrity checking

use crate::core::*;
use blake3::Hash;

pub struct IntegritySystem;

impl IntegritySystem {
    pub fn new() -> Self {
        Self
    }

    /// Generate error correction codes for data
    pub fn generate_error_correction(&self, data: &[u8], redundancy_level: usize) -> Vec<u8> {
        // Simple XOR-based error correction for demonstration
        let mut correction_codes = Vec::new();

        for chunk in data.chunks(redundancy_level.max(1)) {
            let mut xor_result = 0u8;
            for &byte in chunk {
                xor_result ^= byte;
            }
            correction_codes.push(xor_result);
        }

        correction_codes
    }

    /// Verify data integrity using hash
    pub fn verify_integrity(&self, data: &[u8], expected_hash: &Hash) -> bool {
        let actual_hash = blake3::hash(data);
        actual_hash == *expected_hash
    }

    /// Calculate hash for integrity verification
    pub fn calculate_hash(&self, data: &[u8]) -> Hash {
        blake3::hash(data)
    }

    /// Attempt to correct errors in data using correction codes
    pub fn correct_errors(
        &self,
        data: &mut [u8],
        correction_codes: &[u8],
        redundancy_level: usize,
    ) -> Result<usize> {
        let mut corrections_made = 0;
        let chunk_size = redundancy_level.max(1);

        for (chunk_idx, chunk) in data.chunks_mut(chunk_size).enumerate() {
            if chunk_idx >= correction_codes.len() {
                break;
            }

            let mut xor_result = 0u8;
            for &byte in chunk.iter() {
                xor_result ^= byte;
            }

            // If XOR doesn't match correction code, attempt simple correction
            if xor_result != correction_codes[chunk_idx] {
                // Simple error correction: flip the first bit if there's a mismatch
                if !chunk.is_empty() {
                    chunk[0] ^= xor_result ^ correction_codes[chunk_idx];
                    corrections_made += 1;
                }
            }
        }

        Ok(corrections_made)
    }
}

impl Default for IntegritySystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integrity_system_creation() {
        let system = IntegritySystem::new();
        let data = b"test data";
        let hash = system.calculate_hash(data);
        assert!(system.verify_integrity(data, &hash));
    }

    #[test]
    fn test_integrity_system_default() {
        let system = IntegritySystem::default();
        let correction_codes = system.generate_error_correction(b"test", 2);
        assert!(!correction_codes.is_empty());
    }

    #[test]
    fn test_error_correction_generation() {
        let system = IntegritySystem::new();
        let data = b"Hello, World!";

        let correction_codes = system.generate_error_correction(data, 4);
        assert!(!correction_codes.is_empty());

        // Should generate codes for each chunk
        let expected_chunks = (data.len() + 3) / 4; // ceiling division
        assert_eq!(correction_codes.len(), expected_chunks);
    }

    #[test]
    fn test_integrity_verification() {
        let system = IntegritySystem::new();
        let original_data = b"integrity test data";
        let hash = system.calculate_hash(original_data);

        // Same data should verify
        assert!(system.verify_integrity(original_data, &hash));

        // Different data should not verify
        let modified_data = b"integrity test data!";
        assert!(!system.verify_integrity(modified_data, &hash));
    }

    #[test]
    fn test_hash_calculation() {
        let system = IntegritySystem::new();
        let data1 = b"test data 1";
        let data2 = b"test data 2";

        let hash1 = system.calculate_hash(data1);
        let hash2 = system.calculate_hash(data2);

        // Different data should produce different hashes
        assert_ne!(hash1, hash2);

        // Same data should produce same hash
        let hash1_again = system.calculate_hash(data1);
        assert_eq!(hash1, hash1_again);
    }

    #[test]
    fn test_error_correction() {
        let system = IntegritySystem::new();
        let original_data = b"error correction test data";
        let mut test_data = original_data.to_vec();

        // Generate correction codes
        let correction_codes = system.generate_error_correction(&test_data, 4);

        // Introduce an error
        test_data[0] ^= 0x01;

        // Attempt correction
        let corrections = system
            .correct_errors(&mut test_data, &correction_codes, 4)
            .unwrap();
        assert_eq!(corrections, 1);
    }

    #[test]
    fn test_error_correction_edge_cases() {
        let system = IntegritySystem::new();

        // Empty data
        let mut empty_data = vec![];
        let empty_codes = system.generate_error_correction(&empty_data, 1);
        assert!(empty_codes.is_empty());

        let corrections = system
            .correct_errors(&mut empty_data, &empty_codes, 1)
            .unwrap();
        assert_eq!(corrections, 0);

        // Single byte
        let mut single_byte = vec![0x42];
        let single_codes = system.generate_error_correction(&single_byte, 1);
        assert_eq!(single_codes.len(), 1);
        assert_eq!(single_codes[0], 0x42);

        let corrections = system
            .correct_errors(&mut single_byte, &single_codes, 1)
            .unwrap();
        assert_eq!(corrections, 0);
    }

    #[test]
    fn test_redundancy_levels() {
        let system = IntegritySystem::new();
        let data = b"redundancy level testing data";

        // Different redundancy levels should produce different numbers of codes
        let codes_1 = system.generate_error_correction(data, 1);
        let codes_4 = system.generate_error_correction(data, 4);
        let codes_8 = system.generate_error_correction(data, 8);

        assert!(codes_1.len() >= codes_4.len());
        assert!(codes_4.len() >= codes_8.len());
    }
}
