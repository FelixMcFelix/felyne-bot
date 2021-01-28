mod admin;
mod cat_control;
mod checks;
mod info;
mod utils;

use self::{admin::*, cat_control::*, checks::*, info::*, utils::*};

use crate::{
	audio_resources::*,
	config::{BotConfig, ConfigParseError, Control as CfgControl, ControlMode},
	constants::*,
	dbs::*,
	event_handler::*,
	voicehunt::*,
	watchcat::*,
};
use dashmap::DashMap;
use serenity::{
	async_trait,
	client::*,
	framework::standard::{
		macros::{check, command, group, help},
		Args,
		CommandOptions,
		CommandResult,
		Reason as CheckReason,
		StandardFramework,
	},
	http::client::Http,
	model::prelude::*,
	prelude::*,
	utils::*,
	Result as SResult,
};
use songbird::{
	self,
	input::{
		cached::{Compressed, Memory},
		Input,
	},
	Bitrate,
	SerenityInit,
};
use std::{
	collections::{HashMap, HashSet},
	convert::TryInto,
	env,
	fs::File,
	io::prelude::*,
	sync::Arc,
};
use tokio_postgres::Client as DbClient;
use tracing::*;

#[group]
#[commands(github, ids)]
struct Public;

#[group]
#[checks(Control)]
#[owner_privilege]
#[commands(hunt, cart, volume, watch)]
struct Control;

#[group]
#[checks(Admin)]
#[owner_privilege]
#[commands(log_to, felyne_prefix, admin_ctl_mode, ctl_mode)]
struct Admin;
