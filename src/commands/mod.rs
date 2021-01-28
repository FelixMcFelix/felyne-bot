mod admin;
mod cat_control;
mod checks;
mod info;
mod opt;
mod utils;

use self::{admin::*, cat_control::*, checks::*, info::*, opt::*, utils::*};

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
#[commands(
	log_to,
	felyne_prefix,
	admin_ctl_mode,
	ctl_mode,
	server_opt,
	server_ack,
	remove_server_ack,
	server_label,
	server_unlabel,
	gather_mode
)]
struct Admin;
