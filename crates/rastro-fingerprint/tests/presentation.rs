use rastro_fingerprint::{Disclosure, Presentation, View};

#[test]
fn a_view_alone_means_the_safe_disclosure() {
    // Arrange & Act: the conversion is what keeps redaction on by default structural. A
    // caller who has never heard of the second axis cannot opt out of it by omission.
    let presentation = Presentation::from(View::Complete);

    // Assert
    assert_eq!(presentation.view(), View::Complete);
    assert_eq!(presentation.disclosure(), Disclosure::Redacted);
}

#[test]
fn the_named_constructors_carry_their_view_and_redact() {
    // Act
    let diffable = Presentation::diffable();
    let complete = Presentation::complete();

    // Assert
    assert_eq!(diffable.view(), View::Diffable);
    assert_eq!(complete.view(), View::Complete);
    assert_eq!(diffable.disclosure(), Disclosure::Redacted);
    assert_eq!(complete.disclosure(), Disclosure::Redacted);
}

#[test]
fn raw_opts_out_of_redaction_without_changing_the_view() {
    // Act
    let presentation = Presentation::diffable().raw();

    // Assert
    assert_eq!(presentation.disclosure(), Disclosure::Raw);
    assert_eq!(
        presentation.view(),
        View::Diffable,
        "opting out of redaction must not silently widen the view"
    );
}
