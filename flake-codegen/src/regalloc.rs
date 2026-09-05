//! CFG-aware register allocation for Flake IR.
//!
//! Backwards liveness builds a conservative interference graph, then a
//! hotness-guided greedy coloring assigns non-overlapping locals to Windows
//! x64 **callee-saved** GPRs. Values therefore survive calls without extra
//! spills while short-lived locals can safely reuse registers. Remaining
//! locals live in stack slots.
//! Argument/scratch registers (`rax`, `rcx`, `rdx`, `r8`–`r11`) are left
//! free for the ABI and instruction selection.

use std::collections::{HashMap, HashSet};

use flake_ir::{BlockId, Callee, Function, Inst, LocalId};

use crate::x86::Reg;

/// Callee-saved pool. `r12` is omitted because its encoding requires a SIB
/// byte (same r/m as `rsp`) that our stack-slot addressing does not need.
const POOL: [Reg; 6] = [Reg::Rbx, Reg::Rsi, Reg::Rdi, Reg::R13, Reg::R14, Reg::R15];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loc {
    Reg(Reg),
    Slot(i32),
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub loc: Vec<Loc>,
    pub saved: Vec<Reg>,
    pub spill: i32,
    pub frame_size: i32,
}

impl Frame {
    pub fn loc(&self, id: LocalId) -> Loc {
        self.loc
            .get(id.0 as usize)
            .copied()
            .unwrap_or(Loc::Slot(-8 * (id.0 as i32 + 1)))
    }

    pub fn load(&self, asm: &mut crate::x86::Asm, id: LocalId, dst: Reg) {
        match self.loc(id) {
            Loc::Reg(r) if r == dst => {}
            Loc::Reg(r) => asm.mov_rr(dst, r),
            Loc::Slot(d) => asm.mov_rm_rbp(dst, d),
        }
    }

    pub fn store(&self, asm: &mut crate::x86::Asm, id: LocalId, src: Reg) {
        match self.loc(id) {
            Loc::Reg(r) if r == src => {}
            Loc::Reg(r) => asm.mov_rr(r, src),
            Loc::Slot(d) => asm.mov_mr_rbp(d, src),
        }
    }
}

/// Assign profitable, non-interfering locals to callee-saved registers.
pub fn allocate(func: &Function) -> Frame {
    let n = func.locals.len();
    let mut weights = vec![0u32; n.max(1)];
    for block in &func.blocks {
        for inst in &block.insts {
            let (defs, uses) = defs_uses(inst);
            for id in defs.into_iter().chain(uses) {
                if (id.0 as usize) < weights.len() {
                    weights[id.0 as usize] += 1;
                }
            }
        }
    }
    // Prefer parameters slightly so they stay in regs across the body.
    for p in &func.params {
        if (p.0 as usize) < weights.len() {
            weights[p.0 as usize] = weights[p.0 as usize].saturating_add(2);
        }
    }

    let interference = build_interference(func, n);
    let mut colors = vec![None; n];
    let primary: Vec<_> = (0..n).filter(|id| weights[*id] >= 3).collect();
    color_candidates(&primary, &weights, &interference, &mut colors, POOL.len());

    // Once a function already pays to preserve a register, use that register
    // for short-lived two-reference temporaries when liveness permits it.
    let active_colors = colors
        .iter()
        .flatten()
        .copied()
        .max()
        .map_or(0, |color| color + 1);
    if active_colors > 0 {
        let secondary: Vec<_> = (0..n).filter(|id| weights[*id] == 2).collect();
        color_candidates(
            &secondary,
            &weights,
            &interference,
            &mut colors,
            active_colors,
        );
    }

    let mut loc = vec![Loc::Slot(0); n];
    let mut used_colors = [false; POOL.len()];
    for (id, color) in colors.into_iter().enumerate() {
        if let Some(color) = color {
            loc[id] = Loc::Reg(POOL[color]);
            used_colors[color] = true;
        }
    }
    let saved: Vec<_> = used_colors
        .iter()
        .enumerate()
        .filter_map(|(color, used)| used.then_some(POOL[color]))
        .collect();

    // Stack slots: save area for original callee-saved, then spilled locals.
    let nsave = saved.len() as i32;
    let mut slot = nsave;
    for item in loc.iter_mut() {
        if matches!(item, Loc::Slot(_)) {
            slot += 1;
            *item = Loc::Slot(-8 * slot);
        }
    }
    let spill = -8 * (slot + 1);
    let extra = 3; // concat / write_file temps
    let mut frame_size = (slot + extra) * 8 + 32;
    if frame_size % 16 != 0 {
        frame_size += 16 - (frame_size % 16);
    }
    if frame_size < 32 {
        frame_size = 32;
    }
    Frame {
        loc,
        saved,
        spill,
        frame_size,
    }
}

fn color_candidates(
    candidates: &[usize],
    weights: &[u32],
    interference: &[HashSet<usize>],
    colors: &mut [Option<usize>],
    color_limit: usize,
) {
    let mut order = candidates.to_vec();
    order.sort_by(|a, b| {
        weights[*b]
            .cmp(&weights[*a])
            .then(interference[*b].len().cmp(&interference[*a].len()))
            .then(a.cmp(b))
    });
    for id in order {
        let mut unavailable = [false; POOL.len()];
        for neighbor in &interference[id] {
            if let Some(color) = colors[*neighbor] {
                unavailable[color] = true;
            }
        }
        if let Some(color) = (0..color_limit).find(|color| !unavailable[*color]) {
            colors[id] = Some(color);
        }
    }
}

pub fn compute_liveness(func: &Function) -> (Vec<HashSet<usize>>, Vec<HashSet<usize>>) {
    let n = func.locals.len();
    let block_indices: HashMap<BlockId, usize> = func
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, index))
        .collect();
    let mut block_uses = vec![HashSet::new(); func.blocks.len()];
    let mut block_defs = vec![HashSet::new(); func.blocks.len()];
    for (index, block) in func.blocks.iter().enumerate() {
        for inst in &block.insts {
            let (defs, uses) = defs_uses(inst);
            for local in uses {
                let id = local.0 as usize;
                if id < n && !block_defs[index].contains(&id) {
                    block_uses[index].insert(id);
                }
            }
            for local in defs {
                let id = local.0 as usize;
                if id < n {
                    block_defs[index].insert(id);
                }
            }
        }
    }

    let successors: Vec<Vec<usize>> = func
        .blocks
        .iter()
        .map(|block| match block.insts.last() {
            Some(Inst::Jump { target }) => block_indices.get(target).copied().into_iter().collect(),
            Some(Inst::Branch {
                then_block,
                else_block,
                ..
            }) => [then_block, else_block]
                .into_iter()
                .filter_map(|target| block_indices.get(target).copied())
                .collect(),
            _ => Vec::new(),
        })
        .collect();

    let mut live_in = vec![HashSet::new(); func.blocks.len()];
    let mut live_out = vec![HashSet::new(); func.blocks.len()];
    loop {
        let mut changed = false;
        for index in (0..func.blocks.len()).rev() {
            let next_out: HashSet<_> = successors[index]
                .iter()
                .flat_map(|successor| live_in[*successor].iter().copied())
                .collect();
            let mut next_in = block_uses[index].clone();
            next_in.extend(
                next_out
                    .iter()
                    .filter(|local| !block_defs[index].contains(local))
                    .copied(),
            );
            if next_out != live_out[index] || next_in != live_in[index] {
                live_out[index] = next_out;
                live_in[index] = next_in;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (live_in, live_out)
}

fn build_interference(func: &Function, n: usize) -> Vec<HashSet<usize>> {
    let block_indices: HashMap<BlockId, usize> = func
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, index))
        .collect();
    let (live_in, live_out) = compute_liveness(func);

    let mut graph = vec![HashSet::new(); n];
    if let Some(entry) = block_indices.get(&func.entry).copied() {
        let live_params: Vec<_> = func
            .params
            .iter()
            .map(|param| param.0 as usize)
            .filter(|param| *param < n && live_in[entry].contains(param))
            .collect();
        for (index, left) in live_params.iter().enumerate() {
            for right in &live_params[index + 1..] {
                graph[*left].insert(*right);
                graph[*right].insert(*left);
            }
        }
    }
    for (index, block) in func.blocks.iter().enumerate() {
        let mut live = live_out[index].clone();
        for inst in block.insts.iter().rev() {
            let (defs, uses) = defs_uses(inst);
            // Instruction selection may need every input concurrently and may
            // materialize an aggregate destination before it has consumed all
            // of those inputs. Keep such operands in distinct locations. A
            // plain move is the sole coalescing-friendly exception.
            for (left_index, left) in uses.iter().enumerate() {
                let left = left.0 as usize;
                if left >= n {
                    continue;
                }
                for right in &uses[left_index + 1..] {
                    let right = right.0 as usize;
                    if right < n && left != right {
                        graph[left].insert(right);
                        graph[right].insert(left);
                    }
                }
            }
            if !matches!(inst, Inst::Move { .. }) {
                for def in &defs {
                    let def = def.0 as usize;
                    if def >= n {
                        continue;
                    }
                    for used in &uses {
                        let used = used.0 as usize;
                        if used < n && def != used {
                            graph[def].insert(used);
                            graph[used].insert(def);
                        }
                    }
                }
            }
            let move_source = match inst {
                Inst::Move { src, .. } => Some(src.0 as usize),
                _ => None,
            };
            for def in &defs {
                let id = def.0 as usize;
                if id >= n {
                    continue;
                }
                for other in live.iter().copied() {
                    if other != id && Some(other) != move_source {
                        graph[id].insert(other);
                        graph[other].insert(id);
                    }
                }
            }
            for def in defs {
                live.remove(&(def.0 as usize));
            }
            live.extend(
                uses.into_iter()
                    .map(|local| local.0 as usize)
                    .filter(|id| *id < n),
            );
        }
    }
    graph
}

pub fn defs_uses(inst: &Inst) -> (Vec<LocalId>, Vec<LocalId>) {
    let mut defs = Vec::new();
    let mut uses = Vec::new();
    match inst {
        Inst::LoadConst { dest, .. } | Inst::LoadFunction { dest, .. } => defs.push(*dest),
        Inst::Move { dest, src } => {
            defs.push(*dest);
            uses.push(*src);
        }
        Inst::Binary { dest, lhs, rhs, .. } => {
            defs.push(*dest);
            uses.push(*lhs);
            uses.push(*rhs);
        }
        Inst::Unary { dest, src, .. } => {
            defs.push(*dest);
            uses.push(*src);
        }
        Inst::Call { dest, callee, args } => {
            if let Some(d) = dest {
                defs.push(*d);
            }
            if let Callee::Local(local) = callee {
                uses.push(*local);
            }
            uses.extend(args.iter().copied());
        }
        Inst::Spawn { dest, callee, args } => {
            defs.push(*dest);
            if let Callee::Local(local) = callee {
                uses.push(*local);
            }
            uses.extend(args.iter().copied());
        }
        Inst::Await { dest, task } => {
            defs.push(*dest);
            uses.push(*task);
        }
        Inst::GetIndex { dest, obj, index } => {
            defs.push(*dest);
            uses.push(*obj);
            uses.push(*index);
        }
        Inst::SetIndex { obj, index, value } => {
            uses.push(*obj);
            uses.push(*index);
            uses.push(*value);
        }
        Inst::GetField { dest, obj, .. } => {
            defs.push(*dest);
            uses.push(*obj);
        }
        Inst::SetField { obj, value, .. } => {
            uses.push(*obj);
            uses.push(*value);
        }
        Inst::MakeList { dest, items } => {
            defs.push(*dest);
            uses.extend(items.iter().copied());
        }
        Inst::MakeMap { dest, keys, values } => {
            defs.push(*dest);
            uses.extend(keys.iter().copied());
            uses.extend(values.iter().copied());
        }
        Inst::MakeStruct { dest, fields, .. } => {
            defs.push(*dest);
            uses.extend(fields.iter().map(|(_, id)| *id));
        }
        Inst::MakeRange { dest, start, end } => {
            defs.push(*dest);
            uses.push(*start);
            uses.push(*end);
        }
        Inst::MakeIter { dest, src } => {
            defs.push(*dest);
            uses.push(*src);
        }
        Inst::IterNext { value, more, iter } => {
            defs.push(*value);
            defs.push(*more);
            uses.push(*iter);
        }
        Inst::Concat { dest, parts } => {
            defs.push(*dest);
            uses.extend(parts.iter().copied());
        }
        Inst::Jump { .. } => {}
        Inst::Branch { cond, .. } => uses.push(*cond),
        Inst::Return { value } => {
            if let Some(id) = value {
                uses.push(*id);
            }
        }
    }
    (defs, uses)
}
