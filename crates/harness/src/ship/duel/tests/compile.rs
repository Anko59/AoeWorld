use super::errors;

#[test]
fn compile_errors_keep_code_location_and_named_items() {
    let text = "   Compiling sim v0.1.0\n\
error[E0425]: cannot find function `new_api` in this scope\n  --> crates/sim/src/tests.rs:3:5\n   |\n3  |     new_api();\n\n\
error[E0432]: unresolved import `crate::thing::Made`\n --> crates/sim/tests/it.rs:1:5\n\n\
error: aborting due to 2 previous errors\n\nerror: could not compile `sim` (lib test) due to 2 previous errors\nwarning: build failed, waiting for other jobs to finish...\nerror: command `/usr/local/cargo/bin/cargo '--color=auto' test --no-run --package sim --locked` exited with code 101\n";
    let found = errors(text);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].path, "crates/sim/src/tests.rs");
    assert_eq!(found[0].code.as_deref(), Some("E0425"));
    assert_eq!(found[0].items, ["new_api"]);
    assert_eq!(found[1].path, "crates/sim/tests/it.rs");
    assert_eq!(found[1].items, ["crate::thing::Made"]);
}

#[test]
fn an_error_without_a_code_or_location_is_kept_so_it_is_not_explained_away() {
    let found =
        errors("error: linking with `cc` failed\nerror[E0599]: no method named `go` found\n");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].code, None);
    assert!(found[1].path.is_empty());
}
