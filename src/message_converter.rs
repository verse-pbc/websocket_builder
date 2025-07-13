use anyhow::Result;

/// A trait for converting between wire format and application types.
///
/// This trait handles the conversion of messages between the wire format (byte slices)
/// used by the WebSocket protocol and the application-specific types used by
/// the middleware chain.
///
/// Since this library only supports text WebSocket messages, outbound messages
/// are converted directly to strings.
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

    /// Converts an outgoing application message to a string for text WebSocket messages.
    ///
    /// # Arguments
    /// * `message` - The application message to convert
    ///
    /// # Returns
    /// * `Ok(String)` - Successfully converted message
    /// * `Err` - Conversion failed
    fn outbound_to_string(&self, message: O) -> Result<String>;
}
