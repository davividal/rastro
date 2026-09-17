//! Tells cargo that the build version is an input, so changing it rebuilds.
//!
//! Without this the constant in `lib.rs` is baked in at the first compile and a later build
//! with a different `RASTRO_BUILD_VERSION` silently keeps the old string: `option_env!` is
//! read when the crate is compiled, and cargo has no other way to know it was consulted. CI
//! builds from a clean tree and would not notice; a developer reproducing a rolling build
//! locally would, and would be debugging the wrong thing.

fn main() {
    println!("cargo::rerun-if-env-changed=RASTRO_BUILD_VERSION");
}
