use crate::arg_builder::ArgBuilder;
use crate::config::FilledInstanceConfig;
use crate::Config;
use bollard::container::{CreateContainerOptions, LogsOptions};
use bollard::models::{
    ContainerInspectResponse, ContainerState, HostConfig, HostConfigLogConfig, PortBinding,
};
use bollard::Docker;
use chrono::{DateTime, Datelike, Timelike, Utc};
use futures::StreamExt;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::time::Duration;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;

#[derive(Debug)]
enum StartServerError {
    ContainerDidntStart(bollard::errors::Error),
    ContainerHasNoCreated,
}

impl Display for StartServerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StartServerError::ContainerDidntStart(err) => {
                write!(f, "The container did not start: {}", err)
            }
            StartServerError::ContainerHasNoCreated => {
                write!(f, "The container was not assigned a created time")
            }
        }
    }
}

impl std::error::Error for StartServerError {}

pub enum PollStatus {
    DidWork,
    NoWork,
}

#[derive(Debug)]
pub struct RunningServer {
    container_id: String,
    game_port: u16,
    start_time: DateTime<Utc>,
}

#[derive(Debug)]
pub enum ServerState {
    NotRunning,
    Running(RunningServer),
}

#[derive(Debug)]
pub struct Server {
    pub id: String,
    pub config: FilledInstanceConfig,
    pub state: ServerState,
    pub is_old: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SerializedServer {
    pub name: String,
    pub container_id: String,
    pub game_port: u16,
}

#[derive(Default)]
pub struct ServerCluster {
    servers: Vec<Server>,
}

impl Server {
    pub fn new(id: String, config: FilledInstanceConfig) -> Self {
        Server {
            id,
            config,
            state: ServerState::NotRunning,
            is_old: false,
        }
    }

    pub async fn start(&mut self, game_port: u16, docker: &Docker) -> Result<(), Box<dyn Error>> {
        let mut env_vars = Vec::new();
        ArgBuilder::new()
            .set_name(self.config.name.clone())
            .set_game_port(game_port)
            .set_game_config(self.config.game_config.clone())
            .build(&mut env_vars);

        info!("Starting {} with game port {}", self.id, game_port);
        debug!("Environment variables:");
        for env_var in &env_vars {
            debug!("  {}", env_var);
        }

        // Ensure the log directory exists
        if let Err(why) = tokio::fs::create_dir_all(&self.config.game_config.logs_dir).await {
            warn!(
                "Failed to create log directory {}: {}",
                self.config.game_config.logs_dir, why
            );
        }

        let start_time = Utc::now();
        let log_file_path = Path::new(&self.config.game_config.logs_dir).join(format!(
            "{} {}-{}-{} {}-{}-{}.txt",
            self.id,
            start_time.year(),
            start_time.month(),
            start_time.day(),
            start_time.hour(),
            start_time.minute(),
            start_time.second()
        ));

        let maybe_log_file = match OpenOptions::new()
            .write(true)
            .create(true)
            .open(&log_file_path)
            .await
        {
            Ok(file) => {
                info!("Writing logs to {}", log_file_path.display());
                Some(file)
            }
            Err(why) => {
                warn!(
                    "Failed to open log file {}: {}",
                    log_file_path.display(),
                    why
                );
                None
            }
        };

        let mut binds = vec![format!(
            "{}:/mnt/titanfall",
            self.config.game_config.game_dir
        )];
        binds.extend(self.config.game_config.mods.iter().filter_map(|mod_dir| {
            Path::new(mod_dir)
                .file_name()
                .and_then(|mod_name| mod_name.to_str())
                .map(|mod_name| format!("{}:/mnt/mods/{}:ro", mod_dir, mod_name))
        }));
        binds.extend(self.config.game_config.extra_binds.iter().cloned());

        let container_config = bollard::container::Config {
            image: Some(self.config.game_config.docker_image.clone()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            attach_stdin: Some(true),
            env: Some(env_vars),
            exposed_ports: Some(
                [(format!("{}/udp", game_port), HashMap::new())]
                    .into_iter()
                    .collect(),
            ),
            host_config: Some(HostConfig {
                binds: Some(binds),
                port_bindings: Some(
                    [(
                        format!("{}/udp", game_port),
                        Some(vec![PortBinding {
                            host_ip: None,
                            host_port: Some(game_port.to_string()),
                        }]),
                    )]
                    .into_iter()
                    .collect(),
                ),
                auto_remove: Some(true),

                memory: self.config.game_config.perf_memory_limit_bytes,
                memory_swap: self.config.game_config.perf_virtual_memory_limit_bytes,
                cpu_period: self.config.game_config.perf_cpus.map(|_| 100000),
                cpu_quota: self
                    .config
                    .game_config
                    .perf_cpus
                    .map(|cpus| (cpus * 100000.) as i64),
                cpuset_cpus: self.config.game_config.perf_cpu_set.clone(),

                log_config: Some(HostConfigLogConfig {
                    typ: Some("local".to_string()),
                    ..Default::default()
                }),

                ..Default::default()
            }),
            ..Default::default()
        };
        let create_response = docker
            .create_container(
                Some(CreateContainerOptions {
                    name: format!("r2wraith-{}", self.id),
                }),
                container_config,
            )
            .await?;
        if !create_response.warnings.is_empty() {
            for warning in &create_response.warnings {
                warn!("{}", warning);
            }
        }

        let container_id = create_response.id;
        docker
            .start_container::<String>(&container_id, None)
            .await?;

        if let Some(mut log_file) = maybe_log_file {
            let mut log_stream = docker.logs::<String>(
                &container_id,
                Some(LogsOptions {
                    follow: true,
                    stdout: true,
                    stderr: true,
                    ..Default::default()
                }),
            );
            tokio::spawn(async move {
                let maybe_res: Result<(), Box<dyn Error>> = async {
                    while let Some(v) = log_stream.next().await {
                        let stripped_v = strip_ansi_escapes::strip(v?.into_bytes())?;
                        log_file.write_all(&stripped_v).await?;
                    }
                    Ok(())
                }
                .await;

                if let Err(why) = maybe_res {
                    warn!("Failed to pipe logs: {}", why);
                }
                info!("Finished piping logs!");
            });
        }

        let inspect_response = docker
            .inspect_container(&container_id, None)
            .await
            .map_err(StartServerError::ContainerDidntStart)?;
        let start_time = get_container_created(&inspect_response)
            .ok_or(StartServerError::ContainerHasNoCreated)?;

        info!("Server {} has been started", self.id);

        self.state = ServerState::Running(RunningServer {
            container_id,
            game_port,
            start_time,
        });
        Ok(())
    }

    pub async fn stop(&mut self, docker: &Docker) {
        if let ServerState::Running(running_server) = &self.state {
            if let Err(why) = docker
                .stop_container(&running_server.container_id, None)
                .await
            {
                error!("Failed to stop {}: {}", self.id, why);
                return;
            }

            // Wait for the container to actually stop
            loop {
                if docker
                    .inspect_container(&running_server.container_id, None)
                    .await
                    .is_err()
                {
                    info!("Stopped {}", self.id);
                    break;
                }

                debug!("Waiting for {} to stop", self.id);
                sleep(Duration::from_millis(100)).await;
            }
        }
        self.state = ServerState::NotRunning;
    }
}

impl ServerCluster {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Server> {
        self.servers.iter_mut().find(|server| server.id == name)
    }

    pub fn load_servers(&mut self, mut new_servers: Vec<Server>) {
        for new_server in &mut new_servers {
            // Try to match this up with an existing server
            match self
                .servers
                .iter_mut()
                .find(|server| server.id == new_server.id)
            {
                Some(matching_server) => {
                    // Carry the state across from the old server
                    std::mem::swap(&mut new_server.state, &mut matching_server.state);

                    if new_server.config != matching_server.config {
                        warn!("Server {} config has changed, this will only apply the next time the server is started", new_server.id);
                    }
                }
                None => debug!("Loaded new server {}", new_server.id),
            }
        }

        let mut old_servers = new_servers;
        std::mem::swap(&mut old_servers, &mut self.servers);
        for mut old_server in old_servers {
            if let ServerState::Running { .. } = &old_server.state {
                warn!(
                    "Server {} is no longer in the config, use the \"stopold\" command to stop it",
                    old_server.id
                );

                old_server.is_old = true;
                self.servers.push(old_server);
            }
        }
    }

    pub async fn stop_old(&mut self, docker: &Docker) {
        for server in &mut self.servers {
            if server.is_old {
                server.stop(docker).await;
            }
        }

        self.servers.retain(|server| !server.is_old);
    }

    pub async fn stop_all(&mut self, docker: &Docker) {
        for server in &mut self.servers {
            server.stop(docker).await;
        }
    }

    pub fn serialize(&self) -> Vec<SerializedServer> {
        self.servers
            .iter()
            .filter_map(|server| match &server.state {
                ServerState::Running(RunningServer {
                    container_id,
                    game_port,
                    ..
                }) => Some(SerializedServer {
                    name: server.id.clone(),
                    container_id: container_id.to_string(),
                    game_port: *game_port,
                }),
                _ => None,
            })
            .collect()
    }

    pub async fn deserialize(
        &mut self,
        serialized_servers: Vec<SerializedServer>,
        docker: &Docker,
    ) {
        for serialized_server in serialized_servers {
            let matching_server = match self.get_mut(&serialized_server.name) {
                Some(server) => server,
                None => {
                    warn!("Server {} is no longer in the config, so won't be controlled by R2Wraith. It might still be running!", serialized_server.name);
                    continue;
                }
            };

            let maybe_inspect = docker
                .inspect_container(&serialized_server.container_id, None)
                .await
                .ok();
            let inspect = match maybe_inspect {
                Some(inspect) if get_container_is_running(&inspect) => inspect,
                _ => {
                    warn!(
                        "Server {} doesn't appear to be running anymore",
                        serialized_server.name
                    );
                    continue;
                }
            };
            let start_time = match get_container_created(&inspect) {
                Some(start_time) => start_time,
                None => {
                    warn!(
                        "Server {} does not have a valid created time",
                        serialized_server.name
                    );
                    continue;
                }
            };

            debug!(
                "Restored {} with container {}",
                matching_server.id, serialized_server.container_id
            );
            matching_server.state = ServerState::Running(RunningServer {
                container_id: serialized_server.container_id.clone(),
                game_port: serialized_server.game_port,
                start_time,
            });
        }
    }

    pub async fn poll(&mut self, config: &Config, docker: &Docker) -> PollStatus {
        let poll_time = Utc::now();
        let restart_servers_futures =
            self.servers
                .iter_mut()
                .enumerate()
                .map(|(server_index, server)| async move {
                    let running_server = match &server.state {
                        ServerState::Running(running_server) => running_server,
                        ServerState::NotRunning => return Some(server_index),
                    };

                    let has_stopped = matches!(
                        docker
                            .inspect_container(&running_server.container_id, None)
                            .await
                            .ok(),
                        None | Some(ContainerInspectResponse {
                            state: None
                                | Some(ContainerState {
                                    running: None | Some(false),
                                    ..
                                }),
                            ..
                        })
                    );
                    if has_stopped {
                        warn!(
                            "Server {} appears to have stopped (container {} is no longer running)",
                            server.id, running_server.container_id
                        );
                        server.state = ServerState::NotRunning;
                        return Some(server_index);
                    }

                    if let Some(schedule) = &server.config.game_config.restart_schedule {
                        if let Some(next_restart_time) =
                            schedule.after(&running_server.start_time).next()
                        {
                            if next_restart_time < poll_time {
                                warn!("Server {} has passed a scheduled restart", server.id);
                                server.stop(docker).await;
                                if let ServerState::NotRunning = server.state {
                                    return Some(server_index);
                                }
                            }
                        }
                    }

                    return None;
                });

        let restart_server_indices = futures::future::join_all(restart_servers_futures).await;
        let mut game_ports_in_use: HashSet<_> = self
            .servers
            .iter()
            .filter_map(|server| match &server.state {
                ServerState::NotRunning => None,
                ServerState::Running(RunningServer { game_port, .. }) => Some(*game_port),
            })
            .collect();

        struct RestartServerDetails {
            game_port: u16,
        }
        let restart_server_details = restart_server_indices
            .into_iter()
            .filter_map(|index| index)
            .filter_map(|server_index| {
                let server = &self.servers[server_index];

                // Allocate free ports
                let game_port = match server.config.game_port {
                    Some(port) if !game_ports_in_use.contains(&port) => port,
                    Some(used_port) => {
                        error!("Specified game port {} is not free", used_port);
                        return None;
                    }
                    None => match config
                        .game_ports
                        .clone()
                        .into_iter()
                        .find(|port| !game_ports_in_use.contains(port))
                    {
                        Some(port) => port,
                        None => {
                            error!(
                                "No game ports between {} and {} are free",
                                config.game_ports.start(),
                                config.game_ports.end()
                            );
                            return None;
                        }
                    },
                };

                // Ensure other servers can't use these ports
                game_ports_in_use.insert(game_port);

                Some((server_index, RestartServerDetails { game_port }))
            })
            .collect::<HashMap<_, _>>();

        if restart_server_details.is_empty() {
            return PollStatus::NoWork;
        }

        let restart_server_details = &restart_server_details;
        let start_server_futures =
            self.servers
                .iter_mut()
                .enumerate()
                .map(|(server_index, server)| async move {
                    let details = match restart_server_details.get(&server_index) {
                        Some(details) => details,
                        None => return,
                    };

                    if let Err(why) = server.start(details.game_port, docker).await {
                        error!("Could not start {}: {}", server.id, why);
                    }
                });
        futures::future::join_all(start_server_futures).await;
        PollStatus::DidWork
    }
}

fn get_container_is_running(inspect: &ContainerInspectResponse) -> bool {
    inspect
        .state
        .as_ref()
        .and_then(|state| state.running)
        .unwrap_or(false)
}

fn get_container_created(details: &ContainerInspectResponse) -> Option<DateTime<Utc>> {
    details
        .created
        .as_ref()
        .and_then(|time| DateTime::parse_from_rfc3339(time).ok())
        .map(|time| time.with_timezone(&Utc))
}
