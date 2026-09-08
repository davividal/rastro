//! What rastro means by a container engine, independent of any engine's spelling.

mod cgroup_control;
mod container_engine;
mod container_engines;
mod docker_engine;
mod docker_server;

pub use cgroup_control::CgroupControl;
pub use container_engine::ContainerEngine;
pub use container_engines::ContainerEngines;
pub use docker_engine::DockerEngine;
pub use docker_server::DockerServer;
