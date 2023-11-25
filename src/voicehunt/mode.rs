use crate::VoiceHuntCommand;
use enum_primitive::*;
use serenity::{client::Context, model::id::ChannelId};
use tokio_postgres::Row;

enum_from_primitive! {
/// This should only be used if GatherMode != NeverGather.
#[derive(Clone, Copy, Debug)]
pub enum JoinMode {
	Carted = 0,
	Hunt,
	DirectedHunt,
	Watch,
}
}

#[derive(Clone, Copy, Debug)]
pub enum Join {
	Carted,
	Hunt,
	DirectedHunt(ChannelId),
	Watch,
}

impl Join {
	pub fn to_val(self) -> i32 {
		(match self {
			Self::Carted => JoinMode::Carted,
			Self::Hunt => JoinMode::Hunt,
			Self::DirectedHunt(_) => JoinMode::DirectedHunt,
			Self::Watch => JoinMode::Watch,
		}) as i32
	}

	pub fn to_channel(self) -> Option<i64> {
		match self {
			Self::DirectedHunt(a) => Some(i64::from(a)),
			_ => None,
		}
	}

	pub async fn user_friendly_print(&self, ctx: &Context) -> String {
		match self {
			Self::Carted => "taking a break".to_string(),
			Self::Watch => "hanging out quietly".to_string(),
			Self::Hunt => "hunting".to_string(),
			Self::DirectedHunt(r) => match r.name(ctx).await {
				Ok(chan) => format!("hunting in `{}`", chan),
				Err(_) => format!("hunting in a channel with the ID {}", r),
			},
		}
	}

	pub fn as_command(&self) -> VoiceHuntCommand {
		match self {
			Self::Carted => VoiceHuntCommand::Carted,
			Self::Hunt => VoiceHuntCommand::BraveHunt,
			Self::DirectedHunt(c) => VoiceHuntCommand::DirectedHunt(*c),
			Self::Watch => VoiceHuntCommand::Stalk,
		}
	}
}

impl From<&Row> for Join {
	fn from(row: &Row) -> Self {
		let mode = JoinMode::from_i32(row.get(0)).expect("Invalid Db state!");

		match mode {
			JoinMode::Carted => Self::Carted,
			JoinMode::Hunt => Self::Hunt,
			JoinMode::DirectedHunt => {
				let i_role: i64 = row.get(1);
				Self::DirectedHunt(ChannelId::new(i_role as u64))
			},
			JoinMode::Watch => Self::Watch,
		}
	}
}

impl Default for Join {
	fn default() -> Self {
		Self::Watch
	}
}
