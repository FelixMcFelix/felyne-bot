//! Specific trace file formats which may be stored.

use super::{Label, TimedEvent};

use serde::{Deserialize, Serialize};

/// Versioning wrapper for an inner trace value.
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum FelyneTrace {
	/// Initial variant of call statistics.
	Vers1(FelyneTraceV1),
	/// Call statistics including voice server names.
	Vers2(FelyneTraceV2),
}

/// Anonymised digest format of events in a Discord call.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FelyneTraceV1 {
	/// A sorted list of discrete events, timed from the listener's join point.
	pub events: Vec<TimedEvent>,
	/// Length of the call, in nanoseconds.
	pub length: u128,
	/// Self-described type of the server.
	pub label: Label,
	/// Discord voice server region, if available.
	pub region: Option<String>,
	/// A list of opaque user IDs who opted out of RTP event summaries.
	pub optout_users: Vec<u64>,
	/// The total number of users in the call, not including this listener.
	pub total_user_count: usize,
	/// The number of users present in the call when the listener joined.
	pub starting_user_count: usize,
}

/// Anonymised digest format of events in a Discord call, including actual voice server.
///
/// Discord introduced automatic voice server selection, where 
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FelyneTraceV2 {
	/// A sorted list of discrete events, timed from the listener's join point.
	pub events: Vec<TimedEvent>,
	/// Length of the call, in nanoseconds.
	pub length: u128,
	/// Self-described type of the server.
	pub label: Label,
	/// Discord voice server region set in the *guild*, if available.
	pub region: Option<String>,
	/// Discord voice server region set in the *guild*, if available.
	pub region_override: Option<String>,
	/// The first Discord voice server actually used in this call, if available.
	///
	/// If this changes, this will be recorded via [`ChangeServer`] events.
	///
	/// [`ChangeServer`]: super::Event::ChangeServer
	pub server: Option<String>,
	/// A list of opaque user IDs who opted out of RTP event summaries.
	pub optout_users: Vec<u64>,
	/// The total number of users in the call, not including this listener.
	pub total_user_count: usize,
	/// The number of users present in the call when the listener joined.
	pub starting_user_count: usize,
}
