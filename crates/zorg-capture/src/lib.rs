//! Capture and template boundary for Zorg.

use zorg_core::{ZorgError, ZorgResult};

/// Input for a capture request.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CaptureRequest {
    /// Requested zettel title.
    pub title: String,
}

/// Runs a capture request.
pub fn capture(_request: &CaptureRequest) -> ZorgResult<()> {
    Err(ZorgError::Unsupported(
        "zorg-capture is a foundation stub; capture implementation is pending",
    ))
}
