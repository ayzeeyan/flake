use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SOURCE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
enum Backend {
    Interpreter,
    Vm,
    Native,
}

impl Backend {
    const ALL: [Self; 3] = [Self::Interpreter, Self::Vm, Self::Native];

    fn flag(self) -> Option<&'static str> {
        match self {
            Self::Interpreter => None,
            Self::Vm => Some("--vm"),
            Self::Native => Some("--native"),
        }
    }
}

struct TempSource {
    path: PathBuf,
}

impl TempSource {
    fn new(label: &str, source: &str) -> Self {
        let id = NEXT_SOURCE_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("flake-m5-{label}-{}-{id}.flk", std::process::id()));
        std::fs::write(&path, source).expect("write temporary Flake source");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempSource {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn flake_bin() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_flake"));
    command.env("NO_COLOR", "1");
    let std_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("std");
    command.env("FLAKE_STD", std_dir);
    command
}

fn run(source: &TempSource, backend: Backend, skip_check: bool) -> Output {
    let mut command = flake_bin();
    command.arg("run");
    if skip_check {
        command.arg("--skip-check");
    }
    if let Some(flag) = backend.flag() {
        command.arg(flag);
    }
    command
        .arg(source.path())
        .output()
        .unwrap_or_else(|error| panic!("run {backend:?}: {error}"))
}

fn assert_all_backends(label: &str, source: &str, expected: &str) {
    let source = TempSource::new(label, source);
    for backend in Backend::ALL {
        let output = run(&source, backend, false);
        assert!(
            output.status.success(),
            "{label} failed on {backend:?}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{label} output diverged on {backend:?}"
        );
    }
}

fn assert_all_backends_fail(label: &str, source: &str, skip_check: bool, markers: &[&str]) {
    let source = TempSource::new(label, source);
    for backend in Backend::ALL {
        let output = run(&source, backend, skip_check);
        assert!(
            !output.status.success(),
            "{label} unexpectedly succeeded on {backend:?}:\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        for marker in markers {
            assert!(
                stderr.contains(&marker.to_lowercase()),
                "{label} on {backend:?} omitted `{marker}`:\n{stderr}"
            );
        }
    }
}

#[test]
fn language_feature_matrix_agrees_across_all_backends() {
    let cases = [
        (
            "numeric-control",
            r#"
fn sum_until(limit: Int) -> Int {
    var total = 0
    for n in range(1, limit) {
        if n == 4 { continue }
        total += n
    }
    total
}
fn main() {
    print(sum_until(7))
    print(17 % 5)
    print(2 + 3 * 4)
    print(-3 < -2 && 9 >= 9)
}
"#,
            "17\n2\n14\ntrue\n",
        ),
        (
            "float-helpers",
            r#"
fn main() {
    print(abs(-1.25))
    print(min(4.5, -2.25, 3.0))
    print(max(-4.5, -2.25, -3.0))
    print(1 + 2.5)
    print(7.5 % 2.0)
    print(2 < 2.5)
    let nan = 0.0 / 0.0
    print(nan == nan, nan != nan)
    print(float(7) / 2.0)
    print(int(9.75))
}
"#,
            "1.25\n-2.25\n-2.25\n3.5\n1.5\ntrue\nfalse true\n3.5\n9\n",
        ),
        (
            "strings-and-prelude",
            r#"
fn main() {
    print(join(split("flake::lang", "::"), "/"))
    print(trim("  snow  "), upper("ice"), lower("CRYSTAL"))
    print(starts_with("flake", "fl"), ends_with("flake", "ke"))
    print(contains("crystal", "sta"))
    print(first("snow"), last("flake"))
    print(str(42), type_of(42), len("flake"))
    assert(true, "unreachable")
}
"#,
            "flake/lang\nsnow ICE crystal\ntrue true\ntrue\ns e\n42 Int 5\n",
        ),
        (
            "mutable-lists",
            r#"
fn main() {
    var values = [1, 2, 3]
    values[-1] += 7
    push(values, 11)
    print(values)
    print(pop(values))
    print(values[0], values[-1], len(values), contains(values, 2))
    let words = ["snow", "flake"]
    let ratios = [1.25, -2.5]
    let flags = [true, false]
    print(words)
    print(ratios, ratios[0])
    print(flags)
}
"#,
            concat!(
                "[1, 2, 10, 11]\n11\n1 10 3 true\n",
                "[\"snow\", \"flake\"]\n",
                "[1.25, -2.5] 1.25\n",
                "[true, false]\n",
            ),
        ),
        (
            "typed-maps",
            r#"
fn main() {
    let ports = { "http": 80, "https": 443 }
    ports["http"] += 1
    print(ports["http"], contains(ports, "smtp"), len(ports))

    let names = { 2: "two", 1: "one" }
    names[2] = "second"
    print(names)

    let flags = { false: 0, true: 1 }
    print(flags[true], flags)

    let growing = { 9: "nine" }
    growing[8] = "eight"
    growing[7] = "seven"
    growing[6] = "six"
    growing[5] = "five"
    growing[4] = "four"
    growing[3] = "three"
    growing[2] = "two"
    growing[1] = "one"
    growing[0] = "zero"
    print(growing)

    let letters = { "z": 26, "a": 1, "m": 13 }
    print(letters)
}
"#,
            concat!(
                "81 false 2\n",
                "{1: \"one\", 2: \"second\"}\n",
                "1 {false: 0, true: 1}\n",
                "{0: \"zero\", 1: \"one\", 2: \"two\", 3: \"three\", 4: \"four\", ",
                "5: \"five\", 6: \"six\", 7: \"seven\", 8: \"eight\", 9: \"nine\"}\n",
                "{\"a\": 1, \"m\": 13, \"z\": 26}\n",
            ),
        ),
        (
            "struct-mutation",
            r#"
struct Counter { value: Int label: String }

fn bump(counter: Counter) -> Int {
    counter.value += 1
    counter.value
}

fn main() {
    let counter = Counter { value: 40, label: "jobs" }
    print(bump(counter), bump(counter), counter.value, counter.label)
}
"#,
            "41 42 42 jobs\n",
        ),
        (
            "enums-and-patterns",
            r#"
enum Message { Quit Code(Int) Text(String) }

fn show(message: Message) -> String {
    match message {
        Message.Quit => "quit"
        Message.Code(value) => match value { 0 => "zero" 42 => "answer" _ => "code" }
        Message.Text(value) => match value { "flake" => "snow" _ => value }
    }
}

fn main() {
    print(show(Message.Quit))
    print(show(Message.Code(42)))
    print(show(Message.Text("flake")))
    print(match true { true => "yes" false => "no" })
}
"#,
            "quit\nanswer\nsnow\nyes\n",
        ),
        (
            "result-propagation",
            r#"
enum Result { Ok(String) Err(String) }

fn load(found: Bool) -> Result {
    if found { Result.Ok("flake") } else { Result.Err("missing") }
}

fn decorate(found: Bool) -> Result {
    let value = load(found)?
    Result.Ok("<{value}>")
}

fn show(result: Result) -> String {
    match result {
        Result.Ok(value) => "ok {value}"
        Result.Err(error) => "err {error}"
    }
}

fn main() {
    print(show(decorate(true)))
    print(show(decorate(false)))
}
"#,
            "ok <flake>\nerr missing\n",
        ),
        (
            "indirect-wide-call",
            r#"
fn sum5(a: Int, b: Int, c: Int, d: Int, e: Int) -> Int { a + b + c + d + e }
fn apply5(f: fn(Int, Int, Int, Int, Int) -> Int, a: Int, b: Int, c: Int, d: Int, e: Int) -> Int {
    f(a, b, c, d, e)
}
fn main() {
    let selected = sum5
    print(selected(1, 2, 3, 4, 5))
    print(apply5(selected, 5, 6, 7, 8, 9))
}
"#,
            "15\n35\n",
        ),
        (
            "recursion-and-short-circuit",
            r#"
fn fib(n: Int) -> Int {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
fn must_not_run() -> Bool / panic {
    assert(false, "short circuit failed")
    true
}
fn main() {
    print(fib(8))
    print(false && must_not_run())
    print(true || must_not_run())
}
"#,
            "21\nfalse\ntrue\n",
        ),
        (
            "structured-concurrency-values",
            r#"
fn square(value: Int) -> Int { value * value }
fn main() / conc + io {
    let left = spawn square(6)
    let right = spawn square(7)
    let a = await left
    let b = await right
    print(a + b)
}
"#,
            "85\n",
        ),
        (
            "descending-ranges",
            r#"
fn main() {
    var text = ""
    for n in range(3, 0) { text = "{text}{n}" }
    for n in 2..0 { text = "{text}{n}" }
    print(text)
}
"#,
            "32121\n",
        ),
    ];

    for (label, source, expected) in cases {
        assert_all_backends(label, source, expected);
    }
}

#[test]
fn cooperative_backends_share_structured_task_ordering() {
    let source = TempSource::new(
        "task-order",
        r#"
fn work(value: Int) -> Int / io {
    print("work {value}")
    value * 2
}
fn main() / conc + io {
    let first_task = spawn work(20)
    let second_task = spawn work(1)
    print("spawned")
    print(await first_task)
    print(await second_task)
}
"#,
    );
    let expected = "spawned\nwork 20\n40\nwork 1\n2\n";
    for backend in [Backend::Interpreter, Backend::Vm] {
        let output = run(&source, backend, false);
        assert!(
            output.status.success(),
            "task ordering failed on {backend:?}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
    }
}

#[test]
fn runtime_failures_keep_shared_semantics_across_backends() {
    assert_all_backends_fail(
        "assert-failure",
        r#"
fn main() / panic {
    assert(false, "consistency sentinel")
}
"#,
        false,
        &["consistency sentinel"],
    );

    assert_all_backends_fail(
        "missing-map-key",
        r#"
fn main() {
    let values = { "known": 1 }
    print(values["missing"])
}
"#,
        false,
        &["map", "key"],
    );

    assert_all_backends_fail(
        "unawaited-task-failure",
        r#"
fn fail() / panic { assert(false, "child failure sentinel") }
fn main() / conc + panic { spawn fail() }
"#,
        false,
        &["child failure sentinel"],
    );

    assert_all_backends_fail(
        "await-cancelled-task",
        r#"
fn work() -> Int { 42 }
fn main() / conc {
    let t = spawn work()
    cancel(t)
    let _ = await t
}
"#,
        false,
        &["cancelled"],
    );

    for (label, expression, marker) in [
        ("division-by-zero", "42 / 0", "division by zero"),
        (
            "addition-overflow",
            "9223372036854775807 + 1",
            "integer overflow",
        ),
        (
            "division-overflow",
            "(-9223372036854775807 - 1) / -1",
            "integer overflow",
        ),
        (
            "negation-overflow",
            "-(-9223372036854775807 - 1)",
            "integer overflow",
        ),
        (
            "absolute-overflow",
            "abs(-9223372036854775807 - 1)",
            "integer overflow",
        ),
        ("nan-comparison", "(0.0 / 0.0) < 1.0", "cannot compare nan"),
        ("nan-minimum", "min(0.0 / 0.0, 1.0)", "cannot compare nan"),
    ] {
        let source = format!("fn main() {{ print({expression}) }}");
        assert_all_backends_fail(label, &source, false, &[marker]);
    }
}

#[test]
fn skip_check_still_rejects_invalid_builtin_arity_on_every_backend() {
    for (label, call, marker) in [
        (
            "bad-assert-arity",
            "assert(true, \"ok\", \"extra\")",
            "assert",
        ),
        ("bad-abs-arity", "abs(1, 2)", "abs"),
        ("bad-range-arity", "range(1, 2, 3)", "range"),
        ("bad-min-arity", "min(1)", "min"),
    ] {
        let source = format!("fn main() {{ {call} }}");
        assert_all_backends_fail(label, &source, true, &[marker, "argument"]);
    }
}

#[test]
fn all_backends_reject_a_second_task_join() {
    let source = TempSource::new(
        "double-await",
        r#"
fn answer() -> Int { 42 }
fn main() / conc {
    let task = spawn answer()
    await task
    await task
}
"#,
    );
    for backend in [Backend::Interpreter, Backend::Vm, Backend::Native] {
        let output = run(&source, backend, false);
        assert!(
            !output.status.success(),
            "{backend:?} accepted a second join"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("already awaited"), "{backend:?}: {stderr}");
    }
}

#[test]
fn stdlib_expansion_agrees_across_all_backends() {
    let source = r#"
import list
import string
import math
import option
import result

fn double_num(x: Int) -> Int { x * 2 }
fn is_gt_two(x: Int) -> Bool { x > 2 }
fn add_acc(acc: Int, x: Int) -> Int { acc + x }

fn main() {
    let xs = [1, 2, 3, 4]
    print(list.index_of(xs, 3), list.index_of(xs, 99))
    print(list.contains_item(xs, 2), list.contains_item(xs, 5))
    print(list.map(xs, double_num))
    print(list.filter(xs, is_gt_two))
    print(list.fold(xs, 10, add_acc))
    print(list.any(xs, is_gt_two), list.all(xs, is_gt_two))
    print(list.flatten([[1, 2], [3, 4]]))
    print(list.min_item(xs), list.max_item(xs))

    print(string.lines("a\nb\nc"))
    print(string.words("  hello   world  "))
    print(string.pad_left("42", 5, "0"))
    print(string.pad_right("hi", 5, "."))
    print(string.slice("flake", 1, 4))
    print(string.char_at("flake", 2))

    print(math.gcd(48, 18), math.lcm(12, 18))
    print(math.factorial(5), math.is_even(6), math.is_odd(7))

    let opt_some = option.Option.Some(10)
    let opt_none = option.Option.None
    print(option.is_none(opt_some), option.is_none(opt_none))
    print(option.unwrap_or(option.map_option(opt_some, double_num), 0))

    let res_ok = result.Result.Ok(5)
    let res_err = result.Result.Err("fail")
    print(result.is_ok(result.map_result(res_ok, double_num)))
    print(result.error_or(result.map_err(res_err, string.to_upper), "none"))
}
"#;
    let expected = concat!(
        "2 -1\n",
        "true false\n",
        "[2, 4, 6, 8]\n",
        "[3, 4]\n",
        "20\n",
        "true false\n",
        "[1, 2, 3, 4]\n",
        "1 4\n",
        "[\"a\", \"b\", \"c\"]\n",
        "[\"hello\", \"world\"]\n",
        "00042\n",
        "hi...\n",
        "lak\n",
        "a\n",
        "6 36\n",
        "120 true true\n",
        "false true\n",
        "20\n",
        "true\n",
        "FAIL\n",
    );
    assert_all_backends("stdlib-expansion", source, expected);
}

#[test]
fn map_keys_values_and_range_contains_agree_across_all_backends() {
    let source = r#"
fn main() {
    let m = { "gamma": 300, "alpha": 100, "beta": 200 }
    print(keys(m))
    print(values(m))

    let int_map = { 2: "two", 1: "one", 3: "three" }
    print(keys(int_map))
    print(values(int_map))

    let r = range(10, 20)
    print(contains(r, 10), contains(r, 15), contains(r, 19), contains(r, 20), contains(r, 9))
    let rev = range(20, 10)
    print(contains(rev, 20), contains(rev, 15), contains(rev, 11), contains(rev, 10), contains(rev, 21))
}
"#;
    let expected = concat!(
        "[\"alpha\", \"beta\", \"gamma\"]\n",
        "[100, 200, 300]\n",
        "[1, 2, 3]\n",
        "[\"one\", \"two\", \"three\"]\n",
        "true true true false false\n",
        "true true true false false\n",
    );
    assert_all_backends("keys-values-range", source, expected);
}

#[test]
fn stdlib_v052_expansion_agrees_across_all_backends() {
    let source = r#"
import list
import string
import math
import map
import option
import result

fn is_gt_two(x) -> Bool { x > 2 }
fn is_lt_three(x) -> Bool { x < 3 }
fn parity_key(x) -> String { if math.is_even(x) { "even" } else { "odd" } }

fn main() {
    let xs = [1, 2, 3]
    let ys = [10, 20, 30]
    let zipped = list.zip(xs, ys)
    print(zipped)
    let unzipped = list.unzip(zipped)
    print(unzipped)

    let nums = [1, 2, 3, 4, 5, 2, 1]
    print(list.take_while(nums, is_lt_three))
    print(list.drop_while(nums, is_lt_three))
    print(list.find_index(nums, is_gt_two))
    print(list.unique(nums))
    print(list.count_where(nums, is_gt_two))
    print(list.repeat_item("a", 3))
    print(list.chunk(nums, 3))

    print(string.capitalize("flake"))
    print(string.reverse_str("flake"))
    print(string.is_digit("7"), string.is_digit("a"))
    print(string.is_alpha("z"), string.is_alpha("9"))

    print(math.is_prime(7), math.is_prime(8), math.is_prime(1))
    print(math.sum_range(1, 6))
    print(math.product([2, 3, 4]))
    print(math.mean([10, 20, 30]))

    let m1 = { "a": 1, "b": 2 }
    let m2 = { "b": 20, "c": 30 }
    let merged = map.merge(m1, m2)
    print(merged)
    print(map.get_or(m1, "a", 999), map.get_or(m1, "z", 999))
    print(map.count_by([1, 2, 3, 4, 5, 6], parity_key))

    let opt_some = option.Option.Some(42)
    let opt_none = option.Option.None
    print(option.is_some(option.filter_option(opt_some, is_gt_two)))
    print(option.is_some(option.filter_option(opt_some, is_lt_three)))
    print(option.expect_some(opt_some, "missing"))

    let res_ok = result.Result.Ok(10)
    print(result.is_ok_and(res_ok, is_gt_two))
    print(result.expect_ok(res_ok, "bad"))
}
"#;
    let expected = concat!(
        "[[1, 10], [2, 20], [3, 30]]\n",
        "[[1, 2, 3], [10, 20, 30]]\n",
        "[1, 2]\n",
        "[3, 4, 5, 2, 1]\n",
        "2\n",
        "[1, 2, 3, 4, 5]\n",
        "3\n",
        "[\"a\", \"a\", \"a\"]\n",
        "[[1, 2, 3], [4, 5, 2], [1]]\n",
        "Flake\n",
        "ekalf\n",
        "true false\n",
        "true false\n",
        "true false false\n",
        "15\n",
        "24\n",
        "20\n",
        "{\"a\": 1, \"b\": 20, \"c\": 30}\n",
        "1 999\n",
        "{\"even\": 3, \"odd\": 3}\n",
        "true\n",
        "false\n",
        "42\n",
        "true\n",
        "10\n",
    );
    assert_all_backends("stdlib-v052-expansion", source, expected);
}

#[test]
fn entries_is_empty_and_has_key_agree_across_all_backends() {
    let source = r#"
fn main() {
    let m = { "b": 20, "a": 10, "c": 30 }
    print(entries(m))
    print(has_key(m, "a"), has_key(m, "z"))

    let im = { 2: "two", 1: "one" }
    print(entries(im))
    print(has_key(im, 1), has_key(im, 3))

    print(is_empty([]), is_empty([1, 2]))
    print(is_empty(""), is_empty("flake"))
    print(is_empty({}), is_empty(m))
}
"#;
    let expected = concat!(
        "[[\"a\", 10], [\"b\", 20], [\"c\", 30]]\n",
        "true false\n",
        "[[1, \"one\"], [2, \"two\"]]\n",
        "true false\n",
        "true false\n",
        "true false\n",
        "true false\n",
    );
    assert_all_backends("entries-is-empty-has-key", source, expected);
}

#[test]
fn stdlib_v055_expansion_agrees_across_all_backends() {
    let source = r#"
import option
import result
import list
import string
import map
import math

fn inc(x: dyn) -> dyn { x + 1 }
fn is_positive(x: dyn) -> Bool { x > 0 }
fn fallback_opt() -> option.Option { option.Option.Some(100) }
fn fallback_res(err: String) -> result.Result { result.Result.Ok(999) }
fn duplicate(x: dyn) -> [dyn] { [x, x] }
fn default_zero() -> dyn { 0 }
fn wrap_some(v: dyn) -> option.Option { option.Option.Some(v) }

fn main() {
    // Option additions
    let opt_some = option.Option.Some(5)
    let opt_none = option.Option.None
    let nested_opt = option.Option.Some(option.Option.Some(42))
    print(option.unwrap_or(option.and_then_option(opt_some, wrap_some), 0))
    print(option.unwrap_or(option.or_else_option(opt_none, fallback_opt), 0))
    print(option.is_some_and(opt_some, is_positive))
    print(option.unwrap_or_else(opt_none, default_zero))
    print(option.unwrap_or(option.flatten_option(nested_opt), 0))

    // Result additions
    let res_ok = result.Result.Ok(10)
    let res_err = result.Result.Err("fail")
    let nested_res = result.Result.Ok(result.Result.Ok(77))
    print(result.unwrap_or(result.flatten_result(nested_res), 0))
    print(result.unwrap_or(result.or_else_result(res_err, fallback_res), 0))
    print(result.unwrap_or_else(res_err, string.reverse_str))

    // List additions
    let xs = [10, 20, 30]
    print(list.head(xs), list.last(xs))
    print(list.intersperse([1, 2, 3], 0))
    print(list.partition([1, 2, 3, 4, 5], math.is_even))
    print(list.flat_map([1, 2], duplicate))

    // String additions
    print(string.contains_str("hello world", "world"), string.contains_str("hello", "xyz"))
    print(string.count_occurrences("banana", "an"))
    print(string.truncate("supercalifragilistic", 10, "..."))

    // Map additions
    let pairs = [["x", "1"], ["y", "2"]]
    let m = map.from_entries(pairs)
    print(m["x"], m["y"])
    let inv = map.invert_map({"a": "1", "b": "2"})
    print(inv["1"], inv["2"])

    // Math additions
    print(math.square(7), math.cube(3))
    print(math.div_ceil(10, 3), math.div_ceil(9, 3))
    print(math.in_range(5, 1, 10), math.in_range(10, 1, 10))
}
"#;
    let expected = concat!(
        "5\n",
        "100\n",
        "true\n",
        "0\n",
        "42\n",
        "77\n",
        "999\n",
        "liaf\n",
        "10 30\n",
        "[1, 0, 2, 0, 3]\n",
        "[[2, 4], [1, 3, 5]]\n",
        "[1, 1, 2, 2]\n",
        "true false\n",
        "2\n",
        "superca...\n",
        "1 2\n",
        "a b\n",
        "49 27\n",
        "4 3\n",
        "true false\n",
    );
    assert_all_backends("stdlib-v055-expansion", source, expected);
}

#[test]
fn concurrency_maturity_ownership_and_results() {
    let source = r#"
import result

fn compute_val(x: Int, mult: Int) -> Int {
    x * mult
}

fn compute_result(x: Int) -> result.Result[Int, String] {
    if x > 0 {
        result.Result.Ok(x * 2)
    } else {
        result.Result.Err("non-positive input")
    }
}

fn main() / io + conc {
    // 1. Concurrent arithmetic tasks
    let t1 = spawn compute_val(10, 4)
    let t2 = spawn compute_val(2, 1)
    let r1 = await t1
    let r2 = await t2
    print(r1 + r2)

    // 2. Tasks returning typed Result enum
    let t_ok = spawn compute_result(21)
    let res = await t_ok
    match res {
        result.Result.Ok(val) => print(val),
        result.Result.Err(msg) => print(msg),
    }

    let t_err = spawn compute_result(-5)
    let res_err = await t_err
    match res_err {
        result.Result.Ok(val) => print(val),
        result.Result.Err(msg) => print(msg),
    }
}
"#;
    let expected = concat!(
        "42\n",
        "42\n",
        "non-positive input\n",
    );
    assert_all_backends("concurrency-maturity-results", source, expected);
}

#[test]
fn concurrency_intra_function_task_passing() {
    let source = r#"
fn work_a(x: Int) -> Int { x + 10 }
fn work_b(y: Int) -> Int { y * 3 }

fn join_both(t1: Task[Int], t2: Task[Int]) -> Int / conc {
    let a = await t1
    let b = await t2
    a + b
}

fn main() / io + conc {
    let t1 = spawn work_a(5)
    let t2 = spawn work_b(10)
    let total = join_both(t1, t2)
    print(total)
}
"#;
    let expected = "45\n";
    assert_all_backends("concurrency-task-passing", source, expected);
}

#[test]
fn concurrency_nursery_and_cancellation() {
    let source = r#"
fn worker(n: Int) -> Int {
    n * 10
}

fn main() / io + conc {
    let res = nursery {
        let t1 = spawn worker(3)
        let t2 = spawn worker(4)
        let a = await t1
        let b = await t2
        a + b
    }
    print(res)

    let t3 = spawn worker(5)
    print(is_cancelled(t3))
    cancel(t3)
    print(is_cancelled(t3))
}
"#;
    let expected = concat!(
        "70\n",
        "false\n",
        "true\n",
    );
    assert_all_backends("concurrency-nursery-cancellation", source, expected);
}

#[test]
fn concurrency_nursery_implicit_drain() {
    let source = r#"
fn work_val(n: Int) -> Int {
    n + 100
}

fn main() / io + conc {
    let outcome = nursery {
        let t1 = spawn work_val(1)
        let t2 = spawn work_val(2)
        // t1 and t2 not awaited explicitly; nursery drains them cleanly
        42
    }
    print(outcome)
}
"#;
    let expected = "42\n";
    assert_all_backends("concurrency-nursery-drain", source, expected);
}

#[test]
fn concurrency_cancelled_task_await_fails() {
    let source = r#"
fn slow() -> Int { 123 }

fn main() / io + conc {
    let t = spawn slow()
    cancel(t)
    let res = await t
    print(res)
}
"#;
    assert_all_backends_fail(
        "concurrency-cancelled-await-fails",
        source,
        true,
        &["task was cancelled"],
    );
}

#[test]
fn concurrency_nursery_with_structs_and_lists() {
    let source = r#"
struct Job { id: Int, weight: Int }

fn run_job(j: Job) -> Int {
    j.id * j.weight
}

fn main() / io + conc {
    let score = nursery {
        let t1 = spawn run_job(Job { id: 2, weight: 10 })
        let t2 = spawn run_job(Job { id: 3, weight: 20 })
        await t1 + await t2
    }
    print("total score: {score}")
}
"#;
    let expected = "total score: 80\n";
    assert_all_backends("concurrency-nursery-structs", source, expected);
}






