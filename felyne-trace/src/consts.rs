/// Opaque ID of the call's listener.
pub const LISTENER_ID: u32 = u32::MAX;

/// Opaque ID of any user who is missing.
///
/// This is also the maximum opaque ID, due to the shared SSRC/UserID niche.
pub const MISSING_ID: u32 = LISTENER_ID - 1;
