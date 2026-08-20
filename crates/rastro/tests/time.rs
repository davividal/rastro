//! Reading the host's timekeeping, without needing a `timedatectl` to run.

use rastro::collectors::time::{ClockSettings, TimeCollector, Timedatectl};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, View};

/// The real output of `timedatectl show` on the development box.
const SHOW: &str = "\
Timezone=Etc/UTC
LocalRTC=no
CanNTP=yes
NTP=yes
NTPSynchronized=yes
TimeUSec=Thu 2026-08-20 09:43:44 UTC
RTCTimeUSec=Mon 2026-08-24 04:56:22 UTC
";

fn settings() -> ClockSettings {
    Timedatectl::parse(SHOW).expect("this output is well formed")
}

fn object_of(observation: &Observation) -> Vec<(String, Observation)> {
    match observation.content() {
        Content::Object(entries) => entries
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        other => panic!("expected an object, got {other:?}"),
    }
}

fn keys_of(observation: &Observation) -> Vec<String> {
    object_of(observation)
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

#[test]
fn parse_reads_the_timezone() {
    // Act: the value that moves every log timestamp, cron schedule and timer on the box.
    let settings = settings();

    // Assert
    assert_eq!(settings.timezone.as_str(), "Etc/UTC");
}

#[test]
fn parse_reads_the_yes_and_no_settings() {
    // Act
    let settings = settings();

    // Assert
    assert!(!settings.local_real_time_clock);
    assert!(settings.can_synchronise);
    assert!(settings.synchronisation_enabled);
    assert!(settings.synchronised);
}

#[test]
fn parse_reads_a_hardware_clock_running_in_local_time() {
    // Arrange: `true` here makes every timestamp ambiguous twice a year, so the two cases
    // must not be conflated.
    let local = SHOW.replace("LocalRTC=no", "LocalRTC=yes");

    // Act
    let settings = Timedatectl::parse(&local).expect("well formed");

    // Assert
    assert!(settings.local_real_time_clock);
}

#[test]
fn parse_keeps_the_clock_readings_as_the_tool_rendered_them() {
    // Act: despite the field being called `TimeUSec`, what the tool prints is a formatted
    // date rather than a microsecond count.
    let settings = settings();

    // Assert
    assert_eq!(
        settings.system_clock.as_str(),
        "Thu 2026-08-20 09:43:44 UTC"
    );
    assert_eq!(
        settings.hardware_clock.as_str(),
        "Mon 2026-08-24 04:56:22 UTC"
    );
}

#[test]
fn parse_refuses_output_missing_a_setting() {
    // Act: systemd prints all seven unconditionally, so a missing one means the output is
    // not what rastro believes. Defaulting `LocalRTC` to false would be a claim about how
    // the hardware clock runs.
    let result = Timedatectl::parse("Timezone=Etc/UTC\n");

    // Assert
    let failure = result.expect_err("a missing setting must not be defaulted");
    assert!(
        failure.to_string().contains("LocalRTC"),
        "the message must name the missing setting, got: {failure}"
    );
}

#[test]
fn the_clocks_do_not_reach_the_diffable_view_and_the_configuration_does() {
    // Arrange: measured. Two `timedatectl show` runs two seconds apart on the box differed
    // in exactly these two fields and nothing else.
    let observation = Observation::from(&settings());

    // Act
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives the diffable view");

    // Assert
    assert_eq!(
        keys_of(&diffable),
        [
            "can_synchronise",
            "local_real_time_clock",
            "synchronisation_enabled",
            "synchronised",
            "timezone"
        ]
    );
}

#[test]
fn the_complete_view_keeps_the_clocks() {
    // Act
    let observation = Observation::from(&settings());

    // Assert
    assert!(keys_of(&observation).contains(&"system_clock".to_owned()));
    assert!(keys_of(&observation).contains(&"hardware_clock".to_owned()));
}

#[test]
fn losing_synchronisation_stays_visible_in_a_diff() {
    // Arrange: a box that loses sync and does not regain it is a fault worth seeing, not
    // noise, which is why this field is not volatile alongside the clocks.
    let unsynchronised = SHOW.replace("NTPSynchronized=yes", "NTPSynchronized=no");

    // Act
    let observation = Observation::from(&Timedatectl::parse(&unsynchronised).expect("legal"));
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives");

    // Assert
    assert!(keys_of(&diffable).contains(&"synchronised".to_owned()));
}

#[test]
fn presence_is_undetermined_without_the_tool_rather_than_absent() {
    // Act: a box with no `timedatectl` still has a timezone and a hardware clock.
    match TimeCollector::reading(None).presence() {
        Presence::Undetermined { reason } => assert!(
            reason.contains("cannot be told"),
            "the reason must say rastro could not see, got: {reason}"
        ),
        other => panic!("expected an undetermined presence, got {other:?}"),
    }
}

#[test]
fn presence_is_present_when_the_tool_is_on_the_host() {
    // Arrange
    let timedatectl = Timedatectl::using(
        rastro::collectors::canonical_tool::CanonicalTool::located_in("sh", &["/bin"])
            .expect("every unix has /bin/sh"),
    );

    // Act & Assert
    assert_eq!(
        TimeCollector::reading(Some(timedatectl)).presence(),
        Presence::Present
    );
}

#[test]
fn collect_fails_rather_than_reporting_no_timekeeping_without_the_tool() {
    // Act & Assert
    assert!(TimeCollector::reading(None).collect().is_err());
}
