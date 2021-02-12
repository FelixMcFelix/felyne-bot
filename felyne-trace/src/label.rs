use enum_primitive::*;
use serde::{Deserialize, Serialize};

enum_from_primitive! {
/// Self-described type of the server.
#[derive(Copy, Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[non_exhaustive]
pub enum Label {
	/// Default value.
	Unlabelled = 0,
	/// This server is primarily social, or has no focus.
	Social,
	/// This server is dedicated to gaming.
	Gaming,
	/// This server is dedicated to MMORPG play, *n*-man content lasting for significant lengths of time.
	Raid,
	/// This server is dedicated to art.
	Art,
	/// This server is dedicated to music: discussion, listening...
	Music,
	/// This server is dedicated to tech, hardware, and software.
	Tech,
	/// This server label is not captured here.
	Other,
	/// This server is dedicated to tabletop RPGs.
	Tabletop,
}
}

impl Default for Label {
	fn default() -> Self {
		Self::Unlabelled
	}
}
