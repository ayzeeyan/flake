//! Linear-scan-style register allocation for Flake IR.
//!
//! Hot locals are assigned to Windows x64 **callee-saved** GPRs so they
//! survive calls without extra spills. Remaining locals live in stack slots.
//! Argument/scratch registers (`rax`, `rcx`, `rdx`, `r8`–`r11`) are left
//! free for the ABI and instruction selection.

use flake_ir::{Function, Inst, LocalId};

use crate::x86::Reg;

/// Callee-saved pool. `r12` is omitted because its encoding requires a SIB
/// byte (same r/m as `rsp`) that our stack-slot addressing does not need.
const POOL: [Reg; 6] = [Reg::Rbx, Reg::Rsi, Reg::Rdi, Reg::R13, Reg::R14, Reg::R15];

#[derive(Debug, Clone, Copy)]
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

/// Assign the hottest locals to callee-saved registers.
pub fn allocate(func: &Function) -> Frame {
    let n = func.locals.len();
    let mut uses = vec![0u32; n.max(1)];
    for block in &func.blocks {
        for inst in &block.insts {
            for id in refs(inst) {
                if (id.0 as usize) < uses.len() {
                    uses[id.0 as usize] += 1;
                }
            }
        }
    }
    // Prefer parameters slightly so they stay in regs across the body.
    for p in &func.params {
        if (p.0 as usize) < uses.len() {
            uses[p.0 as usize] = uses[p.0 as usize].saturating_add(2);
        }
    }

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| uses[*b].cmp(&uses[*a]).then(a.cmp(b)));

    let mut loc = vec![Loc::Slot(0); n];
    let mut saved = Vec::new();
    let mut assigned = 0usize;
    for i in order {
        if uses[i] == 0 {
            continue;
        }
        if assigned < POOL.len() && uses[i] >= 3 {
            let r = POOL[assigned];
            loc[i] = Loc::Reg(r);
            if !saved.contains(&r) {
                saved.push(r);
            }
            assigned += 1;
        }
    }

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

fn refs(inst: &Inst) -> Vec<LocalId> {
    let mut v = Vec::new();
    match inst {
        Inst::LoadConst { dest, .. } => v.push(*dest),
        Inst::Move { dest, src } => {
            v.push(*dest);
            v.push(*src);
        }
        Inst::Binary {
            dest, lhs, rhs, ..
        } => {
            v.push(*dest);
            v.push(*lhs);
            v.push(*rhs);
        }
        Inst::Unary { dest, src, .. } => {
            v.push(*dest);
            v.push(*src);
        }
        Inst::Call { dest, args, .. } => {
            if let Some(d) = dest {
                v.push(*d);
            }
            v.extend(args.iter().copied());
        }
        Inst::GetIndex { dest, obj, index } => {
            v.push(*dest);
            v.push(*obj);
            v.push(*index);
        }
        Inst::SetIndex { obj, index, value } => {
            v.push(*obj);
            v.push(*index);
            v.push(*value);
        }
        Inst::GetField { dest, obj, .. } => {
            v.push(*dest);
            v.push(*obj);
        }
        Inst::SetField { obj, value, .. } => {
            v.push(*obj);
            v.push(*value);
        }
        Inst::MakeList { dest, items } => {
            v.push(*dest);
            v.extend(items.iter().copied());
        }
        Inst::MakeMap {
            dest,
            keys,
            values,
        } => {
            v.push(*dest);
            v.extend(keys.iter().copied());
            v.extend(values.iter().copied());
        }
        Inst::MakeStruct { dest, fields, .. } => {
            v.push(*dest);
            v.extend(fields.iter().map(|(_, id)| *id));
        }
        Inst::MakeRange { dest, start, end } => {
            v.push(*dest);
            v.push(*start);
            v.push(*end);
        }
        Inst::MakeIter { dest, src } => {
            v.push(*dest);
            v.push(*src);
        }
        Inst::IterNext { value, more, iter } => {
            v.push(*value);
            v.push(*more);
            v.push(*iter);
        }
        Inst::Concat { dest, parts } => {
            v.push(*dest);
            v.extend(parts.iter().copied());
        }
        Inst::Jump { .. } => {}
        Inst::Branch { cond, .. } => v.push(*cond),
        Inst::Return { value } => {
            if let Some(id) = value {
                v.push(*id);
            }
        }
    }
    v
}
