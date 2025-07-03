#!/bin/bash
# verify_release_ready.sh - Quality verification script for 0.1.0 release

echo "🔍 Verifying 0.1.0 release readiness..."

# Test suite
echo "Running test suite..."
cargo test --all-features --lib
if [ $? -ne 0 ]; then echo "❌ Tests failing"; exit 1; fi

echo "Running integration tests..."
cargo test --test cli_integration
if [ $? -ne 0 ]; then echo "❌ Integration tests failing"; exit 1; fi

# Compilation clean
echo "Checking compilation warnings..."
cargo check --all-targets 2>&1 | grep -q "warning:"
if [ $? -eq 0 ]; then echo "❌ Compilation warnings present"; exit 1; fi

# CLI functionality
echo "Testing CLI..."
cargo run --bin stegano -- --help > /dev/null
if [ $? -ne 0 ]; then echo "❌ CLI not working"; exit 1; fi

# Documentation builds
echo "Building documentation..."
cargo doc --all-features --no-deps > /dev/null
if [ $? -ne 0 ]; then echo "❌ Documentation build failed"; exit 1; fi

# Security audit
echo "Running security audit..."
cargo audit > /dev/null 2>&1
if [ $? -ne 0 ]; then echo "⚠️ Security audit found issues (non-blocking for 0.1.0)"; fi

# Additional 0.1.0 specific checks
echo "Running 0.1.0 specific checks..."

# Check that basic CLI operations work
echo "Testing CLI embed/extract workflow..."
mkdir -p /tmp/stegano_test
echo "Secret test message" > /tmp/stegano_test/secret.txt
echo -n "Test carrier data that is large enough for embedding" > /tmp/stegano_test/carrier.bin
head -c 5000 /dev/zero >> /tmp/stegano_test/carrier.bin

# Test embed
cargo run --bin stegano -- embed \
    --input /tmp/stegano_test/carrier.bin \
    --payload /tmp/stegano_test/secret.txt \
    --output /tmp/stegano_test/output.bin > /dev/null
if [ $? -ne 0 ]; then echo "❌ CLI embed failed"; exit 1; fi

# Test extract
cargo run --bin stegano -- extract \
    --input /tmp/stegano_test/output.bin \
    --output /tmp/stegano_test/extracted.txt > /dev/null
if [ $? -ne 0 ]; then echo "❌ CLI extract failed"; exit 1; fi

# Verify content
if ! diff /tmp/stegano_test/secret.txt /tmp/stegano_test/extracted.txt > /dev/null; then
    echo "❌ CLI roundtrip content mismatch"
    exit 1
fi

# Test analyze
cargo run --bin stegano -- analyze \
    --input /tmp/stegano_test/carrier.bin > /dev/null
if [ $? -ne 0 ]; then echo "❌ CLI analyze failed"; exit 1; fi

# Cleanup
rm -rf /tmp/stegano_test

# Check version and metadata
echo "Checking package metadata..."
cargo metadata --format-version 1 --no-deps | grep -q '"version":"0.1.0"'
if [ $? -ne 0 ]; then echo "❌ Version not set to 0.1.0"; exit 1; fi

echo "✅ Release readiness verified!"
echo ""
echo "📊 Release Summary:"
echo "  ✅ 100% test pass rate"
echo "  ✅ Zero compilation warnings"
echo "  ✅ CLI functionality working"
echo "  ✅ Documentation builds successfully"
echo "  ✅ Core embed/extract workflow verified"
echo "  ✅ Version metadata correct"
echo ""
echo "🎯 Ready for 0.1.0 release tag!"