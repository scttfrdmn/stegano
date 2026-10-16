use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Integration tests for the CLI tool
#[test]
fn test_cli_help() {
    let output = Command::new("cargo")
        .args(["run", "--bin", "stegano", "--", "--help"])
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
        .args([
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
        .args([
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
        .args([
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
        .args([
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
        .args([
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
        .args([
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
        .args([
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
        .args([
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
    // The WAV case must be a real RIFF/WAVE header (fmt + data chunk), because the
    // signal handler parses chunks: 16-bit mono 44.1 kHz with 5000 data bytes.
    let mut wav_header = b"RIFF".to_vec();
    wav_header.extend_from_slice(&(36u32 + 5000).to_le_bytes());
    wav_header.extend_from_slice(b"WAVEfmt ");
    wav_header.extend_from_slice(&16u32.to_le_bytes());
    wav_header.extend_from_slice(&[1, 0, 1, 0]); // PCM, 1 channel
    wav_header.extend_from_slice(&44100u32.to_le_bytes());
    wav_header.extend_from_slice(&88200u32.to_le_bytes());
    wav_header.extend_from_slice(&[2, 0, 16, 0]); // block_align 2, 16 bits
    wav_header.extend_from_slice(b"data");
    wav_header.extend_from_slice(&5000u32.to_le_bytes());
    let test_cases = vec![
        ("test.wav", wav_header),
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
            .args([
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

/// Run the `stegano` binary with `args` (and optional extra environment).
fn run_stegano(args: &[&str], env: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "--quiet", "--bin", "stegano", "--"])
        .args(args);
    cmd.env_remove("STEGANO_PASSWORD");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("Failed to execute stegano")
}

fn embed_args<'a>(carrier: &'a str, payload: &'a str, output: &'a str) -> Vec<&'a str> {
    vec!["embed", "-i", carrier, "-p", payload, "-o", output]
}

#[test]
fn test_cli_detect_exit_codes() {
    let temp_dir = TempDir::new().unwrap();
    let carrier = temp_dir.path().join("carrier.bin");
    let payload = temp_dir.path().join("payload.txt");
    let stego = temp_dir.path().join("stego.bin");
    // All-ones LSBs decode to an impossible length, so the generic handler's
    // heuristic reports "no hidden data" (an all-zero LSB carrier reads as a
    // valid zero-length payload).
    fs::write(&carrier, vec![0xFFu8; 5000]).unwrap();
    fs::write(&payload, "detect me").unwrap();
    let (c, p, s) = (
        carrier.to_str().unwrap(),
        payload.to_str().unwrap(),
        stego.to_str().unwrap(),
    );

    // 1 = no hidden data
    let out = run_stegano(&["detect", "-i", c], &[]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = run_stegano(&embed_args(c, p, s), &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 0 = hidden data found
    let out = run_stegano(&["detect", "-i", s], &[]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 3 = error (missing file), distinct from clap's usage error 2
    let missing = temp_dir.path().join("missing.bin");
    let out = run_stegano(&["detect", "-i", missing.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("IO error"),
        "expected IoError, got: {stderr}"
    );
}

#[test]
fn test_cli_password_from_env_and_file() {
    let temp_dir = TempDir::new().unwrap();
    let carrier = temp_dir.path().join("carrier.bin");
    let payload = temp_dir.path().join("payload.txt");
    let stego = temp_dir.path().join("stego.bin");
    let extracted = temp_dir.path().join("extracted.txt");
    let pw_file = temp_dir.path().join("pw.txt");
    fs::write(&carrier, vec![0x55u8; 8000]).unwrap();
    fs::write(&payload, "env and file password").unwrap();
    fs::write(&pw_file, "s3cret\n").unwrap();
    let (c, p, s) = (
        carrier.to_str().unwrap(),
        payload.to_str().unwrap(),
        stego.to_str().unwrap(),
    );

    // Embed with STEGANO_PASSWORD.
    let out = run_stegano(&embed_args(c, p, s), &[("STEGANO_PASSWORD", "s3cret")]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!String::from_utf8_lossy(&out.stderr).contains("--password is visible"));

    // Extracting without any password must not yield the plaintext.
    let out = run_stegano(
        &["extract", "-i", s, "-o", extracted.to_str().unwrap()],
        &[],
    );
    if out.status.success() {
        assert_ne!(fs::read(&extracted).unwrap(), b"env and file password");
    }

    // Extract with --password-file.
    let out = run_stegano(
        &[
            "extract",
            "-i",
            s,
            "-o",
            extracted.to_str().unwrap(),
            "--password-file",
            pw_file.to_str().unwrap(),
        ],
        &[],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read_to_string(&extracted).unwrap(),
        "env and file password"
    );
}

#[test]
fn test_cli_password_flag_warns() {
    let temp_dir = TempDir::new().unwrap();
    let carrier = temp_dir.path().join("carrier.bin");
    let payload = temp_dir.path().join("payload.txt");
    let stego = temp_dir.path().join("stego.bin");
    fs::write(&carrier, vec![0x55u8; 8000]).unwrap();
    fs::write(&payload, "warned").unwrap();

    let mut args = embed_args(
        carrier.to_str().unwrap(),
        payload.to_str().unwrap(),
        stego.to_str().unwrap(),
    );
    args.extend(["--password", "visible"]);
    let out = run_stegano(&args, &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("--password is visible"));
}

#[test]
fn test_cli_capacity_error_uses_bits() {
    let temp_dir = TempDir::new().unwrap();
    let carrier = temp_dir.path().join("small.bin");
    let payload = temp_dir.path().join("large.txt");
    let stego = temp_dir.path().join("out.bin");
    fs::write(&carrier, vec![0x11u8; 100]).unwrap();
    fs::write(&payload, "A".repeat(1000)).unwrap();

    let out = run_stegano(
        &embed_args(
            carrier.to_str().unwrap(),
            payload.to_str().unwrap(),
            stego.to_str().unwrap(),
        ),
        &[],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    // 1000 bytes = 8000 bits needed; "available" is also reported in bits.
    assert!(stderr.contains("need 8000 bits"), "{stderr}");
}

#[test]
fn test_cli_ec_level_roundtrip_without_password() {
    let temp_dir = TempDir::new().unwrap();
    let carrier = temp_dir.path().join("carrier.bin");
    let payload = temp_dir.path().join("payload.txt");
    let stego = temp_dir.path().join("stego.bin");
    let extracted = temp_dir.path().join("extracted.txt");
    fs::write(&carrier, vec![0x80u8; 8000]).unwrap();
    fs::write(&payload, "reed-solomon protected").unwrap();

    let mut args = embed_args(
        carrier.to_str().unwrap(),
        payload.to_str().unwrap(),
        stego.to_str().unwrap(),
    );
    args.extend(["--ec-level", "medium"]);
    let out = run_stegano(&args, &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = run_stegano(
        &[
            "extract",
            "-i",
            stego.to_str().unwrap(),
            "-o",
            extracted.to_str().unwrap(),
            "--ec-level",
            "medium",
        ],
        &[],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read_to_string(&extracted).unwrap(),
        "reed-solomon protected"
    );
}

#[test]
fn test_cli_rejects_unknown_stealth() {
    let temp_dir = TempDir::new().unwrap();
    let carrier = temp_dir.path().join("carrier.bin");
    let payload = temp_dir.path().join("payload.txt");
    let stego = temp_dir.path().join("o.bin");
    fs::write(&carrier, vec![0x80u8; 5000]).unwrap();
    fs::write(&payload, "x").unwrap();
    let mut args = embed_args(
        carrier.to_str().unwrap(),
        payload.to_str().unwrap(),
        stego.to_str().unwrap(),
    );
    args.extend(["--stealth", "ultra"]);
    let out = run_stegano(&args, &[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown stealth level"));
}
