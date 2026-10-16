//! Which host directory each captured server's `dir` is, from the kernel's mount tables alone.
//!
//! The server names its working directory in its own mount namespace; the walk is the host's. Its
//! `mountinfo` says which device that path is on and where inside it, and the host's says where
//! that device is mounted here. One case per cell of `docs/redis-matrix.md` that seals something.
//!
//! The mount tables' half of what decides a claim, which is the half a capture carries: the other
//! checks the host paths on this box, which the cells' are not, and `redis_claims` tests it.

use std::fs;
use std::path::{Path, PathBuf};

use rastro::collectors::mount_table::{HostView, host_views_of};

mod support;

use support::captured_redis_cell::{cell_directory, servers_of};

fn host_views_in(cell: &str) -> Vec<HostView> {
    let server = &servers_of(cell)[0];
    let directory = fs::read_to_string(server.join("process/cwd.link")).expect("a captured cwd");
    let server_table = fs::read(server.join("process/mountinfo")).expect("a captured table");
    let host_table = fs::read(cell_directory(cell).join("host-mountinfo")).expect("a host table");

    host_views_of(&server_table, &host_table, Path::new(directory.trim_end()))
}

macro_rules! cell {
    ($name:ident, $cell:literal, $host:literal) => {
        #[test]
        fn $name() {
            // Act
            let host_views = host_views_in($cell);

            // Assert: the whole directory, at one host path.
            assert_eq!(
                host_views,
                [HostView {
                    path: PathBuf::from($host),
                    part: PathBuf::new(),
                }]
            );
        }
    };
}

cell!(
    cell_01_the_packages_directory_is_the_hosts_own,
    "01",
    "/var/lib/redis"
);
cell!(
    cell_24_a_named_volume_is_its_directory_under_docker,
    "24",
    "/var/lib/docker/volumes/redis24data/_data"
);
