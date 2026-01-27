#!/usr/bin/env bash
# Functional BLE loopback test for hive-btle
#
# Requirements:
#   - Raspberry Pi (or Linux box with Bluetooth)
#   - Two Bluetooth adapters (onboard + USB dongle)
#   - Run as root or in bluetooth group
#
# Usage:
#   ./scripts/functional-test.sh [--responder-adapter hci0] [--client-adapter hci1]
#
# Exit codes:
#   0 = Test passed
#   1 = Test failed
#   2 = Setup error (missing adapters, build failed, etc.)

set -euo pipefail

# Ensure cargo is in PATH (for non-interactive SSH sessions)
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

# Defaults
RESPONDER_ADAPTER="${RESPONDER_ADAPTER:-hci0}"
CLIENT_ADAPTER="${CLIENT_ADAPTER:-hci1}"
MESH_ID="${MESH_ID:-TEST}"
TIMEOUT="${TIMEOUT:-30}"
LOG_DIR="${LOG_DIR:-/tmp/hive-btle-test}"

# Parse args
while [[ $# -gt 0 ]]; do
    case $1 in
        --responder-adapter) RESPONDER_ADAPTER="$2"; shift 2 ;;
        --client-adapter) CLIENT_ADAPTER="$2"; shift 2 ;;
        --mesh-id) MESH_ID="$2"; shift 2 ;;
        --timeout) TIMEOUT="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 2 ;;
    esac
done

echo "=============================================="
echo "HIVE-BTLE Functional Loopback Test"
echo "=============================================="
echo "Responder adapter: $RESPONDER_ADAPTER"
echo "Client adapter:    $CLIENT_ADAPTER"
echo "Mesh ID:           $MESH_ID"
echo "Timeout:           ${TIMEOUT}s"
echo "=============================================="

# Create log directory
mkdir -p "$LOG_DIR"

# Check adapters exist
echo ""
echo "--- Checking Bluetooth adapters ---"
if ! hciconfig "$RESPONDER_ADAPTER" >/dev/null 2>&1; then
    echo "ERROR: Responder adapter $RESPONDER_ADAPTER not found"
    hciconfig -a
    exit 2
fi
if ! hciconfig "$CLIENT_ADAPTER" >/dev/null 2>&1; then
    echo "ERROR: Client adapter $CLIENT_ADAPTER not found"
    echo "Tip: Plug in a USB Bluetooth dongle"
    hciconfig -a
    exit 2
fi
echo "Both adapters found"

# Ensure adapters are up
echo ""
echo "--- Bringing up adapters ---"
sudo hciconfig "$RESPONDER_ADAPTER" up || true
sudo hciconfig "$CLIENT_ADAPTER" up || true
sleep 1

# Build test binaries
echo ""
echo "--- Building test binaries ---"
cargo build --release --features linux --example ble_responder --example ble_test_client
if [[ $? -ne 0 ]]; then
    echo "ERROR: Build failed"
    exit 2
fi

RESPONDER_BIN="./target/release/examples/ble_responder"
CLIENT_BIN="./target/release/examples/ble_test_client"

# Start responder in background
echo ""
echo "--- Starting responder on $RESPONDER_ADAPTER ---"
RUST_LOG=info "$RESPONDER_BIN" \
    --mesh-id "$MESH_ID" \
    --callsign "PI-RESP" \
    > "$LOG_DIR/responder.log" 2>&1 &
RESPONDER_PID=$!
echo "Responder PID: $RESPONDER_PID"

# Wait for responder to initialize
sleep 3

# Check responder is still running
if ! kill -0 $RESPONDER_PID 2>/dev/null; then
    echo "ERROR: Responder died during startup"
    cat "$LOG_DIR/responder.log"
    exit 1
fi

# Run client test
echo ""
echo "--- Running test client on $CLIENT_ADAPTER ---"
set +e  # Don't exit on client failure
RUST_LOG=info "$CLIENT_BIN" \
    --adapter "$CLIENT_ADAPTER" \
    --mesh-id "$MESH_ID" \
    --timeout "$TIMEOUT" \
    2>&1 | tee "$LOG_DIR/client.log"
CLIENT_EXIT=$?
set -e

# Stop responder
echo ""
echo "--- Stopping responder ---"
kill $RESPONDER_PID 2>/dev/null || true
wait $RESPONDER_PID 2>/dev/null || true

# Report results
echo ""
echo "=============================================="
if [[ $CLIENT_EXIT -eq 0 ]]; then
    echo "FUNCTIONAL TEST PASSED"
    echo "=============================================="
    echo "Logs saved to: $LOG_DIR"
    exit 0
else
    echo "FUNCTIONAL TEST FAILED (exit code: $CLIENT_EXIT)"
    echo "=============================================="
    echo ""
    echo "--- Responder log ---"
    tail -50 "$LOG_DIR/responder.log"
    echo ""
    echo "--- Client log ---"
    tail -50 "$LOG_DIR/client.log"
    exit 1
fi
