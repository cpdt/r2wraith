use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::RangeInclusive;
use std::time::Duration;
use bollard::container::{CreateContainerOptions};
use bollard::Docker;
use bollard::models::{HostConfig, PortBinding};
use log::{debug, error, info, warn};
use serde::{Serialize, Deserialize};
use crate::Config;
use crate::arg_builder::ArgBuilder;
use crate::config::FilledInstanceConfig;

#[derive(Debug)]
enum StartServerError {
    SpecificAuthPortInUse(u16),
    NoAuthPortsAvailable(RangeInclusive<u16>),
    SpecificGamePortInUse(u16),
    NoGamePortsAvailable(RangeInclusive<u16>),
    ProcessCrashedWhileStarting,
}

impl Display for StartServerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StartServerError::SpecificAuthPortInUse(port) => write!(f, "Specified auth port {} is not free", port),
            StartServerError::NoAuthPortsAvailable(ports) => write!(f, "No auth ports between {} and {} are free", ports.start(), ports.end()),
            StartServerError::SpecificGamePortInUse(port) => write!(f, "Specified game port {} is not free", port),
            StartServerError::NoGamePortsAvailable(ports) => write!(f, "No game ports between {} and {} are free", ports.start(), ports.end()),
            StartServerError::ProcessCrashedWhileStarting => write!(f, "The container crashed while initializing"),
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
    auth_port: u16,
    game_port: u16,
}

#[derive(Debug)]
pub enum ServerState {
    NotRunning,
    Running(RunningServer),
}

#[derive(Debug)]
pub struct Server {
    pub name: String,
    pub config: FilledInstanceConfig,
    pub state: ServerState,
    pub is_old: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SerializedServer {
    pub name: String,
    pub container_id: String,
    pub auth_port: u16,
    pub game_port: u16,
}

#[derive(Default)]
pub struct ServerCluster {
    servers: Vec<Server>,
}

impl Server {
    pub fn new(name: String, config: FilledInstanceConfig) -> Self {
        Server {
            name,
            config,
            state: ServerState::NotRunning,
            is_old: false,
        }
    }

    pub async fn stop(&mut self, docker: &Docker) {
        if let ServerState::Running(running_server) = &self.state {
            match docker.stop_container(&running_server.container_id, None).await {
                Ok(()) => info!("Stopped {}", self.name),
                Err(why) => {
                    error!("Failed to stop {}: {}", self.name, why);
                    return;
                }
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
        self.servers.iter_mut().find(|server| server.name == name)
    }

    pub fn load_servers(&mut self, mut new_servers: Vec<Server>) {
        for new_server in &mut new_servers {
            // Try to match this up with an existing server
            match self.servers.iter_mut().find(|server| server.name == new_server.name) {
                Some(matching_server) => {
                    // Carry the state across from the old server
                    std::mem::swap(&mut new_server.state, &mut matching_server.state);

                    if new_server.config != matching_server.config {
                        warn!("Server {} config has changed, this will only apply the next time the server is started", new_server.name);
                    }
                },
                None => debug!("Loaded new server {}", new_server.name),
            }
        }

        let mut old_servers = new_servers;
        std::mem::swap(&mut old_servers, &mut self.servers);
        for mut old_server in old_servers {
            if let ServerState::Running { .. } = &old_server.state {
                warn!("Server {} is no longer in the config, use the \"stopold\" command to stop it", old_server.name);

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
                ServerState::Running(RunningServer { container_id, auth_port, game_port }) => Some(SerializedServer {
                    name: server.name.clone(),
                    container_id: container_id.to_string(),
                    auth_port: *auth_port,
                    game_port: *game_port,
                }),
                _ => None,
            })
            .collect()
    }

    pub async fn deserialize(&mut self, serialized_servers: Vec<SerializedServer>, docker: &Docker) {
        for serialized_server in serialized_servers {
            let matching_server = match self.get_mut(&serialized_server.name) {
                Some(server) => server,
                None => {
                    warn!("Server {} is no longer in the config, so won't be controlled by R2Wraith. It might still be running!", serialized_server.name);
                    continue;
                }
            };

            if !is_container_running(&serialized_server.container_id, docker).await {
                warn!("Server {} doesn't appear to be running anymore", serialized_server.name);
                continue;
            }

            debug!("Restored {} with container {}", matching_server.name, serialized_server.container_id);
            matching_server.state = ServerState::Running(RunningServer {
                container_id: serialized_server.container_id.clone(),
                auth_port: serialized_server.auth_port,
                game_port: serialized_server.game_port,
            });
        }
    }

    pub async fn poll(&mut self, config: &Config, docker: &Docker) -> PollStatus {
        let mut status = PollStatus::NoWork;
        for server_index in 0..self.servers.len() {
            // If the server is currently marked as running, check if the container is still running
            let server = &mut self.servers[server_index];
            if let ServerState::Running(running_server) = &server.state {
                if !is_container_running(&running_server.container_id, docker).await {
                    warn!("Server {} appears to have stopped (container {} is no longer running)", server.name, running_server.container_id);
                    server.state = ServerState::NotRunning;
                }
            }

            let server = &self.servers[server_index];
            if let ServerState::NotRunning = server.state {
                status = PollStatus::DidWork;
                let start_res = self.start_server(&server.name, &server.config, config, docker).await;
                match start_res {
                    Ok(running_server) => self.servers[server_index].state = ServerState::Running(running_server),
                    Err(why) => error!("Could not start {}: {}", server.name, why),
                }
            }
        }
        status
    }

    async fn start_server(&self, name: &str, instance_config: &FilledInstanceConfig, config: &Config, docker: &Docker) -> Result<RunningServer, Box<dyn Error>> {
        let (auth_ports_in_use, game_ports_in_use): (HashSet<_>, HashSet<_>) = self.servers.iter().filter_map(|server| match &server.state {
            ServerState::NotRunning => None,
            ServerState::Running(RunningServer { auth_port, game_port, .. }) => Some((*auth_port, *game_port))
        }).unzip();

        let auth_port = match instance_config.auth_port {
            Some(port) if !auth_ports_in_use.contains(&port) => port,
            Some(used_port) => return Err(Box::new(StartServerError::SpecificAuthPortInUse(used_port))),
            None => {
                config.auth_ports
                    .clone()
                    .into_iter()
                    .find(|port| !auth_ports_in_use.contains(port))
                    .ok_or(Box::new(StartServerError::NoAuthPortsAvailable(config.auth_ports.clone())))?
            }
        };

        let game_port = match instance_config.game_port {
            Some(port) if !game_ports_in_use.contains(&port) => port,
            Some(used_port) => return Err(Box::new(StartServerError::SpecificGamePortInUse(used_port))),
            None => {
                config.game_ports
                    .clone()
                    .into_iter()
                    .find(|port| !game_ports_in_use.contains(port))
                    .ok_or(Box::new(StartServerError::NoGamePortsAvailable(config.game_ports.clone())))?
            }
        };

        let mut env_vars = Vec::new();
        ArgBuilder::new()
            .set_name(instance_config.name.clone())
            .set_auth_port(auth_port)
            .set_game_port(game_port)
            .set_game_config(instance_config.game_config.clone())
            .build(&mut env_vars);

        info!("Starting {} with auth port {} and game port {}", name, auth_port, game_port);
        debug!("Environment variables:");
        for env_var in &env_vars {
            debug!("  {}", env_var);
        }

        let container_config = bollard::container::Config {
            image: Some(config.docker_image.clone()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            env: Some(env_vars),
            exposed_ports: Some([
                (format!("{}/tcp", auth_port), HashMap::new()),
                (format!("{}/udp", game_port), HashMap::new()),
            ].into_iter().collect()),
            host_config: Some(HostConfig {
                binds: Some(vec![format!("{}:/mnt/titanfall:ro", config.game_dir)]),
                port_bindings: Some([
                    (format!("{}/tcp", auth_port), Some(vec![PortBinding {
                        host_ip: None,
                        host_port: Some(auth_port.to_string()),
                    }])),
                    (format!("{}/udp", game_port), Some(vec![PortBinding {
                        host_ip: None,
                        host_port: Some(game_port.to_string()),
                    }])),
                ].into_iter().collect()),
                auto_remove: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let create_response = docker
            .create_container(Some(CreateContainerOptions {
                name: format!("r2wraith-{}", name)
            }), container_config)
            .await?;
        if !create_response.warnings.is_empty() {
            for warning in &create_response.warnings {
                warn!("{}", warning);
            }
        }

        let container_id = create_response.id;
        docker.start_container::<String>(&container_id, None).await?;

        info!("Server {} has started with container {}", name, container_id);

        // Wait for the auth server to start on the required port
        loop {
            debug!("Waiting for server to be ready...");
            tokio::time::sleep(Duration::from_secs(5)).await;

            // Ensure the container is still running so we don't get stuck in an infinite loop
            if !is_container_running(&container_id, docker).await {
                return Err(Box::new(StartServerError::ProcessCrashedWhileStarting));
            }

            if !portpicker::is_free_tcp(auth_port) {
                break;
            }
        }

        Ok(RunningServer {
            container_id,
            auth_port,
            game_port,
        })
    }
}

async fn is_container_running(container_id: &str, docker: &Docker) -> bool {
    docker.inspect_container(container_id, None)
        .await
        .ok()
        .and_then(|details| details.state)
        .and_then(|state| state.running)
        .unwrap_or(false)
}
