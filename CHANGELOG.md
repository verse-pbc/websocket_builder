# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.1] - 2025-01-21

### Added
- Initial release of websocket_builder
- Core middleware-based WebSocket framework
- Split-actor architecture for concurrent inbound/outbound processing
- Type-safe message processing pipeline
- Connection management with configurable limits
- Backpressure support for slow clients
- Integration with Axum web framework
- Comprehensive error handling with state preservation
- Per-connection state management
- Example demonstrating pipeline message transformation
- Full test coverage including integration tests
- Performance benchmarks

### Features
- `WebSocketBuilder` for constructing WebSocket handlers
- `Middleware` trait for processing messages
- `MessageConverter` trait for type-safe message conversion
- `StateFactory` for per-connection state management
- Built-in connection limiting and timeouts
- Graceful shutdown with cancellation tokens

[Unreleased]: https://github.com/verse-pbc/websocket_builder/compare/v0.1.0-alpha.1...HEAD
[0.1.0-alpha.1]: https://github.com/verse-pbc/websocket_builder/releases/tag/v0.1.0-alpha.1