use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Integration tests for the CLI tool
#[test]
fn test_cli_help() {
    let output = Command::new("cargo")
        .args(&["run", "--bin", "stegano", "--", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Scientific data steganography tool"));
    assert!(stdout.contains("embed"));
    assert!(stdout.contains("extract"));
    assert!(stdout.contains("analyze"));
}

#[test]
fn test_cli_embed_extract_roundtrip() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    let carrier_path = temp_dir.path().join("carrier.bin");
    let payload_path = temp_dir.path().join("secret.txt");
    let output_path = temp_dir.path().join("output.bin");
    let extracted_path = temp_dir.path().join("extracted.txt");

    // Create carrier file (large enough for embedding)
    let carrier_data = vec![0x80u8; 5000];
    fs::write(&carrier_path, &carrier_data).unwrap();

    // Create secret payload
    let secret_message = "This is a secret message for CLI testing!";
    fs::write(&payload_path, secret_message).unwrap();

    // Test embed command
    let embed_output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "stegano",
            "--",
            "embed",
            "--input",
            carrier_path.to_str().unwrap(),
            "--payload",
            payload_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute embed command");

    assert!(
        embed_output.status.success(),
        "Embed command failed: {}",
        String::from_utf8_lossy(&embed_output.stderr)
    );

    // Verify output file was created
    assert!(output_path.exists());

    // Test extract command
    let extract_output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "stegano",
            "--",
            "extract",
            "--input",
            output_path.to_str().unwrap(),
            "--output",
            extracted_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute extract command");

    assert!(
        extract_output.status.success(),
        "Extract command failed: {}",
        String::from_utf8_lossy(&extract_output.stderr)
    );

    // Verify extracted content matches original
    let extracted_content = fs::read_to_string(&extracted_path).unwrap();
    assert_eq!(extracted_content, secret_message);
}

#[test]
fn test_cli_embed_extract_with_password() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    let carrier_path = temp_dir.path().join("carrier.bin");
    let payload_path = temp_dir.path().join("secret.txt");
    let output_path = temp_dir.path().join("output.bin");
    let extracted_path = temp_dir.path().join("extracted.txt");

    // Create carrier file
    let carrier_data = vec![0x55u8; 8000];
    fs::write(&carrier_path, &carrier_data).unwrap();

    // Create secret payload
    let secret_message = "Encrypted secret message!";
    fs::write(&payload_path, secret_message).unwrap();

    let password = "test_password_123";

    // Test embed with password
    let embed_output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "stegano",
            "--",
            "embed",
            "--input",
            carrier_path.to_str().unwrap(),
            "--payload",
            payload_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
            "--password",
            password,
        ])
        .output()
        .expect("Failed to execute embed command");

    assert!(
        embed_output.status.success(),
        "Embed with password failed: {}",
        String::from_utf8_lossy(&embed_output.stderr)
    );

    // Test extract with password
    let extract_output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "stegano",
            "--",
            "extract",
            "--input",
            output_path.to_str().unwrap(),
            "--output",
            extracted_path.to_str().unwrap(),
            "--password",
            password,
        ])
        .output()
        .expect("Failed to execute extract command");

    assert!(
        extract_output.status.success(),
        "Extract with password failed: {}",
        String::from_utf8_lossy(&extract_output.stderr)
    );

    // Verify extracted content matches original
    let extracted_content = fs::read_to_string(&extracted_path).unwrap();
    assert_eq!(extracted_content, secret_message);
}

#[test]
fn test_cli_analyze_command() {
    let temp_dir = TempDir::new().unwrap();

    // Create test carrier file
    let carrier_path = temp_dir.path().join("test_carrier.bin");
    let carrier_data = vec![0xAAu8; 10000];
    fs::write(&carrier_path, &carrier_data).unwrap();

    // Test analyze command
    let analyze_output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "stegano",
            "--",
            "analyze",
            "--input",
            carrier_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute analyze command");

    assert!(
        analyze_output.status.success(),
        "Analyze command failed: {}",
        String::from_utf8_lossy(&analyze_output.stderr)
    );

    let stdout = String::from_utf8(analyze_output.stdout).unwrap();
    assert!(stdout.contains("Capacity Analysis"));
    assert!(stdout.contains("Total capacity"));
    assert!(stdout.contains("Recommended capacity"));
    assert!(stdout.contains("Contains hidden data"));
}

#[test]
#[ignore] // TODO: Fix password validation in CLI
fn test_cli_extract_with_wrong_password_fails() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files with password protection
    let carrier_path = temp_dir.path().join("carrier.bin");
    let payload_path = temp_dir.path().join("secret.txt");
    let output_path = temp_dir.path().join("output.bin");
    let extracted_path = temp_dir.path().join("extracted.txt");

    let carrier_data = vec![0x42u8; 6000];
    fs::write(&carrier_path, &carrier_data).unwrap();
    fs::write(&payload_path, "Secret data").unwrap();

    // Embed with one password
    let embed_output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "stegano",
            "--",
            "embed",
            "--input",
            carrier_path.to_str().unwrap(),
            "--payload",
            payload_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
            "--password",
            "correct_password",
        ])
        .output()
        .expect("Failed to execute embed command");

    assert!(embed_output.status.success());

    // Try to extract with wrong password (should fail)
    let extract_output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "stegano",
            "--",
            "extract",
            "--input",
            output_path.to_str().unwrap(),
            "--output",
            extracted_path.to_str().unwrap(),
            "--password",
            "wrong_password",
        ])
        .output()
        .expect("Failed to execute extract command");

    // Should fail because we provided the wrong password
    assert!(
        !extract_output.status.success(),
        "Extract should have failed with wrong password"
    );
}

#[test]
fn test_cli_insufficient_capacity() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    let carrier_path = temp_dir.path().join("small_carrier.bin");
    let payload_path = temp_dir.path().join("large_payload.txt");
    let output_path = temp_dir.path().join("output.bin");

    // Create small carrier file
    let carrier_data = vec![0x11u8; 100]; // Very small
    fs::write(&carrier_path, &carrier_data).unwrap();

    // Create large payload
    let large_payload = "A".repeat(1000); // Much larger than carrier capacity
    fs::write(&payload_path, &large_payload).unwrap();

    // Test embed command (should fail due to insufficient capacity)
    let embed_output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "stegano",
            "--",
            "embed",
            "--input",
            carrier_path.to_str().unwrap(),
            "--payload",
            payload_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute embed command");

    assert!(!embed_output.status.success());
    let stderr = String::from_utf8(embed_output.stderr).unwrap();
    assert!(stderr.contains("InsufficientCapacity") || stderr.contains("capacity"));
}

#[test]
fn test_cli_different_file_formats() {
    let temp_dir = TempDir::new().unwrap();

    // Test with different file extensions to ensure format detection
    let test_cases = vec![
        ("test.wav", vec![0x52, 0x49, 0x46, 0x46]),
        ("test.bin", vec![0xDEu8; 8]),
        ("test.txt", vec![0x48, 0x65, 0x6C, 0x6C, 0x6F]), // "Hello"
    ];

    for (filename, mut header) in test_cases {
        let carrier_path = temp_dir.path().join(filename);
        let payload_path = temp_dir.path().join("payload.txt");

        // Create carrier with appropriate header + padding
        header.extend(vec![0x80u8; 5000]); // Add padding for capacity
        fs::write(&carrier_path, &header).unwrap();
        fs::write(&payload_path, "Test message").unwrap();

        // Test analyze command for format detection
        let analyze_output = Command::new("cargo")
            .args(&[
                "run",
                "--bin",
                "stegano",
                "--",
                "analyze",
                "--input",
                carrier_path.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to execute analyze command");

        assert!(
            analyze_output.status.success(),
            "Analyze failed for {}: {}",
            filename,
            String::from_utf8_lossy(&analyze_output.stderr)
        );
    }
}
