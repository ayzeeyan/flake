//! Lower Flake IR to x86-64 machine code (Windows x64 ABI).

use flake_ir::{BinOp, Callee, Const, Function, Inst, IrType, LocalId, Module, UnOp};

use crate::error::CodegenError;
use crate::regalloc::{Frame, allocate};
use crate::target::TargetOs;
use crate::x86::{Asm, Cc, Reg};

pub struct Compiled {
    pub code: Vec<u8>,
    pub strings: Vec<Vec<u8>>,
    pub entry: usize,
    pub iat_patches: Vec<(usize, usize)>,
    pub str_patches: Vec<(usize, usize)>,
    pub global_patches: Vec<(usize, usize)>,
    pub gas: String,
}

#[derive(Clone, Copy)]
pub enum Import {
    GetStdHandle = 0,
    WriteFile = 1,
    ExitProcess = 2,
    GetProcessHeap = 3,
    HeapAlloc = 4,
    CreateFileA = 5,
    GetFileSize = 6,
    ReadFile = 7,
    CloseHandle = 8,
    GetFileAttributesA = 9,
    GetEnvironmentVariableA = 10,
    GetCurrentDirectoryA = 11,
    DeleteFileA = 12,
    FindFirstFileA = 13,
    FindNextFileA = 14,
    FindClose = 15,
    GetCommandLineA = 16,
    CreateDirectoryA = 17,
    CreateProcessA = 18,
    WaitForSingleObject = 19,
    GetExitCodeProcess = 20,
    CreatePipe = 21,
    SetHandleInformation = 22,
    HeapFree = 23,
}

pub const IMPORTS: &[&str] = &[
    "GetStdHandle",
    "WriteFile",
    "ExitProcess",
    "GetProcessHeap",
    "HeapAlloc",
    "CreateFileA",
    "GetFileSize",
    "ReadFile",
    "CloseHandle",
    "GetFileAttributesA",
    "GetEnvironmentVariableA",
    "GetCurrentDirectoryA",
    "DeleteFileA",
    "FindFirstFileA",
    "FindNextFileA",
    "FindClose",
    "GetCommandLineA",
    "CreateDirectoryA",
    "CreateProcessA",
    "WaitForSingleObject",
    "GetExitCodeProcess",
    "CreatePipe",
    "SetHandleInformation",
    "HeapFree",
];

pub fn compile_module(module: &Module) -> Result<Compiled, CodegenError> {
    compile_module_for(module, TargetOs::Windows)
}

pub fn compile_module_for(module: &Module, os: TargetOs) -> Result<Compiled, CodegenError> {
    let mut asm = Asm::new();
    let mut strings: Vec<Vec<u8>> = Vec::new();
    let mut iat_patches = Vec::new();
    let mut str_patches = Vec::new();
    let mut global_patches = Vec::new();
    let mut gas = String::new();
    match os {
        TargetOs::Windows => {
            gas.push_str("# Flake x86-64 (Windows) — generated, no LLVM/Cranelift\n");
        }
        TargetOs::Linux => {
            gas.push_str("# Flake x86-64 (Linux ELF) — syscalls, no libc/IAT\n");
        }
    }
    gas.push_str(".intel_syntax noprefix\n.text\n");

    intern_str(&mut strings, b"true");
    intern_str(&mut strings, b"false");
    intern_str(&mut strings, b"nil");
    intern_str(&mut strings, b"\n");
    intern_str(&mut strings, b" ");

    emit_start(&mut asm, &mut iat_patches, &mut gas, os);
    crate::runtime::emit_runtime(&mut asm, &mut iat_patches, &mut global_patches, os);

    let mut uniq = 0u32;
    for func in &module.functions {
        emit_function(
            module,
            func,
            &mut asm,
            &mut strings,
            &mut iat_patches,
            &mut str_patches,
            &mut gas,
            &mut uniq,
        )?;
    }

    asm.finish().map_err(CodegenError::new)?;
    Ok(Compiled {
        code: asm.bytes,
        strings,
        entry: 0,
        iat_patches,
        str_patches,
        global_patches,
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

fn intern_cstring(strings: &mut Vec<Vec<u8>>, s: &str) -> usize {
    intern_str(strings, s.as_bytes())
}

fn emit_start(asm: &mut Asm, iat: &mut Vec<(usize, usize)>, gas: &mut String, os: TargetOs) {
    match os {
        TargetOs::Linux => {
            crate::runtime_linux::emit_linux_start(asm, gas);
        }
        TargetOs::Windows => {
            asm.label("_start");
            gas.push_str(".global _start\n_start:\n");
            gas.push_str("    sub rsp, 40\n    call main\n    xor ecx, ecx\n    call rt_exit\n");
            asm.sub_ri(Reg::Rsp, 40);
            asm.call_label("main");
            asm.xor_rr(Reg::Rcx, Reg::Rcx);
            asm.call_label("rt_exit");
            let _ = iat;
        }
    }
}

fn prologue(asm: &mut Asm, frame: &Frame) {
    asm.push(Reg::Rbp);
    asm.mov_rr(Reg::Rbp, Reg::Rsp);
    asm.sub_ri(Reg::Rsp, frame.frame_size);
    for (i, r) in frame.saved.iter().enumerate() {
        asm.mov_mr_rbp(-8 * (i as i32 + 1), *r);
    }
}

fn epilogue(asm: &mut Asm, frame: &Frame) {
    for (i, r) in frame.saved.iter().enumerate() {
        asm.mov_rm_rbp(*r, -8 * (i as i32 + 1));
    }
    asm.mov_rr(Reg::Rsp, Reg::Rbp);
    asm.pop(Reg::Rbp);
    asm.ret();
}

#[allow(clippy::too_many_arguments)]
fn emit_function(
    module: &Module,
    func: &Function,
    asm: &mut Asm,
    strings: &mut Vec<Vec<u8>>,
    iat: &mut Vec<(usize, usize)>,
    strs: &mut Vec<(usize, usize)>,
    gas: &mut String,
    uniq: &mut u32,
) -> Result<(), CodegenError> {
    let _ = iat;
    asm.label(&func.name);
    gas.push_str(&format!("\n{}:\n", func.name));
    let frame = allocate(func);
    prologue(asm, &frame);
    gas.push_str(&format!(
        "    push rbp\n    mov rbp, rsp\n    sub rsp, {}\n",
        frame.frame_size
    ));
    for (i, r) in frame.saved.iter().enumerate() {
        gas.push_str(&format!("    ; save {:?} at [rbp-{}]\n", r, 8 * (i + 1)));
    }
    for (i, loc) in frame.loc.iter().enumerate() {
        match loc {
            crate::regalloc::Loc::Reg(r) => {
                gas.push_str(&format!("    ; local {i} -> {:?}\n", r));
            }
            crate::regalloc::Loc::Slot(d) => {
                gas.push_str(&format!("    ; local {i} -> [rbp{d:+}]\n"));
            }
        }
    }

    let win_args = [Reg::Rcx, Reg::Rdx, Reg::R8, Reg::R9];
    for (i, _) in func.params.iter().enumerate() {
        if i < 4 {
            frame.store(asm, LocalId(i as u32), win_args[i]);
        } else {
            let disp = 48 + 8 * ((i as i32) - 4);
            asm.mov_rm_rbp(Reg::Rax, disp);
            frame.store(asm, LocalId(i as u32), Reg::Rax);
        }
    }

    for block in &func.blocks {
        let lab = format!("{}_bb{}", func.name, block.id.0);
        asm.label(&lab);
        gas.push_str(&format!(".{lab}:\n"));
        for inst in &block.insts {
            emit_inst(module, func, &frame, inst, asm, strings, strs, gas, uniq)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_inst(
    module: &Module,
    func: &Function,
    frame: &Frame,
    inst: &Inst,
    asm: &mut Asm,
    strings: &mut Vec<Vec<u8>>,
    strs: &mut Vec<(usize, usize)>,
    gas: &mut String,
    uniq: &mut u32,
) -> Result<(), CodegenError> {
    match inst {
        Inst::LoadConst { dest, value } => match value {
            Const::Int(n) => {
                asm.mov_ri(Reg::Rax, *n);
                frame.store(asm, *dest, Reg::Rax);
                gas.push_str(&format!("    mov rax, {n}\n    store local {}\n", dest.0));
            }
            Const::Bool(v) => {
                asm.mov_ri(Reg::Rax, if *v { 1 } else { 0 });
                frame.store(asm, *dest, Reg::Rax);
            }
            Const::Nil => {
                asm.xor_rr(Reg::Rax, Reg::Rax);
                frame.store(asm, *dest, Reg::Rax);
            }
            Const::String(s) => {
                let idx = intern_cstring(strings, s);
                let at = asm.lea_rip(Reg::Rax);
                strs.push((at, idx));
                frame.store(asm, *dest, Reg::Rax);
            }
            Const::Float(n) => {
                asm.mov_ri(Reg::Rax, n.to_bits() as i64);
                frame.store(asm, *dest, Reg::Rax);
            }
        },
        Inst::LoadFunction { dest, name } => {
            if !module
                .functions
                .iter()
                .any(|function| function.name == *name)
            {
                return Err(CodegenError::new(format!(
                    "native backend cannot use builtin `{name}` as a function value; wrap it in a user function"
                )));
            }
            asm.lea_label(Reg::Rax, name);
            frame.store(asm, *dest, Reg::Rax);
            gas.push_str(&format!("    lea rax, [rip + {name}]\n"));
        }
        Inst::Move { dest, src } => {
            frame.load(asm, *src, Reg::Rax);
            frame.store(asm, *dest, Reg::Rax);
        }
        Inst::Binary { dest, op, lhs, rhs } => {
            let lhs_ty = local_ty(func, *lhs);
            let rhs_ty = local_ty(func, *rhs);
            let stringy = is_string_ty(&lhs_ty) || is_string_ty(&rhs_ty);
            let dyn_cmp = matches!(lhs_ty, IrType::Dyn) || matches!(rhs_ty, IrType::Dyn);
            if matches!(
                op,
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
            ) && (stringy || dyn_cmp)
            {
                frame.load(asm, *lhs, Reg::Rcx);
                frame.load(asm, *rhs, Reg::Rdx);
                if stringy && matches!(op, BinOp::Eq | BinOp::Ne) {
                    asm.call_label("rt_streq");
                    if matches!(op, BinOp::Ne) {
                        asm.test_rr(Reg::Rax, Reg::Rax);
                        asm.setcc(Cc::Z, Reg::Rax);
                        asm.movzx_rax_al();
                    }
                } else {
                    if stringy {
                        asm.call_label("rt_strcmp");
                    } else {
                        asm.call_label("rt_val_cmp");
                    }
                    emit_signed_cmp_bool(asm, *op);
                }
                frame.store(asm, *dest, Reg::Rax);
                return Ok(());
            }
            frame.load(asm, *lhs, Reg::Rax);
            frame.load(asm, *rhs, Reg::R10);
            let floaty = matches!(local_ty(func, *lhs), IrType::Float)
                || matches!(local_ty(func, *rhs), IrType::Float);
            if floaty {
                if matches!(local_ty(func, *lhs), IrType::Float) {
                    asm.movq_xmm_r(0, Reg::Rax);
                } else {
                    asm.cvtsi2sd_xmm(0, Reg::Rax);
                }
                if matches!(local_ty(func, *rhs), IrType::Float) {
                    asm.movq_xmm_r(1, Reg::R10);
                } else {
                    asm.cvtsi2sd_xmm(1, Reg::R10);
                }
                match op {
                    BinOp::Add => asm.addsd_xmm0_xmm1(),
                    BinOp::Sub => asm.subsd_xmm0_xmm1(),
                    BinOp::Mul => asm.mulsd_xmm0_xmm1(),
                    BinOp::Div => asm.divsd_xmm0_xmm1(),
                    BinOp::Rem => {
                        asm.movq_r_xmm(Reg::R9, 0);
                        asm.divsd_xmm0_xmm1();
                        asm.cvttsd2si_r_xmm0(Reg::R11);
                        asm.cvtsi2sd_xmm0(Reg::R11);
                        asm.mulsd_xmm0_xmm1();
                        asm.movq_r_xmm(Reg::R11, 0);
                        asm.movq_xmm_r(0, Reg::R9);
                        asm.movq_xmm_r(1, Reg::R11);
                        asm.subsd_xmm0_xmm1();
                    }
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        asm.ucomisd_xmm0_xmm1();
                        let id = next_id(uniq);
                        let unordered = format!(".fcmp_unordered{id}");
                        let done = format!(".fcmp_done{id}");
                        asm.jcc_label(Cc::P, &unordered);
                        let cc = match op {
                            BinOp::Eq => Cc::E,
                            BinOp::Ne => Cc::Ne,
                            BinOp::Lt => Cc::B,
                            BinOp::Le => Cc::Be,
                            BinOp::Gt => Cc::A,
                            BinOp::Ge => Cc::Ae,
                            _ => unreachable!(),
                        };
                        asm.setcc(cc, Reg::Rax);
                        asm.movzx_rax_al();
                        asm.jmp_label(&done);
                        asm.label(unordered);
                        if matches!(op, BinOp::Eq | BinOp::Ne) {
                            asm.mov_ri(Reg::Rax, i64::from(matches!(op, BinOp::Ne)));
                        } else {
                            emit_runtime_failure(asm, strings, strs, "cannot compare NaN");
                        }
                        asm.label(done);
                        frame.store(asm, *dest, Reg::Rax);
                        return Ok(());
                    }
                    _ => {}
                }
                asm.movq_r_xmm(Reg::Rax, 0);
                frame.store(asm, *dest, Reg::Rax);
                return Ok(());
            }
            let lhs_ty = local_ty(func, *lhs);
            let rhs_ty = local_ty(func, *rhs);
            if matches!(op, BinOp::Add) {
                if matches!(lhs_ty, IrType::String) || matches!(rhs_ty, IrType::String) {
                    asm.mov_rr(Reg::Rcx, Reg::Rax);
                    asm.mov_rr(Reg::Rdx, Reg::R10);
                    asm.call_label("rt_concat2");
                    frame.store(asm, *dest, Reg::Rax);
                    return Ok(());
                }
                if matches!(lhs_ty, IrType::List(_)) && matches!(rhs_ty, IrType::List(_)) {
                    asm.mov_rr(Reg::Rcx, Reg::Rax);
                    asm.mov_rr(Reg::Rdx, Reg::R10);
                    asm.call_label("rt_list_concat");
                    frame.store(asm, *dest, Reg::Rax);
                    return Ok(());
                }
            }
            match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul => {
                    match op {
                        BinOp::Add => asm.add_rr(Reg::Rax, Reg::R10),
                        BinOp::Sub => asm.sub_rr(Reg::Rax, Reg::R10),
                        BinOp::Mul => asm.imul_rr(Reg::Rax, Reg::R10),
                        _ => unreachable!(),
                    }
                    let id = next_id(uniq);
                    let overflow = format!(".bin_overflow{id}");
                    let done = format!(".bin_done{id}");
                    asm.jcc_label(Cc::O, &overflow);
                    asm.jmp_label(&done);
                    asm.label(overflow);
                    emit_runtime_failure(asm, strings, strs, "integer overflow");
                    asm.label(done);
                }
                BinOp::Div | BinOp::Rem => {
                    let id = next_id(uniq);
                    let nonzero = format!(".div_nonzero{id}");
                    let safe = format!(".div_safe{id}");
                    asm.test_rr(Reg::R10, Reg::R10);
                    asm.jcc_label(Cc::NZ, &nonzero);
                    emit_runtime_failure(asm, strings, strs, "division by zero");
                    asm.label(nonzero);
                    asm.mov_ri(Reg::R11, -1);
                    asm.cmp_rr(Reg::R10, Reg::R11);
                    asm.jcc_label(Cc::Ne, &safe);
                    asm.mov_ri(Reg::R11, i64::MIN);
                    asm.cmp_rr(Reg::Rax, Reg::R11);
                    asm.jcc_label(Cc::Ne, &safe);
                    emit_runtime_failure(asm, strings, strs, "integer overflow");
                    asm.label(safe);
                    asm.cqo();
                    asm.idiv(Reg::R10);
                    if matches!(op, BinOp::Rem) {
                        asm.mov_rr(Reg::Rax, Reg::Rdx);
                    }
                }
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    asm.cmp_rr(Reg::Rax, Reg::R10);
                    let cc = match op {
                        BinOp::Eq => Cc::E,
                        BinOp::Ne => Cc::Ne,
                        BinOp::Lt => Cc::L,
                        BinOp::Le => Cc::Le,
                        BinOp::Gt => Cc::G,
                        BinOp::Ge => Cc::Ge,
                        _ => unreachable!(),
                    };
                    asm.setcc(cc, Reg::Rax);
                    asm.movzx_rax_al();
                }
                BinOp::And => {
                    // bitwise for 0/1 bools
                    asm.bytes.extend_from_slice(&[0x4C, 0x21, 0xD0]); // and rax, r10
                }
                BinOp::Or => {
                    asm.bytes.extend_from_slice(&[0x4C, 0x09, 0xD0]); // or rax, r10
                }
            }
            frame.store(asm, *dest, Reg::Rax);
        }
        Inst::Unary { dest, op, src } => {
            frame.load(asm, *src, Reg::Rax);
            match op {
                UnOp::Neg if matches!(local_ty(func, *src), IrType::Float) => {
                    asm.mov_ri(Reg::R10, i64::MIN);
                    asm.xor_rr(Reg::Rax, Reg::R10);
                }
                UnOp::Neg => {
                    asm.bytes.extend_from_slice(&[0x48, 0xF7, 0xD8]);
                    let id = next_id(uniq);
                    let overflow = format!(".neg_overflow{id}");
                    let done = format!(".neg_done{id}");
                    asm.jcc_label(Cc::O, &overflow);
                    asm.jmp_label(&done);
                    asm.label(overflow);
                    emit_runtime_failure(asm, strings, strs, "integer overflow");
                    asm.label(done);
                }
                UnOp::Not => {
                    asm.test_rr(Reg::Rax, Reg::Rax);
                    asm.setcc(Cc::Z, Reg::Rax);
                    asm.movzx_rax_al();
                }
            }
            frame.store(asm, *dest, Reg::Rax);
        }
        Inst::Call { dest, callee, args } => {
            emit_call(
                func,
                frame,
                dest.as_ref(),
                callee,
                args,
                asm,
                strings,
                strs,
                uniq,
            )?;
        }
        Inst::Spawn { dest, callee, args } => {
            let spill = frame.spill;
            emit_call(func, frame, None, callee, args, asm, strings, strs, uniq)?;
            asm.mov_mr_rbp(spill, Reg::Rax);
            asm.mov_ri(Reg::Rcx, 16);
            asm.call_label("rt_alloc");
            asm.mov_ri(Reg::R10, 0);
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            asm.mov_rm_rbp(Reg::R10, spill);
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
            frame.store(asm, *dest, Reg::Rax);
        }
        Inst::Await { dest, task } => {
            let id = next_id(uniq);
            let ok = format!(".task_ok_{id}");
            let already_awaited = format!(".task_already_{id}");
            let cancelled = format!(".task_cancelled_{id}");

            frame.load(asm, *task, Reg::Rax);
            asm.mov_rm(Reg::R10, Reg::Rax, 0);
            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::Z, &ok);

            asm.mov_ri(Reg::R11, 2);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::E, &cancelled);

            asm.label(&already_awaited);
            emit_runtime_failure(asm, strings, strs, "task was already awaited");

            asm.label(&cancelled);
            emit_runtime_failure(asm, strings, strs, "task was cancelled");

            asm.label(&ok);
            asm.mov_ri(Reg::R10, 1);
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            asm.mov_rm(Reg::Rax, Reg::Rax, 8);
            frame.store(asm, *dest, Reg::Rax);
        }
        Inst::Concat { dest, parts } => {
            if parts.is_empty() {
                let idx = intern_cstring(strings, "");
                let at = asm.lea_rip(Reg::Rax);
                strs.push((at, idx));
                frame.store(asm, *dest, Reg::Rax);
            } else {
                let spill = frame.spill;
                load_as_cstr(func, frame, parts[0], asm, strings, strs, uniq);
                asm.mov_mr_rbp(spill, Reg::Rax);
                for p in &parts[1..] {
                    load_as_cstr(func, frame, *p, asm, strings, strs, uniq);
                    asm.mov_rr(Reg::Rdx, Reg::Rax);
                    asm.mov_rm_rbp(Reg::Rcx, spill);
                    asm.call_label("rt_concat2");
                    asm.mov_mr_rbp(spill, Reg::Rax);
                }
                asm.mov_rm_rbp(Reg::Rax, spill);
                frame.store(asm, *dest, Reg::Rax);
            }
        }
        Inst::Jump { target } => {
            asm.jmp_label(format!("{}_bb{}", func.name, target.0));
        }
        Inst::Branch {
            cond,
            then_block,
            else_block,
        } => {
            frame.load(asm, *cond, Reg::Rax);
            asm.test_rr(Reg::Rax, Reg::Rax);
            asm.jcc_label(Cc::NZ, format!("{}_bb{}", func.name, then_block.0));
            asm.jmp_label(format!("{}_bb{}", func.name, else_block.0));
        }
        Inst::Return { value } => {
            if let Some(v) = value {
                frame.load(asm, *v, Reg::Rax);
            } else {
                asm.xor_rr(Reg::Rax, Reg::Rax);
            }
            epilogue(asm, frame);
        }
        Inst::MakeList { dest, items } => {
            let n = items.len() as i64;
            let cap = n.max(4);
            asm.mov_ri(Reg::Rcx, 24);
            asm.call_label("rt_alloc");
            let spill = frame.spill;
            asm.mov_mr_rbp(spill, Reg::Rax);
            asm.mov_ri(Reg::Rcx, 8 * cap);
            asm.call_label("rt_alloc");
            for (i, item) in items.iter().enumerate() {
                frame.load(asm, *item, Reg::R10);
                asm.mov_mr(Reg::Rax, 8 * i as i32, Reg::R10);
            }
            asm.mov_rr(Reg::Rdx, Reg::Rax);
            asm.mov_rm_rbp(Reg::Rax, spill);
            asm.mov_ri(Reg::R10, n);
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            asm.mov_ri(Reg::R10, cap);
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
            asm.mov_mr(Reg::Rax, 16, Reg::Rdx);
            frame.store(asm, *dest, Reg::Rax);
        }
        Inst::GetIndex { dest, obj, index } => {
            emit_get_index(func, frame, dest, obj, index, asm, strings, strs, uniq);
        }
        Inst::SetIndex { obj, index, value } => {
            emit_set_index(func, frame, obj, index, value, asm, strings, strs, uniq);
        }
        Inst::MakeStruct { dest, name, fields } => {
            let st_def = module.structs.iter().find(|s| s.name == *name);
            let total_fields = st_def.map(|s| s.fields.len()).unwrap_or(fields.len());
            let n = total_fields.max(1) as i64;
            asm.mov_ri(Reg::Rcx, 8 * n);
            asm.call_label("rt_alloc");
            if let Some(st) = st_def {
                for (f_name, val) in fields {
                    if let Some(i) = st.fields.iter().position(|(n, _)| n == f_name) {
                        frame.load(asm, *val, Reg::R10);
                        asm.mov_mr(Reg::Rax, 8 * i as i32, Reg::R10);
                    }
                }
            } else {
                for (i, (_, val)) in fields.iter().enumerate() {
                    frame.load(asm, *val, Reg::R10);
                    asm.mov_mr(Reg::Rax, 8 * i as i32, Reg::R10);
                }
            }
            frame.store(asm, *dest, Reg::Rax);
        }
        Inst::GetField { dest, obj, field } => {
            let off = field_offset(module, func, *obj, field)?;
            frame.load(asm, *obj, Reg::Rax);
            asm.mov_rm(Reg::Rax, Reg::Rax, off);
            frame.store(asm, *dest, Reg::Rax);
        }
        Inst::SetField { obj, field, value } => {
            let off = field_offset(module, func, *obj, field)?;
            frame.load(asm, *obj, Reg::Rax);
            frame.load(asm, *value, Reg::R10);
            asm.mov_mr(Reg::Rax, off, Reg::R10);
        }
        Inst::MakeRange { dest, start, end } => {
            asm.mov_ri(Reg::Rcx, 16);
            asm.call_label("rt_alloc");
            frame.load(asm, *start, Reg::R10);
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            frame.load(asm, *end, Reg::R10);
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
            frame.store(asm, *dest, Reg::Rax);
        }
        Inst::MakeIter { dest, src } => {
            emit_make_iter(func, frame, dest, src, asm, uniq);
        }
        Inst::IterNext { value, more, iter } => {
            emit_iter_next(frame, value, more, iter, asm, uniq);
        }
        Inst::MakeMap { dest, keys, values } => {
            let cap = (keys.len() as i64).max(8);
            asm.mov_ri(Reg::Rcx, 16 + 16 * cap);
            asm.call_label("rt_alloc");
            asm.xor_rr(Reg::R10, Reg::R10);
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            asm.mov_ri(Reg::R10, cap);
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
            frame.store(asm, *dest, Reg::Rax);
            let string_keys = match local_ty(func, *dest) {
                IrType::Map(key, _) => map_key_mode(&key) == 1,
                _ => false,
            };
            for (key, value) in keys.iter().zip(values.iter()) {
                frame.load(asm, *dest, Reg::Rcx);
                frame.load(asm, *key, Reg::Rdx);
                frame.load(asm, *value, Reg::R8);
                asm.mov_ri(Reg::R9, i64::from(string_keys));
                asm.call_label("rt_map_set");
                frame.store(asm, *dest, Reg::Rax);
            }
        }
    }
    let _ = gas;
    Ok(())
}

fn next_id(uniq: &mut u32) -> u32 {
    let id = *uniq;
    *uniq += 1;
    id
}

fn emit_runtime_failure(
    asm: &mut Asm,
    strings: &mut Vec<Vec<u8>>,
    strs: &mut Vec<(usize, usize)>,
    message: &str,
) {
    asm.xor_rr(Reg::Rcx, Reg::Rcx);
    let at = asm.lea_rip(Reg::Rdx);
    strs.push((at, intern_cstring(strings, message)));
    asm.call_label("rt_assert");
}

fn load_as_cstr(
    func: &Function,
    frame: &Frame,
    local: LocalId,
    asm: &mut Asm,
    strings: &mut Vec<Vec<u8>>,
    strs: &mut Vec<(usize, usize)>,
    uniq: &mut u32,
) {
    let ty = func
        .local(local)
        .map(|l| l.ty.clone())
        .unwrap_or(IrType::Dyn);
    frame.load(asm, local, Reg::Rcx);
    match ty {
        IrType::String => {
            asm.mov_rr(Reg::Rax, Reg::Rcx);
        }
        IrType::Int => {
            asm.call_label("rt_itoa");
        }
        IrType::Float => {
            asm.call_label("rt_ftoa");
        }
        IrType::Unknown | IrType::Dyn => {
            asm.mov_ri(Reg::R10, 0x10000);
            asm.cmp_rr(Reg::Rcx, Reg::R10);
            let id = next_id(uniq);
            asm.jcc_label(Cc::L, format!(".dyni{id}"));
            asm.mov_rr(Reg::Rax, Reg::Rcx);
            asm.jmp_label(format!(".dynd{id}"));
            asm.label(format!(".dyni{id}"));
            asm.call_label("rt_itoa");
            asm.label(format!(".dynd{id}"));
        }
        IrType::Bool => {
            asm.test_rr(Reg::Rcx, Reg::Rcx);
            let id = next_id(uniq);
            asm.jcc_label(Cc::Z, format!(".boolz{id}"));
            let at = asm.lea_rip(Reg::Rax);
            strs.push((at, intern_cstring(strings, "true")));
            asm.jmp_label(format!(".boold{id}"));
            asm.label(format!(".boolz{id}"));
            let at = asm.lea_rip(Reg::Rax);
            strs.push((at, intern_cstring(strings, "false")));
            asm.label(format!(".boold{id}"));
        }
        IrType::Nil => {
            let at = asm.lea_rip(Reg::Rax);
            strs.push((at, intern_cstring(strings, "nil")));
        }
        IrType::List(element) => {
            asm.mov_ri(Reg::Rdx, map_value_mode(&element));
            asm.call_label("rt_display_list");
        }
        IrType::Map(key, value) => {
            asm.mov_ri(Reg::Rdx, map_key_mode(&key));
            asm.mov_ri(Reg::R8, map_value_mode(&value));
            asm.call_label("rt_display_map");
        }
        IrType::Range => asm.call_label("rt_display_range"),
        IrType::Struct(_) => {
            let at = asm.lea_rip(Reg::Rax);
            strs.push((at, intern_cstring(strings, "<struct>")));
        }
        _ => asm.mov_rr(Reg::Rax, Reg::Rcx),
    }
}

fn field_offset(
    module: &Module,
    func: &Function,
    obj: LocalId,
    field: &str,
) -> Result<i32, CodegenError> {
    let ty = func.local(obj).map(|l| &l.ty);
    if let Some(IrType::Struct(name)) = ty {
        if let Some(st) = module
            .structs
            .iter()
            .find(|s| s.name == *name || s.name.ends_with(&format!(".{name}")))
        {
            if let Some(i) = st.fields.iter().position(|(n, _)| n == field) {
                return Ok(8 * i as i32);
            }
        }
    }
    for st in &module.structs {
        if let Some(i) = st.fields.iter().position(|(n, _)| n == field) {
            return Ok(8 * i as i32);
        }
    }
    Err(CodegenError::new(format!("unknown field `{field}`")))
}

fn emit_make_iter(
    func: &Function,
    frame: &Frame,
    dest: &LocalId,
    src: &LocalId,
    asm: &mut Asm,
    uniq: &mut u32,
) {
    let ty = func
        .local(*src)
        .map(|l| l.ty.clone())
        .unwrap_or(IrType::Dyn);
    match ty {
        IrType::Range => {
            asm.mov_ri(Reg::Rcx, 32);
            asm.call_label("rt_alloc");
            frame.load(asm, *src, Reg::R11);
            asm.mov_rm(Reg::R8, Reg::R11, 0);
            asm.mov_rm(Reg::R9, Reg::R11, 8);
            asm.mov_ri(Reg::R10, 1);
            let id = next_id(uniq);
            asm.cmp_rr(Reg::R9, Reg::R8);
            asm.jcc_label(Cc::Ge, format!(".rpos{id}"));
            asm.mov_ri(Reg::R10, -1);
            asm.label(format!(".rpos{id}"));
            asm.mov_ri(Reg::R11, 1); // kind = range
            asm.mov_mr(Reg::Rax, 0, Reg::R11);
            asm.mov_mr(Reg::Rax, 8, Reg::R8);
            asm.mov_mr(Reg::Rax, 16, Reg::R9);
            asm.mov_mr(Reg::Rax, 24, Reg::R10);
            frame.store(asm, *dest, Reg::Rax);
        }
        _ => {
            asm.mov_ri(Reg::Rcx, 24);
            asm.call_label("rt_alloc");
            asm.mov_ri(Reg::R11, 2); // kind = list
            asm.mov_mr(Reg::Rax, 0, Reg::R11);
            frame.load(asm, *src, Reg::R10);
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
            asm.xor_rr(Reg::R10, Reg::R10);
            asm.mov_mr(Reg::Rax, 16, Reg::R10);
            frame.store(asm, *dest, Reg::Rax);
        }
    }
}

fn emit_iter_next(
    frame: &Frame,
    value: &LocalId,
    more: &LocalId,
    iter: &LocalId,
    asm: &mut Asm,
    uniq: &mut u32,
) {
    let id = next_id(uniq);
    frame.load(asm, *iter, Reg::Rax);
    asm.mov_rm(Reg::R10, Reg::Rax, 0); // kind
    asm.mov_ri(Reg::R11, 1);
    asm.cmp_rr(Reg::R10, Reg::R11);
    asm.jcc_label(Cc::E, format!(".nrange{id}"));

    // list: [kind, list, idx]
    asm.mov_rm(Reg::R8, Reg::Rax, 8); // list
    asm.mov_rm(Reg::R9, Reg::Rax, 16); // idx
    asm.mov_rm(Reg::R10, Reg::R8, 0); // len
    asm.cmp_rr(Reg::R9, Reg::R10);
    asm.jcc_label(Cc::Ge, format!(".ndone{id}"));
    asm.mov_rm(Reg::R8, Reg::R8, 16); // data
    asm.mov_rr(Reg::R11, Reg::R9);
    asm.shl_ri(Reg::R11, 3);
    asm.add_rr(Reg::R8, Reg::R11);
    asm.mov_rm(Reg::R8, Reg::R8, 0);
    frame.store(asm, *value, Reg::R8);
    asm.add_ri(Reg::R9, 1);
    asm.mov_mr(Reg::Rax, 16, Reg::R9);
    asm.mov_ri(Reg::R8, 1);
    frame.store(asm, *more, Reg::R8);
    asm.jmp_label(format!(".nend{id}"));

    asm.label(format!(".nrange{id}"));
    asm.mov_rm(Reg::R8, Reg::Rax, 8); // next
    asm.mov_rm(Reg::R9, Reg::Rax, 16); // end
    asm.mov_rm(Reg::R10, Reg::Rax, 24); // step
    asm.test_rr(Reg::R10, Reg::R10);
    asm.jcc_label(Cc::L, format!(".nrev{id}"));
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, format!(".ndone{id}"));
    asm.jmp_label(format!(".nyield{id}"));
    asm.label(format!(".nrev{id}"));
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Le, format!(".ndone{id}"));
    asm.label(format!(".nyield{id}"));
    frame.store(asm, *value, Reg::R8);
    asm.add_rr(Reg::R8, Reg::R10);
    asm.mov_mr(Reg::Rax, 8, Reg::R8);
    asm.mov_ri(Reg::R8, 1);
    frame.store(asm, *more, Reg::R8);
    asm.jmp_label(format!(".nend{id}"));

    asm.label(format!(".ndone{id}"));
    asm.xor_rr(Reg::R8, Reg::R8);
    frame.store(asm, *more, Reg::R8);
    asm.label(format!(".nend{id}"));
}

#[allow(clippy::too_many_arguments)]
fn emit_call(
    func: &Function,
    frame: &Frame,
    dest: Option<&LocalId>,
    callee: &Callee,
    args: &[LocalId],
    asm: &mut Asm,
    strings: &mut Vec<Vec<u8>>,
    strs: &mut Vec<(usize, usize)>,
    uniq: &mut u32,
) -> Result<(), CodegenError> {
    match callee {
        Callee::Static(name) if name == "print" => {
            emit_native_print(func, frame, dest, args, asm, strings, strs, uniq)
        }
        Callee::Static(name) if name == "len" => emit_native_len(func, frame, dest, args, asm),
        Callee::Static(name) if name == "push" => emit_native_push(frame, dest, args, asm),
        Callee::Static(name) if name == "pop" => emit_native_pop(frame, dest, args, asm),
        Callee::Static(name) if name == "join" => {
            emit_binary_rt(frame, "rt_join", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "split" => {
            emit_binary_rt(frame, "rt_split", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "abs" => {
            emit_native_abs(func, frame, dest, args, asm, strings, strs, uniq)
        }
        Callee::Static(name) if name == "min" => {
            emit_native_minmax(func, frame, dest, args, asm, strings, strs, uniq, true)
        }
        Callee::Static(name) if name == "max" => {
            emit_native_minmax(func, frame, dest, args, asm, strings, strs, uniq, false)
        }
        Callee::Static(name) if name == "range" => emit_native_range(frame, dest, args, asm),
        Callee::Static(name) if name == "str" => {
            if args.is_empty() {
                return Err(CodegenError::new("str() expected 1 argument"));
            }
            load_as_cstr(func, frame, args[0], asm, strings, strs, uniq);
            store_rax(frame, dest, asm);
            Ok(())
        }
        Callee::Static(name) if name == "int" => {
            emit_native_int(func, frame, dest, args, asm, uniq)
        }
        Callee::Static(name) if name == "type_of" => {
            emit_native_type_of(func, frame, dest, args, asm, strings, strs)
        }
        Callee::Static(name) if name == "assert" => {
            emit_native_assert(frame, dest, args, asm, strings, strs)
        }
        Callee::Static(name) if name == "read_file" => {
            if args.is_empty() {
                return Err(CodegenError::new("read_file() expected 1 argument"));
            }
            frame.load(asm, args[0], Reg::Rcx);
            asm.call_label("rt_read_file");
            store_rax(frame, dest, asm);
            Ok(())
        }
        Callee::Static(name) if name == "write_file" => {
            if args.len() < 2 {
                return Err(CodegenError::new("write_file() expected 2 arguments"));
            }
            let spill = frame.spill;
            load_as_cstr(func, frame, args[1], asm, strings, strs, uniq);
            asm.mov_mr_rbp(spill, Reg::Rax);
            load_as_cstr(func, frame, args[0], asm, strings, strs, uniq);
            asm.mov_rr(Reg::Rcx, Reg::Rax);
            asm.mov_rm_rbp(Reg::Rdx, spill);
            asm.call_label("rt_write_file");
            if let Some(d) = dest {
                asm.xor_rr(Reg::Rax, Reg::Rax);
                frame.store(asm, *d, Reg::Rax);
            }
            Ok(())
        }
        Callee::Static(name) if name == "starts_with" => {
            emit_binary_rt(frame, "rt_starts_with", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "ends_with" => {
            emit_binary_rt(frame, "rt_ends_with", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "contains" => {
            emit_native_contains(func, frame, dest, args, asm)
        }
        Callee::Static(name) if name == "first" => {
            emit_native_first_last(func, frame, dest, args, asm, true)
        }
        Callee::Static(name) if name == "last" => {
            emit_native_first_last(func, frame, dest, args, asm, false)
        }
        Callee::Static(name) if name == "float" => emit_native_float(func, frame, dest, args, asm),
        Callee::Static(name) if name == "trim" => {
            emit_unary_rt(frame, "rt_trim", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "upper" => {
            emit_unary_rt(frame, "rt_upper", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "lower" => {
            emit_unary_rt(frame, "rt_lower", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "file_exists" => {
            emit_unary_rt(frame, "rt_file_exists", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "env" => {
            emit_unary_rt(frame, "rt_env", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "cwd" => {
            asm.call_label("rt_cwd");
            store_rax(frame, dest, asm);
            Ok(())
        }
        Callee::Static(name) if name == "remove_file" => {
            emit_unary_rt(frame, "rt_remove_file", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "is_dir" => {
            emit_unary_rt(frame, "rt_is_dir", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "is_file" => {
            emit_unary_rt(frame, "rt_is_file", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "list_dir" => {
            emit_unary_rt(frame, "rt_list_dir", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "args" => {
            asm.call_label("rt_args");
            store_rax(frame, dest, asm);
            Ok(())
        }
        Callee::Static(name) if name == "append_file" => {
            if args.len() < 2 {
                return Err(CodegenError::new("append_file() expected 2 arguments"));
            }
            let spill = frame.spill;
            load_as_cstr(func, frame, args[1], asm, strings, strs, uniq);
            asm.mov_mr_rbp(spill, Reg::Rax);
            load_as_cstr(func, frame, args[0], asm, strings, strs, uniq);
            asm.mov_rr(Reg::Rcx, Reg::Rax);
            asm.mov_rm_rbp(Reg::Rdx, spill);
            asm.call_label("rt_append_file");
            if let Some(d) = dest {
                asm.xor_rr(Reg::Rax, Reg::Rax);
                frame.store(asm, *d, Reg::Rax);
            }
            Ok(())
        }
        Callee::Static(name) if name == "create_dir" => {
            emit_unary_rt(frame, "rt_create_dir", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "run_cmd" => {
            emit_unary_rt(frame, "rt_run_cmd", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "keys" => {
            emit_unary_rt(frame, "rt_map_keys", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "values" => {
            emit_unary_rt(frame, "rt_map_values", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "entries" => {
            emit_unary_rt(frame, "rt_map_entries", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "is_empty" => {
            emit_unary_rt(frame, "rt_is_empty", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "has_key" => {
            emit_native_contains(func, frame, dest, args, asm)
        }
        Callee::Static(name) if name == "cancel" => {
            let id = next_id(uniq);
            let done = format!(".cancel_done_{id}");
            frame.load(asm, args[0], Reg::Rax);
            asm.mov_rm(Reg::R10, Reg::Rax, 0);
            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::NZ, &done);
            asm.mov_ri(Reg::R10, 2);
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            asm.label(&done);
            asm.mov_ri(Reg::Rax, 0);
            store_rax(frame, dest, asm);
            Ok(())
        }
        Callee::Static(name) if name == "is_cancelled" => {
            frame.load(asm, args[0], Reg::Rax);
            asm.mov_rm(Reg::R10, Reg::Rax, 0);
            asm.mov_ri(Reg::R11, 2);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.setcc(Cc::E, Reg::Rax);
            asm.movzx_rax_al();
            store_rax(frame, dest, asm);
            Ok(())
        }
        Callee::Static(name) if name == "is_completed" => {
            let id = next_id(uniq);
            let done = format!(".is_completed_done_{id}");
            frame.load(asm, args[0], Reg::Rax);
            asm.mov_rm(Reg::R10, Reg::Rax, 0);
            asm.mov_ri(Reg::Rax, 1);
            asm.mov_ri(Reg::R11, 1);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::E, &done);
            asm.mov_ri(Reg::R11, 4);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::E, &done);
            asm.xor_rr(Reg::Rax, Reg::Rax);
            asm.label(&done);
            store_rax(frame, dest, asm);
            Ok(())
        }
        Callee::Static(name) if name == "task_status" => {
            let id = next_id(uniq);
            let s_pending = intern_cstring(strings, "pending");
            let s_joined = intern_cstring(strings, "joined");
            let s_cancelled = intern_cstring(strings, "cancelled");
            let s_running = intern_cstring(strings, "running");
            let s_completed = intern_cstring(strings, "completed");

            let l_joined = format!(".ts_joined_{id}");
            let l_cancelled = format!(".ts_cancelled_{id}");
            let l_running = format!(".ts_running_{id}");
            let l_completed = format!(".ts_completed_{id}");
            let l_done = format!(".ts_done_{id}");

            frame.load(asm, args[0], Reg::Rax);
            asm.mov_rm(Reg::R10, Reg::Rax, 0);

            asm.mov_ri(Reg::R11, 1);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::E, &l_joined);

            asm.mov_ri(Reg::R11, 2);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::E, &l_cancelled);

            asm.mov_ri(Reg::R11, 3);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::E, &l_running);

            asm.mov_ri(Reg::R11, 4);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::E, &l_completed);

            let at = asm.lea_rip(Reg::Rax);
            strs.push((at, s_pending));
            asm.jmp_label(&l_done);

            asm.label(&l_joined);
            let at = asm.lea_rip(Reg::Rax);
            strs.push((at, s_joined));
            asm.jmp_label(&l_done);

            asm.label(&l_cancelled);
            let at = asm.lea_rip(Reg::Rax);
            strs.push((at, s_cancelled));
            asm.jmp_label(&l_done);

            asm.label(&l_running);
            let at = asm.lea_rip(Reg::Rax);
            strs.push((at, s_running));
            asm.jmp_label(&l_done);

            asm.label(&l_completed);
            let at = asm.lea_rip(Reg::Rax);
            strs.push((at, s_completed));

            asm.label(&l_done);
            store_rax(frame, dest, asm);
            Ok(())
        }
        Callee::Static(name) => emit_user_call(frame, name, dest, args, asm),
        Callee::Local(id) => {
            let space = prepare_call_args(frame, args, asm);
            frame.load(asm, *id, Reg::Rax);
            asm.call_r(Reg::Rax);
            finish_call_args(space, asm);
            store_rax(frame, dest, asm);
            Ok(())
        }
    }
}

fn store_rax(frame: &Frame, dest: Option<&LocalId>, asm: &mut Asm) {
    if let Some(d) = dest {
        frame.store(asm, *d, Reg::Rax);
    }
}

fn emit_unary_rt(
    frame: &Frame,
    label: &str,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) {
    frame.load(asm, args[0], Reg::Rcx);
    asm.call_label(label);
    store_rax(frame, dest, asm);
}

fn emit_binary_rt(
    frame: &Frame,
    label: &str,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) {
    frame.load(asm, args[0], Reg::Rcx);
    frame.load(asm, args[1], Reg::Rdx);
    asm.call_label(label);
    store_rax(frame, dest, asm);
}

#[allow(clippy::too_many_arguments)]
fn emit_native_print(
    func: &Function,
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
    strings: &mut Vec<Vec<u8>>,
    strs: &mut Vec<(usize, usize)>,
    uniq: &mut u32,
) -> Result<(), CodegenError> {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            let at = asm.lea_rip(Reg::Rcx);
            strs.push((at, intern_cstring(strings, " ")));
            asm.call_label("rt_print_cstr");
        }
        let ty = local_ty(func, *arg);
        frame.load(asm, *arg, Reg::Rcx);
        match ty {
            IrType::String => asm.call_label("rt_print_cstr"),
            IrType::Int => asm.call_label("rt_print_i64"),
            IrType::Float => {
                asm.call_label("rt_ftoa");
                asm.mov_rr(Reg::Rcx, Reg::Rax);
                asm.call_label("rt_print_cstr");
            }
            IrType::Dyn | IrType::Unknown => {
                let id = next_id(uniq);
                asm.mov_ri(Reg::R10, 0x10000);
                asm.cmp_rr(Reg::Rcx, Reg::R10);
                asm.jcc_label(Cc::L, format!(".pri{id}"));
                asm.call_label("rt_print_cstr");
                asm.jmp_label(format!(".prd{id}"));
                asm.label(format!(".pri{id}"));
                asm.call_label("rt_print_i64");
                asm.label(format!(".prd{id}"));
            }
            IrType::Bool => {
                asm.test_rr(Reg::Rcx, Reg::Rcx);
                asm.jcc_label(Cc::Z, format!("{}_bfalse_{}", func.name, arg.0));
                let at = asm.lea_rip(Reg::Rcx);
                strs.push((at, intern_cstring(strings, "true")));
                asm.call_label("rt_print_cstr");
                asm.jmp_label(format!("{}_bdone_{}", func.name, arg.0));
                asm.label(format!("{}_bfalse_{}", func.name, arg.0));
                let at = asm.lea_rip(Reg::Rcx);
                strs.push((at, intern_cstring(strings, "false")));
                asm.call_label("rt_print_cstr");
                asm.label(format!("{}_bdone_{}", func.name, arg.0));
            }
            IrType::Nil => {
                let at = asm.lea_rip(Reg::Rcx);
                strs.push((at, intern_cstring(strings, "nil")));
                asm.call_label("rt_print_cstr");
            }
            IrType::List(element) => {
                asm.mov_ri(Reg::Rdx, map_value_mode(&element));
                asm.call_label("rt_display_list");
                asm.mov_rr(Reg::Rcx, Reg::Rax);
                asm.call_label("rt_print_cstr");
            }
            IrType::Map(key, value) => {
                asm.mov_ri(Reg::Rdx, map_key_mode(&key));
                asm.mov_ri(Reg::R8, map_value_mode(&value));
                asm.call_label("rt_display_map");
                asm.mov_rr(Reg::Rcx, Reg::Rax);
                asm.call_label("rt_print_cstr");
            }
            IrType::Range => {
                asm.call_label("rt_display_range");
                asm.mov_rr(Reg::Rcx, Reg::Rax);
                asm.call_label("rt_print_cstr");
            }
            IrType::Struct(_) => {
                let at = asm.lea_rip(Reg::Rcx);
                strs.push((at, intern_cstring(strings, "<struct>")));
                asm.call_label("rt_print_cstr");
            }
            IrType::Task(_) => {
                let at = asm.lea_rip(Reg::Rcx);
                strs.push((at, intern_cstring(strings, "<task>")));
                asm.call_label("rt_print_cstr");
            }
            _ => asm.call_label("rt_print_i64"),
        }
    }
    asm.call_label("rt_print_nl");
    if let Some(d) = dest {
        asm.xor_rr(Reg::Rax, Reg::Rax);
        frame.store(asm, *d, Reg::Rax);
    }
    Ok(())
}

fn emit_native_len(
    func: &Function,
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
    if args.is_empty() {
        return Err(CodegenError::new("len() expected 1 argument"));
    }
    frame.load(asm, args[0], Reg::Rcx);
    match local_ty(func, args[0]) {
        IrType::String => asm.call_label("rt_strlen"),
        _ => asm.mov_rm(Reg::Rax, Reg::Rcx, 0),
    }
    store_rax(frame, dest, asm);
    Ok(())
}

fn emit_native_push(
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
    if args.len() < 2 {
        return Err(CodegenError::new("push() expected 2 arguments"));
    }
    frame.load(asm, args[0], Reg::Rcx);
    frame.load(asm, args[1], Reg::Rdx);
    asm.call_label("rt_list_push");
    frame.store(asm, args[0], Reg::Rax);
    if let Some(d) = dest {
        asm.xor_rr(Reg::Rax, Reg::Rax);
        frame.store(asm, *d, Reg::Rax);
    }
    Ok(())
}

fn emit_native_pop(
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
    if args.is_empty() {
        return Err(CodegenError::new("pop() expected 1 argument"));
    }
    frame.load(asm, args[0], Reg::Rcx);
    asm.call_label("rt_list_pop");
    store_rax(frame, dest, asm);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_native_abs(
    func: &Function,
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
    strings: &mut Vec<Vec<u8>>,
    strs: &mut Vec<(usize, usize)>,
    uniq: &mut u32,
) -> Result<(), CodegenError> {
    if args.len() != 1 {
        return Err(CodegenError::new("abs() expected 1 argument"));
    }
    frame.load(asm, args[0], Reg::Rax);
    if matches!(local_ty(func, args[0]), IrType::Float) {
        // IEEE-754 absolute value is the input with its sign bit cleared.
        asm.mov_ri(Reg::R10, i64::MAX);
        asm.and_rr(Reg::Rax, Reg::R10);
    } else {
        let id = next_id(uniq);
        let overflow = format!(".abs_overflow{id}");
        let done = format!(".abs_done{id}");
        asm.test_rr(Reg::Rax, Reg::Rax);
        asm.jcc_label(Cc::Ge, &done);
        asm.bytes.extend_from_slice(&[0x48, 0xF7, 0xD8]);
        asm.jcc_label(Cc::O, &overflow);
        asm.jmp_label(&done);
        asm.label(overflow);
        emit_runtime_failure(asm, strings, strs, "integer overflow");
        asm.label(done);
    }
    store_rax(frame, dest, asm);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_native_minmax(
    func: &Function,
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
    strings: &mut Vec<Vec<u8>>,
    strs: &mut Vec<(usize, usize)>,
    uniq: &mut u32,
    is_min: bool,
) -> Result<(), CodegenError> {
    if args.len() < 2 {
        return Err(CodegenError::new("min/max expected at least 2 arguments"));
    }
    let floaty = args
        .iter()
        .all(|arg| matches!(local_ty(func, *arg), IrType::Float));
    frame.load(asm, args[0], Reg::Rax);
    for a in &args[1..] {
        let id = next_id(uniq);
        frame.load(asm, *a, Reg::R10);
        let keep = format!(".mm{id}");
        if floaty {
            asm.movq_xmm_r(0, Reg::Rax);
            asm.movq_xmm_r(1, Reg::R10);
            asm.ucomisd_xmm0_xmm1();
            let unordered = format!(".mm_nan{id}");
            asm.jcc_label(Cc::P, &unordered);
            asm.jcc_label(if is_min { Cc::Be } else { Cc::Ae }, &keep);
            asm.mov_rr(Reg::Rax, Reg::R10);
            asm.jmp_label(&keep);
            asm.label(unordered);
            emit_runtime_failure(asm, strings, strs, "cannot compare NaN");
        } else {
            asm.cmp_rr(Reg::Rax, Reg::R10);
            asm.jcc_label(if is_min { Cc::Le } else { Cc::Ge }, &keep);
            asm.mov_rr(Reg::Rax, Reg::R10);
        }
        asm.label(keep);
    }
    store_rax(frame, dest, asm);
    Ok(())
}

fn emit_native_range(
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
    asm.mov_ri(Reg::Rcx, 16);
    asm.call_label("rt_alloc");
    match args.len() {
        1 => {
            asm.xor_rr(Reg::R10, Reg::R10);
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            frame.load(asm, args[0], Reg::R10);
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
        }
        2 => {
            frame.load(asm, args[0], Reg::R10);
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            frame.load(asm, args[1], Reg::R10);
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
        }
        _ => return Err(CodegenError::new("range() expected 1 or 2 arguments")),
    }
    store_rax(frame, dest, asm);
    Ok(())
}

fn emit_native_int(
    func: &Function,
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
    uniq: &mut u32,
) -> Result<(), CodegenError> {
    if args.is_empty() {
        return Err(CodegenError::new("int() expected 1 argument"));
    }
    frame.load(asm, args[0], Reg::Rcx);
    match local_ty(func, args[0]) {
        IrType::String => asm.call_label("rt_atoi"),
        IrType::Float => {
            asm.movq_xmm_r(0, Reg::Rcx);
            asm.cvttsd2si_r_xmm0(Reg::Rax);
        }
        IrType::Int | IrType::Bool | IrType::Nil => asm.mov_rr(Reg::Rax, Reg::Rcx),
        _ => {
            let id = next_id(uniq);
            asm.mov_ri(Reg::R10, 0x10000);
            asm.cmp_rr(Reg::Rcx, Reg::R10);
            asm.jcc_label(Cc::L, format!(".inti{id}"));
            asm.call_label("rt_atoi");
            asm.jmp_label(format!(".intd{id}"));
            asm.label(format!(".inti{id}"));
            asm.mov_rr(Reg::Rax, Reg::Rcx);
            asm.label(format!(".intd{id}"));
        }
    }
    store_rax(frame, dest, asm);
    Ok(())
}

fn emit_native_type_of(
    func: &Function,
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
    strings: &mut Vec<Vec<u8>>,
    strs: &mut Vec<(usize, usize)>,
) -> Result<(), CodegenError> {
    if args.is_empty() {
        return Err(CodegenError::new("type_of() expected 1 argument"));
    }
    let name = ir_type_name(&local_ty(func, args[0]));
    let at = asm.lea_rip(Reg::Rax);
    strs.push((at, intern_cstring(strings, name)));
    store_rax(frame, dest, asm);
    Ok(())
}

fn emit_native_assert(
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
    _strings: &mut Vec<Vec<u8>>,
    _strs: &mut Vec<(usize, usize)>,
) -> Result<(), CodegenError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CodegenError::new("assert() expected 1 or 2 arguments"));
    }
    frame.load(asm, args[0], Reg::Rcx);
    if args.len() >= 2 {
        frame.load(asm, args[1], Reg::Rdx);
    } else {
        asm.xor_rr(Reg::Rdx, Reg::Rdx);
    }
    asm.call_label("rt_assert");
    if let Some(d) = dest {
        asm.xor_rr(Reg::Rax, Reg::Rax);
        frame.store(asm, *d, Reg::Rax);
    }
    Ok(())
}

fn emit_native_contains(
    func: &Function,
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
    if args.len() < 2 {
        return Err(CodegenError::new("contains() expected 2 arguments"));
    }
    match local_ty(func, args[0]) {
        IrType::List(_) => {
            frame.load(asm, args[0], Reg::Rcx);
            frame.load(asm, args[1], Reg::Rdx);
            asm.call_label("rt_list_contains");
        }
        IrType::Map(key, _) => {
            frame.load(asm, args[0], Reg::Rcx);
            frame.load(asm, args[1], Reg::Rdx);
            asm.mov_ri(Reg::R8, i64::from(map_key_mode(&key) == 1));
            asm.call_label("rt_map_has");
        }
        IrType::Range => {
            frame.load(asm, args[0], Reg::Rcx);
            frame.load(asm, args[1], Reg::Rdx);
            asm.call_label("rt_range_contains");
        }
        _ => {
            frame.load(asm, args[0], Reg::Rcx);
            frame.load(asm, args[1], Reg::Rdx);
            asm.call_label("rt_contains");
        }
    }
    store_rax(frame, dest, asm);
    Ok(())
}

fn emit_native_first_last(
    func: &Function,
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
    first: bool,
) -> Result<(), CodegenError> {
    if args.is_empty() {
        return Err(CodegenError::new("first/last expected 1 argument"));
    }
    match local_ty(func, args[0]) {
        IrType::String => {
            frame.load(asm, args[0], Reg::Rcx);
            if first {
                asm.xor_rr(Reg::Rdx, Reg::Rdx);
            } else {
                asm.mov_ri(Reg::Rdx, -1);
            }
            asm.call_label("rt_str_index");
        }
        _ => {
            frame.load(asm, args[0], Reg::Rcx);
            if first {
                asm.call_label("rt_list_first");
            } else {
                asm.call_label("rt_list_last");
            }
        }
    }
    store_rax(frame, dest, asm);
    Ok(())
}

fn emit_native_float(
    func: &Function,
    frame: &Frame,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
    if args.is_empty() {
        return Err(CodegenError::new("float() expected 1 argument"));
    }
    frame.load(asm, args[0], Reg::Rax);
    match local_ty(func, args[0]) {
        IrType::Float => {}
        _ => {
            asm.cvtsi2sd_xmm0(Reg::Rax);
            asm.movq_r_xmm(Reg::Rax, 0);
        }
    }
    store_rax(frame, dest, asm);
    Ok(())
}

fn emit_user_call(
    frame: &Frame,
    name: &str,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
    let space = prepare_call_args(frame, args, asm);
    asm.call_label(name);
    finish_call_args(space, asm);
    store_rax(frame, dest, asm);
    Ok(())
}

fn prepare_call_args(frame: &Frame, args: &[LocalId], asm: &mut Asm) -> i32 {
    let win_args = [Reg::Rcx, Reg::Rdx, Reg::R8, Reg::R9];
    let extra = args.len().saturating_sub(4);
    let mut space = 0i32;
    if extra > 0 {
        space = 32 + 8 * extra as i32;
        if space % 16 != 0 {
            space += 8;
        }
        asm.sub_ri(Reg::Rsp, space);
        for (i, a) in args.iter().enumerate().skip(4) {
            frame.load(asm, *a, Reg::Rax);
            asm.mov_mr_rsp(32 + 8 * (i as i32 - 4), Reg::Rax);
        }
    }
    for (i, a) in args.iter().enumerate().take(4) {
        frame.load(asm, *a, win_args[i]);
    }
    space
}

fn finish_call_args(space: i32, asm: &mut Asm) {
    if space > 0 {
        asm.add_ri(Reg::Rsp, space);
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_get_index(
    func: &Function,
    frame: &Frame,
    dest: &LocalId,
    obj: &LocalId,
    index: &LocalId,
    asm: &mut Asm,
    strings: &mut Vec<Vec<u8>>,
    strs: &mut Vec<(usize, usize)>,
    uniq: &mut u32,
) {
    let obj_ty = local_ty(func, *obj);
    let idx_ty = local_ty(func, *index);
    match obj_ty {
        IrType::Map(key, _) => {
            frame.load(asm, *obj, Reg::Rcx);
            frame.load(asm, *index, Reg::Rdx);
            asm.mov_ri(Reg::R8, i64::from(map_key_mode(&key) == 1));
            asm.call_label("rt_map_get");
            frame.store(asm, *dest, Reg::Rax);
        }
        IrType::String => {
            frame.load(asm, *obj, Reg::Rcx);
            frame.load(asm, *index, Reg::Rdx);
            asm.call_label("rt_str_index");
            frame.store(asm, *dest, Reg::Rax);
        }
        IrType::List(_) => {
            let id = next_id(uniq);
            let idxpos = format!(".lidxpos{id}");
            let oob = format!(".lidx_oob{id}");
            let ok = format!(".lidx_ok{id}");

            frame.load(asm, *obj, Reg::Rax);
            frame.load(asm, *index, Reg::R10);
            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::Ge, &idxpos);
            asm.mov_rm(Reg::R11, Reg::Rax, 0);
            asm.add_rr(Reg::R10, Reg::R11);
            asm.label(&idxpos);

            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::L, &oob);
            asm.mov_rm(Reg::R11, Reg::Rax, 0);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::Ge, &oob);
            asm.jmp_label(&ok);

            asm.label(&oob);
            emit_runtime_failure(asm, strings, strs, "index out of bounds");

            asm.label(&ok);
            asm.shl_ri(Reg::R10, 3);
            asm.mov_rm(Reg::Rax, Reg::Rax, 16);
            asm.add_rr(Reg::Rax, Reg::R10);
            asm.mov_rm(Reg::Rax, Reg::Rax, 0);
            frame.store(asm, *dest, Reg::Rax);
        }
        IrType::Dyn | IrType::Unknown if is_string_ty(&idx_ty) => {
            frame.load(asm, *obj, Reg::Rcx);
            frame.load(asm, *index, Reg::Rdx);
            asm.mov_ri(Reg::R8, 1);
            asm.call_label("rt_map_get");
            frame.store(asm, *dest, Reg::Rax);
        }
        _ => {
            let id = next_id(uniq);
            let is_map = format!(".dget_map{id}");
            let is_list = format!(".dget_list{id}");
            let done = format!(".dget_done{id}");
            frame.load(asm, *index, Reg::R10);
            asm.mov_ri(Reg::R11, 0x10000);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::Ge, &is_map);
            asm.jmp_label(&is_list);

            asm.label(&is_map);
            frame.load(asm, *obj, Reg::Rcx);
            frame.load(asm, *index, Reg::Rdx);
            asm.mov_ri(Reg::R8, 1);
            asm.call_label("rt_map_get");
            frame.store(asm, *dest, Reg::Rax);
            asm.jmp_label(&done);

            asm.label(&is_list);
            frame.load(asm, *obj, Reg::Rax);
            frame.load(asm, *index, Reg::R10);
            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::Ge, format!(".idxpos{id}"));
            asm.mov_rm(Reg::R11, Reg::Rax, 0);
            asm.add_rr(Reg::R10, Reg::R11);
            asm.label(format!(".idxpos{id}"));
            let oob = format!(".idx_oob{id}");
            let ok = format!(".idx_ok{id}");
            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::L, &oob);
            asm.mov_rm(Reg::R11, Reg::Rax, 0);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::Ge, &oob);
            asm.jmp_label(&ok);
            asm.label(oob);
            emit_runtime_failure(asm, strings, strs, "index out of bounds");
            asm.label(ok);
            asm.shl_ri(Reg::R10, 3);
            asm.mov_rm(Reg::Rax, Reg::Rax, 16);
            asm.add_rr(Reg::Rax, Reg::R10);
            asm.mov_rm(Reg::Rax, Reg::Rax, 0);
            frame.store(asm, *dest, Reg::Rax);
            asm.label(done);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_set_index(
    func: &Function,
    frame: &Frame,
    obj: &LocalId,
    index: &LocalId,
    value: &LocalId,
    asm: &mut Asm,
    strings: &mut Vec<Vec<u8>>,
    strs: &mut Vec<(usize, usize)>,
    uniq: &mut u32,
) {
    let obj_ty = local_ty(func, *obj);
    let idx_ty = local_ty(func, *index);
    match obj_ty {
        IrType::Map(key, _) => {
            frame.load(asm, *obj, Reg::Rcx);
            frame.load(asm, *index, Reg::Rdx);
            frame.load(asm, *value, Reg::R8);
            asm.mov_ri(Reg::R9, i64::from(map_key_mode(&key) == 1));
            asm.call_label("rt_map_set");
            frame.store(asm, *obj, Reg::Rax);
        }
        IrType::Dyn | IrType::Unknown if is_string_ty(&idx_ty) => {
            frame.load(asm, *obj, Reg::Rcx);
            frame.load(asm, *index, Reg::Rdx);
            frame.load(asm, *value, Reg::R8);
            asm.mov_ri(Reg::R9, 1);
            asm.call_label("rt_map_set");
            frame.store(asm, *obj, Reg::Rax);
        }
        IrType::List(_) => {
            let id = next_id(uniq);
            let idxpos = format!(".lsidxpos{id}");
            let oob = format!(".lsidx_oob{id}");
            let ok = format!(".lsidx_ok{id}");

            frame.load(asm, *obj, Reg::Rax);
            frame.load(asm, *index, Reg::R10);
            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::Ge, &idxpos);
            asm.mov_rm(Reg::R11, Reg::Rax, 0);
            asm.add_rr(Reg::R10, Reg::R11);
            asm.label(&idxpos);

            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::L, &oob);
            asm.mov_rm(Reg::R11, Reg::Rax, 0);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::Ge, &oob);
            asm.jmp_label(&ok);

            asm.label(&oob);
            emit_runtime_failure(asm, strings, strs, "index out of bounds");

            asm.label(&ok);
            asm.shl_ri(Reg::R10, 3);
            asm.mov_rm(Reg::Rax, Reg::Rax, 16);
            asm.add_rr(Reg::Rax, Reg::R10);
            frame.load(asm, *value, Reg::R11);
            asm.mov_mr(Reg::Rax, 0, Reg::R11);
        }
        _ => {
            let id = next_id(uniq);
            let is_map = format!(".dset_map{id}");
            let is_list = format!(".dset_list{id}");
            let done = format!(".dset_done{id}");
            frame.load(asm, *index, Reg::R10);
            asm.mov_ri(Reg::R11, 0x10000);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::Ge, &is_map);
            asm.jmp_label(&is_list);

            asm.label(&is_map);
            frame.load(asm, *obj, Reg::Rcx);
            frame.load(asm, *index, Reg::Rdx);
            frame.load(asm, *value, Reg::R8);
            asm.mov_ri(Reg::R9, 1);
            asm.call_label("rt_map_set");
            frame.store(asm, *obj, Reg::Rax);
            asm.jmp_label(&done);

            asm.label(&is_list);
            frame.load(asm, *obj, Reg::Rax);
            frame.load(asm, *index, Reg::R10);
            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::Ge, format!(".sidxpos{id}"));
            asm.mov_rm(Reg::R11, Reg::Rax, 0);
            asm.add_rr(Reg::R10, Reg::R11);
            asm.label(format!(".sidxpos{id}"));
            let oob = format!(".sidx_oob{id}");
            let ok = format!(".sidx_ok{id}");
            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::L, &oob);
            asm.mov_rm(Reg::R11, Reg::Rax, 0);
            asm.cmp_rr(Reg::R10, Reg::R11);
            asm.jcc_label(Cc::Ge, &oob);
            asm.jmp_label(&ok);
            asm.label(oob);
            emit_runtime_failure(asm, strings, strs, "index out of bounds");
            asm.label(ok);
            asm.shl_ri(Reg::R10, 3);
            asm.mov_rm(Reg::Rax, Reg::Rax, 16);
            asm.add_rr(Reg::Rax, Reg::R10);
            frame.load(asm, *value, Reg::R10);
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            asm.label(done);
        }
    }
}

fn local_ty(func: &Function, id: LocalId) -> IrType {
    func.local(id).map(|l| l.ty.clone()).unwrap_or(IrType::Dyn)
}

fn is_string_ty(ty: &IrType) -> bool {
    matches!(ty, IrType::String)
}

fn emit_signed_cmp_bool(asm: &mut Asm, op: BinOp) {
    asm.test_rr(Reg::Rax, Reg::Rax);
    let cc = match op {
        BinOp::Eq => Cc::Z,
        BinOp::Ne => Cc::NZ,
        BinOp::Lt => Cc::L,
        BinOp::Le => Cc::Le,
        BinOp::Gt => Cc::G,
        BinOp::Ge => Cc::Ge,
        _ => Cc::Z,
    };
    asm.setcc(cc, Reg::Rax);
    asm.movzx_rax_al();
}

fn map_key_mode(ty: &IrType) -> i64 {
    match ty {
        IrType::String => 1,
        IrType::Bool => 2,
        _ => 0,
    }
}

fn map_value_mode(ty: &IrType) -> i64 {
    match ty {
        IrType::String => 1,
        IrType::Int => 2,
        IrType::Bool => 3,
        IrType::Float => 4,
        _ => 0,
    }
}

fn ir_type_name(ty: &IrType) -> &'static str {
    match ty {
        IrType::Nil => "Nil",
        IrType::Bool => "Bool",
        IrType::Int => "Int",
        IrType::Float => "Float",
        IrType::String => "String",
        IrType::List(_) => "List",
        IrType::Map(_, _) => "Map",
        IrType::Struct(_) => "Struct",
        IrType::Task(_) => "Task",
        IrType::Range => "Range",
        IrType::Iter => "Iter",
        IrType::Func(_) => "Function",
        IrType::Dyn | IrType::Unknown => "dyn",
    }
}
