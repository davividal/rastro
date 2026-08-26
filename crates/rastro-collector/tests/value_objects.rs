use rastro_collector::{Content, Observation, ProcessName, Scalar, SettingValue};

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
