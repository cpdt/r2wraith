use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::Duration;
use log::{debug, info, warn, error};
use crate::arg_builder::KNOWN_VARS;
use crate::config::Config;
use crate::process::Process;
use crate::server_cluster::{SerializedServer, Server, ServerCluster, ServerState};

mod arg_builder;
mod config;
mod ports;
mod process;
mod server_cluster;

pub struct InstallConfig {
    full_exe_path: PathBuf,
    game_dir: PathBuf,
}

fn get_server_list_from_config(config: &Config) -> Vec<Server> {
    config.servers.iter().map(|(name, instance_config)| {
        let filled_instance_config = instance_config.clone().make_filled(config.defaults.clone());
        Server {
            name: name.clone(),
            config: filled_instance_config,
            state: ServerState::NotRunning,
            is_old: false,
        }
    }).collect()
}

enum ReplCommand {
    StopAll,
    StopWraith,
    SetServers(Vec<Server>),
    StopOld,
    RestartAll,
    Restart(String),
}

fn load_config(config_path: &Path) -> Result<Config, Box<dyn Error>> {
    Ok(toml::from_str(&std::fs::read_to_string(config_path)?)?)
}

fn load_serialized_servers(restore_path: &Path) -> Result<Vec<SerializedServer>, Box<dyn Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(restore_path)?)?)
}

fn store_serialized_servers(restore_path: &Path, server_cluster: &ServerCluster) -> Result<(), Box<dyn Error>> {
    let serialized_servers = serde_json::to_string(&server_cluster.serialize_all())?;
    std::fs::write(&restore_path, serialized_servers)?;
    Ok(())
}

fn main() {
    simple_logger::SimpleLogger::new().init().unwrap();

    let mut args = std::env::args();
    let exe_name = args.next().unwrap();

    let config_file_path = match args.next() {
        Some(path) => path,
        None => {
            eprintln!("Usage: {} [path to config file]", exe_name);
            eprintln!();
            exit(1);
        }
    };

    info!("R2Wraith {}", env!("CARGO_PKG_VERSION"));

    let full_config_path = std::env::current_dir().unwrap().join(&config_file_path);
    let restore_file_path = std::env::current_dir().unwrap().join(&format!("{}.restore.json", config_file_path));

    let config_dir = full_config_path.parent().unwrap();
    let config = match load_config(&full_config_path) {
        Ok(config) => config,
        Err(why) => {
            error!("Failed to read config file: {}", why);
            return;
        }
    };

    let restore_serialized_servers = match load_serialized_servers(&restore_file_path) {
        Ok(servers) => {
            match std::fs::remove_file(&restore_file_path) {
                Ok(()) => debug!("Removed restore file at {}", restore_file_path.display()),
                Err(why) => warn!("Failed to remove restore file at {}: {}", restore_file_path.display(), why),
            };

            servers
        },
        Err(why) => {
            warn!("Failed to load server restore data: {}", why);
            vec![]
        }
    };

    // Resolve the NorthstarLauncher.exe executable to tell where the Titanfall root is
    let full_exe_path = config_dir.join(&config.executable_path);
    let game_dir = full_exe_path.parent().unwrap();
    let install_config = InstallConfig {
        game_dir: game_dir.to_path_buf(),
        full_exe_path,
    };

    // Remove variables we need to override from the autoexec file
    clean_autoexec_file(KNOWN_VARS, &install_config);

    let (repl_sender, repl_receiver) = channel::<ReplCommand>();

    let mut servers = get_server_list_from_config(&config);

    // Patch restore data to the server list
    for serialized_server in restore_serialized_servers {
        let matching_server = match servers.iter_mut().find(|server| server.name == serialized_server.name) {
            Some(server) => server,
            None => {
                warn!("Server {} is no longer in the config, so won't be controlled by R2Wraith. It might still be running!", serialized_server.name);
                continue;
            }
        };

        let process = match Process::new(serialized_server.pid) {
            Some(process) => process,
            None => {
                warn!("Server {} doesn't appear to be running anymore (no process with ID {})", serialized_server.name, serialized_server.pid);
                continue;
            },
        };

        // Ignore the process if it has the wrong name
        if process.name != config.process_name {
            warn!("Server {} doesn't appear to be running anymore (process title changed to {})", serialized_server.name, process.name);
            continue;
        }

        debug!("Restored {} with process {}", matching_server.name, process.id);
        matching_server.state = ServerState::Running { process };
    }

    let server_thread = std::thread::spawn(move || {
        let mut server_cluster = ServerCluster::new(servers);
        loop {
            server_cluster.poll(&config, &install_config);

            match repl_receiver.recv_timeout(Duration::from_secs_f64(config.poll_seconds)) {
                Ok(ReplCommand::StopAll) => {
                    debug!("Stopping all servers...");
                    server_cluster.stop_all();
                    break;
                },
                Ok(ReplCommand::StopWraith) => {
                    match store_serialized_servers(&restore_file_path, &server_cluster) {
                        Ok(()) => debug!("Written restore details to {}", restore_file_path.display()),
                        Err(why) => error!("Failed to write restore details to {}: {}", restore_file_path.display(), why),
                    }
                    break;
                }
                Ok(ReplCommand::SetServers(servers)) => {
                    server_cluster.set_servers(servers);
                    info!("Finished reloading config");
                }
                Ok(ReplCommand::StopOld) => {
                    server_cluster.stop_old();
                }
                Ok(ReplCommand::RestartAll) => {
                    server_cluster.stop_all();
                }
                Ok(ReplCommand::Restart(server_name)) => {
                    server_cluster.stop_by_name(&server_name);
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    // Start REPL
    loop {
        let mut buffer = String::new();
        if let Err(_) = std::io::stdin().read_line(&mut buffer) {
            continue;
        }

        let command = buffer.trim();

        if command == "help" || command == "?" {
            println!("< Available commands:");
            println!("<   version - Display the version of R2Wraith");
            println!("<   stopwraith - Stop R2Wraith, keeping servers running and writing a restore file");
            println!("<   stopall - Shutdown all servers and stop R2Wraith");
            println!("<   restartall - Restart all servers");
            println!("<   restart [name] - Restart a server by name");
            println!("<   reload - Reload the configuration file, starting any added servers");
            println!("<   stopold - Stop any servers that have been removed from configuration");
        } else if command == "version" {
            println!("R2Wraith {}", env!("CARGO_PKG_VERSION"));
        } else if command == "stopall" {
            repl_sender.send(ReplCommand::StopAll).unwrap();
            break;
        } else if command == "stopwraith" {
            repl_sender.send(ReplCommand::StopWraith).unwrap();
            break;
        } else if command == "reload" {
            let new_config = match load_config(&full_config_path) {
                Ok(config) => config,
                Err(why) => {
                    println!("< Failed to read config file: {}", why);
                    continue;
                }
            };
            let new_servers = get_server_list_from_config(&new_config);
            repl_sender.send(ReplCommand::SetServers(new_servers)).unwrap();
        } else if command == "stopold" {
            repl_sender.send(ReplCommand::StopOld).unwrap();
        } else if command == "restartall" {
            repl_sender.send(ReplCommand::RestartAll).unwrap();
        } else if command.starts_with("restart ") {
            let server_name = command["restart ".len()..].trim();
            repl_sender.send(ReplCommand::Restart(server_name.to_string())).unwrap();
        } else {
            println!("< Unknown command {}", command);
        }
    }

    server_thread.join().unwrap();

    println!("Bye!");
}

fn clean_autoexec_file(vars: &[&str], install_config: &InstallConfig) {
    const AUTOEXEC_CFG_NAME: &str = "autoexec_ns_server.cfg";
    const AUTOEXEC_CFG_OLD_NAME: &str = "autoexec_ns_server.old.cfg";
    let autoexec_dir = install_config.game_dir.join("R2Northstar/mods/Northstar.CustomServers/mod/cfg");
    let autoexec_file_path = autoexec_dir.join(AUTOEXEC_CFG_NAME);
    let autoexec_old_file_path = autoexec_dir.join(AUTOEXEC_CFG_OLD_NAME);

    let autoexec_text = match std::fs::read_to_string(&autoexec_file_path) {
        Ok(text) => text,
        Err(why) => {
            warn!("Failed to read {}: {}", autoexec_file_path.display(), why);
            return;
        }
    };

    // Comment out all lines that start with a known variable
    let new_text = autoexec_text
        .split("\n")
        .map(|line| {
            if vars.iter().any(|var| line.starts_with(var)) {
                format!("#{}", line)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if new_text == autoexec_text {
        return;
    }

    info!("Removing some vars from your {} file so they can be overridden, the original file will be saved to {}", AUTOEXEC_CFG_NAME, AUTOEXEC_CFG_OLD_NAME);
    info!("(in {})", autoexec_dir.display());

    let write_old_result = std::fs::write(&autoexec_old_file_path, autoexec_text);
    if let Err(why) = write_old_result {
        warn!("Failed to write {}: {}", autoexec_old_file_path.display(), why);
        warn!("{} has not been modified.", AUTOEXEC_CFG_NAME);
        return;
    }

    let write_new_result = std::fs::write(&autoexec_file_path, new_text);
    if let Err(why) = write_new_result {
        warn!("Failed to write {}: {}", autoexec_file_path.display(), why);
        warn!("{} has not been modified.", AUTOEXEC_CFG_NAME);
        return;
    }
}
