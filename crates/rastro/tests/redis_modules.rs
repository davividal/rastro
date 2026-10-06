//! What `MODULE LIST` becomes: the code a server has loaded into itself.

use rastro::collectors::redis::{ModuleList, Reply};
use rastro_collector::Observation;
use rastro_fingerprint::Sensitivity;

mod support;

use support::observation::{field, integer, is_null, items_of, keys_of, text};

fn bulk(text: &str) -> Reply {
    Reply::Bulk(text.to_owned())
}

/// One module as redis 7 describes it: name, version, where it was loaded from, and with what.
fn module(name: &str, version: i64, path: &str, args: &[&str]) -> Reply {
    Reply::Array(vec![
        bulk("name"),
        bulk(name),
        bulk("ver"),
        Reply::Integer(version),
        bulk("path"),
        bulk(path),
        bulk("args"),
        Reply::Array(args.iter().map(|arg| bulk(arg)).collect()),
    ])
}

fn modules(reply: Reply) -> Observation {
    Observation::from(&ModuleList::parse(reply).expect("a real reply"))
}

#[test]
fn each_module_is_keyed_by_name_with_its_version_and_origin() {
    // Arrange
    let reply = Reply::Array(vec![
        module(
            "search",
            21005,
            "/usr/lib/redis/modules/redisearch.so",
            &["MAXDOCTABLESIZE", "1000000"],
        ),
        module("ReJSON", 20609, "/usr/lib/redis/modules/rejson.so", &[]),
    ]);

    // Act
    let modules = modules(reply);

    // Assert
    assert_eq!(keys_of(&modules), ["ReJSON", "search"]);
    let search = field(&modules, "search");
    assert_eq!(integer(&field(&search, "version")), 21005);
    assert_eq!(
        text(&field(&search, "path")),
        "/usr/lib/redis/modules/redisearch.so"
    );
    let args: Vec<String> = items_of(&field(&search, "args")).iter().map(text).collect();
    assert_eq!(args, ["MAXDOCTABLESIZE", "1000000"]);
}

#[test]
fn an_older_server_names_no_path_and_no_arguments() {
    // Arrange: redis 5 answers the name and the version and nothing else.
    let reply = Reply::Array(vec![Reply::Array(vec![
        bulk("name"),
        bulk("ReJSON"),
        bulk("ver"),
        Reply::Integer(10007),
    ])]);

    // Act
    let rejson = field(&modules(reply), "ReJSON");

    // Assert: null, which is "not reported", never an empty list, which would be "none given".
    assert_eq!(integer(&field(&rejson, "version")), 10007);
    assert!(is_null(&field(&rejson, "path")));
    assert!(is_null(&field(&rejson, "args")));
}

#[test]
fn a_server_with_nothing_loaded_has_no_modules() {
    // Act & Assert
    assert!(keys_of(&modules(Reply::Array(Vec::new()))).is_empty());
}

#[test]
fn a_module_without_a_name_is_refused() {
    // Act
    let result = ModuleList::parse(Reply::Array(vec![Reply::Array(vec![
        bulk("ver"),
        Reply::Integer(1),
    ])]));

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_module_built_into_the_server_has_no_path() {
    // Arrange: measured on redis 8.0.2, whose built-in `vectorset` reports `path` empty.
    let reply = Reply::Array(vec![module("vectorset", 1, "", &[])]);

    // Act
    let vectorset = field(&modules(reply), "vectorset");

    // Assert: empty is not a file, so it is null like a path never reported.
    assert!(is_null(&field(&vectorset, "path")));
}

#[test]
fn a_modules_arguments_are_sensitive() {
    // Arrange: a module can take a password as a load argument, RediSearch documents one, and no
    // value is judged by its name, as with a container's environment.
    let reply = Reply::Array(vec![module(
        "search",
        21005,
        "/usr/lib/redis/modules/redisearch.so",
        &["OSS_GLOBAL_PASSWORD", "hunter2"],
    )]);

    // Act
    let args = field(&field(&modules(reply), "search"), "args");

    // Assert
    assert_eq!(args.sensitivity(), Sensitivity::Sensitive);
}
