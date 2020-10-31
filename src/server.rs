use enum_primitive::*;
use sqlx::{sqlite::SqliteRow, Error as SqlError, FromRow, Row};

enum_from_primitive! {
pub enum Label {
	Unlabelled = 0,
	Social,
	Gaming,
	Raid,
	Art,
	Music,
	Tech,
	Other,
}
}

const LABELS: &[&str] = &[
	"none", "social", "gaming", "raid", "art", "music", "tech", "other",
];

impl Label {
	pub const LabelList: &'static [&'static str] = LABELS;

	pub fn to_str(self) -> Option<&'static str> {
		LABELS.get(self as usize).map(|a| *a)
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
			_ => Label::Other as i16,
		})
	}
}

impl<'r> FromRow<'r, SqliteRow> for Label {
	fn from_row(row: &'r SqliteRow) -> Result<Self, SqlError> {
		row.try_get(0).and_then(|val| {
			Self::from_i16(val).ok_or_else(|| SqlError::ColumnDecode {
				index: "0".to_string(),
				source: "Invalid mode?".into(),
			})
		})
	}
}
