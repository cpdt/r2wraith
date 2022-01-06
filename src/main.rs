use std::error::Error;
use std::path::Path;
use std::time::Duration;
use log::{debug, error, info, warn};
use shiplift::Docker;
use tokio::sync::mpsc::unbounded_channel;
use toml::Value::String;
use crate::config::Config;
use crate::server_cluster::{PollStatus, SerializedServer, Server, ServerCluster};

mod arg_builder;
mod config;
mod server_cluster;

enum ReplCommand<'docker> {
    StopAll,
    StopWraith,
    SetServers(Vec<Server<'docker>>),
    StopOld,
    RestartAll,
    Restart(String),
}

#[tokio::main]
async fn main() {
    simple_logger::SimpleLogger::new().init().unwrap();

    let mut args = std::env::args();
    let exe_name = args.next().unwrap();

    let config_file_path = match args.next() {
        Some(path) => path,
        None => {
            eprintln!("Usage: {} [path to config file]", exe_name);
            eprintln!();
            std::process::exit(1);
        }
    };

    info!("R2Wraith {}", env!("CARGO_PKG_VERSION"));

    match Docker::new().version().await {
        Ok(version) => info!("Docker {}", version.version),
        Err(why) => {
            error!("Failed to connect to Docker: {}", why);
            std::process::exit(1);
        }
    }

    let full_config_path = std::env::current_dir().unwrap().join(&config_file_path);
    let restore_file_path = std::env::current_dir().unwrap().join(&format!("{}.restore.json", config_file_path));

    let config_dir = full_config_path.parent().unwrap();
    let mut config = match load_config(&full_config_path) {
        Ok(config) => config,
        Err(why) => {
            error!("Failed to read config file: {}", why);
            std::process::exit(1);
        }
    };

    // Change the titanfall path to be relative to the config file
    config.game_dir = full_config_path
        .join(config.game_dir)
        .to_string_lossy()
        .into_string();

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

    let docker = Docker::new();
    let mut server_cluster = ServerCluster::new();
    server_cluster.load_servers(get_server_list_from_config(&config));
    server_cluster.deserialize(restore_serialized_servers, &docker).await;

    server_cluster.poll(&config, &docker).await;
    info!("Ready!");

    let (mut repl_sender, mut repl_receiver) = unbounded_channel::<ReplCommand>();

    let server_join_handle = tokio::spawn(async move {
        let docker = docker;
        loop {
            let receive_command = repl_receiver.recv();
            let wait_timeout = tokio::time::sleep(Duration::from_secs_f64(config.poll_seconds));

            tokio::select! {
                command = receive_command => {
                    match command {
                        Some(ReplCommand::StopAll) => {
                            debug!("Stopping all servers...");
                            server_cluster.stop_all().await;
                            break;
                        }
                        Some(ReplCommand::StopWraith) => {
                            match store_serialized_servers(&restore_file_path, &server_cluster) {
                                Ok(()) => debug!("Written restore details to {}", restore_file_path.display()),
                                Err(why) => error!("Failed to write restore details to {}: {}", restore_file_path.display(), why),
                            }
                            break;
                        }
                        Some(ReplCommand::SetServers(servers)) => {
                            server_cluster.load_servers(servers).await;
                            info!("Finished reloading config");
                        }
                        Some(ReplCommand::StopOld) => {
                            server_cluster.stop_old().await;
                        }
                        Some(ReplCommand::RestartAll) => {
                            server_cluster.stop_all().await;
                        }
                        Some(ReplCommand::Restart(server_name)) => {
                            match server_cluster.get_mut(&server_name) {
                                Some(server) => server.stop().await,
                                None => info!("Unknown server {}", server_name),
                            }
                        }
                        None => break,
                    }
                }
                _ = wait_timeout => {}
            }

            if let PollStatus::DidWork = server_cluster.poll(&config, &docker).await {
                info!("Done");
            }
        }
    });

    // Start REPL
    let repl_join_handle = tokio::task::spawn_blocking(|| {
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
                println!("< R2Wraith {}", env!("CARGO_PKG_VERSION"));
            } else if command == "stopwraith" {
                repl_sender.send(ReplCommand::StopWraith).unwrap();
                break;
            } else if command == "stopall" {
                repl_sender.send(ReplCommand::StopAll).unwrap();
                break;
            } else if command == "restartall" {
                repl_sender.send(ReplCommand::RestartAll).unwrap();
            } else if command.starts_with("restart ") {
                let server_name = command["restart ".len()..].trim();
                repl_sender.send(ReplCommand::Restart(server_name.to_string())).unwrap();
            } else if command.starts_with("reload") {
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
            }
         }
    });

    server_join_handle.await.unwrap();
    repl_join_handle.await.unwrap();
}

fn load_config(config_path: &Path) -> Result<Config, Box<dyn Error>> {
    Ok(toml::from_str(&std::fs::read_to_string(config_path)?)?)
}

fn load_serialized_servers(restore_path: &Path) -> Result<Vec<SerializedServer>, Box<dyn Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(restore_path)?)?)
}

fn get_server_list_from_config(config: &Config) -> Vec<Server> {
    config.servers.iter().map(|(name, instance_config)| {
        let filled_instance_config = instance_config.clone().make_filled(config.defaults.clone());
        Server::new(name.clone(), filled_instance_config)
    }).collect()
}
