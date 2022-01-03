use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::RangeInclusive;
use std::process::{Command, ExitStatus};
use std::time::Duration;
use log::{debug, error, info, warn};
use crate::process::{Process, iter_processes, StopProcessError};
use crate::config::{Config, FilledInstanceConfig};
use crate::{InstallConfig};
use crate::arg_builder::ArgBuilder;
use serde::{Serialize, Deserialize};

#[derive(Debug)]
enum StartInstanceError {
    SpecificAuthPortInUse(u16),
    NoAuthPortsAvailable(RangeInclusive<u16>),
    SpecificGamePortInUse(u16),
    NoGamePortsAvailable(RangeInclusive<u16>),
    ServerReturnedBadStatus(ExitStatus),
    ProcessNotStartedInTime(String),
    ProcessCrashedWhileStarting,
}

impl Display for StartInstanceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StartInstanceError::SpecificAuthPortInUse(port) => write!(f, "Specified auth port {} is not free", port),
            StartInstanceError::NoAuthPortsAvailable(ports) => write!(f, "No auth ports between {} and {} are free", ports.start(), ports.end()),
            StartInstanceError::SpecificGamePortInUse(port) => write!(f, "Specified game port {} is not free", port),
            StartInstanceError::NoGamePortsAvailable(ports) => write!(f, "No game ports between {} and {} are free", ports.start(), ports.end()),
            StartInstanceError::ServerReturnedBadStatus(status) => write!(f, "Server returned failure status {}", status),
            StartInstanceError::ProcessNotStartedInTime(name) => write!(f, "A process matching the name {} did not start in time", name),
            StartInstanceError::ProcessCrashedWhileStarting => write!(f, "The process crashed while initializing"),
        }
    }
}

impl std::error::Error for StartInstanceError {}

pub struct Server {
    pub name: String,
    pub config: FilledInstanceConfig,
    pub state: ServerState,
    pub is_old: bool,
}

impl Server {
    fn stop(&mut self) {
        if let ServerState::Running(RunningServer { process, .. }) = &self.state {
            match process.stop() {
                Ok(_) => debug!("Stopped {} server process {}", self.name, process.id),
                Err(StopProcessError::TerminateFailed) => warn!("Could not stop {} server process {}", self.name, process.id),
                Err(StopProcessError::TimedOut) => warn!("Timed out waiting for {} server process {} to terminate", self.name, process.id),
            }
        }
        self.state = ServerState::NotRunning;
    }
}

pub enum ServerState {
    NotRunning,
    Running(RunningServer),
}

pub struct RunningServer {
    pub process: Process,
    pub auth_port: u16,
    pub game_port: u16,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SerializedServer {
    pub name: String,
    pub pid: u32,
    pub auth_port: u16,
    pub game_port: u16,
}

pub struct ServerCluster {
    pub servers: Vec<Server>,
}

impl ServerCluster {
    pub fn new(servers: Vec<Server>) -> Self {
        ServerCluster {
            servers
        }
    }

    pub fn set_servers(&mut self, mut new_servers: Vec<Server>) {
        for new_server in &mut new_servers {
            // Find an existing server with a matching name
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

    pub fn stop_old(&mut self) {
        for server in &mut self.servers {
            if server.is_old {
                server.stop();
            }
        }

        self.servers.retain(|server| !server.is_old);
    }

    pub fn poll(&mut self, config: &Config, install_config: &InstallConfig) {
        let mut did_start_anything = false;
        for server_index in 0..self.servers.len() {
            // If the server is currently marked as running, check if the process exists and has the
            // correct name
            let server = &mut self.servers[server_index];
            if let ServerState::Running(RunningServer { process, .. }) = &server.state {
                if !process.is_running() {
                    warn!("Server {} appears to have stopped (process {} is no longer running)", server.name, process.id);
                    server.state = ServerState::NotRunning;
                }
            }

            let server = &self.servers[server_index];
            if let ServerState::NotRunning = server.state {
                did_start_anything = true;
                let start_res = self.start_server(&server.name, &server.config, config, install_config);
                match start_res {
                    Ok(running_server) => self.servers[server_index].state = ServerState::Running(running_server),
                    Err(why) => {
                        error!("Could not start {}: {}", server.name, why);
                    }
                }
            }
        }

        if did_start_anything {
            info!("Done");
        }
    }

    pub fn stop_all(&mut self) {
        for server in &mut self.servers {
            server.stop();
        }
    }

    pub fn stop_by_name(&mut self, name: &str) {
        match self.servers.iter_mut().find(|server| server.name == name) {
            Some(server) => server.stop(),
            None => warn!("Unknown server {}", name)
        }
    }

    pub fn serialize_all(&self) -> Vec<SerializedServer> {
        self.servers
            .iter()
            .filter_map(|server| match &server.state {
                ServerState::Running(RunningServer { process, auth_port, game_port, .. }) => Some(SerializedServer {
                    name: server.name.clone(),
                    pid: process.id,
                    auth_port: *auth_port,
                    game_port: *game_port,
                }),
                _ => None,
            })
            .collect()
    }

    fn start_server(&self, name: &str, instance_config: &FilledInstanceConfig, config: &Config, install_config: &InstallConfig) -> Result<RunningServer, Box<dyn Error>> {
        let (auth_ports_in_use, game_ports_in_use): (HashSet<_>, HashSet<_>) = self.servers.iter().filter_map(|server| match &server.state {
            ServerState::NotRunning => None,
            ServerState::Running(RunningServer { auth_port, game_port, .. }) => Some((*auth_port, *game_port))
        }).unzip();

        let auth_port = match instance_config.auth_port {
            Some(port) if !auth_ports_in_use.contains(&port) => port,
            Some(used_port) => return Err(Box::new(StartInstanceError::SpecificAuthPortInUse(used_port))),
            None => {
                config.auth_ports
                    .clone()
                    .into_iter()
                    .find(|port| !auth_ports_in_use.contains(port))
                    .ok_or(Box::new(StartInstanceError::NoAuthPortsAvailable(config.auth_ports.clone())))?
            }
        };

        let game_port = match instance_config.game_port {
            Some(port) if !game_ports_in_use.contains(&port) => port,
            Some(used_port) => return Err(Box::new(StartInstanceError::SpecificGamePortInUse(used_port))),
            None => {
                config.game_ports
                    .clone()
                    .into_iter()
                    .find(|port| !game_ports_in_use.contains(port))
                    .ok_or(Box::new(StartInstanceError::NoGamePortsAvailable(config.game_ports.clone())))?
            }
        };

        let mut args = Vec::new();
        ArgBuilder::new()
            .set_name(instance_config.name.clone())
            .set_auth_port(auth_port)
            .set_game_port(game_port)
            .set_game_config(instance_config.game_config.clone())
            .build(&mut args);
        args.extend(instance_config.game_config.extra_args.iter().cloned());

        info!("Starting {} with auth port {} and game port {}", name, auth_port, game_port);
        let pretty_args = args.iter().map(|arg| format!("\"{}\"", arg)).collect::<Vec<_>>().join(" ");
        debug!("Running: \"{}\" {}", &config.executable_path, pretty_args);

        // Build a list of processes that the new one can't be before starting, so we don't attach
        // to the right server process.
        let invalid_pids: HashSet<_> = iter_processes()
            .map(|process| process.id)
            .collect();

        // Start the launcher executable
        let run_status = Command::new(&install_config.full_exe_path)
            .current_dir(&install_config.game_dir)
            .args(args)
            .status()?;

        if !run_status.success() {
            return Err(Box::new(StartInstanceError::ServerReturnedBadStatus(run_status)));
        }

        // Poll for a bit to wait for the server to start
        let check_exists_times = 5;
        for check_exists_index in 1..=check_exists_times {
            std::thread::sleep(Duration::from_secs(5));
            debug!("Checking for new processes matching {} (try {}/{})", config.process_name, check_exists_index, check_exists_times);

            let matching_processes = iter_processes()
                .find(|process| process.name == config.process_name && !invalid_pids.contains(&process.id));
            if let Some(process) = matching_processes {
                info!("Server {} has started with process {}", name, process.id);
                match process.set_priority(instance_config.game_config.priority) {
                    Ok(_) => debug!("Set priority of server process to {:?}", instance_config.game_config.priority),
                    Err(_) => warn!("Could not set priority of server process to {:?}", instance_config.game_config.priority)
                };

                loop {
                    std::thread::sleep(Duration::from_secs(1));

                    // Ensure the process is still running, so we don't get stuck in an infinite
                    // check loop
                    if !process.is_running() {
                        return Err(Box::new(StartInstanceError::ProcessCrashedWhileStarting));
                    }

                    if portpicker::is_free_tcp(auth_port) {
                        debug!("Waiting for server to be ready...");
                    } else {
                        info!("Server {} is ready!", name);
                        return Ok(RunningServer {
                            process,
                            auth_port,
                            game_port,
                        });
                    }
                }
            }
        }

        return Err(Box::new(StartInstanceError::ProcessNotStartedInTime(config.process_name.clone())));
    }
}
