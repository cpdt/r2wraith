use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};
use bollard::container::{CreateContainerOptions};
use bollard::Docker;
use bollard::models::{ContainerInspectResponse, ContainerState, HostConfig, PortBinding};
use log::{debug, error, info, warn};
use serde::{Serialize, Deserialize};
use tokio::net::TcpStream;
use crate::Config;
use crate::arg_builder::ArgBuilder;
use crate::config::FilledInstanceConfig;

#[derive(Debug)]
enum StartServerError {
    ContainerCrashedWhileStarting,
    ContainerHasNoIp,
}

impl Display for StartServerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StartServerError::ContainerCrashedWhileStarting => write!(f, "The container crashed while initializing"),
            StartServerError::ContainerHasNoIp => write!(f, "The container was not assigned an IP address"),
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
    pub id: String,
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
    pub fn new(id: String, config: FilledInstanceConfig) -> Self {
        Server {
            id,
            config,
            state: ServerState::NotRunning,
            is_old: false,
        }
    }

    pub async fn start(
        &mut self,
        auth_port: u16,
        game_port: u16,
        config: &Config,
        docker: &Docker
    ) -> Result<(), Box<dyn Error>> {
        // Ensure the log directory exists
        if let Err(why) = tokio::fs::create_dir_all(&self.config.game_config.logs_dir).await {
            warn!("Failed to create log directory {}: {}", self.config.game_config.logs_dir, why);
        }

        let mut env_vars = Vec::new();
        ArgBuilder::new()
            .set_name(self.config.name.clone())
            .set_auth_port(auth_port)
            .set_game_port(game_port)
            .set_game_config(self.config.game_config.clone())
            .build(&mut env_vars);

        info!("Starting {} with auth port {} and game port {}", self.id, auth_port, game_port);
        debug!("Environment variables:");
        for env_var in &env_vars {
            debug!("  {}", env_var);
        }

        let mut binds = vec![
            format!("{}:/mnt/titanfall", config.game_dir),
            format!("{}:/mnt/titanfall/R2Northstar/logs", self.config.game_config.logs_dir),
        ];
        if let Some(mods_dir) = &self.config.game_config.mods_dir {
            binds.push(format!("{}:/mnt/mods:ro", mods_dir));
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
                binds: Some(binds),
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

                memory: self.config.game_config.perf_memory_limit_bytes,
                memory_swap: self.config.game_config.perf_virtual_memory_limit_bytes,
                cpu_period: self.config.game_config.perf_cpus.map(|_| 100000),
                cpu_quota: self.config.game_config.perf_cpus.map(|cpus| (cpus * 100000.) as i64),
                cpuset_cpus: self.config.game_config.perf_cpu_set.clone(),

                ..Default::default()
            }),
            ..Default::default()
        };
        let create_response = docker
            .create_container(Some(CreateContainerOptions {
                name: format!("r2wraith-{}", self.id)
            }), container_config)
            .await?;
        if !create_response.warnings.is_empty() {
            for warning in &create_response.warnings {
                warn!("{}", warning);
            }
        }

        let container_id = create_response.id;
        docker.start_container::<String>(&container_id, None).await?;

        let inspect_response = docker.inspect_container(&container_id, None)
            .await
            .ok();
        let container_ip = inspect_response
            .as_ref()
            .and_then(get_container_ip_address)
            .and_then(|ip| ip.parse::<Ipv4Addr>().ok())
            .ok_or(StartServerError::ContainerHasNoIp)?;
        let container_auth_address = SocketAddrV4::new(container_ip, auth_port);

        info!("Server {} is starting...", self.id);
        let now = Instant::now();

        // Wait for the auth server to start on the required port
        loop {
            debug!("Waiting for {} to be ready...", self.id);
            tokio::time::sleep(Duration::from_secs(5)).await;

            // Ensure the container is still running so we don't get stuck in an infinite loop
            if !is_container_running(&container_id, docker).await {
                return Err(Box::new(StartServerError::ContainerCrashedWhileStarting));
            }

            if let Ok(_) = TcpStream::connect(container_auth_address).await {
                break;
            }
        }

        info!("Server {} has started in {}s", self.id, now.elapsed().as_secs_f64());

        self.state = ServerState::Running(RunningServer {
            container_id,
            auth_port,
            game_port,
        });
        Ok(())
    }

    pub async fn stop(&mut self, docker: &Docker) {
        if let ServerState::Running(running_server) = &self.state {
            match docker.stop_container(&running_server.container_id, None).await {
                Ok(()) => info!("Stopped {}", self.id),
                Err(why) => {
                    error!("Failed to stop {}: {}", self.id, why);
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
        self.servers.iter_mut().find(|server| server.id == name)
    }

    pub fn load_servers(&mut self, mut new_servers: Vec<Server>) {
        for new_server in &mut new_servers {
            // Try to match this up with an existing server
            match self.servers.iter_mut().find(|server| server.id == new_server.id) {
                Some(matching_server) => {
                    // Carry the state across from the old server
                    std::mem::swap(&mut new_server.state, &mut matching_server.state);

                    if new_server.config != matching_server.config {
                        warn!("Server {} config has changed, this will only apply the next time the server is started", new_server.id);
                    }
                },
                None => debug!("Loaded new server {}", new_server.id),
            }
        }

        let mut old_servers = new_servers;
        std::mem::swap(&mut old_servers, &mut self.servers);
        for mut old_server in old_servers {
            if let ServerState::Running { .. } = &old_server.state {
                warn!("Server {} is no longer in the config, use the \"stopold\" command to stop it", old_server.id);

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
                    name: server.id.clone(),
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

            debug!("Restored {} with container {}", matching_server.id, serialized_server.container_id);
            matching_server.state = ServerState::Running(RunningServer {
                container_id: serialized_server.container_id.clone(),
                auth_port: serialized_server.auth_port,
                game_port: serialized_server.game_port,
            });
        }
    }

    pub async fn poll(&mut self, config: &Config, docker: &Docker) -> PollStatus {
        let restart_servers_futures = self.servers.iter_mut().enumerate().map(|(server_index, server)| async move {
            let running_server = match &server.state {
                ServerState::Running(running_server) => running_server,
                ServerState::NotRunning => return Some(server_index),
            };

            let details = match docker.inspect_container(&running_server.container_id, None).await.ok() {
                None | Some(ContainerInspectResponse { state: None | Some(ContainerState { running: None | Some(false), .. }), .. }) => {
                    warn!("Server {} appears to have stopped (container {} is no longer running)", server.id, running_server.container_id);
                    server.state = ServerState::NotRunning;
                    return Some(server_index);
                }
                Some(details) => details,
            };
            let container_ip = match get_container_ip_address(&details).and_then(|address| address.parse::<Ipv4Addr>().ok()) {
                Some(ip) => ip,
                None => {
                    warn!("Failed to get local IP address of {}, not doing port check", server.id);
                    return None;
                }
            };

            if let Err(why) = TcpStream::connect(SocketAddrV4::new(container_ip, running_server.auth_port)).await {
                warn!("Server {} appears to have frozen (can't connect to auth server: {})", server.id, why);
                server.stop(docker).await;
                if let ServerState::NotRunning = &server.state {
                    return Some(server_index);
                }
            }

            return None;
        });

        let restart_server_indices = futures::future::join_all(restart_servers_futures).await;
        let (mut auth_ports_in_use, mut game_ports_in_use): (HashSet<_>, HashSet<_>) =
            self
                .servers
                .iter()
                .filter_map(|server| match &server.state {
                    ServerState::NotRunning => None,
                    ServerState::Running(RunningServer { auth_port, game_port, .. }) => Some((*auth_port, *game_port))
                })
                .unzip();

        struct RestartServerDetails {
            auth_port: u16,
            game_port: u16,
        }
        let restart_server_details = restart_server_indices
            .into_iter()
            .filter_map(|index| index)
            .filter_map(|server_index| {
                let server = &self.servers[server_index];

                // Allocate free ports
                let auth_port = match server.config.auth_port {
                    Some(port) if !auth_ports_in_use.contains(&port) => port,
                    Some(used_port) => {
                        error!("Specified auth port {} is not free", used_port);
                        return None;
                    }
                    None => match config
                        .auth_ports
                        .clone()
                        .into_iter()
                        .find(|port| !auth_ports_in_use.contains(port)) {
                        Some(port) => port,
                        None => {
                            error!("No auth ports between {} and {} are free", config.auth_ports.start(), config.auth_ports.end());
                            return None;
                        }
                    }
                };
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
                        .find(|port| !game_ports_in_use.contains(port)) {
                        Some(port) => port,
                        None => {
                            error!("No game ports between {} and {} are free", config.game_ports.start(), config.game_ports.end());
                            return None;
                        }
                    }
                };

                // Ensure other servers can't use these ports
                auth_ports_in_use.insert(auth_port);
                game_ports_in_use.insert(game_port);

                Some((
                    server_index,
                    RestartServerDetails {
                        auth_port,
                        game_port,
                    }
                ))
            })
            .collect::<HashMap<_, _>>();

        if restart_server_details.is_empty() {
            return PollStatus::NoWork;
        }

        let restart_server_details = &restart_server_details;
        let start_server_futures = self.servers.iter_mut().enumerate().map(|(server_index, server)| async move {
            let details = match restart_server_details.get(&server_index) {
                Some(details) => details,
                None => return,
            };

            if let Err(why) = server.start(details.auth_port, details.game_port, config, docker).await {
                error!("Could not start {}: {}", server.id, why);
            }
        });
        futures::future::join_all(start_server_futures).await;
        PollStatus::DidWork
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

fn get_container_ip_address(details: &ContainerInspectResponse) -> Option<&str> {
    let network_settings = details.network_settings.as_ref()?;
    let networks = network_settings.networks.as_ref()?;
    let first_network = networks.iter().next()?.1;
    Some(first_network.ip_address.as_ref()?)
}
