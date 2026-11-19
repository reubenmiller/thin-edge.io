//! Pins the compile errors (and especially their spans) produced by invalid
//! `define_config!` schemas — errors should point at the offending part of
//! the schema, not at generated code

#[test]
// The compiler diagnostics do not depend on the platform; they are checked on Linux,
// where the build of the test cases is cached
#[cfg_attr(windows, ignore = "the compiler output is checked on Linux")]
fn compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
