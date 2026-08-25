use std::process::Command;

const EXAMPLES: &[&str] = &[
    "hello.flk",
    "fibonacci.flk",
    "fizzbuzz.flk",
    "effects.flk",
    "lists.flk",
    "ownership.flk",
    "config.flk",
    "modules.flk",
    "stdlib.flk",
    "borrow.flk",
    "enum.flk",
    "visible.flk",
    "app.flk",
    "concurrency.flk",
    "data.flk",
    "nursery.flk",
    "task_pipeline.flk",
    "pattern_matching.flk",
    "geometry.flk",
    "projects/inventory/main.flk",
    "projects/telemetry/main.flk",
    "projects/release/main.flk",
    "projects/pipeline/main.flk",
    "projects/analytics/main.flk",
    "projects/query_engine/main.flk",
    "projects/pkg_workspace/app/main.flk",
    "projects/service_hub/hub_app/main.flk",
    "projects/v07_showcase/main.flk",
];

fn flake_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_flake"));
    cmd.env("NO_COLOR", "1");
    cmd
}

fn example(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join(name)
}

fn run_example(name: &str) -> String {
    run_example_with(name, &[])
}

fn run_example_vm(name: &str) -> String {
    run_example_with(name, &["--vm"])
}

fn run_example_with(name: &str, extra: &[&str]) -> String {
    let mut cmd = flake_bin();
    cmd.arg("run");
    for flag in extra {
        cmd.arg(flag);
    }
    let output = cmd.arg(example(name)).output().expect("run example");
    assert!(
        output.status.success(),
        "{name} {extra:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn check_example(name: &str) {
    let output = flake_bin()
        .arg("check")
        .arg(example(name))
        .output()
        .expect("check example");
    assert!(
        output.status.success(),
        "{name} check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn all_examples_typecheck() {
    for name in EXAMPLES {
        check_example(name);
    }
}

#[test]
fn hello_output() {
    assert_eq!(run_example("hello.flk"), "Hello, World!\n");
}

#[test]
fn fibonacci_output() {
    assert_eq!(run_example("fibonacci.flk"), "fib(10) = 55\n");
}

#[test]
fn fizzbuzz_output() {
    let out = run_example("fizzbuzz.flk");
    assert!(out.contains("FizzBuzz"), "{out}");
    assert!(out.contains("Fizz\n"), "{out}");
    assert!(out.contains("Buzz\n"), "{out}");
}

#[test]
fn effects_output() {
    let out = run_example("effects.flk");
    assert!(out.contains("Hello, Flake!"), "{out}");
    assert!(out.contains("2 + 2 = 4"), "{out}");
}

#[test]
fn lists_output() {
    let out = run_example("lists.flk");
    assert!(out.contains("sum = 15"), "{out}");
    assert!(out.contains("clarity, crystallized"), "{out}");
}

#[test]
fn ownership_output() {
    let out = run_example("ownership.flk");
    assert!(out.contains("consumed strict"), "{out}");
    assert!(out.contains("once: gradual"), "{out}");
    assert!(out.contains("twice: gradual"), "{out}");
}

#[test]
fn config_output() {
    assert_eq!(run_example("config.flk"), "listening on localhost:8080\n");
}

#[test]
fn modules_output() {
    assert_eq!(run_example("modules.flk"), "2 + 2 = 4\nsquare(5) = 25\n");
}

#[test]
fn stdlib_output() {
    let out = run_example("stdlib.flk");
    assert!(out.contains("first = 1 last = 3"), "{out}");
    assert!(out.contains("rest = [2, 3]"), "{out}");
    assert!(out.contains("reverse = [3, 2, 1] sum = 6"), "{out}");
    assert!(out.contains("hi\n"), "{out}");
    assert!(out.contains("FLAKE"), "{out}");
    assert!(out.contains("a:b:c"), "{out}");
    assert!(out.contains("nana"), "{out}");
}

#[test]
fn borrow_output() {
    let out = run_example("borrow.flk");
    assert!(out.contains("borrowed Flake"), "{out}");
    assert!(out.contains("moved Flake"), "{out}");
}

#[test]
fn enum_output() {
    assert_eq!(run_example("enum.flk"), "red\nrgb 1,2,3\nok 42\nerr nope\n");
}

#[test]
fn visible_output() {
    assert_eq!(run_example("visible.flk"), "42\n42\n");
}

#[test]
fn app_output() {
    assert_eq!(
        run_example("app.flk"),
        "sum = 6\nok 42\nfail negative\nFLAKE\n"
    );
}

#[test]
fn concurrency_output() {
    assert_eq!(
        run_example("concurrency.flk"),
        "tasks spawned\nleft = 36\nright = 49\n"
    );
}

#[test]
fn data_output() {
    assert_eq!(
        run_example("data.flk"),
        "port 81\nerror: unknown service: smtp\nsecure\ntrue\n"
    );
}

#[test]
fn nursery_output() {
    assert_eq!(
        run_example("nursery.flk"),
        concat!(
            "starting nursery showcase\n",
            "nursery result = 126\n",
            "cancelled before = false, after = true\n",
            "completed nursery showcase\n",
        )
    );
}

#[test]
fn task_pipeline_output() {
    assert_eq!(
        run_example("task_pipeline.flk"),
        concat!(
            "pipeline scheduled\n",
            "compile: completed with 36\n",
            "test: completed with 49\n",
            "package: rejected (package: empty input)\n",
        )
    );
}

#[test]
fn geometry_output() {
    assert_eq!(
        run_example("geometry.flk"),
        concat!(
            "circle area: 75\n",
            "rect area: 24\n",
            "circle center dist sq: 25\n",
            "rect center dist sq: 25\n",
        )
    );
}

#[test]
fn hierarchical_inventory_project_output() {
    assert_eq!(
        run_example("projects/inventory/main.flk"),
        "premium x4 = 146\n"
    );
}

#[test]
fn transitive_telemetry_project_output() {
    assert_eq!(
        run_example("projects/telemetry/main.flk"),
        "[AVERAGE: 25]\n"
    );
}

#[test]
fn release_project_output() {
    assert_eq!(
        run_example("projects/release/main.flk"),
        concat!(
            "release checks scheduled\n",
            "format: ready (92)\n",
            "tests: blocked (tests scored 88)\n",
            "package: ready (81)\n",
        )
    );
}

#[test]
fn pipeline_project_output() {
    assert_eq!(
        run_example("projects/pipeline/main.flk"),
        concat!(
            "starting pipeline execution\n",
            "[TX-101] standard: processed(score: 100)\n",
            "[TX-102] premium: processed(score: 210)\n",
            "[TX-103] standard: failed(invalid value)\n",
            "pipeline batch total score: 310\n",
        )
    );
}

#[test]
fn analytics_project_output() {
    assert_eq!(
        run_example("projects/analytics/main.flk"),
        concat!(
            "starting analytics pipeline\n",
            "[system] cpu_usage: 45\n",
            "[system] mem_usage: 75\n",
            "[network] req_latency: 120\n",
            "[system] disk_io: 30\n",
            "[network] dns_lookup: 15\n",
            "=== System & Network Metrics ===\n",
            "Samples: 5 | Total: 285 | Min: 15 | Max: 120 | Avg: 57\n",
        )
    );
}

#[test]
fn pkg_workspace_project_output() {
    assert_eq!(
        run_example("projects/pkg_workspace/app/main.flk"),
        concat!("Hello, Flake User from core_lib!\n", "300\n", "92\n",)
    );
}

#[test]
fn pattern_matching_output() {
    assert_eq!(
        run_example("pattern_matching.flk"),
        concat!(
            "circle with radius 10\n",
            "rectangle 4x8\n",
            "point\n",
            "moving to (5, 12)\n",
            "drawing circle radius 25\n",
            "drawing rectangle 100x50\n",
            "quitting\n",
            "origin\n",
            "point (3, 4)\n",
            "other list\n",
        )
    );
}

#[test]
fn query_engine_project_output() {
    assert_eq!(
        run_example("projects/query_engine/main.flk"),
        concat!(
            "running query engine project\n",
            "Filter: ((status == \"active\") AND (dept == \"eng\"))\n",
            "Matched 2 record(s):\n",
            "#1: Task Alpha [active]\n",
            "#4: Task Delta [active]\n",
        )
    );
}

#[test]
fn v07_showcase_project_output() {
    assert_eq!(
        run_example("projects/v07_showcase/main.flk"),
        "Flake v0.7 Showcase Pipeline Output: 43\n"
    );
}

#[test]
fn service_hub_project_output() {
    assert_eq!(
        run_example("projects/service_hub/hub_app/main.flk"),
        concat!(
            "Service Hub initialized for Production\n",
            "500\n",
            "Throughput: 500\n",
        )
    );
}

#[test]
fn native_matches_interpreter_on_all_examples() {
    for name in EXAMPLES {
        let interp = run_example(name);
        let native = run_example_with(name, &["--native"]);
        assert_eq!(interp, native, "{name}: interpreter and native diverged");
    }
}

#[test]
fn vm_matches_interpreter_on_all_examples() {
    for name in EXAMPLES {
        let interp = run_example(name);
        let vm = run_example_vm(name);
        assert_eq!(interp, vm, "{name}: interpreter and VM diverged");
    }
}

fn run_snippet(src: &str, extra: &[&str]) -> String {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "flake-snippet-{}-{}.flk",
        std::process::id(),
        extra.join("_").replace('-', "")
    ));
    std::fs::write(&path, src).expect("write snippet");
    let mut cmd = flake_bin();
    cmd.arg("run");
    for flag in extra {
        cmd.arg(flag);
    }
    let output = cmd.arg(&path).output().expect("run snippet");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "snippet {extra:?} failed:\n{}\n{src}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn snippets_agree_across_backends() {
    let snippets = [
        "fn main() { print(1 + 2 * 3) print(trim(\"  hi  \")) print(upper(\"ab\")) }",
        r#"
enum Color { Red Green Rgb(Int, Int, Int) }
fn main() {
    print(match Color.Red { Color.Red => 1 Color.Green => 2 Color.Rgb(r, g, b) => r })
    print(match Color.Rgb(9, 0, 0) { Color.Red => 0 Color.Green => 0 Color.Rgb(r, g, b) => r })
}
"#,
        r#"
fn add(a: Int, b: Int) -> Int { a + b }
fn main() { print(add(40, 2)) print(if true { "yes" } else { "no" }) }
"#,
    ];
    for src in snippets {
        let interp = run_snippet(src, &[]);
        let vm = run_snippet(src, &["--vm"]);
        let native = run_snippet(src, &["--native"]);
        assert_eq!(interp, vm, "interpreter vs VM:\n{src}");
        assert_eq!(interp, native, "interpreter vs native:\n{src}");
    }
}
