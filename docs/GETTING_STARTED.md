# Getting Started with Stegano

Welcome to Stegano, the advanced scientific data steganography system. This guide will help you get up and running quickly.

## 🚀 Quick Installation

### Prerequisites
- Rust 1.70+ (install from [rustup.rs](https://rustup.rs/))
- macOS 12+ for Metal GPU acceleration (optional)
- Git for development

### Build from Source
```bash
git clone https://github.com/scttfrdmn/stegano.git
cd stegano
cargo build --release
```

The compiled binary will be available at `target/release/stegano`.

## 🎯 Basic Usage

### 1. Embed Secret Data
Hide a text file inside an image:
```bash
./stegano embed --input photo.png --payload secret.txt --output hidden.png
```

### 2. Extract Hidden Data
Recover the hidden data:
```bash
./stegano extract --input hidden.png --output recovered.txt
```

### 3. Analyze Capacity
Check how much data you can hide:
```bash
./stegano analyze --input photo.png
```

## 🔐 Password Protection

For sensitive data, always use password protection:

```bash
# Embed with password
./stegano embed \
    --input carrier.bin \
    --payload confidential.txt \
    --output protected.bin \
    --password "your_secure_password"

# Extract with same password
./stegano extract \
    --input protected.bin \
    --output recovered.txt \
    --password "your_secure_password"
```

## 📊 Supported File Formats

| Format | File Extensions | Best For |
|--------|----------------|----------|
| **Genomic Data** | `.fasta`, `.fa`, `.fastq`, `.fq` | DNA/RNA sequences |
| **Medical Imaging** | `.dcm`, `.tiff`, `.tif` | DICOM, microscopy |
| **Audio** | `.wav` | Signal data, recordings |
| **Images** | `.png` | General image data |
| **Variant Data** | `.vcf` | Genetic variants |
| **Alignment** | `.sam`, `.bam` | Sequence alignments |
| **Time Series** | `.csv` | Scientific measurements |
| **Binary** | `.bin`, others | Any file type |

## 🎛️ Capacity Guidelines

### Text Files
- **Capacity**: ~12.5% of carrier size
- **Example**: 1MB text file can hide ~125KB
- **Best Practice**: Use with large log files or documents

### Images (PNG)
- **Capacity**: ~25-37.5% depending on stealth level
- **Example**: 2MB image can hide ~500-750KB
- **Best Practice**: Use high-resolution images for better capacity

### Audio (WAV)
- **Capacity**: ~6.25% of audio data
- **Example**: 10MB audio can hide ~625KB
- **Best Practice**: Use longer recordings for more space

### Binary Data
- **Capacity**: ~12.5% of carrier size
- **Example**: 5MB binary can hide ~625KB
- **Best Practice**: Use any large file as carrier

## 🔍 Real-World Examples

### Example 1: Research Data Protection
```bash
# Hide sensitive research findings in a public dataset
./stegano embed \
    --input public_dataset.csv \
    --payload research_findings.txt \
    --output protected_dataset.csv \
    --password "lab_secure_2025"
```

### Example 2: Medical Image Embedding
```bash
# Embed patient metadata in medical image
./stegano embed \
    --input patient_scan.tiff \
    --payload metadata.json \
    --output scan_with_metadata.tiff \
    --password "medical_encryption_key"
```

### Example 3: Genomic Sequence Protection
```bash
# Hide variant analysis in reference genome
./stegano embed \
    --input reference_genome.fasta \
    --payload variant_analysis.vcf \
    --output protected_genome.fasta \
    --password "genomics_2025"
```

## ⚡ Performance Tips

### For Large Files
- Use `--use-gpu` flag on macOS for Metal acceleration
- Process files in chunks for memory efficiency
- Consider file format - images and audio offer better capacity

### For Maximum Security
- Always use strong passwords (20+ characters)
- Use mixed file formats to avoid pattern detection
- Verify extraction to ensure data integrity

### For Best Stealth
- Choose carrier files larger than 10x your payload
- Use common file formats in your environment
- Avoid suspicious file size changes

## 🔧 Advanced Usage

### Custom Embedding Parameters
```bash
# High stealth, low capacity
./stegano embed \
    --input carrier.png \
    --payload secret.txt \
    --output result.png \
    --stealth-level high

# Fast embedding, standard security
./stegano embed \
    --input carrier.wav \
    --payload data.bin \
    --output result.wav \
    --error-correction low
```

### Batch Processing
```bash
# Process multiple files
for file in data/*.png; do
    ./stegano embed \
        --input "$file" \
        --payload secret.txt \
        --output "hidden/$(basename "$file")" \
        --password "batch_key_2025"
done
```

### Verification Workflow
```bash
# 1. Analyze capacity first
./stegano analyze --input carrier.bin

# 2. Embed with verification
./stegano embed \
    --input carrier.bin \
    --payload secret.txt \
    --output result.bin \
    --password "verify_key"

# 3. Verify integrity
./stegano analyze --input result.bin

# 4. Extract and compare
./stegano extract \
    --input result.bin \
    --output extracted.txt \
    --password "verify_key"

diff secret.txt extracted.txt
```

## 🚨 Common Issues & Solutions

### "Insufficient Capacity" Error
- **Problem**: Payload too large for carrier
- **Solution**: Use a larger carrier file or compress payload
- **Check**: Run `analyze` command first to check capacity

### "Format Not Supported" Error
- **Problem**: File format not recognized
- **Solution**: Use supported formats or rename to `.bin`
- **Workaround**: Binary format handles any file type

### "Extraction Failed" Error
- **Problem**: Wrong password or corrupted data
- **Solution**: Verify password, check file integrity
- **Debug**: Use `analyze` to check if data is present

### Performance Issues
- **Problem**: Slow processing on large files
- **Solution**: Enable GPU acceleration on macOS
- **Alternative**: Process files in smaller chunks

## 📚 Next Steps

1. **Read the API Documentation**: Understand advanced features
2. **Explore Examples**: Check `examples/` directory for use cases
3. **Join the Community**: Report issues and contribute
4. **Enterprise Features**: Contact for commercial licensing

## 🔒 Security Best Practices

1. **Use Strong Passwords**: 20+ characters with mixed case, numbers, symbols
2. **Verify Extractions**: Always compare original vs extracted data
3. **Secure Key Management**: Never hardcode passwords in scripts
4. **Regular Updates**: Keep Stegano updated for security patches
5. **Audit Trails**: Log steganography operations for compliance

---

Ready to protect your scientific data? Start with the basic embed/extract workflow and gradually explore advanced features!