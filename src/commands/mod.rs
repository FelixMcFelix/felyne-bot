mod admin;
mod cat_control;
mod checks;
mod info;
mod opt;
mod utils;

use self::{admin::*, cat_control::*, checks::*, info::*, opt::*, utils::*};

use serenity::{
	client::Context,
	framework::standard::{
		help_commands,
		macros::{group, help},
		Args,
		CommandGroup,
		CommandResult,
		HelpOptions,
	},
	model::prelude::{Message, UserId},
};
use std::collections::HashSet;

#[group]
#[description = "Info about me!"]
#[summary = "Info!"]
#[commands(github, optin, optout, ack, remove_ack)]
struct Everyone;

#[group]
#[checks(Control)]
#[owner_privilege]
#[description = "Tell me where to hunt, and I'll go there!"]
#[summary = "Hunting!"]
#[only_in(guilds)]
#[commands(hunt, cart, volume, watch)]
struct Control;

#[group]
#[checks(Admin)]
#[owner_privilege]
#[description = "Tell me how to go about my business!"]
#[summary = "Admin!"]
#[only_in(guilds)]
#[commands(
	see_config,
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

#[help]
#[individual_command_tip = "Mrowr! (Hello! I'm here to bring the sounds \
	of the Hunt to your voice calls!)\n\n\
	Ask me about any command you see here!"]
#[command_not_found_text = "Mryawr! (I don't know what `{}` means!)"]
#[max_levenshtein_distance(3)]
#[indention_prefix = "+"]
#[lacking_permissions = "Hide"]
#[lacking_role = "Hide"]
#[lacking_conditions = "Hide"]
#[wrong_channel = "Hide"]
pub async fn my_help(
	context: &Context,
	msg: &Message,
	args: Args,
	help_options: &'static HelpOptions,
	groups: &[&'static CommandGroup],
	owners: HashSet<UserId>,
) -> CommandResult {
	let _ = help_commands::with_embeds(context, msg, args, help_options, groups, owners).await;
	Ok(())
}
