# Felyne bot for Discord!
Bring the sounds of the hunt to your Discord server!
Felyne brings the soundscapes of Monster Hunter Portable 3rd to your voice channels.

Optionally, you can help measure how VoIP traffic works over the Internet!
See [this page](https://github.com/FelixMcFelix/felyne-bot/blob/master/MEASUREMENT.md) for details!

Get all the necessary sound files with: https://github.com/codestation/mhtools

# Commands
Felyne responds to commands via the `!` prefix, direct mentions, or a server-custom prefix.

## User
 * `!help` -- List all usable commands.
 * `!info` -- List important info about this server's config!
 * `!hunt` -- Autonomous channel joining mode.
 * `!hunt <channel_id>` -- Force join channel using its ID.
 * `!watch` -- Asks Felyne to just hang out in the most populated channel.
 * `!watch <channel_id>` -- As above, overriding the channel.
 * `!cart` -- Asks Felyne to leave.
 * `!volume <vol>` -- Set volume. 0.0 < vol < 2.0.
 * `!vol <vol>` -- As above.
 * `!github` -- Print a link to this page.
 * `!optin` -- See [this page](https://github.com/FelixMcFelix/felyne-bot/blob/master/MEASUREMENT.md).
 * `!optout` -- See [this page](https://github.com/FelixMcFelix/felyne-bot/blob/master/MEASUREMENT.md).
 * `!ack` -- See [this page](https://github.com/FelixMcFelix/felyne-bot/blob/master/MEASUREMENT.md).
 * `!remove-ack` -- See [this page](https://github.com/FelixMcFelix/felyne-bot/blob/master/MEASUREMENT.md).

## Setup/Administration
These are locked to the server owner by default, unless `admin-ctl-mode` is used.

 * `!see-config` -- See configuration for Felyne on this server.
 * `!log-to <channel_mention>` -- Log deleted messages in e.g. `#watchtower`.
 * `!felyne-prefix <prefix string>` -- Sets a new prefix for this server.
 * `!ctl-mode` -- Change who can control Felyne's voice behaviour. Use this command without any parameters for details.
 * `!admin-ctl-mode` -- Change who can use setup/admin commands. Use this command without any parameters for details.
 * `!server-opt` -- See [this page](https://github.com/FelixMcFelix/felyne-bot/blob/master/MEASUREMENT.md).
 * `!server-ack` -- See [this page](https://github.com/FelixMcFelix/felyne-bot/blob/master/MEASUREMENT.md).
 * `!remove-server-ack` -- See [this page](https://github.com/FelixMcFelix/felyne-bot/blob/master/MEASUREMENT.md).
 * `!server-label` -- See [this page](https://github.com/FelixMcFelix/
 felyne-bot/blob/master/MEASUREMENT.md).
 * `!server-unlabel` -- See [this page](https://github.com/FelixMcFelix/felyne-bot/blob/master/MEASUREMENT.md).
 * `!gather-mode` -- See [this page](https://github.com/FelixMcFelix/felyne-bot/blob/master/MEASUREMENT.md).

# Prerequisites
 * Rust stable
 * ssl-dev
 * pkg-config
 * libsodium-dev
 * ffmpeg
 * libopus-dev
 * A Postgres database
