use enum_primitive::*;
use serde::{Deserialize, Serialize};
use serenity::{
	client::Context,
	framework::standard::Args,
	model::{
		id::{GuildId, RoleId},
		user::User,
	},
};
use tokio_postgres::Row;

enum_from_primitive! {
#[derive(Clone, Copy, Debug)]
pub enum GatherMode {
	// This is removed since it is covered by opt-in-out modes.
	// NeverGather = 0,
	GatherActive = 1,
	AlwaysGather,
}
}

const GMODES: &[&str] = &[
	// "never-gather",
	"when-active",
	"always-gather",
];

impl GatherMode {
	pub const LABEL_LIST: &'static [&'static str] = GMODES;

	pub fn from_str(label: &str) -> Option<Self> {
		Self::from_i32(match label {
			// a if a == GMODES[0] => 0,
			a if a == GMODES[0] => 1,
			a if a == GMODES[1] => 2,
			_ => -1,
		})
	}

	pub fn parse(args: &mut Args) -> Result<Option<Self>, ConfigParseError> {
		if args.is_empty() {
			return Ok(None);
		}

		let mode = args
			.single::<String>()
			.map_err(|_| ConfigParseError::ArgTake)?;

		Ok(GatherMode::from_str(&mode))
	}

	pub async fn user_friendly_print(&self) -> String {
		match self {
			// Self::NeverGather => " ".to_string(),
			Self::GatherActive => " when I'm hunting".to_string(),
			Self::AlwaysGather => " whenever I'm hanging out".to_string(),
		}
	}
}

impl From<&Row> for GatherMode {
	fn from(row: &Row) -> Self {
		Self::from_i32(row.get(0)).expect("Invalid Db state!")
	}
}

impl Default for GatherMode {
	fn default() -> Self {
		// NOTE: the default opt-out effectively nullifies this.
		// this is set like so to make it easier to enable measurement
		// in one command.
		Self::GatherActive
	}
}

enum_from_primitive! {
/// This should only be used if GatherMode != NeverGather.
#[derive(Clone, Copy, Debug)]
pub enum OptInOutMode {
	ServerIn = 0,
	UserIn,
	ServerOut,
}
}

const OMODES: &[&str] = &["server-opt-in", "user-opt-in", "server-opt-out"];

impl OptInOutMode {
	pub const LABEL_LIST: &'static [&'static str] = OMODES;

	pub fn from_str(label: &str) -> Option<Self> {
		Self::from_i32(match label {
			a if a == OMODES[0] => 0,
			a if a == OMODES[1] => 1,
			a if a == OMODES[2] => 2,
			_ => -1,
		})
	}
}

#[derive(Clone, Copy, Debug)]
pub enum OptInOut {
	ServerIn,
	UserIn(RoleId),
	ServerOut,
}

impl OptInOut {
	pub fn to_val(self) -> i32 {
		(match self {
			Self::ServerIn => OptInOutMode::ServerIn,
			Self::UserIn(_) => OptInOutMode::UserIn,
			Self::ServerOut => OptInOutMode::ServerOut,
		}) as i32
	}

	pub fn to_role(self) -> Option<i64> {
		match self {
			Self::UserIn(a) => Some(a.get() as i64),
			_ => None,
		}
	}

	pub fn parse(args: &mut Args) -> Result<Option<Self>, ConfigParseError> {
		if args.is_empty() {
			return Ok(None);
		}

		let mode = args
			.single::<String>()
			.map_err(|_| ConfigParseError::ArgTake)?;

		match OptInOutMode::from_str(&mode) {
			Some(OptInOutMode::ServerIn) => Ok(Some(Self::ServerIn)),
			Some(OptInOutMode::ServerOut) => Ok(Some(Self::ServerOut)),
			Some(OptInOutMode::UserIn) => {
				let role = args
					.single::<String>()
					.map_err(|_| ConfigParseError::MissingRole)?;

				let role = serenity::utils::parse_role_mention(role.as_str()).or_else(|| {
					role.parse::<u64>().ok().and_then(|v| {
						if v == 0 {
							None
						} else {
							Some(RoleId::new(v))
						}
					})
				});

				if let Some(role) = role {
					Ok(Some(OptInOut::UserIn(role)))
				} else {
					Err(ConfigParseError::IllegalRole)
				}
			},
			None => Err(ConfigParseError::BadMode),
		}
	}

	pub async fn user_friendly_print(&self, ctx: &Context) -> String {
		match self {
			Self::ServerIn => "".to_string(),
			Self::ServerOut => " never".to_string(),
			Self::UserIn(r) => match r.to_role_cached(ctx) {
				Some(role) => format!(" to folks with the role `{}`", role.name),
				None => format!(" to folks with the role ID {}", r),
			},
		}
	}

	pub fn opted_out(&self) -> bool {
		matches!(self, Self::ServerOut)
	}

	pub async fn is_user_explicit_in(&self, ctx: &Context, user: &User, guild: GuildId) -> bool {
		match self {
			Self::UserIn(r) => user.has_role(ctx, guild, r).await.unwrap_or_default(),
			_ => false,
		}
	}
}

impl From<&Row> for OptInOut {
	fn from(row: &Row) -> Self {
		let mode = OptInOutMode::from_i32(row.get(0)).expect("Invalid Db state!");

		match mode {
			OptInOutMode::ServerIn => Self::ServerIn,
			OptInOutMode::UserIn => {
				let i_role: i64 = row.get(1);
				Self::UserIn(RoleId::new(i_role as u64))
			},
			OptInOutMode::ServerOut => Self::ServerOut,
		}
	}
}

impl Default for OptInOut {
	fn default() -> Self {
		Self::ServerOut
	}
}

enum_from_primitive! {
#[derive(Clone, Copy, Debug)]
pub enum ControlMode {
	OwnerOnly = 0,
	WithRole,
	All,
}
}

const CMODES: &[&str] = &["owner", "role", "all"];

impl ControlMode {
	pub const LABEL_LIST: &'static [&'static str] = CMODES;

	pub fn from_str(label: &str) -> Option<Self> {
		Self::from_i32(match label {
			a if a == CMODES[0] => 0,
			a if a == CMODES[1] => 1,
			a if a == CMODES[2] => 2,
			_ => -1,
		})
	}
}

#[derive(Clone, Copy, Debug)]
pub enum Control {
	OwnerOnly,
	WithRole(RoleId),
	All,
}

impl Control {
	pub fn to_val(self) -> i32 {
		(match self {
			Self::OwnerOnly => ControlMode::OwnerOnly,
			Self::WithRole(_) => ControlMode::WithRole,
			Self::All => ControlMode::All,
		}) as i32
	}

	pub fn to_role(self) -> Option<i64> {
		match self {
			Self::WithRole(a) => Some(a.get() as i64),
			_ => None,
		}
	}

	pub fn parse(args: &mut Args) -> Result<Option<Self>, ConfigParseError> {
		if args.is_empty() {
			return Ok(None);
		}

		let mode = args
			.single::<String>()
			.map_err(|_| ConfigParseError::ArgTake)?;

		match ControlMode::from_str(&mode) {
			Some(ControlMode::OwnerOnly) => Ok(Some(Control::OwnerOnly)),
			Some(ControlMode::All) => Ok(Some(Control::All)),
			Some(ControlMode::WithRole) => {
				let role = args
					.single::<String>()
					.map_err(|_| ConfigParseError::MissingRole)?;

				let role = serenity::utils::parse_role_mention(role.as_str()).or_else(|| {
					role.parse::<u64>().ok().and_then(|v| {
						if v == 0 {
							None
						} else {
							Some(RoleId::new(v))
						}
					})
				});

				if let Some(role) = role {
					Ok(Some(Control::WithRole(role)))
				} else {
					Err(ConfigParseError::IllegalRole)
				}
			},
			None => Err(ConfigParseError::BadMode),
		}
	}

	pub async fn user_friendly_print(&self, ctx: &Context) -> String {
		match self {
			Self::OwnerOnly => "only the owner".to_string(),
			Self::All => "anyone".to_string(),
			Self::WithRole(r) => match r.to_role_cached(ctx) {
				Some(role) => role.name,
				None => format!("{}", r),
			},
		}
	}
}

impl From<&Row> for Control {
	fn from(row: &Row) -> Self {
		let mode = ControlMode::from_i32(row.get(0)).expect("Invalid Db state!");

		match mode {
			ControlMode::OwnerOnly => Self::OwnerOnly,
			ControlMode::All => Self::All,
			ControlMode::WithRole => {
				let i_role: i64 = row.get(1);
				Self::WithRole(RoleId::new(i_role as u64))
			},
		}
	}
}

impl Default for Control {
	fn default() -> Self {
		Self::OwnerOnly
	}
}

#[derive(Debug)]
pub enum ConfigParseError {
	ArgTake,
	BadMode,
	IllegalRole,
	MissingRole,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BotConfig {
	pub database: DatabaseConfig,
	pub token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DatabaseConfig {
	pub user: String,
	pub password: String,
	pub host: String,
	pub port: Option<u16>,
}
