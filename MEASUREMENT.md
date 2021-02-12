# Measurement

Felyne allows you (users and server-operators) to opt in to an Internet traffic measurement study to help model voice traffic.
I'm interested in measuring this to build more reliable traffic generators and to understand how VoIP traffic (which has surged over the last year!) behaves and interacts with other network traffic, and whether these are heavily different from previous work.

## What is measured
To help understand and model how VoIP traffic behaves, Felyne looks at voice packets sent in calls it's a part of.
VoIP audio streams are spilt into individual messages containing 20ms fragments of speech each, which also contain a random number allocated to each user (SSRC).
Discord sends extra information to map UserIDs to these SSRCs.

Felyne looks at events on the call, and records:
* The size of each voice packet,
* How early/late each voice packet is received,
* Whether each packet contains any extension data,
* The type and size of any extension data,
* Non-identifying extension data,
* When user connect/disconnect events occur,
* The server's type, if it is set,
* The region of the voice server (USE, USW, EU, etc.),
* Any control packets sent by Discord,
* The number of users who participated in a call who have opted out of the above user-specific measurements.

These should allow modelling of both how *calls* occur (i.e., load caused by the server reflecting audio traffic, and to how many people) and of how *users* act (how much bandwidth cost they generate, due to how long speech bursts last).

All data is fully anonymised, removing any User-to-SSRC and Server ID mappings and replacing these with new opaque integers (counting from 0 upwards).
Anonymising data as required by the GDPR ensures that measured calls can never be mapped back to their users or server by anyone in the call.
Data, traffic generators, parsers, and models will be made publicly available.

## How can I opt out?
In any server with Felyne, type `@Felyne#6610 optout`.

To do this, Felyne needs to remember the unique ID underlying your account.
If the current server is not opted in in any way, then Felyne will not perform any monitoring, but will still remember this globally.
This setting overrides any opt-in role.

Due to true anonymisation, there is no way to alter or remove users from existing measurements.

## How can I opt in?
First, speak to your server's owner to enable this functionality for your server.
They can then opt-in via the following:
* `@Felyne#6610 server-opt server-opt-in` for all users,
* `@Felyne#6610 server-opt user-opt-in <role / role-id>` to set an opt-in role on your server,
* or `@Felyne#6610 server-opt server-opt-in` to return to the default.

Server admins may also categorise the server using `@Felyne#6610 server-label`/`server-unlabel`.

In the second case, as a user you can type `@Felyne#6610 optin` to be given the listed role.
Felyne will need role-giving privileges for this to work automatically.

By default, Felyne will only record packet data while live via `@Felyne#6610 hunt`.
If you want to change this:
* `@Felyne#6610 gather-mode always-gather` to extend this to `watch`,
* or `@Felyne#6610 gather-mode when-active` to return to the default.

## How can I be explicitly acknowledged?
* `@Felyne#6610 server-ack [custom name]` for servers,
* `@Felyne#6610 ack [custom name]` for users.

These will be manually vetted.
To remove these, use `@Felyne#6610 remove-server-ack` or `@Felyne#6610 remove-ack` respectively.
