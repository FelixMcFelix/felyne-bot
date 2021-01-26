use enum_primitive::*;
use tokio_postgres::{Error as SqlError, Row};

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

impl From<&Row> for Label {
	fn from(row: &Row) -> Self {
		Self::from_i16(row.get(0)).expect("Invalid Db state!")
	}
}
