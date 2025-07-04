# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0-alpha.1] - 2025-01-04

### Breaking Changes
- Removed `AxumWebSocketExt` and `FastWebSocketExt` traits - use `UnifiedWebSocketExt` instead
- Removed `start_axum()` and `start_fast()` methods - use `start_websocket()` instead
- The unified API is now the only API - no more backend-specific methods

### Added
- Unified WebSocket API that works with both tungstenite and fastwebsockets backends
- `WebSocketUpgrade` type alias that automatically resolves to the correct backend type
- `UnifiedWebSocketExt` trait providing `start_websocket()` method
- Support for fastwebsockets backend via `fastwebsockets` feature flag

### Changed
- Simplified API surface - one method works with both backends
- Backend selection is now purely feature-flag based
- Examples updated to use the unified API
- Documentation consolidated into main README

### Fixed
- WebSocket buffering issues with fastwebsockets are resolved

### Removed
- Backend-specific extension traits
- Backend-specific methods
- Separate documentation files (content moved to README)

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

[Unreleased]: https://github.com/verse-pbc/websocket_builder/compare/v0.2.0-alpha.1...HEAD
[0.2.0-alpha.1]: https://github.com/verse-pbc/websocket_builder/compare/v0.1.0-alpha.1...v0.2.0-alpha.1
[0.1.0-alpha.1]: https://github.com/verse-pbc/websocket_builder/releases/tag/v0.1.0-alpha.1