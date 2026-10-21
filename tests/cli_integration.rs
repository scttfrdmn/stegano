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
        ("test.txt", b"Hello".to_vec()),
        ("test.csv", b"t,v\n".to_vec()),
    ];

    for (filename, mut header) in test_cases {
        let carrier_path = temp_dir.path().join(filename);
        let payload_path = temp_dir.path().join("payload.txt");

        // Create carrier with appropriate header + padding.  Text and CSV
        // carriers must be UTF-8 text, so they get text padding.
        if filename.ends_with(".txt") {
            header.extend(b"\nline of text".repeat(400));
        } else if filename.ends_with(".csv") {
            header.extend((0..1000).flat_map(|i| format!("{i},{}.5\n", i * 3).into_bytes()));
        } else {
            header.extend(vec![0x80u8; 5000]); // Add padding for capacity
        }
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

// ---------------------------------------------------------------------------
// Streaming vs. in-memory paths (#78)
// ---------------------------------------------------------------------------

fn fastq_carrier(records: usize) -> Vec<u8> {
    let mut s = String::new();
    for i in 0..records {
        s.push_str(&format!(
            "@read{i} 1:N:0:ACGT\nACGTACGTACGTACGTACGTACGTACGTAC\n+\nIIIIIFFFFF::::,,,,IIIIIFFFFF::\n"
        ));
    }
    s.into_bytes()
}

fn vcf_carrier(records: usize) -> Vec<u8> {
    let mut s = String::from(
        "##fileformat=VCFv4.2\n##contig=<ID=chr1,length=1000000000>\n\
         ##INFO=<ID=DP,Number=1,Type=Integer,Description=\"Depth\">\n\
         #CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\n",
    );
    for i in 0..records {
        s.push_str(&format!(
            "chr1\t{}\t.\tA\tG\t50\tPASS\tDP={}\n",
            100 + i * 10,
            10 + i % 30
        ));
    }
    s.into_bytes()
}

/// Run `stegano` and assert success, returning stdout.
fn run_ok(args: &[&str]) -> String {
    let out = run_stegano(args, &[]);
    assert!(
        out.status.success(),
        "stegano {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Embed + extract `carrier` (named `name`) with the given extra flags and
/// return (embed stdout, extract stdout, stego file bytes).
fn cli_roundtrip(
    dir: &TempDir,
    name: &str,
    carrier: &[u8],
    embed_extra: &[&str],
    extract_extra: &[&str],
) -> (String, String, Vec<u8>) {
    let input = dir.path().join(name);
    let payload = dir.path().join("payload.txt");
    let stego = dir.path().join(format!("stego-{name}"));
    let extracted = dir.path().join(format!("extracted-{name}.txt"));
    fs::write(&input, carrier).unwrap();
    fs::write(&payload, "streamed secret").unwrap();

    let mut args = embed_args(
        input.to_str().unwrap(),
        payload.to_str().unwrap(),
        stego.to_str().unwrap(),
    );
    args.extend(["--password", "pw"]);
    args.extend(embed_extra);
    let embed_out = run_ok(&args);

    let mut args = vec![
        "extract",
        "-i",
        stego.to_str().unwrap(),
        "-o",
        extracted.to_str().unwrap(),
        "--password",
        "pw",
    ];
    args.extend(extract_extra);
    let extract_out = run_ok(&args);
    assert_eq!(fs::read_to_string(&extracted).unwrap(), "streamed secret");
    (embed_out, extract_out, fs::read(&stego).unwrap())
}

#[test]
fn test_cli_fastq_streaming_roundtrip() {
    let dir = TempDir::new().unwrap();
    let (embed_out, extract_out, _) =
        cli_roundtrip(&dir, "reads.fastq", &fastq_carrier(400), &[], &[]);
    assert!(embed_out.contains("Streaming FASTQ"), "{embed_out}");
    assert!(extract_out.contains("Streaming FASTQ"), "{extract_out}");

    // No extension: detected from the magic bytes of the file head.
    let (embed_out, _, _) = cli_roundtrip(&dir, "reads.dat", &fastq_carrier(400), &[], &[]);
    assert!(embed_out.contains("Streaming FASTQ"), "{embed_out}");
}

#[test]
fn test_cli_vcf_streaming_roundtrip() {
    let dir = TempDir::new().unwrap();
    let (embed_out, extract_out, _) = cli_roundtrip(&dir, "calls.vcf", &vcf_carrier(300), &[], &[]);
    assert!(embed_out.contains("Streaming VCF"), "{embed_out}");
    assert!(extract_out.contains("Streaming VCF"), "{extract_out}");

    // A streamable explicit strategy streams; the extract side auto-detects.
    let (embed_out, _, _) = cli_roundtrip(
        &dir,
        "calls-id.vcf",
        &vcf_carrier(300),
        &["--strategy", "id"],
        &["--strategy", "id"],
    );
    assert!(embed_out.contains("Streaming VCF"), "{embed_out}");

    // A non-streamable strategy falls back to the in-memory path on embed;
    // streaming extract still reads it.
    let (embed_out, extract_out, _) = cli_roundtrip(
        &dir,
        "calls-fmt.vcf",
        &vcf_carrier(300),
        &["--strategy", "multi"],
        &[],
    );
    assert!(embed_out.contains("In-memory"), "{embed_out}");
    assert!(extract_out.contains("Streaming VCF"), "{extract_out}");
}

fn sam_carrier(records: usize) -> Vec<u8> {
    let mut s = String::from("@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:1000000\n");
    for i in 0..records {
        s.push_str(&format!(
            "read{i}\t0\tchr1\t{}\t60\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\n",
            i + 1
        ));
    }
    s.into_bytes()
}

#[test]
fn test_cli_sam_streaming_roundtrip() {
    let dir = TempDir::new().unwrap();
    let (embed_out, extract_out, stego) = cli_roundtrip(
        &dir,
        "aln.sam",
        &sam_carrier(300),
        &["--strategy", "auxiliary"],
        &[],
    );
    assert!(embed_out.contains("Streaming SAM"), "{embed_out}");
    assert!(extract_out.contains("Streaming SAM"), "{extract_out}");
    assert!(stego.windows(6).any(|w| w == b"\tzs:Z:"));

    // The default SAM strategy is in-memory only; extraction still streams.
    let (embed_out, extract_out, _) =
        cli_roundtrip(&dir, "aln-default.sam", &sam_carrier(300), &[], &[]);
    assert!(embed_out.contains("In-memory"), "{embed_out}");
    assert!(extract_out.contains("Streaming SAM"), "{extract_out}");
}

#[test]
fn test_cli_no_stream_interoperates_with_streaming() {
    let dir = TempDir::new().unwrap();
    for (name, carrier) in [("a.fastq", fastq_carrier(400)), ("b.vcf", vcf_carrier(300))] {
        // In-memory embed, streaming extract.
        let (embed_out, extract_out, _) =
            cli_roundtrip(&dir, name, &carrier, &["--no-stream"], &[]);
        assert!(embed_out.contains("--no-stream"), "{embed_out}");
        assert!(extract_out.contains("Streaming"), "{extract_out}");

        // Streaming embed, in-memory extract.
        let (embed_out, extract_out, _) =
            cli_roundtrip(&dir, name, &carrier, &[], &["--no-stream"]);
        assert!(embed_out.contains("Streaming"), "{embed_out}");
        assert!(extract_out.contains("In-memory"), "{extract_out}");
    }
}

#[test]
fn test_cli_streaming_capacity_error_leaves_no_output() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("small.fastq");
    let payload = dir.path().join("big.txt");
    let stego = dir.path().join("out.fastq");
    fs::write(&input, fastq_carrier(2)).unwrap();
    fs::write(&payload, "A".repeat(4000)).unwrap();
    let out = run_stegano(
        &embed_args(
            input.to_str().unwrap(),
            payload.to_str().unwrap(),
            stego.to_str().unwrap(),
        ),
        &[],
    );
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("capacity"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !stego.exists(),
        "a failed streaming embed must not leave output"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn test_cli_png_uses_in_memory_path() {
    let (w, h) = (64u32, 64u32);
    let mut png_bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png_bytes, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        let data: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                [(x * 7) as u8, (y * 5) as u8, (x ^ y) as u8, 255]
            })
            .collect();
        writer.write_image_data(&data).unwrap();
    }
    let dir = TempDir::new().unwrap();
    let (embed_out, extract_out, _) =
        cli_roundtrip(&dir, "image.png", &png_bytes, &["--stealth", "low"], &[]);
    assert!(embed_out.contains("In-memory"), "{embed_out}");
    assert!(!embed_out.contains("Streaming"), "{embed_out}");
    assert!(extract_out.contains("In-memory"), "{extract_out}");
}

// ---------------------------------------------------------------------------
// FITS --strategy (#95)
// ---------------------------------------------------------------------------

/// A single-HDU FITS file with a 1-D image of `n` samples at `bitpix`.
fn fits_carrier(bitpix: i64, n: usize) -> Vec<u8> {
    let cards = [
        "SIMPLE  =                    T".to_string(),
        format!("BITPIX  = {bitpix:>20}"),
        format!("NAXIS   = {:>20}", 1),
        format!("NAXIS1  = {n:>20}"),
        "END".to_string(),
    ];
    let mut f: Vec<u8> = cards
        .iter()
        .flat_map(|c| format!("{c:<80}").into_bytes())
        .collect();
    f.resize(f.len().div_ceil(2880) * 2880, b' ');
    for i in 0..n {
        if bitpix == -32 {
            f.extend_from_slice(&(1.0f32 + i as f32 * 0.37).to_be_bytes());
        } else {
            f.extend_from_slice(&((i as i16).wrapping_mul(97)).to_be_bytes());
        }
    }
    f.resize(f.len().div_ceil(2880) * 2880, 0);
    f
}

#[test]
fn test_cli_fits_float_requires_strategy() {
    let dir = TempDir::new().unwrap();
    let carrier = fits_carrier(-32, 4000);

    let input = dir.path().join("float.fits");
    let payload = dir.path().join("payload.txt");
    let stego = dir.path().join("stego-default.fits");
    fs::write(&input, &carrier).unwrap();
    fs::write(&payload, "secret").unwrap();
    let mut args = embed_args(
        input.to_str().unwrap(),
        payload.to_str().unwrap(),
        stego.to_str().unwrap(),
    );
    args.extend(["--password", "pw"]);
    let out = run_stegano(&args, &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "float FITS embed without opt-in succeeded"
    );
    assert!(stderr.contains("floating point"), "{stderr}");
    assert!(stderr.contains("float_mantissa"), "{stderr}");

    // An unknown strategy is a clear error.
    args.extend(["--strategy", "bogus"]);
    let out = run_stegano(&args, &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("Unknown fits_strategy"), "{stderr}");

    let (_, _, stego) = cli_roundtrip(
        &dir,
        "float.fits",
        &carrier,
        &["--strategy", "float_mantissa"],
        &[],
    );
    assert_eq!(stego.len(), carrier.len());
    assert_ne!(stego, carrier);
}

#[test]
fn test_cli_fits_integer_pixel_lsb() {
    let dir = TempDir::new().unwrap();
    let carrier = fits_carrier(16, 4000);
    let (_, _, stego) = cli_roundtrip(
        &dir,
        "int.fits",
        &carrier,
        &["--strategy", "pixel_lsb"],
        &["--strategy", "pixel_lsb"],
    );
    assert_eq!(stego.len(), carrier.len());
}

/// Parse `N` from "Detection-resistance score: N/10" in embed output.
fn detection_score(stdout: &str) -> u8 {
    let line = stdout
        .lines()
        .find(|l| l.contains("Detection-resistance score:"))
        .unwrap_or_else(|| panic!("no score line in:\n{stdout}"));
    let after = line.split("score:").nth(1).unwrap().trim();
    after
        .split('/')
        .next()
        .unwrap()
        .parse()
        .unwrap_or_else(|_| panic!("score not computed: {line}"))
}

#[test]
fn test_cli_embed_validate_reports_computed_score() {
    let dir = TempDir::new().unwrap();
    let payload = dir.path().join("secret.txt");
    fs::write(&payload, "validation score test").unwrap();

    // Clean-looking carrier: only even byte values, so every (2k, 2k+1) pair
    // is maximally unbalanced and pairs-of-values does not flag it; the short
    // payload only touches a small prefix.
    let even: Vec<u8> = (0..20_000u32)
        .map(|i| ((i * 37 % 251) as u8) & !1)
        .collect();
    // LSB-saturated carrier: every byte value equally often (a permutation
    // of 0..=255 repeated), the pair-equalised histogram full LSB replacement
    // produces, so pairs-of-values flags it.
    let saturated: Vec<u8> = (0..20_000u32).map(|i| (i * 167 % 256) as u8).collect();

    let (mut scores, mut outputs) = (Vec::new(), Vec::new());
    for (name, data) in [("even.bin", &even), ("saturated.bin", &saturated)] {
        let carrier = dir.path().join(name);
        let output = dir.path().join(format!("out-{name}"));
        fs::write(&carrier, data).unwrap();
        let mut args = embed_args(
            carrier.to_str().unwrap(),
            payload.to_str().unwrap(),
            output.to_str().unwrap(),
        );
        args.push("--validate");
        let out = run_stegano(&args, &[("STEGANO_PASSWORD", "pw")]);
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(
            out.status.success(),
            "embed --validate failed: {}\n{stdout}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!stdout.contains("Overall Score: 0/10"), "{stdout}");
        scores.push(detection_score(&stdout));
        outputs.push(stdout);
    }
    assert_eq!(
        scores,
        vec![10, 0],
        "clean vs LSB-saturated carrier: {outputs:#?}"
    );
}

/// `analyze` reports zero off-bin quality values for a clean RTA3-binned
/// FASTQ and a non-zero count once the quality strategy has embedded into it.
#[test]
fn test_cli_analyze_fastq_off_bin_detection() {
    let temp_dir = TempDir::new().unwrap();
    let clean_path = temp_dir.path().join("binned.fastq");
    let payload_path = temp_dir.path().join("secret.txt");
    let embedded_path = temp_dir.path().join("embedded.fastq");

    // RTA3 bins Q2/Q12/Q23/Q37 as Phred+33.
    let bins = b"#-8F";
    let mut fastq = String::new();
    for r in 0..300usize {
        let qual: String = (0..100)
            .map(|i| bins[(r * 7 + i * 13 + i * i) % 4] as char)
            .collect();
        fastq.push_str(&format!("@read{r}\n{}\n+\n{qual}\n", "ACGT".repeat(25)));
    }
    fs::write(&clean_path, fastq).unwrap();
    fs::write(&payload_path, "fingerprint").unwrap();

    let analyze = |path: &std::path::Path, extra: &[&str]| {
        let mut args = vec!["run", "--bin", "stegano", "--", "analyze", "--input"];
        args.push(path.to_str().unwrap());
        args.extend_from_slice(extra);
        let out = Command::new("cargo").args(&args).output().unwrap();
        assert!(
            out.status.success(),
            "analyze failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };

    let clean = analyze(&clean_path, &[]);
    assert!(clean.contains("Alphabet: rta3-4bin"), "{clean}");
    assert!(clean.contains("Off-bin values: 0 "), "{clean}");

    let embed = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "stegano",
            "--",
            "embed",
            "--input",
            clean_path.to_str().unwrap(),
            "--payload",
            payload_path.to_str().unwrap(),
            "--output",
            embedded_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        embed.status.success(),
        "embed failed: {}",
        String::from_utf8_lossy(&embed.stderr)
    );

    let embedded = analyze(
        &embedded_path,
        &["--quality-reference", clean_path.to_str().unwrap()],
    );
    assert!(embedded.contains("Alphabet: rta3-4bin"), "{embedded}");
    assert!(!embedded.contains("Off-bin values: 0 "), "{embedded}");
    assert!(embedded.contains("changed after binning"), "{embedded}");
    assert!(embedded.contains("Reference divergence"), "{embedded}");

    // Declared alphabet; the options are rejected for non-FASTQ input.
    let declared = analyze(&embedded_path, &["--quality-bins", "2,12,23,37"]);
    assert!(declared.contains("[declared]"), "{declared}");
    let bin_path = temp_dir.path().join("carrier.bin");
    fs::write(&bin_path, vec![0u8; 1000]).unwrap();
    let out = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "stegano",
            "--",
            "analyze",
            "--input",
            bin_path.to_str().unwrap(),
            "--quality-bins",
            "rta3",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
}
