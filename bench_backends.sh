#!/bin/bash
# Compare WebSocket backends performance

echo "WebSocket Backend Performance Comparison"
echo "========================================"
echo ""

# Run with tungstenite (default)
echo "Testing Tungstenite backend..."
cargo bench --bench backend_comparison -- --save-baseline tungstenite
echo ""

# Run with fastwebsockets
echo "Testing FastWebSockets backend..."
cargo bench --bench backend_comparison --no-default-features --features fastwebsockets -- --save-baseline fastwebsockets
echo ""

# Show comparison
echo "Comparing results..."
cargo bench --bench backend_comparison -- --load-baseline tungstenite --baseline fastwebsockets
echo ""

echo "Done! View detailed results at: target/criterion/report/index.html"