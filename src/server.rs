use crate::config::ConfigParseError;
use enum_primitive::*;
use felyne_trace::Label as StoredLabel;
use serenity::framework::standard::Args;
use tokio_postgres::Row;

enum_from_primitive! {
#[derive(Copy, Clone, Debug)]
#[non_exhaustive]
pub enum Label {
	Unlabelled = 0,
	Social,
	Gaming,
	Raid,
	Art,
	Music,
	Tech,
	Other,
	Tabletop,
}
}

const LABELS: &[&str] = &[
	"none", "social", "gaming", "raid", "art", "music", "tech", "other", "tabletop",
];

impl Label {
	pub const LABEL_LIST: &'static [&'static str] = LABELS;

	pub fn to_str(&self) -> Option<&'static str> {
		LABELS.get(*self as usize).copied()
	}

	pub fn from_str(label: &str) -> Option<Self> {
		Self::from_i16(match label {
			a if a == LABELS[0] => 0,
			a if a == LABELS[1] => 1,
			a if a == LABELS[2] => 2,
			a if a == LABELS[3] => 3,
			a if a == LABELS[4] => 4,
			a if a == LABELS[5] => 5,
			a if a == LABELS[6] => 6,
			a if a == LABELS[7] => 7,
			a if a == LABELS[8] => 8,
			_ => Label::Other as i16,
		})
	}

	pub fn parse(args: &mut Args) -> Result<Option<Self>, ConfigParseError> {
		if args.is_empty() {
			return Ok(None);
		}

		let mode = args
			.single::<String>()
			.map_err(|_| ConfigParseError::ArgTake)?;

		Ok(Self::from_str(&mode))
	}
}

impl From<&Row> for Label {
	fn from(row: &Row) -> Self {
		Self::from_i32(row.get(0)).expect("Invalid Db state!")
	}
}

impl From<Label> for StoredLabel {
	fn from(label: Label) -> Self {
		match label {
			Label::Unlabelled => Self::Unlabelled,
			Label::Social => Self::Social,
			Label::Gaming => Self::Gaming,
			Label::Raid => Self::Raid,
			Label::Art => Self::Art,
			Label::Music => Self::Music,
			Label::Tech => Self::Tech,
			Label::Other => Self::Other,
			Label::Tabletop => Self::Tabletop,
		}
	}
}

impl Default for Label {
	fn default() -> Self {
		Self::Unlabelled
	}
}
