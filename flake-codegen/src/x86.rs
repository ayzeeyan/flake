//! Minimal x86-64 assembler (Intel-ish helpers, raw machine code).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Reg {
    Rax = 0,
    Rcx = 1,
    Rdx = 2,
    Rbx = 3,
    Rsp = 4,
    Rbp = 5,
    Rsi = 6,
    Rdi = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

impl Reg {
    fn id(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cc {
    O,
    P,
    Z,
    NZ,
    L,
    Le,
    G,
    Ge,
    E,
    Ne,
    /// Unsigned above (`ja`).
    A,
    /// Unsigned below (`jb`).
    B,
    /// Unsigned below or equal (`jbe`).
    Be,
    /// Unsigned above or equal (`jae`).
    Ae,
}

pub struct Asm {
    pub bytes: Vec<u8>,
    labels: HashMap<String, usize>,
    patches: Vec<Patch>,
}

struct Patch {
    at: usize,
    label: String,
    /// true = rel32 from at+4, false = abs64 (not used)
    rel32: bool,
}

impl Asm {
    pub fn new() -> Self {
        Self {
            bytes: Vec::new(),
            labels: HashMap::new(),
            patches: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn label(&mut self, name: impl Into<String>) {
        self.labels.insert(name.into(), self.bytes.len());
    }

    #[allow(dead_code)]
    pub fn here(&self) -> usize {
        self.bytes.len()
    }

    pub fn push(&mut self, r: Reg) {
        self.rex_b(r);
        self.bytes.push(0x50 + (r.id() & 7));
    }

    pub fn pop(&mut self, r: Reg) {
        self.rex_b(r);
        self.bytes.push(0x58 + (r.id() & 7));
    }

    pub fn ret(&mut self) {
        self.bytes.push(0xC3);
    }

    pub fn mov_rr(&mut self, dst: Reg, src: Reg) {
        if dst == src {
            return;
        }
        // mov dst, src  (89 /r)  dst = r/m, src = reg
        self.rex_wr(src, dst);
        self.bytes.push(0x89);
        self.modrm_rr(src, dst);
    }

    pub fn mov_ri(&mut self, dst: Reg, imm: i64) {
        if imm == 0 {
            self.xor_rr(dst, dst);
            return;
        }
        if (0..=0x7FFF_FFFF).contains(&imm) {
            if dst.id() >= 8 {
                self.bytes.push(0x41); // REX.B
            }
            self.bytes.push(0xB8 + (dst.id() & 7));
            self.bytes.extend_from_slice(&(imm as u32).to_le_bytes());
            return;
        }
        self.rex_wb(dst);
        self.bytes.push(0xB8 + (dst.id() & 7));
        self.bytes.extend_from_slice(&imm.to_le_bytes());
    }

    pub fn mov_rm_rbp(&mut self, dst: Reg, disp: i32) {
        // mov dst, [rbp+disp]
        self.rex_wr_rm(dst, Reg::Rbp);
        self.bytes.push(0x8B);
        self.modrm_disp(dst, Reg::Rbp, disp);
    }

    pub fn mov_mr_rbp(&mut self, disp: i32, src: Reg) {
        // mov [rbp+disp], src
        self.rex_wr_rm(src, Reg::Rbp);
        self.bytes.push(0x89);
        self.modrm_disp(src, Reg::Rbp, disp);
    }

    pub fn mov_m8_rbp(&mut self, disp: i32, src: Reg) {
        if src.id() >= 8 {
            self.bytes.push(0x44);
        } else if matches!(src, Reg::Rsp | Reg::Rbp | Reg::Rsi | Reg::Rdi) {
            self.bytes.push(0x40);
        }
        self.bytes.push(0x88);
        self.modrm_disp(src, Reg::Rbp, disp);
    }

    pub fn mov_m8_imm_rbp(&mut self, disp: i32, value: u8) {
        self.bytes.push(0xC6);
        self.modrm_disp(Reg::Rax, Reg::Rbp, disp);
        self.bytes.push(value);
    }

    pub fn cmp_m8_imm_rbp(&mut self, disp: i32, value: u8) {
        self.bytes.push(0x80);
        if (-128..=127).contains(&disp) {
            self.bytes.push(0b01_111_101);
            self.bytes.push(disp as i8 as u8);
        } else {
            self.bytes.push(0b10_111_101);
            self.bytes.extend_from_slice(&disp.to_le_bytes());
        }
        self.bytes.push(value);
    }

    pub fn mov_rm(&mut self, dst: Reg, base: Reg, disp: i32) {
        self.rex_wr(dst, base);
        self.bytes.push(0x8B);
        self.modrm_disp(dst, base, disp);
    }

    pub fn mov_mr(&mut self, base: Reg, disp: i32, src: Reg) {
        self.rex_wr(src, base);
        self.bytes.push(0x89);
        self.modrm_disp(src, base, disp);
    }

    /// `mov [rsp+disp], src` (SIB form; `rsp` cannot use a plain ModRM).
    pub fn mov_mr_rsp(&mut self, disp: i32, src: Reg) {
        self.rex_wr(src, Reg::Rsp);
        self.bytes.push(0x89);
        if (-128..128).contains(&disp) {
            self.bytes.push(((src.id() & 7) << 3) | 0b01_000_100);
            self.bytes.push(0x24);
            self.bytes.push(disp as i8 as u8);
        } else {
            self.bytes.push(((src.id() & 7) << 3) | 0b10_000_100);
            self.bytes.push(0x24);
            self.bytes.extend_from_slice(&disp.to_le_bytes());
        }
    }

    pub fn add_ri(&mut self, dst: Reg, imm: i32) {
        self.rex_wb(dst);
        self.bytes.push(0x81);
        self.modrm_rr_op(0, dst);
        self.bytes.extend_from_slice(&imm.to_le_bytes());
    }

    pub fn shl_ri(&mut self, dst: Reg, imm: u8) {
        self.rex_wb(dst);
        self.bytes.push(0xC1);
        self.modrm_rr_op(4, dst);
        self.bytes.push(imm);
    }

    pub fn add_rr(&mut self, dst: Reg, src: Reg) {
        self.rex_wr(src, dst);
        self.bytes.push(0x01);
        self.modrm_rr(src, dst);
    }

    pub fn sub_rr(&mut self, dst: Reg, src: Reg) {
        self.rex_wr(src, dst);
        self.bytes.push(0x29);
        self.modrm_rr(src, dst);
    }

    pub fn sub_ri(&mut self, dst: Reg, imm: i32) {
        self.rex_wb(dst);
        self.bytes.push(0x81);
        self.modrm_rr_op(5, dst);
        self.bytes.extend_from_slice(&imm.to_le_bytes());
    }

    pub fn imul_rr(&mut self, dst: Reg, src: Reg) {
        self.rex_wr(dst, src);
        self.bytes.push(0x0F);
        self.bytes.push(0xAF);
        self.modrm_rr(dst, src);
    }

    pub fn cqo(&mut self) {
        self.bytes.push(0x48);
        self.bytes.push(0x99);
    }

    pub fn idiv(&mut self, src: Reg) {
        self.rex_wb(src);
        self.bytes.push(0xF7);
        self.modrm_rr_op(7, src);
    }

    pub fn xor_rr(&mut self, dst: Reg, src: Reg) {
        self.rex_wr(src, dst);
        self.bytes.push(0x31);
        self.modrm_rr(src, dst);
    }

    pub fn and_rr(&mut self, dst: Reg, src: Reg) {
        self.rex_wr(src, dst);
        self.bytes.push(0x21);
        self.modrm_rr(src, dst);
    }

    pub fn cmp_rr(&mut self, a: Reg, b: Reg) {
        self.rex_wr(b, a);
        self.bytes.push(0x39);
        self.modrm_rr(b, a);
    }

    pub fn test_rr(&mut self, a: Reg, b: Reg) {
        self.rex_wr(b, a);
        self.bytes.push(0x85);
        self.modrm_rr(b, a);
    }

    pub fn setcc(&mut self, cc: Cc, dst: Reg) {
        // setcc r/m8 — use low byte of dst, then movzx
        if dst.id() >= 8 {
            self.bytes.push(0x41); // REX.B
        } else if matches!(dst, Reg::Rsi | Reg::Rdi | Reg::Rsp | Reg::Rbp) {
            self.bytes.push(0x40);
        }
        self.bytes.push(0x0F);
        self.bytes.push(setcc_op(cc));
        self.modrm_rr_op(0, dst);
    }

    pub fn movzx_rax_al(&mut self) {
        self.bytes.extend_from_slice(&[0x48, 0x0F, 0xB6, 0xC0]);
    }

    pub fn jmp_label(&mut self, label: impl Into<String>) {
        self.bytes.push(0xE9);
        self.reloc_rel32(label.into());
    }

    pub fn jcc_label(&mut self, cc: Cc, label: impl Into<String>) {
        self.bytes.push(0x0F);
        self.bytes.push(jcc_op(cc));
        self.reloc_rel32(label.into());
    }

    pub fn call_label(&mut self, label: impl Into<String>) {
        self.bytes.push(0xE8);
        self.reloc_rel32(label.into());
    }

    /// `call r64`
    pub fn call_r(&mut self, r: Reg) {
        if r.id() >= 8 {
            self.bytes.push(0x41);
        }
        self.bytes.push(0xFF);
        self.bytes.push(0xD0 + (r.id() & 7));
    }

    /// `movq xmm{k}, r64`  (SSE2)
    pub fn movq_xmm_r(&mut self, xmm: u8, src: Reg) {
        // 66 REX.W 0F 6E /r
        let mut rex = 0x48;
        if src.id() >= 8 {
            rex |= 0x01;
        }
        self.bytes.push(0x66);
        self.bytes.push(rex);
        self.bytes.push(0x0F);
        self.bytes.push(0x6E);
        self.bytes.push(0b11_000_000 | (xmm << 3) | (src.id() & 7));
    }

    /// `movq r64, xmm{k}`
    pub fn movq_r_xmm(&mut self, dst: Reg, xmm: u8) {
        let mut rex = 0x48;
        if dst.id() >= 8 {
            rex |= 0x01;
        }
        self.bytes.push(0x66);
        self.bytes.push(rex);
        self.bytes.push(0x0F);
        self.bytes.push(0x7E);
        self.bytes.push(0b11_000_000 | (xmm << 3) | (dst.id() & 7));
    }

    /// `addsd xmm0, xmm1`
    pub fn addsd_xmm0_xmm1(&mut self) {
        self.bytes.extend_from_slice(&[0xF2, 0x0F, 0x58, 0xC1]);
    }

    /// `subsd xmm0, xmm1`
    pub fn subsd_xmm0_xmm1(&mut self) {
        self.bytes.extend_from_slice(&[0xF2, 0x0F, 0x5C, 0xC1]);
    }

    /// `mulsd xmm0, xmm1`
    pub fn mulsd_xmm0_xmm1(&mut self) {
        self.bytes.extend_from_slice(&[0xF2, 0x0F, 0x59, 0xC1]);
    }

    /// `divsd xmm0, xmm1`
    pub fn divsd_xmm0_xmm1(&mut self) {
        self.bytes.extend_from_slice(&[0xF2, 0x0F, 0x5E, 0xC1]);
    }

    /// `ucomisd xmm0, xmm1`
    pub fn ucomisd_xmm0_xmm1(&mut self) {
        self.bytes.extend_from_slice(&[0x66, 0x0F, 0x2E, 0xC1]);
    }

    /// `cvtsi2sd xmm0, r64`
    pub fn cvtsi2sd_xmm0(&mut self, src: Reg) {
        self.cvtsi2sd_xmm(0, src);
    }

    /// `cvtsi2sd xmm{k}, r64`
    pub fn cvtsi2sd_xmm(&mut self, xmm: u8, src: Reg) {
        let mut rex = 0x48;
        if src.id() >= 8 {
            rex |= 0x01;
        }
        self.bytes.extend_from_slice(&[0xF2, rex, 0x0F, 0x2A]);
        self.bytes
            .push(0b11_000_000 | ((xmm & 7) << 3) | (src.id() & 7));
    }

    /// `cvttsd2si r64, xmm0`
    pub fn cvttsd2si_r_xmm0(&mut self, dst: Reg) {
        let mut rex = 0x48;
        if dst.id() >= 8 {
            rex |= 0x04;
        }
        self.bytes.extend_from_slice(&[0xF2, rex, 0x0F, 0x2C]);
        self.bytes.push(0b11_000_000 | ((dst.id() & 7) << 3));
    }

    /// `call qword ptr [rip+rel32]` — IAT import. `disp` is patched later as absolute RVA diff.
    pub fn call_indirect_rip(&mut self) -> usize {
        self.bytes.extend_from_slice(&[0xFF, 0x15]);
        let at = self.bytes.len();
        self.bytes.extend_from_slice(&0i32.to_le_bytes());
        at
    }

    pub fn lea_rip(&mut self, dst: Reg) -> usize {
        self.rex_wr_rm(dst, Reg::Rbp); // r/m unused; we emit RIP form
        self.bytes.push(0x8D);
        // mod=00, r/m=101 means RIP+disp32 in 64-bit
        let modrm = ((dst.id() & 7) << 3) | 0b101;
        self.bytes.push(modrm);
        let at = self.bytes.len();
        self.bytes.extend_from_slice(&0i32.to_le_bytes());
        at
    }

    /// `lea dst, [rip+label]` for an in-image code or data symbol.
    pub fn lea_label(&mut self, dst: Reg, label: impl Into<String>) {
        self.rex_wr_rm(dst, Reg::Rbp);
        self.bytes.push(0x8D);
        self.bytes.push(((dst.id() & 7) << 3) | 0b101);
        self.reloc_rel32(label.into());
    }

    #[allow(dead_code)]
    pub fn patch_rip(&mut self, at: usize, target_rva: u32, instr_end_rva: u32) {
        let rel = target_rva as i32 - instr_end_rva as i32;
        self.bytes[at..at + 4].copy_from_slice(&rel.to_le_bytes());
    }

    fn reloc_rel32(&mut self, label: String) {
        let at = self.bytes.len();
        self.bytes.extend_from_slice(&0i32.to_le_bytes());
        self.patches.push(Patch {
            at,
            label,
            rel32: true,
        });
    }

    pub fn finish(&mut self) -> Result<(), String> {
        let patches = std::mem::take(&mut self.patches);
        for p in patches {
            let target = *self
                .labels
                .get(&p.label)
                .ok_or_else(|| format!("undefined label {}", p.label))?;
            if p.rel32 {
                let next = (p.at + 4) as i32;
                let rel = target as i32 - next;
                self.bytes[p.at..p.at + 4].copy_from_slice(&rel.to_le_bytes());
            }
        }
        Ok(())
    }

    fn rex_b(&mut self, r: Reg) {
        if r.id() >= 8 {
            self.bytes.push(0x41);
        }
    }

    fn rex_wb(&mut self, r: Reg) {
        let mut rex = 0x48;
        if r.id() >= 8 {
            rex |= 0x01;
        }
        self.bytes.push(rex);
    }

    fn rex_wr(&mut self, reg: Reg, rm: Reg) {
        let mut rex = 0x48;
        if reg.id() >= 8 {
            rex |= 0x04;
        }
        if rm.id() >= 8 {
            rex |= 0x01;
        }
        self.bytes.push(rex);
    }

    fn rex_wr_rm(&mut self, reg: Reg, rm: Reg) {
        self.rex_wr(reg, rm);
    }

    fn modrm_rr(&mut self, reg: Reg, rm: Reg) {
        let byte = 0b11_000_000 | ((reg.id() & 7) << 3) | (rm.id() & 7);
        self.bytes.push(byte);
    }

    fn modrm_rr_op(&mut self, op: u8, rm: Reg) {
        let byte = 0b11_000_000 | (op << 3) | (rm.id() & 7);
        self.bytes.push(byte);
    }

    fn modrm_disp(&mut self, reg: Reg, rm: Reg, disp: i32) {
        // rbp as r/m requires a displacement
        if (-128..=127).contains(&disp) {
            let byte = 0b01_000_000 | ((reg.id() & 7) << 3) | (rm.id() & 7);
            self.bytes.push(byte);
            self.bytes.push(disp as i8 as u8);
        } else {
            let byte = 0b10_000_000 | ((reg.id() & 7) << 3) | (rm.id() & 7);
            self.bytes.push(byte);
            self.bytes.extend_from_slice(&disp.to_le_bytes());
        }
    }
}

fn setcc_op(cc: Cc) -> u8 {
    match cc {
        Cc::O => 0x90,
        Cc::P => 0x9A,
        Cc::Z | Cc::E => 0x94,
        Cc::NZ | Cc::Ne => 0x95,
        Cc::L => 0x9C,
        Cc::Le => 0x9E,
        Cc::G => 0x9F,
        Cc::Ge => 0x9D,
        Cc::A => 0x97,
        Cc::B => 0x92,
        Cc::Be => 0x96,
        Cc::Ae => 0x93,
    }
}

fn jcc_op(cc: Cc) -> u8 {
    match cc {
        Cc::O => 0x80,
        Cc::P => 0x8A,
        Cc::Z | Cc::E => 0x84,
        Cc::NZ | Cc::Ne => 0x85,
        Cc::L => 0x8C,
        Cc::Le => 0x8E,
        Cc::G => 0x8F,
        Cc::Ge => 0x8D,
        Cc::A => 0x87,
        Cc::B => 0x82,
        Cc::Be => 0x86,
        Cc::Ae => 0x83,
    }
}
