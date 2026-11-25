//! Mistakes an author makes are compile errors, with a message that says
//! what to write instead.

#[test]
fn macro_misuse_fails_to_compile() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
