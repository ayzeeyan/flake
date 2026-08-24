use flake_ast::Source;

use crate::{Callee, Inst, IrType, lower, print_module};

fn ir(src: &str) -> String {
    let source = Source::new("t.flk", src);
    let module = lower(&source).unwrap_or_else(|e| panic!("{}", e.display(&source)));
    print_module(&module)
}

#[test]
fn lowers_add() {
    let dump = ir("fn add(a: Int, b: Int) -> Int { a + b }");
    assert!(dump.contains("fn add(a: Int, b: Int) -> Int"), "{dump}");
    assert!(dump.contains("add %"), "{dump}");
    assert!(dump.contains("return %"), "{dump}");
}

#[test]
fn materializes_typed_function_addresses_for_indirect_calls() {
    let source = Source::new(
        "functions.flk",
        r#"
fn greet(name: String) -> String { "Hello, {name}" }
fn apply(f: fn(String) -> String, name: String) -> String { f(name) }
fn main() { print(apply(greet, "Flake")) }
"#,
    );
    let module = lower(&source).expect("lower function values");
    assert!(
        module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .any(|block| block.insts.iter().any(|inst| matches!(
                inst,
                Inst::LoadFunction { name, .. } if name == "greet"
            )))
    );
    let apply = module
        .functions
        .iter()
        .find(|function| function.name == "apply")
        .expect("apply function");
    assert_eq!(
        apply.local(apply.params[0]).map(|local| &local.ty),
        Some(&IrType::Func(Box::new(IrType::String)))
    );
    assert!(apply.blocks.iter().any(|block| {
        block.insts.iter().any(|inst| {
            matches!(inst, Inst::Call { callee: Callee::Local(_), dest: Some(dest), .. }
            if apply.local(*dest).is_some_and(|local| local.ty == IrType::String))
        })
    }));
}

#[test]
fn lowers_hello() {
    let src = include_str!("../../examples/hello.flk");
    let source = Source::new("hello.flk", src);
    let module = lower(&source).unwrap();
    assert_eq!(module.functions.len(), 1);
    assert_eq!(module.functions[0].name, "main");
    let dump = print_module(&module);
    assert!(dump.contains("call print"), "{dump}");
    assert!(dump.contains("concat"), "{dump}");
}

#[test]
fn lowers_if_and_loop() {
    let dump = ir(r#"
fn f(n: Int) {
    if n > 0 { 1 } else { 0 }
}
"#);
    assert!(dump.contains("br %"), "{dump}");
    assert!(dump.contains("goto bb"), "{dump}");
}

#[test]
fn records_effects_and_strict() {
    let dump = ir("strict fn greet(name: String) / io { print(name) }");
    assert!(dump.contains("/ io"), "{dump}");
    assert!(dump.contains("strict"), "{dump}");
}

#[test]
fn every_block_ends_with_terminator() {
    let src = include_str!("../../examples/fizzbuzz.flk");
    let source = Source::new("fizzbuzz.flk", src);
    let module = lower(&source).unwrap();
    for func in &module.functions {
        for block in &func.blocks {
            assert!(
                block.insts.last().is_some_and(Inst::is_terminator),
                "bb{} in {} has no terminator: {:?}",
                block.id.0,
                func.name,
                block.insts
            );
        }
    }
}

#[test]
fn lowers_enum_match() {
    let dump = ir(r#"
enum Color { Red Green }
fn f(c: Color) -> Int {
    match c {
        Color.Red => 1
        Color.Green => 2
    }
}
"#);
    assert!(dump.contains("[%"), "{dump}");
    assert!(dump.contains("br %"), "{dump}");
}

#[test]
fn lowers_concurrency_to_native_shape() {
    let dump = ir(r#"
fn work(n: Int) -> Int { n + 1 }
fn main() / conc + io {
    let task: Task[Int] = spawn work(41)
    print(await task)
}
"#);
    assert!(
        dump.contains("/ conc + io") || dump.contains("/ io + conc"),
        "{dump}"
    );
    assert!(dump.contains("spawn work"), "{dump}");
    assert!(dump.contains("await"), "{dump}");
    assert!(dump.contains("Task[Int]"), "{dump}");
}

#[test]
fn lowers_result_try_to_early_return_cfg() {
    let source = Source::new(
        "result.flk",
        r#"
enum Result { Ok(Int) Err(String) }
fn use_it(r: Result) -> Result {
    let value = r?
    Result.Ok(value)
}
fn main() { use_it(Result.Ok(42)) }
"#,
    );
    let module = lower(&source).expect("lower result try");
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "use_it")
        .expect("use_it function");
    let returns = function
        .blocks
        .iter()
        .flat_map(|block| &block.insts)
        .filter(|inst| matches!(inst, Inst::Return { .. }))
        .count();
    assert_eq!(returns, 2, "{:#?}", function.blocks);
}

#[test]
fn infers_concrete_map_key_and_value_ir_types() {
    let source = Source::new(
        "map.flk",
        "fn main() { let values = { 1: \"one\", 2: \"two\" } print(values[1]) }",
    );
    let module = lower(&source).expect("lower map");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    assert!(
        main.locals.iter().any(|local| {
            local.ty == IrType::Map(Box::new(IrType::Int), Box::new(IrType::String))
        })
    );
}

#[test]
fn infers_concrete_list_element_ir_types() {
    let source = Source::new(
        "lists.flk",
        "fn main() { let words = [\"flake\", \"snow\"] let values = [1.5, 2.5] print(words, values) }",
    );
    let module = lower(&source).expect("lower lists");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    assert!(
        main.locals
            .iter()
            .any(|local| local.ty == IrType::List(Box::new(IrType::String)))
    );
    assert!(
        main.locals
            .iter()
            .any(|local| local.ty == IrType::List(Box::new(IrType::Float)))
    );
}

#[test]
fn numeric_native_calls_preserve_their_argument_ir_type() {
    let source = Source::new(
        "numeric-natives.flk",
        r#"
fn main() {
    let absolute = abs(-1.25)
    let low = min(3, 1, 2)
    let high = max(1.0, 3.5, 2.0)
    print(absolute, low, high)
}
"#,
    );
    let module = lower(&source).expect("lower numeric natives");
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main function");
    for (name, expected) in [
        ("abs", IrType::Float),
        ("min", IrType::Int),
        ("max", IrType::Float),
    ] {
        let destination = main
            .blocks
            .iter()
            .flat_map(|block| &block.insts)
            .find_map(|inst| match inst {
                Inst::Call {
                    dest: Some(dest),
                    callee: Callee::Static(callee),
                    ..
                } if callee == name => Some(*dest),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing call to {name}"));
        assert_eq!(main.local(destination).unwrap().ty, expected);
    }
}

#[test]
fn lowers_scalar_match_without_assuming_an_enum_tag() {
    let dump = ir("fn classify(n: Int) -> Int { match n { 0 => 10 1 => 20 _ => 30 } }");
    assert!(dump.contains("eq"), "{dump}");
    assert!(dump.contains("br %"), "{dump}");
}

#[test]
fn lowers_nested_and_list_patterns() {
    let dump = ir(r#"
enum Inner { Leaf(Int) Empty }
enum Tree { Node(Inner, Inner) Single(Inner) }
fn describe(t: Tree) -> String {
    match t {
        Tree.Node(Inner.Leaf(a), Inner.Leaf(b)) => "{a} and {b}"
        _ => "other"
    }
}
fn describe_list(xs: [Int]) -> Int {
    match xs {
        [a, b] => a + b
        _ => 0
    }
}
"#);
    assert!(dump.contains("len"), "{dump}");
    assert!(dump.contains("br %"), "{dump}");
}

#[test]
fn ir_constant_folding_and_dce_optimizations() {
    let dump = ir(r#"
fn compute() -> Int {
    let x = 10 + 20 * 2
    let dead = 999
    if 2 > 1 {
        x + 2
    } else {
        100
    }
}
"#);
    // 10 + 40 + 2 = 52
    assert!(dump.contains("const 52"), "{dump}");
    assert!(
        !dump.contains("const 999"),
        "dead code was not eliminated: {dump}"
    );
    assert!(
        !dump.contains("100"),
        "unreachable else block was not eliminated: {dump}"
    );
}

#[test]
fn aliased_struct_mutation_preserves_dynamic_lookup() {
    let dump = ir(r#"
struct Box { val: Int }
fn bump(b: Box) {
    b.val = b.val + 1
}
fn test() -> Int {
    let b1 = Box { val: 10 }
    bump(b1)
    b1.val
}
"#);
    assert!(
        dump.contains(".val"),
        "dynamic field access must be preserved: {dump}"
    );
}

#[test]
fn version_is_semver() {
    assert!(crate::version().contains('.'));
}
