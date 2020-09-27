use enum_primitive::*;
use serenity::{
	framework::standard::Args,
	model::id::RoleId,
};
use sqlx::{sqlite::SqliteRow, Error as SqlError, FromRow, Row};

enum_from_primitive! {
#[derive(Clone, Copy, Debug)]
pub enum GatherMode {
	NeverGather = 0,
	GatherActive,
	AlwaysGather,
}
}

const GMODES: &[&str] = &[
	"never-gather",
	"when-active",
	"always-gather",
];

impl GatherMode {
	pub const LabelList: &'static [&'static str] = GMODES;

	pub fn to_str(self) -> Option<&'static str> {
		GMODES.get(self as usize).map(|a| *a)
	}

	pub fn from_str(label: &str) -> Option<Self> {
		Self::from_i16(match label {
			a if a == GMODES[0] => 0,
			a if a == GMODES[1] => 1,
			a if a == GMODES[2] => 2,
			_ => -1,
		})
	}
}

impl<'r> FromRow<'r, SqliteRow> for GatherMode {
	fn from_row(row: &'r SqliteRow) -> Result<Self, SqlError> {
		row.try_get(0)
			.and_then(|val|
				Self::from_i16(val).ok_or_else(||
					SqlError::ColumnDecode{index: "0".to_string(), source: "Invalid mode?".into()}
				)
			)
	}
}

enum_from_primitive! {
/// This should only be used if GatherMode != NeverGather.
#[derive(Clone, Copy, Debug)]
pub enum OptInOutMode {
	ServerIn = 0,
	UserIn,
}
}

const OMODES: &[&str] = &[
	"server-opt-in",
	"user-opt-in",
];

impl OptInOutMode {
	pub const LabelList: &'static [&'static str] = OMODES;

	pub fn to_str(self) -> Option<&'static str> {
		OMODES.get(self as usize).map(|a| *a)
	}

	pub fn from_str(label: &str) -> Option<Self> {
		Self::from_i16(match label {
			a if a == OMODES[0] => 0,
			a if a == OMODES[1] => 1,
			_ => -1,
		})
	}
}

#[derive(Clone, Copy, Debug)]
pub enum OptInOut {
	ServerIn,
	UserIn(RoleId),
}

impl OptInOut {
	pub fn to_val(self) -> i16 {
		(match self {
			Self::ServerIn => OptInOutMode::ServerIn,
			Self::UserIn(_) => OptInOutMode::UserIn,
		}) as i16
	}

	pub fn to_role(self) -> Option<String> {
		match self {
			Self::UserIn(a) => Some(a.0.to_string()),
			_ => None,
		}
	}
}

impl<'r> FromRow<'r, SqliteRow> for OptInOut {
	fn from_row(row: &'r SqliteRow) -> Result<Self, SqlError> {
		let mode = row.try_get(0)
			.and_then(|val|
				OptInOutMode::from_i16(val).ok_or_else(||
					SqlError::ColumnDecode{index: "0".to_string(), source: "Invalid mode?".into()}
				)
			)?;

		Ok(match mode {
			OptInOutMode::ServerIn => Self::ServerIn,
			OptInOutMode::UserIn => {
				let role = row.try_get(1)
					.and_then(|val: &str|
						val.parse::<u64>()
							.map_err(|e|
								SqlError::ColumnDecode{index: "1".to_string(), source: e.into()}
							)
							.map(RoleId)
					)?;
				Self::UserIn(role)
			},
		})
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

const CMODES: &[&str] = &[
	"owner",
	"role",
	"all",
];

impl ControlMode {
	pub const LabelList: &'static [&'static str] = CMODES;

	pub fn to_str(self) -> Option<&'static str> {
		CMODES.get(self as usize).map(|a| *a)
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

		let mode = args.single::<String>()
			.map_err(|_| ConfigParseError::ArgTake)?;

		match ControlMode::from_str(&mode) {
			Some(ControlMode::OwnerOnly) => Ok(Some(Control::OwnerOnly)),
			Some(ControlMode::All) => Ok(Some(Control::All)),
			Some(ControlMode::WithRole) => {
				let role = args.single::<String>()
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

impl<'r> FromRow<'r, SqliteRow> for Control {
	fn from_row(row: &'r SqliteRow) -> Result<Self, SqlError> {
		let mode = row.try_get(0)
			.and_then(|val|
				ControlMode::from_i16(val).ok_or_else(||
					SqlError::ColumnDecode{index: "0".to_string(), source: "Invalid mode?".into()}
				)
			)?;

		Ok(match mode {
			ControlMode::OwnerOnly => Self::OwnerOnly,
			ControlMode::All => Self::All,
			ControlMode::WithRole => {
				let role = row.try_get(1)
					.and_then(|val: &str|
						val.parse::<u64>()
							.map_err(|e|
								SqlError::ColumnDecode{index: "1".to_string(), source: e.into()}
							)
							.map(RoleId)
					)?;
				Self::WithRole(role)
			},
		})
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
