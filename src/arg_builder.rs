use std::collections::HashSet;
use linked_hash_map::LinkedHashMap;
use crate::config::{BoostMeterOverdrive, FilledGameConfig, GraphicsMode, PilotBleedout, PlaylistOverrides, PrivateLobbyPlayerPermissions, Riff};

trait IntoVarValue {
    fn into_var_value(self) -> Option<String>;
}

impl<T> IntoVarValue for Option<T> where T: IntoVarValue {
    fn into_var_value(self) -> Option<String> {
        self.and_then(|val| val.into_var_value())
    }
}

impl IntoVarValue for bool {
    fn into_var_value(self) -> Option<String> {
        Some((self as u8).to_string())
    }
}

impl IntoVarValue for u16 {
    fn into_var_value(self) -> Option<String> {
        Some(self.to_string())
    }
}

impl IntoVarValue for u32 {
    fn into_var_value(self) -> Option<String> {
        Some(self.to_string())
    }
}

impl IntoVarValue for i32 {
    fn into_var_value(self) -> Option<String> {
        Some(self.to_string())
    }
}

impl IntoVarValue for u64 {
    fn into_var_value(self) -> Option<String> {
        Some(self.to_string())
    }
}

impl IntoVarValue for f64 {
    fn into_var_value(self) -> Option<String> {
        Some(self.to_string())
    }
}

impl IntoVarValue for String {
    fn into_var_value(self) -> Option<String> {
        Some(self)
    }
}

#[derive(Debug, Clone)]
pub struct ArgBuilder {
    kv_env_args: LinkedHashMap<String, String>,
    flag_args: HashSet<String>,
    kv_args: LinkedHashMap<String, String>,
    playlist_vars: LinkedHashMap<String, String>,
}

impl ArgBuilder {
    pub fn new() -> Self {
        let builder = ArgBuilder {
            kv_env_args: LinkedHashMap::new(),
            flag_args: HashSet::new(),
            kv_args: LinkedHashMap::new(),
            playlist_vars: LinkedHashMap::new(),
        };

        builder
            .set_kv("+spewlog_enable", false)
    }

    fn set_flag(mut self, key: &str, is_enabled: bool) -> Self {
        if is_enabled {
            self.flag_args.insert(key.to_string());
        } else {
            self.flag_args.remove(key);
        }
        self
    }

    fn set_kv_env(mut self, key: &str, value: impl IntoVarValue) -> Self {
        match value.into_var_value() {
            Some(val) => self.kv_env_args.insert(key.to_string(), val),
            None => self.kv_env_args.remove(key),
        };
        self
    }

    fn set_kv(mut self, key: &str, value: impl IntoVarValue) -> Self {
        match value.into_var_value() {
            Some(val) => self.kv_args.insert(key.to_string(), val),
            None => self.kv_args.remove(key),
        };
        self
    }

    fn set_playlist_var(mut self, key: &str, value: impl IntoVarValue) -> Self {
        match value.into_var_value() {
            Some(val) => self.playlist_vars.insert(key.to_string(), val),
            None => self.playlist_vars.remove(key),
        };
        self
    }

    pub fn set_name(self, name: String) -> Self {
        self.set_kv_env("NS_SERVER_NAME", name)
    }

    pub fn set_auth_port(self, auth_port: u16) -> Self {
        self.set_kv_env("NS_PORT_AUTH", auth_port)
    }

    pub fn set_game_port(self, game_port: u16) -> Self {
        self.set_kv_env("NS_PORT", game_port)
    }

    pub fn set_description(self, description: String) -> Self {
        self.set_kv_env("NS_SERVER_DESC", description)
    }

    pub fn set_password(self, password: String) -> Self {
        self.set_kv_env("NS_SERVER_PASSWORD", password)
    }

    pub fn set_tick_rate(self, tick_rate: u32) -> Self {
        self.set_kv("+base_tickinterval_mp", 1. / tick_rate as f64)
    }

    pub fn set_update_rate(self, update_rate: u32) -> Self {
        self.set_kv("+sv_updaterate_mp", update_rate)
            .set_kv("+sv_max_snapshots_multiplayer", update_rate * 15)
    }

    pub fn set_min_update_rate(self, min_update_rate: u32) -> Self {
        self.set_kv("+sv_minupdaterate", min_update_rate)
    }

    pub fn set_report_to_master(self, report_to_master: bool) -> Self {
        self.set_kv_env("NS_MASTERSERVER_REGISTER", report_to_master)
    }

    pub fn set_master_url(self, master_url: String) -> Self {
        self.set_kv_env("NS_MASTERSERVER_URL", master_url)
    }

    pub fn set_allow_insecure(self, allow_insecure: bool) -> Self {
        self.set_kv_env("NS_INSECURE", allow_insecure)
    }

    pub fn set_use_sockets_for_loopback(self, use_sockets_for_loopback: bool) -> Self {
        self.set_kv("+net_usesocketsforloopback", use_sockets_for_loopback)
    }

    pub fn set_everything_unlocked(self, everything_unlocked: bool) -> Self {
        self.set_kv("+everything_unlocked", everything_unlocked)
    }

    pub fn set_should_return_to_lobby(self, should_return_to_lobby: bool) -> Self {
        self.set_kv("+ns_should_return_to_lobby", should_return_to_lobby)
    }

    pub fn set_player_permissions(self, player_permissions: PrivateLobbyPlayerPermissions) -> Self {
        self.set_kv("+ns_private_match_only_host_can_change_settings", match player_permissions {
            PrivateLobbyPlayerPermissions::All => 0,
            PrivateLobbyPlayerPermissions::MapModeOnly => 1,
            PrivateLobbyPlayerPermissions::None => 2,
        })
    }

    pub fn set_only_host_can_start(self, only_host_can_start: bool) -> Self {
        self.set_kv("+ns_private_match_only_host_can_start", only_host_can_start)
    }

    pub fn set_countdown_length_seconds(self, countdown_length_seconds: u32) -> Self {
        self.set_kv("+ns_private_match_countdown_length", countdown_length_seconds)
    }

    pub fn set_graphics_mode(self, graphics_mode: GraphicsMode) -> Self {
        self.set_flag("-softwared3d11", graphics_mode == GraphicsMode::Software)
    }

    pub fn set_playlist(self, playlist: String) -> Self {
        self.set_kv("+setplaylist", playlist)
    }

    pub fn set_mode(self, mode: Option<String>) -> Self {
        self.set_kv("+mp_gamemode", mode)
    }

    pub fn set_map(self, map: Option<String>) -> Self {
        self.set_kv("+map", map)
    }

    pub fn set_default_mode(self, default_mode: Option<String>) -> Self {
        self.set_kv("+ns_private_match_last_mode", default_mode)
    }

    pub fn set_default_map(self, default_map: Option<String>) -> Self {
        self.set_kv("+ns_private_match_last_map", default_map)
    }

    pub fn set_playlist_overrides(self, playlist_overrides: PlaylistOverrides) -> Self {
        fn riff_value(exists: bool) -> Option<bool> {
            if exists { Some(true) } else { None }
        }

        self

            // Riffs
            .set_playlist_var("riff_floorislava", riff_value(playlist_overrides.riffs.contains(&Riff::FloorIsLava)))
            .set_playlist_var("featured_mode_all_holopilot", riff_value(playlist_overrides.riffs.contains(&Riff::AllHolopilot)))
            .set_playlist_var("featured_mode_all_grapple", riff_value(playlist_overrides.riffs.contains(&Riff::AllGrapple)))
            .set_playlist_var("featured_mode_all_phase", riff_value(playlist_overrides.riffs.contains(&Riff::AllPhase)))
            .set_playlist_var("featured_mode_all_ticks", riff_value(playlist_overrides.riffs.contains(&Riff::AllTicks)))
            .set_playlist_var("featured_mode_tactikill", riff_value(playlist_overrides.riffs.contains(&Riff::Tactikill)))
            .set_playlist_var("featured_mode_amped_tacticals", riff_value(playlist_overrides.riffs.contains(&Riff::AmpedTacticals)))
            .set_playlist_var("featured_mode_rocket_arena", riff_value(playlist_overrides.riffs.contains(&Riff::RocketArena)))
            .set_playlist_var("featured_mode_shotguns_snipers", riff_value(playlist_overrides.riffs.contains(&Riff::ShotgunsSnipers)))
            .set_playlist_var("iron_rules", riff_value(playlist_overrides.riffs.contains(&Riff::IronRules)))
            .set_playlist_var("fp_embark_enabled", riff_value(playlist_overrides.riffs.contains(&Riff::FirstPersonEmbark)))
            .set_playlist_var("riff_instagib", riff_value(playlist_overrides.riffs.contains(&Riff::Instagib)))

            // Match
            .set_playlist_var("classic_mp", playlist_overrides.match_classic_mp_enabled)
            .set_playlist_var("run_epilogue", playlist_overrides.match_epilogue_enabled)
            .set_playlist_var("scorelimit", playlist_overrides.match_scorelimit)
            .set_playlist_var("roundscorelimit", playlist_overrides.match_round_scorelimit)
            .set_playlist_var("timelimit", playlist_overrides.match_timelimit)
            .set_playlist_var("roundtimelimit", playlist_overrides.match_round_timelimit)
            .set_playlist_var("oob_timer_enabled", playlist_overrides.match_oob_timer_enabled)
            .set_playlist_var("max_players", playlist_overrides.match_max_players)
            .set_flag("-maxplayersplaylist", playlist_overrides.match_max_players.is_some())

            // Titan
            .set_playlist_var("earn_meter_titan_multiplier", playlist_overrides.titan_boost_meter_multiplier)
            .set_playlist_var("aegis_upgrades", playlist_overrides.titan_aegis_upgrades_enabled)
            .set_playlist_var("infinite_doomed_state", playlist_overrides.titan_infinite_doomed_state_enabled)
            .set_playlist_var("titan_shield_regen", playlist_overrides.titan_shield_regen_enabled)
            .set_playlist_var("classic_rodeo", playlist_overrides.titan_classic_rodeo_enabled)

            // Pilot bleedout
            .set_playlist_var("riff_player_bleedout", playlist_overrides.pilot_bleedout_mode.map(|value| match value {
                PilotBleedout::Default => 0,
                PilotBleedout::Disabled => 1,
                PilotBleedout::Enabled => 2,
            }))
            .set_playlist_var("player_bleedout_forceHolster", playlist_overrides.pilot_bleedout_holster_when_down)
            .set_playlist_var("player_bleedout_forceDeathOnTeamBleedout", playlist_overrides.pilot_bleedout_die_on_team_bleedout)
            .set_playlist_var("player_bleedout_bleedoutTime", playlist_overrides.pilot_bleedout_bleedout_time)
            .set_playlist_var("player_bleedout_firstAidTime", playlist_overrides.pilot_bleedout_firstaid_time)
            .set_playlist_var("player_bleedout_firstAidTimeSelf", playlist_overrides.pilot_bleedout_selfres_time)
            .set_playlist_var("player_bleedout_firstAidHealPercent", playlist_overrides.pilot_bleedout_firstaid_heal_percent)
            .set_playlist_var("player_bleedout_aiBleedingPlayerMissChance", playlist_overrides.pilot_bleedout_down_ai_miss_chance)

            // Promode
            .set_playlist_var("promode_enable", playlist_overrides.promode_weapons_enabled)

            // Pilot
            .set_playlist_var("pilot_health_multiplier", playlist_overrides.pilot_health_multiplier)
            .set_playlist_var("respawn_delay", playlist_overrides.pilot_respawn_delay)
            .set_playlist_var("boosts_enabled", playlist_overrides.pilot_boosts_enabled.map(|value| !value)) // backwards apparently?
            .set_playlist_var("earn_meter_pilot_overdrive", playlist_overrides.pilot_boost_meter_overdrive.map(|value| match value {
                BoostMeterOverdrive::Enabled => 0,
                BoostMeterOverdrive::Disabled => 1,
                BoostMeterOverdrive::Only => 2,
            }))
            .set_playlist_var("earn_meter_pilot_multiplier", playlist_overrides.pilot_boost_meter_multiplier)
            .set_playlist_var("custom_air_accel_pilot", playlist_overrides.pilot_air_acceleration)
            .set_playlist_var("no_pilot_collision", playlist_overrides.pilot_collision_enabled.map(|value| !value))
    }

    pub fn add_extra_playlist_vars(mut self, playlist_vars: LinkedHashMap<String, String>) -> Self {
        self.playlist_vars.extend(playlist_vars);
        self
    }

    pub fn add_extra_vars(mut self, extra_vars: LinkedHashMap<String, String>) -> Self {
        for (key, value) in extra_vars.into_iter() {
            self.kv_args.insert(format!("+{}", key), value);
        }
        self
    }

    pub fn set_game_config(self, game_config: FilledGameConfig) -> Self {
        self.set_description(game_config.description)
            .set_password(game_config.password)
            .set_tick_rate(game_config.tick_rate)
            .set_update_rate(game_config.update_rate)
            .set_report_to_master(game_config.report_to_master)
            .set_master_url(game_config.master_url)
            .set_allow_insecure(game_config.allow_insecure)
            .set_use_sockets_for_loopback(game_config.use_sockets_for_loopback)
            .set_everything_unlocked(game_config.everything_unlocked)
            .set_should_return_to_lobby(game_config.should_return_to_lobby)
            .set_player_permissions(game_config.player_permissions)
            .set_only_host_can_start(game_config.only_host_can_start)
            .set_countdown_length_seconds(game_config.countdown_length_seconds)
            .set_graphics_mode(game_config.graphics_mode)
            .set_playlist(game_config.playlist)
            .set_mode(game_config.mode)
            .set_map(game_config.map)
            .set_default_mode(game_config.default_mode)
            .set_default_map(game_config.default_map)
            .set_playlist_overrides(game_config.playlist_overrides)
            .add_extra_playlist_vars(game_config.extra_playlist_vars)
            .add_extra_vars(game_config.extra_vars)
    }

    pub fn build(self, out_envs: &mut Vec<String>) {
        let mut extra_args = Vec::new();
        extra_args.extend(self.flag_args);
        extra_args.extend(self.kv_args.into_iter().flat_map(|(key, value)| [key, value]));
        extra_args.push("+setplaylistvaroverrides".to_string());
        let playlist_args_list: Vec<_> = self.playlist_vars.into_iter().flat_map(|(key, value)| [key, value]).collect();
        extra_args.push(playlist_args_list.join(" "));

        let mut env_args = self.kv_env_args;
        env_args.insert("NS_EXTRA_ARGUMENTS".to_string(), extra_args.iter().map(|arg| format!("\"{}\"", arg)).collect::<Vec<_>>().join(" "));
        out_envs.extend(env_args.into_iter().map(|(key, value)| format!("{}={}", key, value)));
    }
}
