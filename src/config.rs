use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphicsMode {
    Default,
    Software,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Priority {
    Normal,
    High,
    RealTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrivateLobbyPlayerPermissions {
    All,
    MapModeOnly,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BoostMeterOverdrive {
    Enabled,
    Disabled,
    Only,
}

#[derive(Hash, Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Riff {
    FloorIsLava, // riff_floorislava
    AllHolopilot, // featured_mode_all_holopilot
    AllGrapple, // featured_mode_all_grapple
    AllPhase, // featured_mode_all_phase
    AllTicks, // featured_mode_all_ticks
    Tactikill, // featured_mode_tactikill
    AmpedTacticals, // featured_mode_amped_tacticals
    RocketArena, // featured_mode_rocket_arena
    ShotgunsSnipers, // featured_mode_shotguns_snipers
    IronRules, // iron_rules
    FirstPersonEmbark, // fp_embark_enabled
    Instagib, // riff_instagib
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PilotBleedout {
    Default,
    Disabled,
    Enabled,
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PlaylistOverrides {
    #[serde(default)]
    pub riffs: HashSet<Riff>,

    // Match
    pub match_classic_mp_enabled: Option<bool>, // classic_mp
    pub match_epilogue_enabled: Option<bool>, // run_epilogue
    pub match_scorelimit: Option<f64>, // scorelimit
    pub match_round_scorelimit: Option<f64>, //roundscorelimit
    pub match_timelimit: Option<f64>, // timelimit
    pub match_round_timelimit: Option<f64>, // roundtimelimit
    pub match_oob_timer_enabled: Option<bool>, // oob_timer_enabled

    // Titan
    pub titan_boost_meter_multiplier: Option<f64>, // earn_meter_titan_multiplier
    pub titan_aegis_upgrades_enabled: Option<bool>, // aegis_upgrades
    pub titan_infinite_doomed_state_enabled: Option<bool>, // infinite_doomed_state
    pub titan_shield_regen_enabled: Option<bool>, // titan_shield_regen
    pub titan_classic_rodeo_enabled: Option<bool>, // classic_rodeo

    // Pilot bleedout
    pub pilot_bleedout_mode: Option<PilotBleedout>, // riff_player_bleedout
    pub pilot_bleedout_holster_when_down: Option<bool>, // player_bleedout_forceHolster
    pub pilot_bleedout_die_on_team_bleedout: Option<bool>, // player_bleedout_forceDeathOnTeamBleedout
    pub pilot_bleedout_bleedout_time: Option<f64>, // player_bleedout_bleedoutTime
    pub pilot_bleedout_firstaid_time: Option<f64>, // player_bleedout_firstAidTime
    pub pilot_bleedout_selfres_time: Option<f64>, // player_bleedout_firstAidTimeSelf
    pub pilot_bleedout_firstaid_heal_percent: Option<f64>, // player_bleedout_firstAidHealPercent
    pub pilot_bleedout_down_ai_miss_chance: Option<f64>, // player_bleedout_aiBleedingPlayerMissChance

    // Promode
    pub promode_weapons_enabled: Option<bool>, // promode_enable

    // Pilot
    pub pilot_health_multiplier: Option<f64>, // pilot_health_multiplier
    pub pilot_respawn_delay: Option<f64>, // respawn_delay
    pub pilot_boosts_enabled: Option<bool>, // boosts_enabled, backwards!!
    pub pilot_boost_meter_overdrive: Option<BoostMeterOverdrive>, // earn_meter_pilot_overdrive
    pub pilot_boost_meter_multiplier: Option<f64>, // earn_meter_pilot_multiplier
    pub pilot_air_acceleration: Option<f64>, // custom_air_accel_pilot
}

impl PlaylistOverrides {
    pub fn or(self, other: PlaylistOverrides) -> Self {
        let mut riffs = other.riffs;
        riffs.extend(self.riffs);

        PlaylistOverrides {
            riffs,

            match_classic_mp_enabled: self.match_classic_mp_enabled.or(other.match_classic_mp_enabled),
            match_epilogue_enabled: self.match_epilogue_enabled.or(other.match_epilogue_enabled),
            match_scorelimit: self.match_scorelimit.or(other.match_scorelimit),
            match_round_scorelimit: self.match_round_scorelimit.or(other.match_round_scorelimit),
            match_timelimit: self.match_timelimit.or(other.match_timelimit),
            match_round_timelimit: self.match_round_timelimit.or(other.match_round_timelimit),
            match_oob_timer_enabled: self.match_oob_timer_enabled.or(other.match_oob_timer_enabled),

            titan_boost_meter_multiplier: self.titan_boost_meter_multiplier.or(other.titan_boost_meter_multiplier),
            titan_aegis_upgrades_enabled: self.titan_aegis_upgrades_enabled.or(other.titan_aegis_upgrades_enabled),
            titan_infinite_doomed_state_enabled: self.titan_infinite_doomed_state_enabled.or(other.titan_infinite_doomed_state_enabled),
            titan_shield_regen_enabled: self.titan_shield_regen_enabled.or(other.titan_shield_regen_enabled),
            titan_classic_rodeo_enabled: self.titan_classic_rodeo_enabled.or(other.titan_classic_rodeo_enabled),

            pilot_bleedout_mode: self.pilot_bleedout_mode.or(other.pilot_bleedout_mode),
            pilot_bleedout_holster_when_down: self.pilot_bleedout_holster_when_down.or(other.pilot_bleedout_holster_when_down),
            pilot_bleedout_die_on_team_bleedout: self.pilot_bleedout_die_on_team_bleedout.or(other.pilot_bleedout_die_on_team_bleedout),
            pilot_bleedout_bleedout_time: self.pilot_bleedout_bleedout_time.or(other.pilot_bleedout_bleedout_time),
            pilot_bleedout_firstaid_time: self.pilot_bleedout_firstaid_time.or(other.pilot_bleedout_firstaid_time),
            pilot_bleedout_selfres_time: self.pilot_bleedout_selfres_time.or(other.pilot_bleedout_selfres_time),
            pilot_bleedout_firstaid_heal_percent: self.pilot_bleedout_firstaid_heal_percent.or(other.pilot_bleedout_firstaid_heal_percent),
            pilot_bleedout_down_ai_miss_chance: self.pilot_bleedout_down_ai_miss_chance.or(other.pilot_bleedout_down_ai_miss_chance),

            promode_weapons_enabled: self.promode_weapons_enabled.or(other.promode_weapons_enabled),

            pilot_health_multiplier: self.pilot_health_multiplier.or(other.pilot_health_multiplier),
            pilot_respawn_delay: self.pilot_respawn_delay.or(other.pilot_respawn_delay),
            pilot_boosts_enabled: self.pilot_boosts_enabled.or(other.pilot_boosts_enabled),
            pilot_boost_meter_overdrive: self.pilot_boost_meter_overdrive.or(other.pilot_boost_meter_overdrive),
            pilot_boost_meter_multiplier: self.pilot_boost_meter_multiplier.or(other.pilot_boost_meter_multiplier),
            pilot_air_acceleration: self.pilot_air_acceleration.or(other.pilot_air_acceleration),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilledGameConfig {
    pub description: String,
    pub password: String,
    pub tick_rate: u32,
    pub report_to_master: bool,
    pub allow_insecure: bool,
    pub use_sockets_for_loopback: bool,
    pub everything_unlocked: bool,
    pub should_return_to_lobby: bool,
    pub player_permissions: PrivateLobbyPlayerPermissions,
    pub only_host_can_start: bool,
    pub countdown_length_seconds: u32,

    pub graphics_mode: GraphicsMode,
    pub priority: Priority,

    pub playlist: String,
    pub mode: Option<String>,
    pub map: Option<String>,
    pub default_mode: Option<String>,
    pub default_map: Option<String>,
    pub playlist_overrides: PlaylistOverrides,

    pub extra_playlist_vars: HashMap<String, String>,
    pub extra_vars: HashMap<String, String>,
    pub extra_args: Vec<String>,
}

#[derive(Default, Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GameConfig {
    pub description: Option<String>,
    pub password: Option<String>,
    pub tick_rate: Option<u32>,
    pub report_to_master: Option<bool>,
    pub allow_insecure: Option<bool>,
    pub use_sockets_for_loopback: Option<bool>,
    pub everything_unlocked: Option<bool>,
    pub should_return_to_lobby: Option<bool>,
    pub player_permissions: Option<PrivateLobbyPlayerPermissions>,
    pub only_host_can_start: Option<bool>,
    pub countdown_length_seconds: Option<u32>,

    pub graphics_mode: Option<GraphicsMode>,
    pub priority: Option<Priority>,

    pub playlist: Option<String>,
    pub mode: Option<String>,
    pub map: Option<String>,
    pub default_mode: Option<String>,
    pub default_map: Option<String>,

    #[serde(flatten)]
    pub playlist_overrides: PlaylistOverrides,

    #[serde(default)]
    pub extra_playlist_vars: HashMap<String, String>,

    #[serde(default)]
    pub extra_vars: HashMap<String, String>,

    #[serde(default)]
    pub extra_args: Vec<String>,
}

impl GameConfig {
    pub fn or(self, other: GameConfig) -> GameConfig {
        let mut extra_playlist_vars = other.extra_playlist_vars;
        extra_playlist_vars.extend(self.extra_playlist_vars);

        let mut extra_vars = other.extra_vars;
        extra_vars.extend(self.extra_vars);

        let mut extra_args = other.extra_args;
        extra_args.extend(self.extra_args);

        GameConfig {
            description: self.description.or(other.description),
            password: self.password.or(other.password),
            tick_rate: self.tick_rate.or(other.tick_rate),
            report_to_master: self.report_to_master.or(other.report_to_master),
            allow_insecure: self.allow_insecure.or(other.allow_insecure),
            use_sockets_for_loopback: self.use_sockets_for_loopback.or(other.use_sockets_for_loopback),
            everything_unlocked: self.everything_unlocked.or(other.everything_unlocked),
            should_return_to_lobby: self.should_return_to_lobby.or(other.should_return_to_lobby),
            player_permissions: self.player_permissions.or(other.player_permissions),
            only_host_can_start: self.only_host_can_start.or(other.only_host_can_start),
            countdown_length_seconds: self.countdown_length_seconds.or(other.countdown_length_seconds),

            graphics_mode: self.graphics_mode.or(other.graphics_mode),
            priority: self.priority.or(other.priority),

            playlist: self.playlist.or(other.playlist),
            mode: self.mode.or(other.mode),
            map: self.map.or(other.map),
            default_mode: self.default_mode.or(other.default_mode),
            default_map: self.default_map.or(other.default_map),

            playlist_overrides: self.playlist_overrides.or(other.playlist_overrides),

            extra_playlist_vars,
            extra_vars,
            extra_args,
        }
    }
}

impl Into<FilledGameConfig> for GameConfig {
    fn into(self) -> FilledGameConfig {
        FilledGameConfig {
            description: self.description.unwrap_or("Your favourite R2Wraith server".to_string()),
            password: self.password.unwrap_or("".to_string()),
            tick_rate: self.tick_rate.unwrap_or(20),
            report_to_master: self.report_to_master.unwrap_or(true),
            allow_insecure: self.allow_insecure.unwrap_or(false),
            use_sockets_for_loopback: self.use_sockets_for_loopback.unwrap_or(true),
            everything_unlocked: self.everything_unlocked.unwrap_or(true),
            should_return_to_lobby: self.should_return_to_lobby.unwrap_or(true),
            player_permissions: self.player_permissions.unwrap_or(PrivateLobbyPlayerPermissions::All),
            only_host_can_start: self.only_host_can_start.unwrap_or(false),
            countdown_length_seconds: self.countdown_length_seconds.unwrap_or(15),

            graphics_mode: self.graphics_mode.unwrap_or(GraphicsMode::Default),
            priority: self.priority.unwrap_or(Priority::High),

            playlist: self.playlist.unwrap_or("private_match".to_string()),
            mode: self.mode,
            map: self.map,
            default_mode: self.default_mode,
            default_map: self.default_map,

            playlist_overrides: self.playlist_overrides,

            extra_playlist_vars: self.extra_playlist_vars,
            extra_vars: self.extra_vars,
            extra_args: self.extra_args,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilledInstanceConfig {
    pub name: String,
    pub auth_port: Option<u16>,
    pub game_port: Option<u16>,
    pub game_config: FilledGameConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct InstanceConfig {
    pub name: String,
    pub auth_port: Option<u16>,
    pub game_port: Option<u16>,

    #[serde(flatten)]
    pub game_config: GameConfig,
}

impl InstanceConfig {
    pub fn make_filled(self, default_game_config: GameConfig) -> FilledInstanceConfig {
        FilledInstanceConfig {
            name: self.name,
            auth_port: self.auth_port,
            game_port: self.game_port,
            game_config: self.game_config.or(default_game_config).into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    #[serde(default = "default_poll_seconds")]
    pub poll_seconds: f64,

    #[serde(default = "default_executable_path")]
    pub executable_path: String,

    #[serde(default = "default_process_name")]
    pub process_name: String,

    #[serde(default = "default_auth_ports")]
    pub auth_ports: RangeInclusive<u16>,

    #[serde(default = "default_game_ports")]
    pub game_ports: RangeInclusive<u16>,

    #[serde(default)]
    pub defaults: GameConfig,

    pub servers: HashMap<String, InstanceConfig>,
}

fn default_poll_seconds() -> f64 {
    5.
}

fn default_executable_path() -> String { "NorthstarLauncher.exe".to_string() }

fn default_process_name() -> String {
    "Titanfall2-unpacked.exe".to_string()
}

fn default_auth_ports() -> RangeInclusive<u16> {
    8081..=8085
}

fn default_game_ports() -> RangeInclusive<u16> {
    37015..=37020
}
