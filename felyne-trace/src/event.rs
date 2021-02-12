use super::Extension;
use serde::{Deserialize, Serialize};

/// A pair of a nanosecond-timestamp, and an [`Event`].
///
/// [`Event`]: Event
pub type TimedEvent = (u128, Event);

/// A discrete event observed on a call.
///
/// These arise from a mixture of RTP and Websocket events: all RTP SSRCs
/// and Discord UserIDs have been merged into discrete, opaque identifiers.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[non_exhaustive]
pub enum Event {
	/// A single voice packet.
	Packet {
		/// Opaque ID of the packet's source.
		sender_id: u64,
		/// Relative sequence number of this packet, indicating
		/// the send order and allowing reordering.
		sequence: u16,
		/// The timestamp of this packet in terms of discrete samples
		/// i.e., Hz.
		timestamp: u32,
		/// The number of bytes of audio data that this packet contained.
		audio_bytes: usize,
		/// RTP extensions attached to this packet, if any,
		extension: Option<Extension>,
	},
	/// A single RTCP (control) packet received by the listener.
	///
	/// Typically sent by Discord's TURN server to indicate connection quality.
	RtcpData(Vec<u8>),
	/// A connection event registered over Websocket.
	///
	/// Used to associate UserIDs with SSRCs.
	Connect(u64),
	/// A disconnection event registered over Websocket.
	Disconnect(u64),
	/// A source has started (`true`) or stopped (`false`) speaking.
	///
	/// This is computed locally, after observing 5 silent frames in sequence.
	Speaking(u64, bool),
	/// A source has set or changed their speaking capabilities,
	/// sent as a flag set. This event is registered over Websocket.
	///
	/// This allows for both UserID/SSRC mapping, and for source prioritisation.
	/// See [the voice model page] for the meaning of each flag.
	///
	/// [the voice model page]: https://docs.rs/serenity-voice-model/0.1.0/serenity_voice_model/struct.SpeakingState.html
	SpeakState(u64, u8),
}
