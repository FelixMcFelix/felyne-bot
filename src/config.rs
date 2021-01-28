use enum_primitive::*;
use serde::{Deserialize, Serialize};
use serenity::{framework::standard::Args, model::id::RoleId};
use tokio_postgres::Row;

enum_from_primitive! {
#[derive(Clone, Copy, Debug)]
pub enum GatherMode {
	NeverGather = 0,
	GatherActive,
	AlwaysGather,
}
}

const GMODES: &[&str] = &["never-gather", "when-active", "always-gather"];

impl GatherMode {
	pub const LABEL_LIST: &'static [&'static str] = GMODES;

	pub fn to_str(&self) -> Option<&'static str> {
		GMODES.get(*self as usize).copied()
	}

	pub fn from_str(label: &str) -> Option<Self> {
		Self::from_i16(match label {
			a if a == GMODES[0] => 0,
			a if a == GMODES[1] => 1,
			a if a == GMODES[2] => 2,
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
}

impl From<&Row> for GatherMode {
	fn from(row: &Row) -> Self {
		Self::from_i16(row.get(0)).expect("Invalid Db state!")
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

	pub fn to_str(&self) -> Option<&'static str> {
		OMODES.get(*self as usize).copied()
	}

	pub fn from_str(label: &str) -> Option<Self> {
		Self::from_i16(match label {
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
	pub fn to_val(self) -> i16 {
		(match self {
			Self::ServerIn => OptInOutMode::ServerIn,
			Self::UserIn(_) => OptInOutMode::UserIn,
			Self::ServerOut => OptInOutMode::ServerOut,
		}) as i16
	}

	pub fn to_role(self) -> Option<String> {
		match self {
			Self::UserIn(a) => Some(a.0.to_string()),
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

				let role = serenity::utils::parse_mention(role.as_str())
					.or_else(|| role.parse::<u64>().ok());

				if let Some(role) = role {
					Ok(Some(Self::UserIn(RoleId(role))))
				} else {
					Err(ConfigParseError::IllegalRole)
				}
			},
			None => Err(ConfigParseError::BadMode),
		}
	}
}

impl From<&Row> for OptInOut {
	fn from(row: &Row) -> Self {
		let mode = OptInOutMode::from_i16(row.get(0)).expect("Invalid Db state!");

		match mode {
			OptInOutMode::ServerIn => Self::ServerIn,
			OptInOutMode::UserIn => {
				let i_role: i64 = row.get(1);
				Self::UserIn(RoleId(i_role as u64))
			},
			OptInOutMode::ServerOut => Self::ServerOut,
		}
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

	pub fn to_str(&self) -> Option<&'static str> {
		CMODES.get(*self as usize).copied()
	}

	pub fn from_str(label: &str) -> Option<Self> {
		Self::from_i16(match label {
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
	pub fn to_val(self) -> i16 {
		(match self {
			Self::OwnerOnly => ControlMode::OwnerOnly,
			Self::WithRole(_) => ControlMode::WithRole,
			Self::All => ControlMode::All,
		}) as i16
	}

	pub fn to_role(self) -> Option<String> {
		match self {
			Self::WithRole(a) => Some(a.0.to_string()),
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

				let role = serenity::utils::parse_mention(role.as_str())
					.or_else(|| role.parse::<u64>().ok());

				if let Some(role) = role {
					Ok(Some(Control::WithRole(RoleId(role))))
				} else {
					Err(ConfigParseError::IllegalRole)
				}
			},
			None => Err(ConfigParseError::BadMode),
		}
	}
}

impl From<&Row> for Control {
	fn from(row: &Row) -> Self {
		let mode = ControlMode::from_i16(row.get(0)).expect("Invalid Db state!");

		match mode {
			ControlMode::OwnerOnly => Self::OwnerOnly,
			ControlMode::All => Self::All,
			ControlMode::WithRole => {
				let i_role: i64 = row.get(1);
				Self::WithRole(RoleId(i_role as u64))
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
