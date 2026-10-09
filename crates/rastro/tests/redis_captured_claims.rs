//! Which host directory each captured server's `dir` is, from the kernel's mount tables alone.
//!
//! The server names its working directory in its own mount namespace; the walk is the host's. Its
//! `mountinfo` says which device that path is on and where inside it, and the host's says where
//! that device is mounted here. One case per cell of `docs/redis-matrix.md` that seals something.

use std::fs;
use std::path::{Path, PathBuf};

use rastro::collectors::mount_table::host_paths_of;

mod support;

use support::captured_redis_cell::{cell_directory, servers_of};

fn host_paths_in(cell: &str) -> Vec<PathBuf> {
    let server = &servers_of(cell)[0];
    let directory = fs::read_to_string(server.join("process/cwd.link")).expect("a captured cwd");
    let server_table = fs::read(server.join("process/mountinfo")).expect("a captured table");
    let host_table = fs::read(cell_directory(cell).join("host-mountinfo")).expect("a host table");

    host_paths_of(&server_table, &host_table, Path::new(directory.trim_end()))
}

macro_rules! cell {
    ($name:ident, $cell:literal, $host:literal) => {
        #[test]
        fn $name() {
            // Act
            let host_paths = host_paths_in($cell);

            // Assert
            assert_eq!(host_paths, [PathBuf::from($host)]);
        }
    };
}

cell!(
    cell_01_the_packages_directory_is_the_hosts_own,
    "01",
    "/var/lib/redis"
);
cell!(
    cell_16_a_directory_on_its_own_volume_is_found_through_it,
    "16",
    "/data/redis"
);
cell!(
    cell_24_a_named_volume_is_its_directory_under_docker,
    "24",
    "/var/lib/docker/volumes/redis24data/_data"
);

#[test]
fn cell_23_a_directory_in_the_containers_own_image_is_not_claimed() {
    // Act: measured, redis 8.10's image declares no volume, so `/data` is on its overlay root.
    let host_paths = host_paths_in("23");

    // Assert: the image is the containers facet's to account for, not the walk's to seal.
    assert!(host_paths.is_empty(), "{host_paths:?}");
}
