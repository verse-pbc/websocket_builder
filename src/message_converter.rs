use anyhow::Result;
use std::borrow::Cow;

/// A trait for converting between wire format and application types.
///
/// This trait handles the conversion of messages between the wire format (byte slices)
/// used by the WebSocket protocol and the application-specific types used by
/// the middleware chain.
///
/// The trait is designed for optimal performance by working directly with byte slices
/// and avoiding unnecessary allocations where possible.
///
/// # Type Parameters
/// * `I` - The type of incoming messages after conversion
/// * `O` - The type of outgoing messages before conversion
pub trait MessageConverter<I, O>: Send + Sync {
    /// Converts an incoming byte slice message to the application type.
    ///
    /// This method works directly with the byte slice from the WebSocket frame,
    /// avoiding intermediate String allocations for better performance.
    ///
    /// # Arguments
    /// * `bytes` - The byte slice message received from the WebSocket
    ///
    /// # Returns
    /// * `Ok(Some(I))` - Successfully converted message
    /// * `Ok(None)` - Message should be ignored (empty, ping, etc.)
    /// * `Err` - Conversion failed
    fn inbound_from_bytes(&self, bytes: &[u8]) -> Result<Option<I>>;

    /// Converts an outgoing application message to bytes.
    ///
    /// Returns a `Cow<[u8]>` to allow zero-copy when the message is already
    /// in the correct format, while still supporting owned data when conversion is needed.
    ///
    /// # Arguments
    /// * `message` - The application message to convert
    ///
    /// # Returns
    /// * `Ok(Cow<[u8]>)` - Successfully converted message
    /// * `Err` - Conversion failed
    fn outbound_to_bytes(&self, message: O) -> Result<Cow<'_, [u8]>>;

    /// Converts an outgoing application message to a string.
    ///
    /// This is a convenience method for text-based protocols. The default implementation
    /// converts through bytes and validates UTF-8.
    ///
    /// # Arguments
    /// * `message` - The application message to convert
    ///
    /// # Returns
    /// * `Ok(String)` - Successfully converted message
    /// * `Err` - Conversion failed
    fn outbound_to_string(&self, message: O) -> Result<String> {
        let bytes = self.outbound_to_bytes(message)?;
        match std::str::from_utf8(&bytes) {
            Ok(s) => Ok(s.to_string()),
            Err(e) => Err(anyhow::anyhow!("Invalid UTF-8 in outbound message: {}", e)),
        }
    }
}

/// High-performance JSON converter that automatically selects the best available parser.
///
/// This converter uses different JSON parsing libraries based on available features:
/// - `sonic-rs` for fastest pure-Rust parsing (when available)
/// - `simd-json` for SIMD-accelerated parsing (when available)
/// - `serde_json` as the reliable fallback (always available)
///
/// # Type Parameters
/// * `I` - Incoming message type that implements serde::Deserialize
/// * `O` - Outgoing message type that implements serde::Serialize
#[derive(Clone, Debug)]
pub struct JsonConverter<I, O> {
    _phantom: std::marker::PhantomData<(I, O)>,
}

impl<I, O> Default for JsonConverter<I, O> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<I, O> JsonConverter<I, O>
where
    I: for<'de> serde::Deserialize<'de> + Send + Sync,
    O: serde::Serialize + Send + Sync,
{
    pub fn new() -> Self {
        Self::default()
    }
}

impl<I, O> MessageConverter<I, O> for JsonConverter<I, O>
where
    I: for<'de> serde::Deserialize<'de> + Send + Sync,
    O: serde::Serialize + Send + Sync,
{
    fn inbound_from_bytes(&self, bytes: &[u8]) -> Result<Option<I>> {
        if bytes.is_empty() {
            return Ok(None);
        }

        // Use the fastest available JSON parser
        #[cfg(feature = "sonic-rs")]
        {
            use sonic_rs::from_slice;
            match from_slice::<I>(bytes) {
                Ok(msg) => Ok(Some(msg)),
                Err(e) => Err(anyhow::anyhow!("JSON parse error: {}", e)),
            }
        }

        #[cfg(all(feature = "simd-json", not(feature = "sonic-rs")))]
        {
            // SIMD JSON requires mutable buffer, so we make a copy
            // This is still faster than string allocation + parsing for large messages
            let mut bytes_copy = bytes.to_vec();
            match simd_json::serde::from_slice::<I>(&mut bytes_copy) {
                Ok(msg) => Ok(Some(msg)),
                Err(e) => Err(anyhow::anyhow!("JSON parse error: {}", e)),
            }
        }

        #[cfg(not(any(feature = "sonic-rs", feature = "simd-json")))]
        {
            match serde_json::from_slice::<I>(bytes) {
                Ok(msg) => Ok(Some(msg)),
                Err(e) => Err(anyhow::anyhow!("JSON parse error: {}", e)),
            }
        }
    }

    fn outbound_to_bytes(&self, message: O) -> Result<Cow<'_, [u8]>> {
        // Use the fastest available JSON serializer
        #[cfg(feature = "sonic-rs")]
        {
            use sonic_rs::to_vec;
            match to_vec(&message) {
                Ok(bytes) => Ok(Cow::Owned(bytes)),
                Err(e) => Err(anyhow::anyhow!("JSON serialize error: {}", e)),
            }
        }

        #[cfg(all(feature = "simd-json", not(feature = "sonic-rs")))]
        {
            use simd_json::serde::to_vec;
            match to_vec(&message) {
                Ok(bytes) => Ok(Cow::Owned(bytes)),
                Err(e) => Err(anyhow::anyhow!("JSON serialize error: {}", e)),
            }
        }

        #[cfg(not(any(feature = "sonic-rs", feature = "simd-json")))]
        {
            match serde_json::to_vec(&message) {
                Ok(bytes) => Ok(Cow::Owned(bytes)),
                Err(e) => Err(anyhow::anyhow!("JSON serialize error: {}", e)),
            }
        }
    }
}

/// Simple string converter for basic text protocols.
///
/// This converter treats all messages as UTF-8 strings without any parsing.
/// Useful for simple protocols or when you want to handle JSON parsing manually.
#[derive(Clone, Debug, Default)]
pub struct StringConverter;

impl StringConverter {
    pub fn new() -> Self {
        Self
    }
}

impl MessageConverter<String, String> for StringConverter {
    fn inbound_from_bytes(&self, bytes: &[u8]) -> Result<Option<String>> {
        if bytes.is_empty() {
            return Ok(None);
        }

        match std::str::from_utf8(bytes) {
            Ok(s) => Ok(Some(s.to_string())),
            Err(e) => Err(anyhow::anyhow!("Invalid UTF-8: {}", e)),
        }
    }

    fn outbound_to_bytes(&self, message: String) -> Result<Cow<'_, [u8]>> {
        Ok(Cow::Owned(message.into_bytes()))
    }

    fn outbound_to_string(&self, message: String) -> Result<String> {
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestMessage {
        id: u64,
        content: String,
    }

    #[test]
    fn test_json_converter() {
        let converter = JsonConverter::<TestMessage, TestMessage>::new();

        let original = TestMessage {
            id: 42,
            content: "Hello, World!".to_string(),
        };

        // Test round-trip conversion
        let bytes = converter.outbound_to_bytes(original.clone()).unwrap();
        let parsed = converter.inbound_from_bytes(&bytes).unwrap().unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_string_converter() {
        let converter = StringConverter::new();

        let original = "Hello, World!".to_string();

        // Test round-trip conversion
        let bytes = converter.outbound_to_bytes(original.clone()).unwrap();
        let parsed = converter.inbound_from_bytes(&bytes).unwrap().unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_empty_message_handling() {
        let converter = JsonConverter::<TestMessage, TestMessage>::new();

        // Empty bytes should return None
        let result = converter.inbound_from_bytes(&[]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_invalid_json() {
        let converter = JsonConverter::<TestMessage, TestMessage>::new();

        // Invalid JSON should return error
        let result = converter.inbound_from_bytes(b"invalid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_string_converter_utf8_validation() {
        let converter = StringConverter::new();

        // Valid UTF-8
        let result = converter.inbound_from_bytes("Hello 🌍".as_bytes()).unwrap();
        assert_eq!(result, Some("Hello 🌍".to_string()));

        // Invalid UTF-8
        let invalid_utf8 = &[0xFF, 0xFE];
        let result = converter.inbound_from_bytes(invalid_utf8);
        assert!(result.is_err());
    }
}
