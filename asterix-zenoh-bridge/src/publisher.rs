use bytes::Bytes;
use log::{debug, error};
use zenoh::Session;

/// Publishes a raw ASTERIX frame to the Zenoh session under `key_expr`.
///
/// The frame is published as-is (raw bytes) with the application/octet-stream
/// encoding so downstream subscribers know it is binary data.
pub async fn publish_frame(session: &Session, key_expr: &str, frame: Bytes) {
    let len = frame.len();
    debug!("Publishing {} bytes on '{}'", len, key_expr);
    if let Err(e) = session.put(key_expr, frame.to_vec()).await {
        error!("Failed to publish frame on '{}': {}", key_expr, e);
    }
}
