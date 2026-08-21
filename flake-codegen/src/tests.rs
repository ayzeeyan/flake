use flake_ast::Source;

use crate::{compile_asm, compile_exe, run_native};

fn src(text: &str) -> Source {
    Source::new("t.flk", text)
}

#[test]
fn pe_starts_with_mz() {
    let pe = compile_exe(&src("fn main() { print(42) }")).expect("codegen");
    assert_eq!(&pe[0..2], b"MZ");
    assert_eq!(&pe[0x80..0x84], b"PE\0\0");
}

#[test]
fn asm_contains_main() {
    let asm = compile_asm(&src("fn add(a: Int, b: Int) -> Int { a + b }\nfn main() { print(add(2, 40)) }"))
        .expect("asm");
    assert!(asm.contains("main:"), "{asm}");
    assert!(asm.contains("add:"), "{asm}");
}

#[test]
fn native_prints_integer() {
    let out = run_native(&src("fn main() { print(41 + 1) }")).expect("run native");
    assert_eq!(out, "42\n");
}

#[test]
fn native_function_call() {
    let out = run_native(&src(
        "fn add(a: Int, b: Int) -> Int { a + b }\nfn main() { print(add(2, 40)) }",
    ))
    .expect("add native");
    assert_eq!(out, "42\n");
}

#[test]
fn native_hello() {
    let text = include_str!("../../examples/hello.flk");
    let out = run_native(&src(text)).expect("hello native");
    assert_eq!(out, "Hello, World!\n");
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}
