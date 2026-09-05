//! Flake IR Optimization Passes.
//!
//! Includes:
//! 1. Constant folding and propagation for single-assignment locals.
//! 2. Unreachable basic block elimination.
//! 3. Dead pure instruction elimination.
//! 4. Copy propagation for immutable locals.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::ir::{
    BasicBlock, BinOp, BlockId, Callee, Const, Function, Inst, Local, LocalId, Module, UnOp,
};

/// Run all IR optimization passes on the module.
pub fn optimize(module: &mut Module) {
    for func in &mut module.functions {
        optimize_function(func);
    }

    for _ in 0..3 {
        let inlined = inline_functions(module);
        if inlined {
            for func in &mut module.functions {
                optimize_function(func);
            }
        } else {
            break;
        }
    }
}

/// Optimize a single function to a fixed point.
pub fn optimize_function(func: &mut Function) {
    for _ in 0..5 {
        let changed = run_optimization_round(func);
        if !changed {
            break;
        }
    }
}

fn run_optimization_round(func: &mut Function) -> bool {
    let mut changed = false;
    changed |= fold_constants_and_propagate(func);
    changed |= thread_jumps(func);
    changed |= eliminate_unreachable_blocks(func);
    changed |= propagate_moves(func);
    changed |= eliminate_dead_instructions(func);
    changed
}

fn count_defs(func: &Function) -> HashMap<LocalId, usize> {
    let mut defs = HashMap::new();
    for param in &func.params {
        *defs.entry(*param).or_insert(0) += 1;
    }
    for block in &func.blocks {
        for inst in &block.insts {
            match inst {
                Inst::LoadConst { dest, .. }
                | Inst::LoadFunction { dest, .. }
                | Inst::Move { dest, .. }
                | Inst::Binary { dest, .. }
                | Inst::Unary { dest, .. }
                | Inst::GetIndex { dest, .. }
                | Inst::GetField { dest, .. }
                | Inst::MakeList { dest, .. }
                | Inst::MakeMap { dest, .. }
                | Inst::MakeStruct { dest, .. }
                | Inst::MakeEnum { dest, .. }
                | Inst::GetEnumTag { dest, .. }
                | Inst::GetEnumField { dest, .. }
                | Inst::MakeRange { dest, .. }
                | Inst::MakeIter { dest, .. }
                | Inst::Concat { dest, .. }
                | Inst::Spawn { dest, .. }
                | Inst::Await { dest, .. } => {
                    *defs.entry(*dest).or_insert(0) += 1;
                }
                Inst::Call { dest: Some(d), .. } => {
                    *defs.entry(*d).or_insert(0) += 1;
                }
                Inst::IterNext { value, more, .. } => {
                    *defs.entry(*value).or_insert(0) += 1;
                    *defs.entry(*more).or_insert(0) += 1;
                }
                _ => {}
            }
        }
    }
    defs
}

/// 1. Constant folding & branch simplification for single-assignment locals.
fn fold_constants_and_propagate(func: &mut Function) -> bool {
    let defs = count_defs(func);
    let mut constants: HashMap<LocalId, Const> = HashMap::new();
    let mut struct_fields: HashMap<LocalId, HashMap<String, LocalId>> = HashMap::new();
    let mut list_items: HashMap<LocalId, Vec<LocalId>> = HashMap::new();
    let mut escaped_or_mutated: HashSet<LocalId> = HashSet::new();

    // Find any locals that are passed to functions/tasks or mutated in place
    let mut aliases: HashMap<LocalId, Vec<LocalId>> = HashMap::new();
    for block in &func.blocks {
        for inst in &block.insts {
            match inst {
                Inst::Move { dest, src } => {
                    aliases.entry(*src).or_default().push(*dest);
                    aliases.entry(*dest).or_default().push(*src);
                }
                Inst::Call { args, .. } | Inst::Spawn { args, .. } => {
                    for a in args {
                        escaped_or_mutated.insert(*a);
                    }
                }
                Inst::SetField { obj, .. } => {
                    escaped_or_mutated.insert(*obj);
                }
                Inst::SetIndex { obj, .. } => {
                    escaped_or_mutated.insert(*obj);
                }
                _ => {}
            }
        }
    }

    // Transitively propagate escaped_or_mutated across aliased locals
    let mut worklist: Vec<LocalId> = escaped_or_mutated.iter().copied().collect();
    while let Some(loc) = worklist.pop() {
        if let Some(linked) = aliases.get(&loc) {
            for neighbor in linked {
                if escaped_or_mutated.insert(*neighbor) {
                    worklist.push(*neighbor);
                }
            }
        }
    }

    let mut changed = false;

    for block in &mut func.blocks {
        for inst in &mut block.insts {
            match *inst {
                Inst::LoadConst { dest, ref value } => {
                    if defs.get(&dest) == Some(&1) {
                        constants.insert(dest, value.clone());
                    }
                }
                Inst::Move { dest, src } => {
                    if defs.get(&dest) == Some(&1) {
                        if let Some(c) = constants.get(&src) {
                            constants.insert(dest, c.clone());
                        }
                    }
                }
                Inst::MakeStruct {
                    dest, ref fields, ..
                } => {
                    if defs.get(&dest) == Some(&1) && !escaped_or_mutated.contains(&dest) {
                        let mut map = HashMap::new();
                        for (f, val_id) in fields {
                            map.insert(f.clone(), *val_id);
                        }
                        struct_fields.insert(dest, map);
                    }
                }
                Inst::MakeList { dest, ref items } => {
                    if defs.get(&dest) == Some(&1) && !escaped_or_mutated.contains(&dest) {
                        list_items.insert(dest, items.clone());
                    }
                }
                Inst::GetField {
                    dest,
                    obj,
                    ref field,
                } => {
                    if let Some(map) = struct_fields.get(&obj) {
                        if let Some(&field_val_id) = map.get(field) {
                            if let Some(c) = constants.get(&field_val_id) {
                                *inst = Inst::LoadConst {
                                    dest,
                                    value: c.clone(),
                                };
                                if defs.get(&dest) == Some(&1) {
                                    constants.insert(dest, c.clone());
                                }
                            } else {
                                *inst = Inst::Move {
                                    dest,
                                    src: field_val_id,
                                };
                            }
                            changed = true;
                        }
                    }
                }
                Inst::GetIndex { dest, obj, index } => {
                    if let Some(items) = list_items.get(&obj) {
                        if let Some(Const::Int(idx)) = constants.get(&index) {
                            if *idx >= 0 && (*idx as usize) < items.len() {
                                let item_id = items[*idx as usize];
                                if let Some(c) = constants.get(&item_id) {
                                    *inst = Inst::LoadConst {
                                        dest,
                                        value: c.clone(),
                                    };
                                    if defs.get(&dest) == Some(&1) {
                                        constants.insert(dest, c.clone());
                                    }
                                } else {
                                    *inst = Inst::Move { dest, src: item_id };
                                }
                                changed = true;
                            }
                        }
                    }
                }
                Inst::Unary { dest, op, src } => {
                    if let Some(c) = constants.get(&src) {
                        if let Some(folded) = fold_unary(op, c) {
                            *inst = Inst::LoadConst {
                                dest,
                                value: folded.clone(),
                            };
                            if defs.get(&dest) == Some(&1) {
                                constants.insert(dest, folded);
                            }
                            changed = true;
                        }
                    }
                }
                Inst::Binary { dest, op, lhs, rhs } => {
                    let l_opt = constants.get(&lhs);
                    let r_opt = constants.get(&rhs);
                    if let (Some(l), Some(r)) = (l_opt, r_opt) {
                        if let Some(folded) = fold_binary(op, l, r) {
                            *inst = Inst::LoadConst {
                                dest,
                                value: folded.clone(),
                            };
                            if defs.get(&dest) == Some(&1) {
                                constants.insert(dest, folded);
                            }
                            changed = true;
                        }
                    } else if let Some(simplified) =
                        simplify_algebraic(op, lhs, rhs, l_opt, r_opt, dest, &func.locals)
                    {
                        *inst = simplified;
                        changed = true;
                    }
                }
                Inst::Branch {
                    cond,
                    then_block,
                    else_block,
                } => {
                    if let Some(Const::Bool(b)) = constants.get(&cond) {
                        let target = if *b { then_block } else { else_block };
                        *inst = Inst::Jump { target };
                        changed = true;
                    }
                }
                _ => {}
            }
        }
    }
    changed
}

fn fold_unary(op: UnOp, c: &Const) -> Option<Const> {
    match (op, c) {
        (UnOp::Neg, Const::Int(n)) => n.checked_neg().map(Const::Int),
        (UnOp::Neg, Const::Float(f)) => Some(Const::Float(-f)),
        (UnOp::Not, Const::Bool(b)) => Some(Const::Bool(!b)),
        _ => None,
    }
}

fn fold_binary(op: BinOp, l: &Const, r: &Const) -> Option<Const> {
    match (op, l, r) {
        // Int arithmetic with runtime overflow safety
        (BinOp::Add, Const::Int(a), Const::Int(b)) => a.checked_add(*b).map(Const::Int),
        (BinOp::Sub, Const::Int(a), Const::Int(b)) => a.checked_sub(*b).map(Const::Int),
        (BinOp::Mul, Const::Int(a), Const::Int(b)) => a.checked_mul(*b).map(Const::Int),
        (BinOp::Div, Const::Int(a), Const::Int(b)) => a.checked_div(*b).map(Const::Int),
        (BinOp::Rem, Const::Int(a), Const::Int(b)) => a.checked_rem(*b).map(Const::Int),

        // Int comparisons
        (BinOp::Eq, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a == b)),
        (BinOp::Ne, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a != b)),
        (BinOp::Lt, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a < b)),
        (BinOp::Le, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a <= b)),
        (BinOp::Gt, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a > b)),
        (BinOp::Ge, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a >= b)),

        // Float arithmetic
        (BinOp::Add, Const::Float(a), Const::Float(b)) => Some(Const::Float(a + b)),
        (BinOp::Sub, Const::Float(a), Const::Float(b)) => Some(Const::Float(a - b)),
        (BinOp::Mul, Const::Float(a), Const::Float(b)) => Some(Const::Float(a * b)),
        (BinOp::Div, Const::Float(a), Const::Float(b)) if *b != 0.0 => Some(Const::Float(a / b)),

        // Float comparisons
        (BinOp::Eq, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a == b)),
        (BinOp::Ne, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a != b)),
        (BinOp::Lt, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a < b)),
        (BinOp::Le, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a <= b)),
        (BinOp::Gt, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a > b)),
        (BinOp::Ge, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a >= b)),

        // Bool logic
        (BinOp::And, Const::Bool(a), Const::Bool(b)) => Some(Const::Bool(*a && *b)),
        (BinOp::Or, Const::Bool(a), Const::Bool(b)) => Some(Const::Bool(*a || *b)),
        (BinOp::Eq, Const::Bool(a), Const::Bool(b)) => Some(Const::Bool(a == b)),
        (BinOp::Ne, Const::Bool(a), Const::Bool(b)) => Some(Const::Bool(a != b)),

        // String operations
        (BinOp::Add, Const::String(a), Const::String(b)) => Some(Const::String(format!("{a}{b}"))),
        (BinOp::Eq, Const::String(a), Const::String(b)) => Some(Const::Bool(a == b)),
        (BinOp::Ne, Const::String(a), Const::String(b)) => Some(Const::Bool(a != b)),

        // Nil equality
        (BinOp::Eq, Const::Nil, Const::Nil) => Some(Const::Bool(true)),
        (BinOp::Ne, Const::Nil, Const::Nil) => Some(Const::Bool(false)),

        _ => None,
    }
}

fn simplify_algebraic(
    op: BinOp,
    lhs: LocalId,
    rhs: LocalId,
    l_const: Option<&Const>,
    r_const: Option<&Const>,
    dest: LocalId,
    locals: &[Local],
) -> Option<Inst> {
    use crate::ty::IrType;
    let lhs_ty = locals.iter().find(|l| l.id == lhs).map(|l| &l.ty);
    let rhs_ty = locals.iter().find(|l| l.id == rhs).map(|l| &l.ty);
    let dest_ty = locals.iter().find(|l| l.id == dest).map(|l| &l.ty);

    match (op, l_const, r_const) {
        // x + 0 -> x (for integer)
        (BinOp::Add, None, Some(Const::Int(0)))
            if dest_ty == Some(&IrType::Int) || lhs_ty == Some(&IrType::Int) =>
        {
            Some(Inst::Move { dest, src: lhs })
        }
        // 0 + x -> x (for integer)
        (BinOp::Add, Some(Const::Int(0)), None)
            if dest_ty == Some(&IrType::Int) || rhs_ty == Some(&IrType::Int) =>
        {
            Some(Inst::Move { dest, src: rhs })
        }
        // x - 0 -> x (for integer)
        (BinOp::Sub, None, Some(Const::Int(0)))
            if dest_ty == Some(&IrType::Int) || lhs_ty == Some(&IrType::Int) =>
        {
            Some(Inst::Move { dest, src: lhs })
        }
        // x * 1 -> x (for integer)
        (BinOp::Mul, None, Some(Const::Int(1)))
            if dest_ty == Some(&IrType::Int) || lhs_ty == Some(&IrType::Int) =>
        {
            Some(Inst::Move { dest, src: lhs })
        }
        // 1 * x -> x (for integer)
        (BinOp::Mul, Some(Const::Int(1)), None)
            if dest_ty == Some(&IrType::Int) || rhs_ty == Some(&IrType::Int) =>
        {
            Some(Inst::Move { dest, src: rhs })
        }
        // x * 0 -> 0 (only for integer, preserving float NaN semantics)
        (BinOp::Mul, None, Some(Const::Int(0)))
            if lhs_ty == Some(&IrType::Int) || dest_ty == Some(&IrType::Int) =>
        {
            Some(Inst::LoadConst {
                dest,
                value: Const::Int(0),
            })
        }
        (BinOp::Mul, Some(Const::Int(0)), None)
            if rhs_ty == Some(&IrType::Int) || dest_ty == Some(&IrType::Int) =>
        {
            Some(Inst::LoadConst {
                dest,
                value: Const::Int(0),
            })
        }
        // x / 1 -> x (for integer)
        (BinOp::Div, None, Some(Const::Int(1)))
            if dest_ty == Some(&IrType::Int) || lhs_ty == Some(&IrType::Int) =>
        {
            Some(Inst::Move { dest, src: lhs })
        }
        // x && true -> x (for boolean)
        (BinOp::And, None, Some(Const::Bool(true)))
            if dest_ty == Some(&IrType::Bool) || lhs_ty == Some(&IrType::Bool) =>
        {
            Some(Inst::Move { dest, src: lhs })
        }
        // true && x -> x (for boolean)
        (BinOp::And, Some(Const::Bool(true)), None)
            if dest_ty == Some(&IrType::Bool) || rhs_ty == Some(&IrType::Bool) =>
        {
            Some(Inst::Move { dest, src: rhs })
        }
        // x && false -> false (for boolean)
        (BinOp::And, None, Some(Const::Bool(false)))
        | (BinOp::And, Some(Const::Bool(false)), None) => Some(Inst::LoadConst {
            dest,
            value: Const::Bool(false),
        }),
        // x || true -> true (for boolean)
        (BinOp::Or, None, Some(Const::Bool(true))) | (BinOp::Or, Some(Const::Bool(true)), None) => {
            Some(Inst::LoadConst {
                dest,
                value: Const::Bool(true),
            })
        }
        // x || false -> x (for boolean)
        (BinOp::Or, None, Some(Const::Bool(false)))
            if dest_ty == Some(&IrType::Bool) || lhs_ty == Some(&IrType::Bool) =>
        {
            Some(Inst::Move { dest, src: lhs })
        }
        // false || x -> x (for boolean)
        (BinOp::Or, Some(Const::Bool(false)), None)
            if dest_ty == Some(&IrType::Bool) || rhs_ty == Some(&IrType::Bool) =>
        {
            Some(Inst::Move { dest, src: rhs })
        }
        _ => None,
    }
}

/// Thread jumps through empty blocks that only contain an unconditional jump.
fn thread_jumps(func: &mut Function) -> bool {
    let mut jump_targets: HashMap<BlockId, BlockId> = HashMap::new();
    for block in &func.blocks {
        if block.insts.len() == 1 {
            if let Some(Inst::Jump { target }) = block.insts.first() {
                if *target != block.id {
                    jump_targets.insert(block.id, *target);
                }
            }
        }
    }

    if jump_targets.is_empty() {
        return false;
    }

    // Resolve chained jumps
    let mut resolved_targets = HashMap::new();
    for (from, to) in &jump_targets {
        let mut cur = *to;
        let mut visited = HashSet::new();
        visited.insert(*from);
        while let Some(&next) = jump_targets.get(&cur) {
            if !visited.insert(cur) {
                break;
            }
            cur = next;
        }
        if cur != *from {
            resolved_targets.insert(*from, cur);
        }
    }

    let mut changed = false;
    for block in &mut func.blocks {
        for inst in &mut block.insts {
            match inst {
                Inst::Jump { target } => {
                    if let Some(&new_target) = resolved_targets.get(target) {
                        if *target != new_target {
                            *target = new_target;
                            changed = true;
                        }
                    }
                }
                Inst::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    if let Some(&new_then) = resolved_targets.get(then_block) {
                        if *then_block != new_then {
                            *then_block = new_then;
                            changed = true;
                        }
                    }
                    if let Some(&new_else) = resolved_targets.get(else_block) {
                        if *else_block != new_else {
                            *else_block = new_else;
                            changed = true;
                        }
                    }
                    if *then_block == *else_block {
                        *inst = Inst::Jump {
                            target: *then_block,
                        };
                        changed = true;
                    }
                }
                _ => {}
            }
        }
    }
    changed
}

/// 2. Eliminate unreachable basic blocks.
fn eliminate_unreachable_blocks(func: &mut Function) -> bool {
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();

    reachable.insert(func.entry);
    queue.push_back(func.entry);

    let block_map: HashMap<BlockId, &BasicBlock> = func.blocks.iter().map(|b| (b.id, b)).collect();

    while let Some(bid) = queue.pop_front() {
        if let Some(b) = block_map.get(&bid) {
            for inst in &b.insts {
                match inst {
                    Inst::Jump { target } => {
                        if reachable.insert(*target) {
                            queue.push_back(*target);
                        }
                    }
                    Inst::Branch {
                        then_block,
                        else_block,
                        ..
                    } => {
                        if reachable.insert(*then_block) {
                            queue.push_back(*then_block);
                        }
                        if reachable.insert(*else_block) {
                            queue.push_back(*else_block);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let original_len = func.blocks.len();
    func.blocks.retain(|b| reachable.contains(&b.id));
    func.blocks.len() != original_len
}

/// 3. Propagate simple local copies (%d = %s) for single-assignment locals.
fn propagate_moves(func: &mut Function) -> bool {
    let defs = count_defs(func);
    let mut changed = false;
    for block in &mut func.blocks {
        let mut copies: HashMap<LocalId, LocalId> = HashMap::new();
        for inst in &mut block.insts {
            // Apply known copies to uses
            changed |= replace_uses(inst, &copies);

            if let Inst::Move { dest, src } = inst {
                if dest != src && defs.get(dest) == Some(&1) && defs.get(src) == Some(&1) {
                    let actual_src = copies.get(src).copied().unwrap_or(*src);
                    copies.insert(*dest, actual_src);
                }
            }
        }
    }
    changed
}

fn replace_uses(inst: &mut Inst, copies: &HashMap<LocalId, LocalId>) -> bool {
    let mut changed = false;
    let mut resolve = |l: &mut LocalId| {
        if let Some(target) = copies.get(l) {
            if l != target {
                *l = *target;
                changed = true;
            }
        }
    };

    match inst {
        Inst::Move { src, .. } => resolve(src),
        Inst::Binary { lhs, rhs, .. } => {
            resolve(lhs);
            resolve(rhs);
        }
        Inst::Unary { src, .. } => resolve(src),
        Inst::Call { callee, args, .. } => {
            if let Callee::Local(id) = callee {
                resolve(id);
            }
            for a in args {
                resolve(a);
            }
        }
        Inst::Spawn { callee, args, .. } => {
            if let Callee::Local(id) = callee {
                resolve(id);
            }
            for a in args {
                resolve(a);
            }
        }
        Inst::Await { task, .. } => resolve(task),
        Inst::GetIndex { obj, index, .. } => {
            resolve(obj);
            resolve(index);
        }
        Inst::SetIndex { obj, index, value } => {
            resolve(obj);
            resolve(index);
            resolve(value);
        }
        Inst::GetField { obj, .. } => resolve(obj),
        Inst::SetField { obj, value, .. } => {
            resolve(obj);
            resolve(value);
        }
        Inst::MakeList { items, .. } => {
            for i in items {
                resolve(i);
            }
        }
        Inst::MakeMap { keys, values, .. } => {
            for k in keys {
                resolve(k);
            }
            for v in values {
                resolve(v);
            }
        }
        Inst::MakeStruct { fields, .. } => {
            for (_, v) in fields {
                resolve(v);
            }
        }
        Inst::MakeEnum { fields, .. } => {
            for v in fields {
                resolve(v);
            }
        }
        Inst::GetEnumTag { obj, .. } | Inst::GetEnumField { obj, .. } => {
            resolve(obj);
        }
        Inst::MakeRange { start, end, .. } => {
            resolve(start);
            resolve(end);
        }
        Inst::MakeIter { src, .. } => resolve(src),
        Inst::IterNext { iter, .. } => resolve(iter),
        Inst::Concat { parts, .. } => {
            for p in parts {
                resolve(p);
            }
        }
        Inst::Branch { cond, .. } => {
            resolve(cond);
        }
        Inst::Return { value: Some(v) } => {
            resolve(v);
        }
        _ => {}
    }
    changed
}

/// 4. Eliminate dead pure instructions whose results are never read.
fn eliminate_dead_instructions(func: &mut Function) -> bool {
    let defs = count_defs(func);
    let mut used_locals = HashSet::new();

    // Count all uses
    for block in &func.blocks {
        for inst in &block.insts {
            collect_uses(inst, &mut used_locals);
        }
    }

    let mut changed = false;
    for block in &mut func.blocks {
        let original_len = block.insts.len();
        block.insts.retain(|inst| {
            if let Some(dest) = pure_dest(inst) {
                if defs.get(&dest) == Some(&1) && !used_locals.contains(&dest) {
                    return false;
                }
            }
            true
        });
        if block.insts.len() != original_len {
            changed = true;
        }
    }
    changed
}

fn pure_dest(inst: &Inst) -> Option<LocalId> {
    match inst {
        Inst::LoadConst { dest, .. }
        | Inst::LoadFunction { dest, .. }
        | Inst::Move { dest, .. }
        | Inst::Binary { dest, .. }
        | Inst::Unary { dest, .. }
        | Inst::MakeStruct { dest, .. }
        | Inst::MakeEnum { dest, .. }
        | Inst::GetEnumTag { dest, .. }
        | Inst::GetEnumField { dest, .. }
        | Inst::MakeList { dest, .. }
        | Inst::GetField { dest, .. }
        | Inst::GetIndex { dest, .. }
        | Inst::MakeRange { dest, .. } => Some(*dest),
        _ => None,
    }
}

fn collect_uses(inst: &Inst, uses: &mut HashSet<LocalId>) {
    match inst {
        Inst::Move { src, .. } | Inst::Unary { src, .. } | Inst::MakeIter { src, .. } => {
            uses.insert(*src);
        }
        Inst::Binary { lhs, rhs, .. } => {
            uses.insert(*lhs);
            uses.insert(*rhs);
        }
        Inst::Call { callee, args, .. } | Inst::Spawn { callee, args, .. } => {
            if let Callee::Local(id) = callee {
                uses.insert(*id);
            }
            for a in args {
                uses.insert(*a);
            }
        }
        Inst::Await { task, .. } => {
            uses.insert(*task);
        }
        Inst::GetIndex { obj, index, .. } => {
            uses.insert(*obj);
            uses.insert(*index);
        }
        Inst::SetIndex { obj, index, value } => {
            uses.insert(*obj);
            uses.insert(*index);
            uses.insert(*value);
        }
        Inst::GetField { obj, .. } => {
            uses.insert(*obj);
        }
        Inst::SetField { obj, value, .. } => {
            uses.insert(*obj);
            uses.insert(*value);
        }
        Inst::MakeList { items, .. } => {
            for i in items {
                uses.insert(*i);
            }
        }
        Inst::MakeMap { keys, values, .. } => {
            for k in keys {
                uses.insert(*k);
            }
            for v in values {
                uses.insert(*v);
            }
        }
        Inst::MakeStruct { fields, .. } => {
            for (_, v) in fields {
                uses.insert(*v);
            }
        }
        Inst::MakeEnum { fields, .. } => {
            for v in fields {
                uses.insert(*v);
            }
        }
        Inst::GetEnumTag { obj, .. } | Inst::GetEnumField { obj, .. } => {
            uses.insert(*obj);
        }
        Inst::MakeRange { start, end, .. } => {
            uses.insert(*start);
            uses.insert(*end);
        }
        Inst::IterNext { iter, .. } => {
            uses.insert(*iter);
        }
        Inst::Concat { parts, .. } => {
            for p in parts {
                uses.insert(*p);
            }
        }
        Inst::Branch { cond, .. } => {
            uses.insert(*cond);
        }
        Inst::Return { value: Some(v) } => {
            uses.insert(*v);
        }
        _ => {}
    }
}

fn remap_local(local: &mut LocalId, map: &HashMap<LocalId, LocalId>) {
    if let Some(new_id) = map.get(local) {
        *local = *new_id;
    }
}

fn remap_inst_locals(inst: &mut Inst, map: &HashMap<LocalId, LocalId>) {
    match inst {
        Inst::LoadConst { dest, .. } | Inst::LoadFunction { dest, .. } => {
            remap_local(dest, map);
        }
        Inst::Move { dest, src } => {
            remap_local(dest, map);
            remap_local(src, map);
        }
        Inst::Binary { dest, lhs, rhs, .. } => {
            remap_local(dest, map);
            remap_local(lhs, map);
            remap_local(rhs, map);
        }
        Inst::Unary { dest, src, .. } => {
            remap_local(dest, map);
            remap_local(src, map);
        }
        Inst::Call { dest, callee, args } => {
            if let Some(d) = dest {
                remap_local(d, map);
            }
            if let Callee::Local(l) = callee {
                remap_local(l, map);
            }
            for a in args {
                remap_local(a, map);
            }
        }
        Inst::Spawn { dest, callee, args } => {
            remap_local(dest, map);
            if let Callee::Local(l) = callee {
                remap_local(l, map);
            }
            for a in args {
                remap_local(a, map);
            }
        }
        Inst::Await { dest, task } => {
            remap_local(dest, map);
            remap_local(task, map);
        }
        Inst::GetIndex { dest, obj, index } => {
            remap_local(dest, map);
            remap_local(obj, map);
            remap_local(index, map);
        }
        Inst::SetIndex { obj, index, value } => {
            remap_local(obj, map);
            remap_local(index, map);
            remap_local(value, map);
        }
        Inst::GetField { dest, obj, .. } => {
            remap_local(dest, map);
            remap_local(obj, map);
        }
        Inst::SetField { obj, value, .. } => {
            remap_local(obj, map);
            remap_local(value, map);
        }
        Inst::MakeList { dest, items } => {
            remap_local(dest, map);
            for it in items {
                remap_local(it, map);
            }
        }
        Inst::MakeMap { dest, keys, values } => {
            remap_local(dest, map);
            for k in keys {
                remap_local(k, map);
            }
            for v in values {
                remap_local(v, map);
            }
        }
        Inst::MakeStruct { dest, fields, .. } => {
            remap_local(dest, map);
            for (_, f) in fields {
                remap_local(f, map);
            }
        }
        Inst::MakeEnum { dest, fields, .. } => {
            remap_local(dest, map);
            for f in fields {
                remap_local(f, map);
            }
        }
        Inst::GetEnumTag { dest, obj } => {
            remap_local(dest, map);
            remap_local(obj, map);
        }
        Inst::GetEnumField { dest, obj, .. } => {
            remap_local(dest, map);
            remap_local(obj, map);
        }
        Inst::MakeRange { dest, start, end } => {
            remap_local(dest, map);
            remap_local(start, map);
            remap_local(end, map);
        }
        Inst::MakeIter { dest, src } => {
            remap_local(dest, map);
            remap_local(src, map);
        }
        Inst::IterNext { value, more, iter } => {
            remap_local(value, map);
            remap_local(more, map);
            remap_local(iter, map);
        }
        Inst::Concat { dest, parts } => {
            remap_local(dest, map);
            for p in parts {
                remap_local(p, map);
            }
        }
        Inst::Branch { cond, .. } => {
            remap_local(cond, map);
        }
        Inst::Return { value } => {
            if let Some(v) = value {
                remap_local(v, map);
            }
        }
        Inst::Jump { .. } => {}
    }
}

/// Inline single-block, non-recursive leaf functions into call sites.
pub fn inline_functions(module: &mut Module) -> bool {
    let mut inlinable: HashMap<String, Function> = HashMap::new();
    for func in &module.functions {
        if func.name == "main" || func.blocks.len() != 1 {
            continue;
        }
        let block = &func.blocks[0];
        let Some(last) = block.insts.last() else {
            continue;
        };
        if !matches!(last, Inst::Return { .. }) {
            continue;
        }
        if block.insts.len() > 32 {
            continue;
        }
        let mut is_leaf = true;
        for inst in &block.insts {
            if let Inst::Call {
                callee: Callee::Static(target),
                ..
            } = inst
            {
                if target == &func.name {
                    is_leaf = false;
                    break;
                }
            }
        }
        if is_leaf {
            inlinable.insert(func.name.clone(), func.clone());
        }
    }

    if inlinable.is_empty() {
        return false;
    }

    let mut any_inlined = false;

    for func in &mut module.functions {
        let mut new_blocks = Vec::with_capacity(func.blocks.len());
        let mut func_changed = false;

        for block in &func.blocks {
            let mut new_insts = Vec::with_capacity(block.insts.len());
            for inst in &block.insts {
                if let Inst::Call {
                    dest,
                    callee: Callee::Static(target),
                    args,
                } = inst
                {
                    if let Some(target_func) = inlinable.get(target) {
                        if target_func.name != func.name && target_func.params.len() == args.len() {
                            func_changed = true;
                            any_inlined = true;

                            let mut local_map: HashMap<LocalId, LocalId> = HashMap::new();
                            for (param_id, arg_id) in target_func.params.iter().zip(args.iter()) {
                                local_map.insert(*param_id, *arg_id);
                            }

                            for target_local in &target_func.locals {
                                if let std::collections::hash_map::Entry::Vacant(e) =
                                    local_map.entry(target_local.id)
                                {
                                    let new_id = LocalId(func.locals.len() as u32);
                                    func.locals.push(Local {
                                        id: new_id,
                                        name: target_local
                                            .name
                                            .clone()
                                            .map(|n| format!("inline_{n}")),
                                        ty: target_local.ty.clone(),
                                    });
                                    e.insert(new_id);
                                }
                            }

                            let target_block = &target_func.blocks[0];
                            let last_idx = target_block.insts.len() - 1;
                            for (idx, target_inst) in target_block.insts.iter().enumerate() {
                                if idx == last_idx {
                                    if let Inst::Return {
                                        value: Some(ret_local),
                                    } = target_inst
                                    {
                                        if let Some(caller_dest) = dest {
                                            let mapped_ret = local_map
                                                .get(ret_local)
                                                .copied()
                                                .unwrap_or(*ret_local);
                                            new_insts.push(Inst::Move {
                                                dest: *caller_dest,
                                                src: mapped_ret,
                                            });
                                        }
                                    } else if let Inst::Return { value: None } = target_inst {
                                        if let Some(caller_dest) = dest {
                                            new_insts.push(Inst::LoadConst {
                                                dest: *caller_dest,
                                                value: Const::Nil,
                                            });
                                        }
                                    }
                                } else {
                                    let mut cloned_inst = target_inst.clone();
                                    remap_inst_locals(&mut cloned_inst, &local_map);
                                    new_insts.push(cloned_inst);
                                }
                            }
                            continue;
                        }
                    }
                }
                new_insts.push(inst.clone());
            }
            new_blocks.push(BasicBlock {
                id: block.id,
                insts: new_insts,
            });
        }

        if func_changed {
            func.blocks = new_blocks;
            optimize_function(func);
        }
    }

    any_inlined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Local;
    use crate::ty::IrType;

    #[test]
    fn folds_constant_arithmetic() {
        let mut func = Function {
            name: "main".into(),
            params: vec![],
            ret: IrType::Int,
            effects: vec![],
            effects_specified: false,
            strict: false,
            owned: false,
            locals: vec![
                Local {
                    id: LocalId(0),
                    name: None,
                    ty: IrType::Int,
                },
                Local {
                    id: LocalId(1),
                    name: None,
                    ty: IrType::Int,
                },
                Local {
                    id: LocalId(2),
                    name: None,
                    ty: IrType::Int,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                insts: vec![
                    Inst::LoadConst {
                        dest: LocalId(0),
                        value: Const::Int(10),
                    },
                    Inst::LoadConst {
                        dest: LocalId(1),
                        value: Const::Int(20),
                    },
                    Inst::Binary {
                        dest: LocalId(2),
                        op: BinOp::Add,
                        lhs: LocalId(0),
                        rhs: LocalId(1),
                    },
                    Inst::Return {
                        value: Some(LocalId(2)),
                    },
                ],
            }],
            entry: BlockId(0),
        };

        optimize_function(&mut func);

        let return_inst = &func.blocks[0].insts.last().unwrap();
        assert!(matches!(
            return_inst,
            Inst::Return {
                value: Some(LocalId(2))
            }
        ));

        let const_inst = &func.blocks[0].insts[0];
        assert_eq!(
            const_inst,
            &Inst::LoadConst {
                dest: LocalId(2),
                value: Const::Int(30),
            }
        );
    }

    #[test]
    fn inlines_simple_leaf_function() {
        let helper = Function {
            name: "square".into(),
            params: vec![LocalId(0)],
            ret: IrType::Int,
            effects: vec![],
            effects_specified: false,
            strict: false,
            owned: false,
            locals: vec![
                Local {
                    id: LocalId(0),
                    name: Some("x".into()),
                    ty: IrType::Int,
                },
                Local {
                    id: LocalId(1),
                    name: None,
                    ty: IrType::Int,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                insts: vec![
                    Inst::Binary {
                        dest: LocalId(1),
                        op: BinOp::Mul,
                        lhs: LocalId(0),
                        rhs: LocalId(0),
                    },
                    Inst::Return {
                        value: Some(LocalId(1)),
                    },
                ],
            }],
            entry: BlockId(0),
        };

        let caller = Function {
            name: "main".into(),
            params: vec![],
            ret: IrType::Int,
            effects: vec![],
            effects_specified: false,
            strict: false,
            owned: false,
            locals: vec![
                Local {
                    id: LocalId(0),
                    name: None,
                    ty: IrType::Int,
                },
                Local {
                    id: LocalId(1),
                    name: None,
                    ty: IrType::Int,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                insts: vec![
                    Inst::LoadConst {
                        dest: LocalId(0),
                        value: Const::Int(5),
                    },
                    Inst::Call {
                        dest: Some(LocalId(1)),
                        callee: Callee::Static("square".into()),
                        args: vec![LocalId(0)],
                    },
                    Inst::Return {
                        value: Some(LocalId(1)),
                    },
                ],
            }],
            entry: BlockId(0),
        };

        let mut module = Module {
            name: "test".into(),
            functions: vec![helper, caller],
            structs: vec![],
        };

        optimize(&mut module);

        let main_func = module.functions.iter().find(|f| f.name == "main").unwrap();
        // After inlining square(5) -> 5 * 5 -> constant-folded to 25!
        let has_call = main_func.blocks[0]
            .insts
            .iter()
            .any(|i| matches!(i, Inst::Call { .. }));
        assert!(!has_call, "call instruction should have been inlined");
        let has_const_25 = main_func.blocks[0].insts.iter().any(|i| {
            matches!(
                i,
                Inst::LoadConst {
                    value: Const::Int(25),
                    ..
                }
            )
        });
        assert!(
            has_const_25,
            "inlined 5 * 5 should be folded to Const::Int(25)"
        );
    }

    #[test]
    fn folds_struct_field_projection_to_constant() {
        let func = Function {
            name: "test_proj".into(),
            params: vec![],
            ret: IrType::Int,
            effects: vec![],
            effects_specified: false,
            strict: false,
            owned: false,
            locals: vec![
                Local {
                    id: LocalId(0),
                    name: None,
                    ty: IrType::Int,
                },
                Local {
                    id: LocalId(1),
                    name: None,
                    ty: IrType::Int,
                },
                Local {
                    id: LocalId(2),
                    name: None,
                    ty: IrType::Struct("Point".into()),
                },
                Local {
                    id: LocalId(3),
                    name: None,
                    ty: IrType::Int,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                insts: vec![
                    Inst::LoadConst {
                        dest: LocalId(0),
                        value: Const::Int(10),
                    },
                    Inst::LoadConst {
                        dest: LocalId(1),
                        value: Const::Int(20),
                    },
                    Inst::MakeStruct {
                        dest: LocalId(2),
                        name: "Point".into(),
                        fields: vec![("x".into(), LocalId(0)), ("y".into(), LocalId(1))],
                    },
                    Inst::GetField {
                        dest: LocalId(3),
                        obj: LocalId(2),
                        field: "y".into(),
                    },
                    Inst::Return {
                        value: Some(LocalId(3)),
                    },
                ],
            }],
            entry: BlockId(0),
        };

        let mut module = Module {
            name: "test".into(),
            functions: vec![func],
            structs: vec![],
        };

        optimize(&mut module);

        let opt_func = &module.functions[0];
        let has_get_field = opt_func.blocks[0]
            .insts
            .iter()
            .any(|i| matches!(i, Inst::GetField { .. }));
        assert!(!has_get_field, "GetField should be eliminated");
        let has_const_20 = opt_func.blocks[0].insts.iter().any(|i| {
            matches!(
                i,
                Inst::LoadConst {
                    value: Const::Int(20),
                    ..
                }
            )
        });
        assert!(
            has_const_20,
            "Point.y projection should fold to Const::Int(20)"
        );
    }

    #[test]
    fn folds_algebraic_identities() {
        let func = Function {
            name: "main".into(),
            params: vec![],
            ret: IrType::Int,
            effects: vec![],
            effects_specified: false,
            strict: false,
            owned: false,
            locals: vec![
                Local {
                    id: LocalId(0),
                    name: None,
                    ty: IrType::Int,
                },
                Local {
                    id: LocalId(1),
                    name: None,
                    ty: IrType::Int,
                },
                Local {
                    id: LocalId(2),
                    name: None,
                    ty: IrType::Int,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                insts: vec![
                    Inst::LoadConst {
                        dest: LocalId(0),
                        value: Const::Int(0),
                    },
                    Inst::Binary {
                        dest: LocalId(2),
                        op: BinOp::Mul,
                        lhs: LocalId(1),
                        rhs: LocalId(0),
                    },
                    Inst::Return {
                        value: Some(LocalId(2)),
                    },
                ],
            }],
            entry: BlockId(0),
        };

        let mut module = Module {
            name: "test".into(),
            functions: vec![func],
            structs: vec![],
        };

        optimize(&mut module);

        let opt_func = &module.functions[0];
        let has_binop = opt_func.blocks[0]
            .insts
            .iter()
            .any(|i| matches!(i, Inst::Binary { .. }));
        assert!(
            !has_binop,
            "Binary multiply by 0 should be simplified to LoadConst"
        );
        let has_zero = opt_func.blocks[0].insts.iter().any(|i| {
            matches!(
                i,
                Inst::LoadConst {
                    value: Const::Int(0),
                    ..
                }
            )
        });
        assert!(has_zero, "Result should fold to 0");
    }

    #[test]
    fn float_multiply_by_zero_is_not_folded() {
        let func = Function {
            name: "main".into(),
            params: vec![],
            ret: IrType::Float,
            effects: vec![],
            effects_specified: false,
            strict: false,
            owned: false,
            locals: vec![
                Local {
                    id: LocalId(0),
                    name: None,
                    ty: IrType::Int,
                },
                Local {
                    id: LocalId(1),
                    name: None,
                    ty: IrType::Float,
                },
                Local {
                    id: LocalId(2),
                    name: None,
                    ty: IrType::Float,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                insts: vec![
                    Inst::LoadConst {
                        dest: LocalId(0),
                        value: Const::Int(0),
                    },
                    Inst::Binary {
                        dest: LocalId(2),
                        op: BinOp::Mul,
                        lhs: LocalId(1),
                        rhs: LocalId(0),
                    },
                    Inst::Return {
                        value: Some(LocalId(2)),
                    },
                ],
            }],
            entry: BlockId(0),
        };

        let mut module = Module {
            name: "test".into(),
            functions: vec![func],
            structs: vec![],
        };

        optimize(&mut module);

        let opt_func = &module.functions[0];
        let has_binop = opt_func.blocks[0]
            .insts
            .iter()
            .any(|i| matches!(i, Inst::Binary { .. }));
        assert!(
            has_binop,
            "Float multiply by 0 must not be blindly folded to 0 because NaN * 0 = NaN"
        );
    }
}
