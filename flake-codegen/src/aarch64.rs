use std::collections::HashMap;

use flake_ir::{BinOp, Const, Function, Inst, Module};

use crate::emit::Compiled;
use crate::error::CodegenError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Reg {
    X0 = 0,
    X1 = 1,
    X2 = 2,
    X3 = 3,
    X4 = 4,
    X5 = 5,
    X6 = 6,
    X7 = 7,
    X8 = 8,
    X9 = 9,
    X10 = 10,
    X11 = 11,
    X12 = 12,
    X13 = 13,
    X14 = 14,
    X15 = 15,
    X16 = 16,
    X17 = 17,
    X18 = 18,
    X19 = 19,
    X20 = 20,
    X21 = 21,
    X22 = 22,
    X23 = 23,
    X24 = 24,
    X25 = 25,
    X26 = 26,
    X27 = 27,
    X28 = 28,
    X29 = 29, // FP
    X30 = 30, // LR
    SP = 31,
    XZR = 32,
}

impl Reg {
    #[must_use]
    pub fn id(self) -> u32 {
        match self {
            Self::XZR => 31,
            Self::SP => 31,
            other => other as u32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    EQ = 0x0,
    NE = 0x1,
    CS = 0x2,
    CC = 0x3,
    MI = 0x4,
    PL = 0x5,
    VS = 0x6,
    VC = 0x7,
    HI = 0x8,
    LS = 0x9,
    GE = 0xa,
    LT = 0xb,
    GT = 0xc,
    LE = 0xd,
    AL = 0xe,
}

pub struct Aarch64Asm {
    pub bytes: Vec<u8>,
    labels: HashMap<String, usize>,
    patches: Vec<Patch>,
}

struct Patch {
    at: usize,
    target: String,
    kind: PatchKind,
}

#[allow(dead_code)]
enum PatchKind {
    B,
    BCond(Cond),
    BL,
    Adr(Reg),
}

impl Default for Aarch64Asm {
    fn default() -> Self {
        Self::new()
    }
}

impl Aarch64Asm {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bytes: Vec::new(),
            labels: HashMap::new(),
            patches: Vec::new(),
        }
    }

    pub fn emit_u32(&mut self, val: u32) {
        self.bytes.extend_from_slice(&val.to_le_bytes());
    }

    pub fn label(&mut self, name: impl Into<String>) {
        let name = name.into();
        let pos = self.bytes.len();
        self.labels.insert(name, pos);
    }

    // --- Data processing (register) ---

    pub fn add_rr(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        // ADD (shifted register, 64-bit): 0x8b000000 | (rm << 16) | (rn << 5) | rd
        let insn = 0x8b000000 | (rm.id() << 16) | (rn.id() << 5) | rd.id();
        self.emit_u32(insn);
    }

    pub fn sub_rr(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        // SUB (shifted register, 64-bit): 0xcb000000 | (rm << 16) | (rn << 5) | rd
        let insn = 0xcb000000 | (rm.id() << 16) | (rn.id() << 5) | rd.id();
        self.emit_u32(insn);
    }

    pub fn mul_rr(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        // MUL (64-bit): 0x9b007c00 | (rm << 16) | (rn << 5) | rd
        let insn = 0x9b007c00 | (rm.id() << 16) | (rn.id() << 5) | rd.id();
        self.emit_u32(insn);
    }

    pub fn sdiv_rr(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        // SDIV (64-bit): 0x9ac00c00 | (rm << 16) | (rn << 5) | rd
        let insn = 0x9ac00c00 | (rm.id() << 16) | (rn.id() << 5) | rd.id();
        self.emit_u32(insn);
    }

    pub fn and_rr(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        // AND (shifted register, 64-bit): 0x8a000000 | (rm << 16) | (rn << 5) | rd
        let insn = 0x8a000000 | (rm.id() << 16) | (rn.id() << 5) | rd.id();
        self.emit_u32(insn);
    }

    pub fn orr_rr(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        // ORR (shifted register, 64-bit): 0xaa000000 | (rm << 16) | (rn << 5) | rd
        let insn = 0xaa000000 | (rm.id() << 16) | (rn.id() << 5) | rd.id();
        self.emit_u32(insn);
    }

    pub fn eor_rr(&mut self, rd: Reg, rn: Reg, rm: Reg) {
        // EOR (shifted register, 64-bit): 0xca000000 | (rm << 16) | (rn << 5) | rd
        let insn = 0xca000000 | (rm.id() << 16) | (rn.id() << 5) | rd.id();
        self.emit_u32(insn);
    }

    pub fn mov_rr(&mut self, rd: Reg, rm: Reg) {
        // MOV (register, 64-bit): ORR rd, XZR, rm
        self.orr_rr(rd, Reg::XZR, rm);
    }

    pub fn cmp_rr(&mut self, rn: Reg, rm: Reg) {
        // CMP: SUBS XZR, rn, rm (64-bit) -> 0xeb00001f | (rm << 16) | (rn << 5)
        let insn = 0xeb00001f | (rm.id() << 16) | (rn.id() << 5);
        self.emit_u32(insn);
    }

    // --- Immediate moves ---

    pub fn movz(&mut self, rd: Reg, imm16: u16, shift: u8) {
        // MOVZ (64-bit): 0xd2800000 | (hw << 21) | (imm16 << 5) | rd
        let hw = ((shift / 16) & 3) as u32;
        let insn = 0xd2800000 | (hw << 21) | ((imm16 as u32) << 5) | rd.id();
        self.emit_u32(insn);
    }

    pub fn movk(&mut self, rd: Reg, imm16: u16, shift: u8) {
        // MOVK (64-bit): 0xf2800000 | (hw << 21) | (imm16 << 5) | rd
        let hw = ((shift / 16) & 3) as u32;
        let insn = 0xf2800000 | (hw << 21) | ((imm16 as u32) << 5) | rd.id();
        self.emit_u32(insn);
    }

    pub fn mov_i64(&mut self, rd: Reg, imm: i64) {
        let u = imm as u64;
        let w0 = (u & 0xffff) as u16;
        let w1 = ((u >> 16) & 0xffff) as u16;
        let w2 = ((u >> 32) & 0xffff) as u16;
        let w3 = ((u >> 48) & 0xffff) as u16;

        self.movz(rd, w0, 0);
        if w1 != 0 {
            self.movk(rd, w1, 16);
        }
        if w2 != 0 {
            self.movk(rd, w2, 32);
        }
        if w3 != 0 {
            self.movk(rd, w3, 48);
        }
    }

    // --- Memory operations ---

    pub fn ldr_offset(&mut self, rt: Reg, rn: Reg, offset: i32) {
        // LDR (64-bit unsigned offset): 0xf9400000 | (imm12 << 10) | (rn << 5) | rt
        if offset >= 0 && offset % 8 == 0 && (offset / 8) < 4096 {
            let imm12 = ((offset / 8) & 0xfff) as u32;
            let insn = 0xf9400000 | (imm12 << 10) | (rn.id() << 5) | rt.id();
            self.emit_u32(insn);
        } else {
            // Use LDUR (unscaled signed offset): 0xf8400000 | (imm9 << 12) | (rn << 5) | rt
            let imm9 = (offset & 0x1ff) as u32;
            let insn = 0xf8400000 | (imm9 << 12) | (rn.id() << 5) | rt.id();
            self.emit_u32(insn);
        }
    }

    pub fn str_offset(&mut self, rt: Reg, rn: Reg, offset: i32) {
        // STR (64-bit unsigned offset): 0xf9000000 | (imm12 << 10) | (rn << 5) | rt
        if offset >= 0 && offset % 8 == 0 && (offset / 8) < 4096 {
            let imm12 = ((offset / 8) & 0xfff) as u32;
            let insn = 0xf9000000 | (imm12 << 10) | (rn.id() << 5) | rt.id();
            self.emit_u32(insn);
        } else {
            // STUR (unscaled signed offset): 0xf8000000 | (imm9 << 12) | (rn << 5) | rt
            let imm9 = (offset & 0x1ff) as u32;
            let insn = 0xf8000000 | (imm9 << 12) | (rn.id() << 5) | rt.id();
            self.emit_u32(insn);
        }
    }

    pub fn stp_pre(&mut self, rt1: Reg, rt2: Reg, rn: Reg, offset: i32) {
        // STP pre-index: 0xa9bf0000 | ((imm7 & 0x7f) << 15) | (rt2 << 10) | (rn << 5) | rt1
        let imm7 = ((offset / 8) & 0x7f) as u32;
        let insn =
            0xa9800000 | 0x00200000 | (imm7 << 15) | (rt2.id() << 10) | (rn.id() << 5) | rt1.id();
        self.emit_u32(insn);
    }

    pub fn ldp_post(&mut self, rt1: Reg, rt2: Reg, rn: Reg, offset: i32) {
        // LDP post-index: 0xa8c00000 | ((imm7 & 0x7f) << 15) | (rt2 << 10) | (rn << 5) | rt1
        let imm7 = ((offset / 8) & 0x7f) as u32;
        let insn =
            0xa8800000 | 0x00400000 | (imm7 << 15) | (rt2.id() << 10) | (rn.id() << 5) | rt1.id();
        self.emit_u32(insn);
    }

    // --- Control Flow ---

    pub fn b_label(&mut self, target: impl Into<String>) {
        let at = self.bytes.len();
        self.patches.push(Patch {
            at,
            target: target.into(),
            kind: PatchKind::B,
        });
        self.emit_u32(0x14000000); // Placeholder B #0
    }

    pub fn b_cond_label(&mut self, cond: Cond, target: impl Into<String>) {
        let at = self.bytes.len();
        self.patches.push(Patch {
            at,
            target: target.into(),
            kind: PatchKind::BCond(cond),
        });
        self.emit_u32(0x54000000 | (cond as u32)); // Placeholder B.cond #0
    }

    pub fn bl_label(&mut self, target: impl Into<String>) {
        let at = self.bytes.len();
        self.patches.push(Patch {
            at,
            target: target.into(),
            kind: PatchKind::BL,
        });
        self.emit_u32(0x94000000); // Placeholder BL #0
    }

    pub fn blr(&mut self, rn: Reg) {
        // BLR Xn: 0xd63f0000 | (rn << 5)
        let insn = 0xd63f0000 | (rn.id() << 5);
        self.emit_u32(insn);
    }

    pub fn ret(&mut self) {
        // RET X30: 0xd65f03c0
        self.emit_u32(0xd65f03c0);
    }

    pub fn svc(&mut self, imm16: u16) {
        // SVC #imm16: 0xd4000001 | (imm16 << 5)
        let insn = 0xd4000001 | ((imm16 as u32) << 5);
        self.emit_u32(insn);
    }

    pub fn cset(&mut self, rd: Reg, cond: Cond) {
        // CSINC rd, XZR, XZR, invert(cond)
        let inv_cond = ((cond as u32) ^ 1) & 0xf;
        let insn = 0x9a9f07e0 | (inv_cond << 12) | rd.id();
        self.emit_u32(insn);
    }

    pub fn finish(&mut self) -> Result<(), String> {
        for patch in &self.patches {
            let Some(&target_pos) = self.labels.get(&patch.target) else {
                return Err(format!("undefined label '{}'", patch.target));
            };
            let at = patch.at;
            let offset_bytes = (target_pos as i64) - (at as i64);
            let offset_insns = offset_bytes / 4;

            match patch.kind {
                PatchKind::B => {
                    let imm26 = (offset_insns & 0x3ffffff) as u32;
                    let insn = 0x14000000 | imm26;
                    self.bytes[at..at + 4].copy_from_slice(&insn.to_le_bytes());
                }
                PatchKind::BCond(cond) => {
                    let imm19 = (offset_insns & 0x7ffff) as u32;
                    let insn = 0x54000000 | (imm19 << 5) | (cond as u32);
                    self.bytes[at..at + 4].copy_from_slice(&insn.to_le_bytes());
                }
                PatchKind::BL => {
                    let imm26 = (offset_insns & 0x3ffffff) as u32;
                    let insn = 0x94000000 | imm26;
                    self.bytes[at..at + 4].copy_from_slice(&insn.to_le_bytes());
                }
                PatchKind::Adr(_) => {}
            }
        }
        Ok(())
    }
}

/// Compile Flake IR module to AArch64 ELF binary.
pub fn compile_module_aarch64(module: &Module) -> Result<Compiled, CodegenError> {
    let mut asm = Aarch64Asm::new();
    let mut strings: Vec<Vec<u8>> = Vec::new();
    let iat_patches = Vec::new();
    let str_patches = Vec::new();
    let mut gas = String::new();
    gas.push_str("# Flake AArch64 (Linux ELF64) — pure Rust, no external assembler/linker\n");

    intern_str(&mut strings, b"true");
    intern_str(&mut strings, b"false");
    intern_str(&mut strings, b"nil");
    intern_str(&mut strings, b"\n");
    intern_str(&mut strings, b" ");

    // _start entry: call main, exit with return code
    asm.label("_start");
    gas.push_str(".global _start\n_start:\n");
    gas.push_str("    bl main\n    mov x8, #93\n    svc #0\n");
    asm.bl_label("main");
    asm.mov_rr(Reg::X0, Reg::X0);
    asm.mov_i64(Reg::X8, 93); // sys_exit on Linux AArch64
    asm.svc(0);

    // Emit runtime functions
    emit_aarch64_runtime(&mut asm);

    // Emit user functions
    for func in &module.functions {
        emit_aarch64_function(func, &mut asm)?;
    }

    asm.finish().map_err(CodegenError::new)?;

    Ok(Compiled {
        code: asm.bytes,
        strings,
        entry: 0,
        iat_patches,
        str_patches,
        global_patches: Vec::new(),
        gas,
    })
}

fn intern_str(strings: &mut Vec<Vec<u8>>, s: &[u8]) -> usize {
    if let Some(i) = strings.iter().position(|t| t.as_slice() == s) {
        return i;
    }
    let i = strings.len();
    let mut v = s.to_vec();
    if v.last() != Some(&0) {
        v.push(0);
    }
    strings.push(v);
    i
}

fn emit_aarch64_runtime(asm: &mut Aarch64Asm) {
    // rt_print_cstr: prints null-terminated string at X0
    asm.label("rt_print_cstr");
    asm.stp_pre(Reg::X29, Reg::X30, Reg::SP, -16);
    asm.mov_rr(Reg::X1, Reg::X0); // buffer in X1
    // sys_write: X8 = 64, X0 = 1 (stdout), X1 = buf, X2 = 32
    asm.mov_i64(Reg::X0, 1);
    asm.mov_i64(Reg::X2, 32);
    asm.mov_i64(Reg::X8, 64);
    asm.svc(0);
    asm.ldp_post(Reg::X29, Reg::X30, Reg::SP, 16);
    asm.ret();

    // rt_alloc: allocates X0 bytes via sys_mmap
    asm.label("rt_alloc");
    asm.stp_pre(Reg::X29, Reg::X30, Reg::SP, -16);
    // sys_mmap: X8 = 222, X0 = 0, X1 = size, X2 = PROT_READ|PROT_WRITE (3), X3 = MAP_PRIVATE|MAP_ANONYMOUS (0x22), X4 = -1, X5 = 0
    asm.mov_rr(Reg::X1, Reg::X0);
    asm.mov_i64(Reg::X0, 0);
    asm.mov_i64(Reg::X2, 3);
    asm.mov_i64(Reg::X3, 0x22);
    asm.mov_i64(Reg::X4, -1);
    asm.mov_i64(Reg::X5, 0);
    asm.mov_i64(Reg::X8, 222);
    asm.svc(0);
    asm.ldp_post(Reg::X29, Reg::X30, Reg::SP, 16);
    asm.ret();

    // Systems APIs are stubs on AArch64 in v0.13. The backend lowers only a
    // scalar IR subset; these labels exist so a later Call lowering does not
    // fail to resolve, and tests skip execution cleanly.
    for name in [
        "rt_exit",
        "rt_print_i64",
        "rt_print_nl",
        "rt_read_file",
        "rt_write_file",
        "rt_file_exists",
        "rt_env",
        "rt_cwd",
        "rt_remove_file",
        "rt_is_dir",
        "rt_is_file",
        "rt_create_dir",
        "rt_append_file",
        "rt_list_dir",
        "rt_args",
        "rt_run_cmd",
        "rt_list_new",
        "rt_list_push",
        "rt_sort_cstr_list",
        "rt_strlen",
        "rt_strndup",
    ] {
        emit_aarch64_stub(asm, name);
    }
}

fn emit_aarch64_stub(asm: &mut Aarch64Asm, name: &str) {
    asm.label(name);
    asm.mov_i64(Reg::X0, 0);
    asm.ret();
}

fn emit_aarch64_function(func: &Function, asm: &mut Aarch64Asm) -> Result<(), CodegenError> {
    asm.label(&func.name);
    // Prologue: STP X29, X30, [SP, #-32]!
    asm.stp_pre(Reg::X29, Reg::X30, Reg::SP, -32);
    asm.mov_rr(Reg::X29, Reg::SP);

    // Default return value 0 in X0
    asm.mov_i64(Reg::X0, 0);

    for bb in &func.blocks {
        asm.label(format!("{}_bb{}", func.name, bb.id.0));
        for inst in &bb.insts {
            match inst {
                Inst::LoadConst { dest: _, value } => match value {
                    Const::Int(n) => {
                        asm.mov_i64(Reg::X0, *n);
                    }
                    Const::Bool(b) => {
                        asm.mov_i64(Reg::X0, if *b { 1 } else { 0 });
                    }
                    Const::Nil => {
                        asm.mov_i64(Reg::X0, 0);
                    }
                    _ => {}
                },
                Inst::Binary {
                    dest: _,
                    op,
                    lhs: _,
                    rhs: _,
                } => match op {
                    BinOp::Add => asm.add_rr(Reg::X0, Reg::X0, Reg::X1),
                    BinOp::Sub => asm.sub_rr(Reg::X0, Reg::X0, Reg::X1),
                    BinOp::Mul => asm.mul_rr(Reg::X0, Reg::X0, Reg::X1),
                    BinOp::Div => asm.sdiv_rr(Reg::X0, Reg::X0, Reg::X1),
                    BinOp::Eq => {
                        asm.cmp_rr(Reg::X0, Reg::X1);
                        asm.cset(Reg::X0, Cond::EQ);
                    }
                    BinOp::Ne => {
                        asm.cmp_rr(Reg::X0, Reg::X1);
                        asm.cset(Reg::X0, Cond::NE);
                    }
                    BinOp::Lt => {
                        asm.cmp_rr(Reg::X0, Reg::X1);
                        asm.cset(Reg::X0, Cond::LT);
                    }
                    BinOp::Le => {
                        asm.cmp_rr(Reg::X0, Reg::X1);
                        asm.cset(Reg::X0, Cond::LE);
                    }
                    BinOp::Gt => {
                        asm.cmp_rr(Reg::X0, Reg::X1);
                        asm.cset(Reg::X0, Cond::GT);
                    }
                    BinOp::Ge => {
                        asm.cmp_rr(Reg::X0, Reg::X1);
                        asm.cset(Reg::X0, Cond::GE);
                    }
                    _ => {}
                },
                Inst::Return { value: _ } => {
                    asm.ldp_post(Reg::X29, Reg::X30, Reg::SP, 32);
                    asm.ret();
                }
                Inst::Jump { target } => {
                    asm.b_label(format!("{}_bb{}", func.name, target.0));
                }
                Inst::Branch {
                    cond: _,
                    then_block,
                    else_block,
                } => {
                    asm.cmp_rr(Reg::X0, Reg::XZR);
                    asm.b_cond_label(Cond::NE, format!("{}_bb{}", func.name, then_block.0));
                    asm.b_label(format!("{}_bb{}", func.name, else_block.0));
                }
                _ => {}
            }
        }
    }

    // Epilogue
    asm.ldp_post(Reg::X29, Reg::X30, Reg::SP, 32);
    asm.ret();
    Ok(())
}
