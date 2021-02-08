use enum_primitive::*;

pub const QUERY_DIR: &str = "./queries";

enum_from_primitive! {
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Query {
	SelectAdminCtl = 0,
	UpsertAdminCtl,

	SelectVoiceCtl,
	UpsertVoiceCtl,

	SelectJoin,
	UpsertJoin,

	SelectGather,
	UpsertGather,

	SelectPrefix,
	UpsertPrefix,

	SelectServerOptIn,
	UpsertServerOptIn,

	SelectUndelete,
	UpsertUndelete,

	SelectAck,
	UpsertAck,
	DeleteAck,

	SelectGuildAck,
	UpsertGuildAck,
	DeleteGuildAck,

	SelectLabel,
	UpsertLabel,
	DeleteLabel,

	SelectOptOuts,
	UpsertOptOut,
	DeleteOptOut,
}
}

impl Query {
	fn query_class(&self) -> &'static str {
		use Query::*;

		match self {
			SelectAdminCtl | SelectVoiceCtl | SelectJoin | SelectGather | SelectPrefix
			| SelectServerOptIn | SelectUndelete | SelectAck | SelectGuildAck | SelectLabel
			| SelectOptOuts => "select",

			UpsertAdminCtl | UpsertVoiceCtl | UpsertJoin | UpsertGather | UpsertPrefix
			| UpsertServerOptIn | UpsertUndelete | UpsertAck | UpsertGuildAck | UpsertLabel
			| UpsertOptOut => "upsert",

			DeleteAck | DeleteGuildAck | DeleteLabel | DeleteOptOut => "delete",
		}
	}

	fn query_subject(&self) -> &'static str {
		use Query::*;

		match self {
			SelectAdminCtl | UpsertAdminCtl => "admin",

			SelectVoiceCtl | UpsertVoiceCtl => "control",

			SelectJoin | UpsertJoin => "join",

			SelectGather | UpsertGather => "gather",

			SelectPrefix | UpsertPrefix => "prefix",

			SelectServerOptIn | UpsertServerOptIn => "serveropt",

			SelectUndelete | UpsertUndelete => "undelete",

			SelectAck | UpsertAck | DeleteAck => "ack",

			SelectGuildAck | UpsertGuildAck | DeleteGuildAck => "gack",

			SelectLabel | UpsertLabel | DeleteLabel => "label",

			SelectOptOuts | UpsertOptOut | DeleteOptOut => "optout",
		}
	}

	pub fn query_dir(&self) -> String {
		format!(
			"{}/{}-{}.sql",
			QUERY_DIR,
			self.query_class(),
			self.query_subject()
		)
	}
}
