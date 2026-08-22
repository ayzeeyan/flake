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
    let asm = compile_asm(&src(
        "fn add(a: Int, b: Int) -> Int { a + b }\nfn main() { print(add(2, 40)) }",
    ))
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
fn native_concurrency_synchronous_fallback() {
    let out = run_native(&src(r#"
fn work(n: Int) -> Int { n + 1 }
fn main() / conc + io {
    let task: Task[Int] = spawn work(41)
    print(await task)
}
"#))
    .expect("native concurrency fallback");
    assert_eq!(out, "42\n");
}

#[test]
fn native_hello() {
    let text = include_str!("../../examples/hello.flk");
    let out = run_native(&src(text)).expect("hello native");
    assert_eq!(out, "Hello, World!\n");
}

#[test]
fn native_concat_int() {
    let out = run_native(&src(r#"fn main() { print("n={40+2}") }"#)).expect("concat");
    assert_eq!(out, "n=42\n");
}

#[test]
fn native_fibonacci() {
    let out = run_native(&src(r#"
fn fib(n: Int) -> Int {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
fn main() { print(fib(10)) }
"#))
    .expect("fib native");
    assert_eq!(out, "55\n");
}

#[test]
fn native_config() {
    let text = include_str!("../../examples/config.flk");
    let out = run_native(&src(text)).expect("config native");
    assert_eq!(out, "listening on localhost:8080\n");
}

#[test]
fn native_fizzbuzz() {
    let text = include_str!("../../examples/fizzbuzz.flk");
    let out = run_native(&src(text)).expect("fizzbuzz native");
    assert!(out.contains("FizzBuzz"), "{out}");
    assert!(out.contains("Fizz\n"), "{out}");
}

#[test]
fn native_lists() {
    let text = include_str!("../../examples/lists.flk");
    let out = run_native(&src(text)).expect("lists native");
    assert!(out.contains("sum = 15"), "{out}");
    assert!(out.contains("doubled = [2, 4, 6, 8, 10]"), "{out}");
    assert!(out.contains("clarity, crystallized"), "{out}");
}

#[test]
fn native_effects() {
    let text = include_str!("../../examples/effects.flk");
    let out = run_native(&src(text)).expect("effects native");
    assert_eq!(out, "Hello, Flake!\n2 + 2 = 4\n");
}

#[test]
fn native_ownership() {
    let text = include_str!("../../examples/ownership.flk");
    let out = run_native(&src(text)).expect("ownership native");
    assert!(out.contains("consumed strict"), "{out}");
    assert!(out.contains("once: gradual"), "{out}");
    assert!(out.contains("twice: gradual"), "{out}");
}

#[test]
fn native_maps() {
    let out = run_native(&src(r#"
fn main() {
    let m = {"host": "localhost", "port": 8080}
    print(m["host"])
    print(m["port"])
    print(len(m))
    m["port"] = 9090
    print(m["port"])
}
"#))
    .expect("maps native");
    assert_eq!(out, "localhost\n8080\n2\n9090\n");
}

#[test]
fn native_abs_min_max() {
    let out = run_native(&src(r#"
fn main() {
    print(abs(-7))
    print(min(3, 1, 4))
    print(max(3, 1, 4))
}
"#))
    .expect("abs min max");
    assert_eq!(out, "7\n1\n4\n");
}

#[test]
fn native_range_builtin() {
    let out = run_native(&src(r#"
fn main() {
    for n in range(3) {
        print(n)
    }
    for n in range(2, 5) {
        print(n)
    }
}
"#))
    .expect("range native");
    assert_eq!(out, "0\n1\n2\n2\n3\n4\n");
}

#[test]
fn native_pop_split_str_int() {
    let out = run_native(&src(r#"
fn main() {
    var xs = [1, 2, 3]
    print(pop(xs))
    print(len(xs))
    print(join(split("a,b,c", ","), "|"))
    print(int("42") + 1)
    print(str(7))
    print(type_of(1))
    print(len("flake"))
    print("hi"[0])
    assert(true)
}
"#))
    .expect("stdlib natives");
    assert_eq!(out, "3\n2\na|b|c\n43\n7\nInt\n5\nh\n");
}

#[test]
fn native_register_heavy_locals() {
    let out = run_native(&src(r#"
fn mix(a: Int, b: Int, c: Int, d: Int, e: Int, f: Int) -> Int {
    let s = a + b + c + d + e + f
    s + s + a
}
fn main() { print(mix(1, 2, 3, 4, 5, 6)) }
"#))
    .expect("regalloc");
    assert_eq!(out, "43\n");
}

#[test]
fn native_float_arith() {
    let out = run_native(&src(r#"
fn main() {
    print(int(1.5 + 2.5))
    print(int(float(10) / float(2)))
}
"#))
    .expect("float native");
    assert_eq!(out, "4\n5\n");
}

#[test]
fn asm_assigns_callee_saved_regs() {
    let asm = compile_asm(&src(r#"
fn add(a: Int, b: Int) -> Int { a + b }
fn main() { print(add(2, 40)) }
"#))
    .expect("asm");
    assert!(
        asm.contains("local 0 ->") && (asm.contains("Rbx") || asm.contains("[rbp")),
        "{asm}"
    );
}

#[test]
fn native_five_args() {
    let out = run_native(&src(r#"
fn sum5(a: Int, b: Int, c: Int, d: Int, e: Int) -> Int {
    a + b + c + d + e
}
fn main() { print(sum5(1, 2, 3, 4, 5)) }
"#))
    .expect("five args");
    assert_eq!(out, "15\n");
}

#[test]
fn native_string_eq() {
    let out = run_native(&src(r#"
fn main() {
    print("flake" == "flake")
    print("a" == "b")
    print("x" != "y")
}
"#))
    .expect("string eq");
    assert_eq!(out, "true\nfalse\ntrue\n");
}

#[test]
fn native_stdlib_natives() {
    let out = run_native(&src(r#"
fn main() {
    print(first([9, 8, 7]))
    print(last([9, 8, 7]))
    print(starts_with("flake", "fl"))
    print(ends_with("flake", "ke"))
    print(contains("abc", "b"))
    print(contains([1, 2, 3], 2))
    print(trim("  hi  "))
    print(upper("ab"))
    print(lower("AB"))
    print(file_exists("no-such-flake-file.txt"))
    print(len(cwd()) > 0)
}
"#))
    .expect("stdlib natives");
    assert_eq!(
        out,
        "9\n7\ntrue\ntrue\ntrue\ntrue\nhi\nAB\nab\nfalse\ntrue\n"
    );
}

#[test]
fn native_modules() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples");
    let path = dir.join("modules.flk");
    let text = std::fs::read_to_string(&path).expect("modules.flk");
    let source = flake_ast::Source::new(path.display().to_string(), text);
    let out = run_native(&source).expect("modules native");
    assert_eq!(out, "2 + 2 = 4\nsquare(5) = 25\n");
}

#[test]
fn native_write_file_roundtrip() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("flake-write-{}.txt", std::process::id()));
    let posix = path.to_string_lossy().replace('\\', "/");
    let program = format!(
        "fn main() {{ write_file(\"{posix}\", \"native-ok\") print(read_file(\"{posix}\")) }}"
    );
    let out = run_native(&src(&program)).expect("write_file native");
    let _ = std::fs::remove_file(&path);
    assert_eq!(out, "native-ok\n");
}

#[test]
fn native_read_file() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("flake-read-{}.txt", std::process::id()));
    std::fs::write(&path, "hello from disk").expect("write temp");
    let posix = path.to_string_lossy().replace('\\', "/");
    let program = format!("fn main() {{ print(read_file(\"{posix}\")) }}");
    let out = run_native(&src(&program)).expect("read_file native");
    let _ = std::fs::remove_file(&path);
    assert_eq!(out, "hello from disk\n");
}

#[test]
fn native_enums_and_match() {
    let text = include_str!("../../examples/enum.flk");
    let out = run_native(&src(text)).expect("enum native");
    assert_eq!(out, "red\nrgb 1,2,3\nok 42\nerr nope\n");
}

#[test]
fn native_result_try_and_literal_patterns() {
    let out = run_native(&src(r#"
enum Result { Ok(Int) Err(String) }
fn source(ok: Bool) -> Result {
    if ok { Result.Ok(40) } else { Result.Err("missing") }
}
fn add_two(ok: Bool) -> Result {
    let value = source(ok)?
    Result.Ok(value + 2)
}
fn show(result: Result) -> String {
    match result {
        Result.Ok(value) => "ok {value}"
        Result.Err(message) => "err {message}"
    }
}
fn classify(value: Int) -> String {
    match value { 0 => "zero" 42 => "answer" _ => "other" }
}
fn main() {
    print(show(add_two(true)))
    print(show(add_two(false)))
    print(classify(42))
}
"#))
    .expect("result and literal patterns native");
    assert_eq!(out, "ok 42\nerr missing\nanswer\n");
}

#[test]
fn native_result_payload_types_survive_try_and_match() {
    let out = run_native(&src(r#"
enum TextResult { Ok(String) Err(String) }
enum FlagResult { Ok(Bool) Err(String) }

fn text_source(ok: Bool) -> TextResult {
    if ok { TextResult.Ok("flake") } else { TextResult.Err("missing text") }
}
fn classify_text(ok: Bool) -> TextResult {
    let text = text_source(ok)?
    TextResult.Ok(match text { "flake" => "snow" _ => "other" })
}
fn show_text(result: TextResult) -> String {
    match result {
        TextResult.Ok(value) => value
        TextResult.Err(message) => message
    }
}

fn flag_source() -> FlagResult { FlagResult.Ok(true) }
fn flip_flag() -> FlagResult {
    let flag = flag_source()?
    FlagResult.Ok(match flag { true => false false => true })
}
fn show_flag(result: FlagResult) -> String {
    match result {
        FlagResult.Ok(value) => "flag {value}"
        FlagResult.Err(message) => message
    }
}

fn main() {
    print(show_text(classify_text(true)))
    print(show_text(classify_text(false)))
    print(show_flag(flip_flag()))
}
"#))
    .expect("typed result payloads native");
    assert_eq!(out, "snow\nmissing text\nflag false\n");
}

#[test]
fn native_int_maps_and_membership() {
    let out = run_native(&src(r#"
fn main() {
    let values = { 1: "one", 2: "two" }
    print(values[1])
    values[2] = "second"
    print(values[2])
    print(contains(values, 1))
    print(contains(values, 3))
    print(values)
    let flags = { false: "off", true: "on" }
    print(flags)
}
"#))
    .expect("integer map native");
    assert_eq!(
        out,
        "one\nsecond\ntrue\nfalse\n{1: \"one\", 2: \"second\"}\n{false: \"off\", true: \"on\"}\n"
    );
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}
