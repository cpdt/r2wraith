# R2Wraith

R2Wraith is a batteries-included server management tool for the [Northstar Titanfall2 mod](https://github.com/R2Northstar/Northstar).
It provides:

 - A declarative high-level interface to configure one or more servers.
 - Process watching functionality to restart crashed servers.
 - Functionality to reload config or restart R2Wraith while keeping servers running.
 - An entirely commandline-based interface for dedicated headless servers.

The latest builds are available on the Releases page. Build instructions are included at the bottom of this document.

## Usage

Run R2Wraith from the command-line like this:

```
r2wraith.exe config.toml
```

Replacing `config.toml` with the path to your configuration file - see the section below on the [configuration format](#configuration-format).

This will immediately start all servers. R2Wraith also provides its own commandline interface, with the following
commands supported:

 - `help` or `?` - Display a list of available commands.
 - `version` - Display the version of R2Wraith.
 - `stopwraith` - Stop R2Wraith, keeping servers running and writing a restore file. This allows R2Wraith to attach to
   the current running servers the next time it's started. Use this to update R2Wraith seamlessly.
 - `stopall` - Shutdown all servers and stop R2Wraith. **Warning: this terminates all servers, even with players connected.**
 - `restartall` - Restart all servers. **Warning: this terminates all servers, even with players connected.**
 - `restart [name]` - Restart a server by name.
 - `reload` - Reload the configuration file, starting any added servers. Changes to existing servers will only apply when
   they are restarted. Servers that are removed in the config will not be stopped, use `stopold` to stop them.
 - `stopold` - Stop any servers that have been removed from configuration.

## Configuration Format

R2Wraith reads a configuration file in the [TOML](https://toml.io/en/) format. For an example, check out the
[example-config.toml] conf file in the repository.

Config file structure and defaults:

```toml
poll-seconds = 5                            # how often to check each server's running state
executable-path = "NorthstarLauncher.exe"   # executable to launch the server, relative to this file
process-name = "Titanfall2-unpacked.exe"    # name of the actual server process
auth-ports = { start = 8081, end = 8085 }   # range of ports available to use for the Northstar auth server
game-ports = { start = 37015, end = 37020 } # range of ports available to use for the game server

[defaults]
# default settings for all servers, see Server properties below

[servers.my-first-server]
name = "My first server"    # required -  name shown in the in-game server list
auth-port = ?               # optional - port to use for the Northstar auth server, picks from one of the auth-ports by default
game-port = ?               # optional - port to use for the game server, picks from one of the game-ports by default
# see Server properties below for more options

[servers.my-second-server]
name = "My second server"
# ...

[servers.my-third-server]
# make as many as you want!
```

### Server properties

R2Wraith provides many properties you can configure for each server. It provides sane defaults for all properties,
you can override these by setting them under each server, or in the `[defaults]` section of the config file to apply to
all servers.

#### `description`

 - A description to show in the in-game server list. Sets the `ns_server_desc` convar.
 - Default: `"Your favourite R2Wraith server"`
 - Example: `description = "My fun server, contact me on Discord for help."`

#### `password`

 - Require a password to join the server. Keeping this empty means no password is required. Sets the `ns_server_password` convar.
 - Default: `""`
 - Example: `password = "Password123"`

#### `tick-rate`

 - Sets the update and tick rate for the server. Clients will need to set the `cl_updaterate_mp` convar to benefit from
   increased tick rates. Sets the `sv_updaterate_mp`, `sv_minupdaterate`, `sv_max_snapshots_multiplayer` and
   `base_tickinterval_mp` convars.
 - Default: `20`
 - Example: `tick-rate = 60`

#### `report-to-master`

 - Whether this server should be registered with the master server, allowing it to be shown on the in-game server list.
   Sets the `ns_report_server_to_masterserver` convar.
 - Default: `true`
 - Example: `report-to-master = false`

#### `allow-insecure`

 - Whether to allow players to join without master server auth/persistence. You probably don't want to change this.
   Sets the `ns_auth_allow_insecure` convar.
 - Default: `false`
 - Example: `allow-insecure = false`

#### `use-sockets-for-loopback`

 - Keep this enabled to be able to connect to a server running on the same machine as the client.
   Sets the `net_usesocketsforloopback` convar.
 - Default: `true`
 - Example: `use-sockets-for-loopback = false`

#### `everything-unlocked`

 - Unlock all weapons, attachments, skins, etc. Sets the `everything_unlocked` convar.
 - Default: `true`
 - Example: `everything_unlocked = true`

#### `should-return-to-lobby`

 - Whether the server should return to the private match lobby after completing a game. When false, this will
   immediately start the next map/mode in the playlist. Sets the `ns_should_return_to_lobby` convar.
 - Default: `true`
 - Example: `should-return-to-lobby = false`
 
#### `player-permissions`

 - Sets the level of game changes players can make in the private lobby screen. Sets the
   `ns_private_match_only_host_can_change_settings` convar.
 - Possible values:
   - `"all"` - players can change all settings.
   - `"map-mode-only"` - players can only change the map and mode.
   - `"none"` - players can change no settings.
 - Default: `"all"`
 - Example: `player-permissions = "none"`

#### `only-host-can-start`

 - When enabled, players will not be able to start matches from the private lobby screen. Sets the
   `ns_private_match_only_host_can_start` convar.
 - Default: `false`
 - Example: `only-host-can-start = true`

#### `countdown-length-seconds`

 - The duration of the countdown in the private lobby screen, before a match is started.
   Sets the `ns_private_match_countdown_length` convar.
 - Default: `15`
 - Example: `countdown-length-seconds = 30`

#### `graphics-mode`

 - Allows enabling software rendering for true-headless dedicated servers.
 - Possible values: `"default"`, `"software"`
 - Default: `"default"`
 - Example: `graphics-mode = "software"`

#### `priority`

 - Controls the priority class of server processes. A higher priority asks the operating system to give the process a
   larger slice of processing time, potentially slowing down other processes with lower priorities.
 - Possible values: `"normal"`, `"high"`, `"real-time"`
 - Default: `"high"`
 - Example: `priority = "normal"`

#### `playlist`

 - Sets the playlist used by this server, determining which maps and modes are active. Sets the `setplaylist` convar.
 - Default: `"private_match"`
 - Example: `playlist = "tdm"`

#### `mode`

 - Limits the server to only play a specific gamemode. You probably want to set `default-mode` too, so the server starts
   in the desired gamemode. Sets the `mp_gamemode` convar.
 - Default: not set
 - Example: `mode = "ctf"`

#### `map`

 - Limits the server to only play on a specific map. You probably want to set `default-map` too, so the server starts
   in the desired map. Sets the `map` convar.
 - Default: not set
 - Example: `map = "mp_forwardbase_kodai"`

#### `default-mode`

 - Sets the initial selected gamemode in the private match screen. Sets the `ns_private_match_last_mode` convar.
 - Default: not set (Northstar defaults to `"tdm"`)
 - Example: `default-mode = "ctf"`

#### `default-map`

 - Sets the initial selected map in the private match screen. Sets the `ns_private_match_last_map` convar.
 - Default: not set (Northstar defaults to `"mp_forwardbase_kodai"`)
 - Example: `default-map = "mp_forwardbase_kodai"`

#### `epilogue-enabled`

 - Allows enabling or disabling the epilogue in supported gamemodes. Historically this has been a common source of
   crashes so it's disabled by default. Sets the `run_epilogue` playlist var.
 - Default: `false`
 - Example: `epilogue-enabled = true`

#### `custom-air-accel`

 - Custom air acceleration factor in gamemodes that support it. Sets the `custom_air_accel_pilot` playlist var.
 - Default: not set
 - Example: `custom-air-accel = 5000`

#### `boosts-enabled`

 - Enables or disables using boosts in gamemodes that support it. Sets the `boosts_enabled` playlist var.
 - Default: not set
 - Example: `boosts-enabled = false`

#### `score-limit`

 - Sets a custom score limit in gamemodes that support it. Sets the `scorelimit` playlist var.
 - Default: not set
 - Example: `scorelimit = 900`

#### `time-limit-minutes`

 - Sets a custom time limit in gamemodes that support it. Sets the `timelimit` playlist var.
 - Default: not set
 - Example: `timelimit = 60`

#### `extra-playlist-vars`

 - A map of any extra playlist override vars to set. These will override playlist vars set via other methods.
 - Example: `extra-playlist-vars = { myvar = "10", enablesquids = "1" }`

#### `extra-vars`

 - A map of any extra convars to set. These will override convars set via other methods.
 - Example: `extra-vars = { ns_will_beep = "1" }`

#### `extra_args`

 - A list of any extra command-line arguments to pass.
 - Example: `extra-args = [ "-coolmode", "-Pong", "20" ]`

## Building

R2Wraith is written in [Rust](https://www.rust-lang.org/). Install the latest stable version with [Rustup](https://rustup.rs/)
then run `cargo build` in the repository to build, and `cargo run` to build and run.

## License

R2Wraith is provided under the MIT license. Check the LICENSE file for details.
