#!/bin/bash
# Stress test WebSocket backends with high throughput and many connections

echo "WebSocket Backend Stress Test Comparison"
echo "========================================"
echo ""

# Run with tungstenite (default)
echo "Testing Tungstenite backend (stress tests)..."
cargo bench --bench stress_test -- --save-baseline tungstenite_stress
echo ""

# Run with fastwebsockets
echo "Testing FastWebSockets backend (stress tests)..."
cargo bench --bench stress_test --no-default-features --features fastwebsockets -- --save-baseline fastwebsockets_stress
echo ""

# Show comparison
echo "Comparing stress test results..."
cargo bench --bench stress_test -- --load-baseline tungstenite_stress --baseline fastwebsockets_stress
echo ""

echo "Done! View detailed results at: target/criterion/report/index.html"