use rastro_collector::{
    Content, EnvironmentVariableName, Observation, ProcessName, Scalar, SettingValue,
};

fn text_of(observation: &Observation) -> &str {
    match observation.content() {
        Content::Scalar(Scalar::Text(value)) => value,
        other => panic!("expected a text observation, got {other:?}"),
    }
}

#[test]
fn process_name_rejects_empty_text() {
    // Act
    let result = ProcessName::new("");

    // Assert
    let error = result.expect_err("a process name cannot be empty");
    assert_eq!(error.to_string(), "a process name cannot be empty");
}

#[test]
fn process_name_preserves_the_kernel_text() {
    // Act
    let name = ProcessName::new("(sd-pam)").expect("this name is not empty");

    // Assert
    assert_eq!(name.as_str(), "(sd-pam)");
}

#[test]
fn process_name_renders_as_text() {
    // Arrange
    let name = ProcessName::new("systemd_exporte").expect("this name is not empty");

    // Act
    let observation = Observation::from(&name);

    // Assert
    assert_eq!(text_of(&observation), "systemd_exporte");
}

#[test]
fn setting_value_rejects_empty_text_with_its_kind() {
    // Act
    let result = SettingValue::new("", "sshd setting");

    // Assert
    let error = result.expect_err("a setting value cannot be empty");
    assert_eq!(error.to_string(), "a sshd setting cannot be empty");
}

#[test]
fn setting_value_preserves_the_reported_text() {
    // Act
    let value = SettingValue::new("prohibit-password", "sshd setting")
        .expect("this setting value is not empty");

    // Assert
    assert_eq!(value.as_str(), "prohibit-password");
}

#[test]
fn setting_value_renders_as_text() {
    // Arrange
    let value =
        SettingValue::new("0.0.0.0:9100", "flag value").expect("this setting value is not empty");

    // Act
    let observation = Observation::from(&value);

    // Assert
    assert_eq!(text_of(&observation), "0.0.0.0:9100");
}

#[test]
fn an_environment_variable_name_keeps_what_the_host_spelled() {
    // Arrange & Assert: POSIX reserves upper case for the shell's own variables and
    // `execve(2)` carries any byte but `=` and NUL, so a rule stricter than that would
    // refuse a name that is really on the box. Three collectors report these now, and each
    // has its own idea of what is well formed — enforcing the strictest here would make the
    // shared type lie about two of them.
    for spelling in ["PATH", "lowercase", "_leading", "dots.and-dashes", "1digit"] {
        assert!(
            EnvironmentVariableName::new(spelling).is_ok(),
            "{spelling} is a name the host can really set"
        );
    }
}

#[test]
fn an_environment_variable_name_refuses_the_separator() {
    // Arrange: the one exception, and it is not a style rule. `=` separates the name from
    // the value, so a name holding one means the entry was split in the wrong place and
    // whatever landed on either side of it is untrustworthy.

    // Act
    let result = EnvironmentVariableName::new("NAME=value");

    // Assert
    let failure = result.expect_err("a name cannot hold the separator");
    assert!(
        failure.to_string().contains("split in the wrong place"),
        "the reason should say what went wrong, got: {failure}"
    );
}

#[test]
fn an_environment_variable_name_refuses_empty_text() {
    // Act & Assert: an entry that begins with `=` names no variable.
    assert!(EnvironmentVariableName::new("").is_err());
}
