//! The NFS server's effective export table, read from `etab`.
//!
//! The fixtures are `/var/lib/nfs/etab` as nfs-utils 2.6.2 on Debian 12 wrote it, from an
//! `/etc/exports` that left most options to their defaults.

mod support;

use rastro::collectors::nfs::Etab;
use rastro_collector::Observation;
use support::observation::{field, items_of, keys_of, text};

/// Two clients of one path and a third export with an `fsid`, defaults filled in by `exportfs`.
const ETAB: &str = "\
/srv/nfs/a\t127.0.0.1(rw,sync,wdelay,hide,nocrossmnt,secure,root_squash,no_all_squash,no_subtree_check,secure_locks,acl,no_pnfs,anonuid=65534,anongid=65534,sec=sys,rw,secure,root_squash,no_all_squash)
/srv/nfs/a\t10.0.0.0/8(ro,sync,wdelay,hide,nocrossmnt,secure,root_squash,no_all_squash,no_subtree_check,secure_locks,acl,no_pnfs,anonuid=65534,anongid=65534,sec=sys,ro,secure,root_squash,no_all_squash)
/srv/nfs/b\t*(ro,sync,wdelay,hide,nocrossmnt,secure,no_root_squash,no_all_squash,no_subtree_check,secure_locks,acl,no_pnfs,fsid=7,anonuid=65534,anongid=65534,sec=sys,ro,secure,no_root_squash,no_all_squash)
";

fn exports(etab: &str) -> Observation {
    Observation::from(&Etab::parse(etab).expect("a well-formed etab"))
}

fn texts(observation: &Observation) -> Vec<String> {
    items_of(observation).iter().map(text).collect()
}

#[test]
fn parse_keys_exports_by_path_then_client() {
    // Act
    let table = exports(ETAB);

    // Assert
    assert_eq!(keys_of(&table), vec!["/srv/nfs/a", "/srv/nfs/b"]);
    assert_eq!(
        keys_of(&field(&table, "/srv/nfs/a")),
        vec!["10.0.0.0/8", "127.0.0.1"]
    );
}

#[test]
fn parse_keeps_the_options_before_the_first_flavour_as_the_exports_own() {
    // Act
    let client = field(&field(&exports(ETAB), "/srv/nfs/b"), "*");

    // Assert: a set, so the order `exportfs` happened to write them in is not state.
    let options = texts(&field(&client, "options"));
    assert!(options.contains(&"fsid=7".to_owned()), "got {options:?}");
    assert!(
        options.contains(&"no_root_squash".to_owned()),
        "got {options:?}"
    );
    assert!(
        !options.iter().any(|option| option.starts_with("sec=")),
        "got {options:?}"
    );
    let mut sorted = options.clone();
    sorted.sort();
    assert_eq!(options, sorted);
}

#[test]
fn parse_groups_the_options_after_a_flavour_under_that_flavour() {
    // Act
    let client = field(&field(&exports(ETAB), "/srv/nfs/a"), "10.0.0.0/8");

    // Assert: `exportfs` writes each flavour's own access options after its `sec=`, and those are
    // what mountd applies to a client using that flavour.
    let security = items_of(&field(&client, "security"));
    assert_eq!(security.len(), 1);
    assert_eq!(texts(&field(&security[0], "flavours")), vec!["sys"]);
    assert_eq!(
        texts(&field(&security[0], "options")),
        vec!["no_all_squash", "ro", "root_squash", "secure"]
    );
}

#[test]
fn parse_keeps_the_flavours_of_one_group_in_preference_order() {
    // Arrange: `sec=krb5p:sys` in `/etc/exports`, as etab wrote it back.
    let etab =
        "/srv/nfs/b\t192.168.0.0/16(rw,sync,sec=krb5p:sys,rw,secure,root_squash,no_all_squash)\n";

    // Act
    let security = items_of(&field(
        &field(&field(&exports(etab), "/srv/nfs/b"), "192.168.0.0/16"),
        "security",
    ));

    // Assert: the order is the order the server offers them in, so it is state.
    assert_eq!(
        texts(&field(&security[0], "flavours")),
        vec!["krb5p", "sys"]
    );
}

#[test]
fn parse_keeps_one_group_per_sec_option() {
    // Arrange: flavours given different access, which `exportfs` writes as two groups.
    let etab = "/srv/x\t*(sync,sec=krb5,rw,sec=sys,ro)\n";

    // Act
    let security = items_of(&field(
        &field(&field(&exports(etab), "/srv/x"), "*"),
        "security",
    ));

    // Assert
    assert_eq!(security.len(), 2);
    assert_eq!(texts(&field(&security[0], "flavours")), vec!["krb5"]);
    assert_eq!(texts(&field(&security[0], "options")), vec!["rw"]);
    assert_eq!(texts(&field(&security[1], "flavours")), vec!["sys"]);
    assert_eq!(texts(&field(&security[1], "options")), vec!["ro"]);
}

#[test]
fn parse_reads_an_empty_table_as_no_exports() {
    // Act & Assert: what a stopped server leaves, since its unit runs `exportfs -au`.
    assert!(keys_of(&exports("")).is_empty());
}

#[test]
fn parse_refuses_a_path_exported_twice_to_one_client() {
    // Arrange
    let etab = "/srv/x\t*(ro,sec=sys,ro)\n/srv/x\t*(rw,sec=sys,rw)\n";

    // Act & Assert: keeping either would drop an export from a table claiming to be complete.
    assert!(Etab::parse(etab).is_err());
}

#[test]
fn parse_refuses_a_path_written_with_an_escape() {
    // Arrange: `exportfs` writes a space in a path as `\040`.
    let etab = "/srv/my\\040share\t*(ro,sec=sys,ro)\n";

    // Act & Assert: a path rastro cannot spell back is not reported under a wrong name.
    assert!(Etab::parse(etab).is_err());
}

#[test]
fn parse_refuses_a_line_it_cannot_split() {
    // Act & Assert
    assert!(Etab::parse("/srv/x *(ro)\n").is_err());
    assert!(Etab::parse("/srv/x\t*ro\n").is_err());
}
