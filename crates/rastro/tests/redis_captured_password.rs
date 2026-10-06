//! Which password each captured server that answered `NOAUTH` would be sent, or why none.
//!
//! The route is the server's own start: its unit from its cgroup, the start command from
//! `systemctl`, the file from that command, and the `default` account replayed from the file. Each
//! case is one cell of `docs/redis-matrix.md` whose server wanted a password; a wrong one sent is
//! an entry in its `ACL LOG`.

use rastro::collectors::redis::password_for;

mod support;

use support::captured_redis_cell::{captured_box, process_id_of, servers_of};

/// What the route gives for the cell's only server: the password, or the refusal's text.
fn route_of(cell: &str) -> Result<String, String> {
    let servers = servers_of(cell);
    assert_eq!(servers.len(), 1, "cell {cell} captured one server");
    let captured = captured_box(cell);

    password_for(
        &captured.proc,
        process_id_of(&servers[0]),
        Some(&captured.systemctl),
    )
    .map(|credential| credential.password)
}

macro_rules! sent {
    ($name:ident, $cell:literal, $password:literal) => {
        #[test]
        fn $name() {
            // Act
            let route = route_of($cell);

            // Assert
            assert_eq!(route, Ok($password.to_owned()));
        }
    };
}

macro_rules! refused {
    ($name:ident, $cell:literal, $reason:literal) => {
        #[test]
        fn $name() {
            // Act
            let route = route_of($cell);

            // Assert: nothing to send, and the reason names what stopped it.
            let reason = route.expect_err("a refusal");
            assert!(reason.contains($reason), "{reason}");
        }
    };
}

sent!(
    cell_03_a_password_in_the_packaged_file_is_sent,
    "03",
    "cell-three-password"
);
refused!(
    cell_04_a_password_set_only_at_runtime_is_not_guessed,
    "04",
    "set at runtime"
);
sent!(
    cell_05_a_rewritten_file_is_sent_its_password_after_the_hash_agrees,
    "05",
    "cell-five-password"
);
refused!(
    cell_06_a_file_edited_after_a_rewrite_is_sent_nothing,
    "06",
    "hash"
);
sent!(
    cell_07_a_password_rotated_since_start_is_sent_the_files,
    "07",
    "cell-seven-at-start"
);
sent!(
    cell_08_the_acl_files_default_account_is_sent,
    "08",
    "cell-eight-default"
);
sent!(
    cell_15_a_password_in_a_globbed_drop_in_is_sent,
    "15",
    "cell-fifteen-drop-in"
);
refused!(
    cell_18_a_default_account_switched_off_is_sent_nothing,
    "18",
    "off"
);
refused!(
    cell_20_a_server_started_by_hand_is_not_guessed_at,
    "20",
    "does not run in a systemd service unit"
);
refused!(
    cell_25_a_server_in_a_container_is_not_guessed_at,
    "25",
    "does not run in a systemd service unit"
);
sent!(
    cell_27_valkey_under_its_own_unit_is_sent_its_password,
    "27",
    "cell-twenty-seven-password"
);
sent!(
    cell_29_valkeys_acl_file_is_read_for_a_unix_socket_server,
    "29",
    "cell-twenty-nine-default"
);
sent!(
    cell_32_debians_own_package_is_sent_its_password,
    "32",
    "cell-thirty-two-password"
);
sent!(
    cell_34_the_units_command_line_outranks_the_file,
    "34",
    "cell-thirty-four-command-line"
);
sent!(
    cell_35_redis_5_under_its_own_unit_is_sent_its_password,
    "35",
    "cell-thirty-five-password"
);
