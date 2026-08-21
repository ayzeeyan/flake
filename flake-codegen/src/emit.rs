//! Lower Flake IR to x86-64 machine code (Windows x64 ABI).

use flake_ir::{BinOp, Callee, Const, Function, Inst, IrType, LocalId, Module, UnOp};

use crate::error::CodegenError;
use crate::x86::{Asm, Cc, Reg};

pub struct Compiled {
    pub code: Vec<u8>,
    pub strings: Vec<Vec<u8>>,
    pub entry: usize,
    pub iat_patches: Vec<(usize, usize)>,
    pub str_patches: Vec<(usize, usize)>,
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
];

pub fn compile_module(module: &Module) -> Result<Compiled, CodegenError> {
    let mut asm = Asm::new();
    let mut strings: Vec<Vec<u8>> = Vec::new();
    let mut iat_patches = Vec::new();
    let mut str_patches = Vec::new();
    let mut gas = String::new();
    gas.push_str("# Flake x86-64 (Windows) — generated, no LLVM/Cranelift\n");
    gas.push_str(".intel_syntax noprefix\n.text\n");

    intern_str(&mut strings, b"true");
    intern_str(&mut strings, b"false");
    intern_str(&mut strings, b"nil");
    intern_str(&mut strings, b"\n");
    intern_str(&mut strings, b" ");

    emit_start(&mut asm, &mut iat_patches, &mut gas);
    crate::runtime::emit_runtime(&mut asm, &mut iat_patches);

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

    asm.finish().map_err(|m| CodegenError::new(m))?;
    Ok(Compiled {
        code: asm.bytes,
        strings,
        entry: 0,
        iat_patches,
        str_patches,
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

fn emit_start(asm: &mut Asm, iat: &mut Vec<(usize, usize)>, gas: &mut String) {
    asm.label("_start");
    gas.push_str(".global _start\n_start:\n");
    gas.push_str("    sub rsp, 40\n    call main\n    xor ecx, ecx\n    jmp ExitProcess\n");
    asm.sub_ri(Reg::Rsp, 40);
    asm.call_label("main");
    asm.xor_rr(Reg::Rcx, Reg::Rcx);
    let p = asm.call_indirect_rip();
    iat.push((p, Import::ExitProcess as usize));
}

fn prologue(asm: &mut Asm, frame: i32) {
    asm.push(Reg::Rbp);
    asm.mov_rr(Reg::Rbp, Reg::Rsp);
    let mut sz = frame;
    if sz % 16 != 0 {
        sz += 16 - (sz % 16);
    }
    if sz == 0 {
        sz = 32;
    }
    asm.sub_ri(Reg::Rsp, sz);
}

fn epilogue(asm: &mut Asm) {
    asm.mov_rr(Reg::Rsp, Reg::Rbp);
    asm.pop(Reg::Rbp);
    asm.ret();
}

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
    let n = func.locals.len() as i32;
    // Extra slots: concat spill, plus padding for the Windows home space.
    let frame = (((n + 3) * 8 + 32) + 15) & !15;
    prologue(asm, frame);
    gas.push_str(&format!(
        "    push rbp\n    mov rbp, rsp\n    sub rsp, {frame}\n"
    ));

    let win_args = [Reg::Rcx, Reg::Rdx, Reg::R8, Reg::R9];
    for (i, _) in func.params.iter().enumerate() {
        if i < 4 {
            asm.mov_mr_rbp(local_disp(LocalId(i as u32)), win_args[i]);
        } else {
            // [rbp+16+32 + 8*(i-4)] = [rbp+48+8*(i-4)]
            let disp = 48 + 8 * ((i as i32) - 4);
            asm.mov_rm_rbp(Reg::Rax, disp);
            asm.mov_mr_rbp(local_disp(LocalId(i as u32)), Reg::Rax);
        }
    }

    for block in &func.blocks {
        let lab = format!("{}_bb{}", func.name, block.id.0);
        asm.label(&lab);
        gas.push_str(&format!(".{lab}:\n"));
        for inst in &block.insts {
            emit_inst(module, func, inst, asm, strings, strs, gas, uniq)?;
        }
    }
    Ok(())
}

fn local_disp(id: LocalId) -> i32 {
    -8 * (id.0 as i32 + 1)
}

fn emit_inst(
    module: &Module,
    func: &Function,
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
                asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
                gas.push_str(&format!(
                    "    mov rax, {n}\n    mov [rbp{:+}], rax\n",
                    local_disp(*dest)
                ));
            }
            Const::Bool(v) => {
                asm.mov_ri(Reg::Rax, if *v { 1 } else { 0 });
                asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
            }
            Const::Nil => {
                asm.xor_rr(Reg::Rax, Reg::Rax);
                asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
            }
            Const::String(s) => {
                let idx = intern_cstring(strings, s);
                let at = asm.lea_rip(Reg::Rax);
                strs.push((at, idx));
                asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
            }
            Const::Float(_) => {
                return Err(CodegenError::new(
                    "native backend does not yet lower Float constants",
                ));
            }
        },
        Inst::Move { dest, src } => {
            asm.mov_rm_rbp(Reg::Rax, local_disp(*src));
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
        Inst::Binary { dest, op, lhs, rhs } => {
            if matches!(op, BinOp::Eq | BinOp::Ne)
                && is_string_ty(&local_ty(func, *lhs))
                && is_string_ty(&local_ty(func, *rhs))
            {
                asm.mov_rm_rbp(Reg::Rcx, local_disp(*lhs));
                asm.mov_rm_rbp(Reg::Rdx, local_disp(*rhs));
                asm.call_label("rt_streq");
                if matches!(op, BinOp::Ne) {
                    asm.test_rr(Reg::Rax, Reg::Rax);
                    asm.mov_ri(Reg::Rax, 0);
                    asm.setcc(Cc::Z, Reg::Rax);
                    asm.movzx_rax_al();
                }
                asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
                return Ok(());
            }
            asm.mov_rm_rbp(Reg::Rax, local_disp(*lhs));
            asm.mov_rm_rbp(Reg::R10, local_disp(*rhs));
            match op {
                BinOp::Add => asm.add_rr(Reg::Rax, Reg::R10),
                BinOp::Sub => asm.sub_rr(Reg::Rax, Reg::R10),
                BinOp::Mul => asm.imul_rr(Reg::Rax, Reg::R10),
                BinOp::Div | BinOp::Rem => {
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
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
        Inst::Unary { dest, op, src } => {
            asm.mov_rm_rbp(Reg::Rax, local_disp(*src));
            match op {
                UnOp::Neg => asm.bytes.extend_from_slice(&[0x48, 0xF7, 0xD8]),
                UnOp::Not => {
                    asm.test_rr(Reg::Rax, Reg::Rax);
                    asm.xor_rr(Reg::Rax, Reg::Rax);
                    asm.setcc(Cc::Z, Reg::Rax);
                    asm.movzx_rax_al();
                }
            }
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
        Inst::Call { dest, callee, args } => {
            emit_call(func, dest.as_ref(), callee, args, asm, strings, strs, uniq)?;
        }
        Inst::Concat { dest, parts } => {
            if parts.is_empty() {
                let idx = intern_cstring(strings, "");
                let at = asm.lea_rip(Reg::Rax);
                strs.push((at, idx));
                asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
            } else {
                let spill = local_disp(LocalId(func.locals.len() as u32));
                load_as_cstr(func, parts[0], asm, strings, strs, uniq);
                asm.mov_mr_rbp(spill, Reg::Rax);
                for p in &parts[1..] {
                    load_as_cstr(func, *p, asm, strings, strs, uniq);
                    asm.mov_rr(Reg::Rdx, Reg::Rax);
                    asm.mov_rm_rbp(Reg::Rcx, spill);
                    asm.call_label("rt_concat2");
                    asm.mov_mr_rbp(spill, Reg::Rax);
                }
                asm.mov_rm_rbp(Reg::Rax, spill);
                asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
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
            asm.mov_rm_rbp(Reg::Rax, local_disp(*cond));
            asm.test_rr(Reg::Rax, Reg::Rax);
            asm.jcc_label(Cc::NZ, format!("{}_bb{}", func.name, then_block.0));
            asm.jmp_label(format!("{}_bb{}", func.name, else_block.0));
        }
        Inst::Return { value } => {
            if let Some(v) = value {
                asm.mov_rm_rbp(Reg::Rax, local_disp(*v));
            } else {
                asm.xor_rr(Reg::Rax, Reg::Rax);
            }
            epilogue(asm);
        }
        Inst::MakeList { dest, items } => {
            let n = items.len() as i64;
            let cap = n.max(16);
            asm.mov_ri(Reg::Rcx, 16 + 8 * cap);
            asm.call_label("rt_alloc");
            asm.mov_ri(Reg::R10, n);
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            asm.mov_ri(Reg::R10, cap);
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
            for (i, item) in items.iter().enumerate() {
                asm.mov_rm_rbp(Reg::R10, local_disp(*item));
                asm.mov_mr(Reg::Rax, 16 + 8 * i as i32, Reg::R10);
            }
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
        Inst::GetIndex { dest, obj, index } => {
            emit_get_index(func, dest, obj, index, asm, uniq);
        }
        Inst::SetIndex { obj, index, value } => {
            emit_set_index(func, obj, index, value, asm, uniq);
        }
        Inst::MakeStruct { dest, fields, .. } => {
            let n = fields.len() as i64;
            asm.mov_ri(Reg::Rcx, 8 * n.max(1));
            asm.call_label("rt_alloc");
            for (i, (_, val)) in fields.iter().enumerate() {
                asm.mov_rm_rbp(Reg::R10, local_disp(*val));
                asm.mov_mr(Reg::Rax, 8 * i as i32, Reg::R10);
            }
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
        Inst::GetField { dest, obj, field } => {
            let off = field_offset(module, func, *obj, field)?;
            asm.mov_rm_rbp(Reg::Rax, local_disp(*obj));
            asm.mov_rm(Reg::Rax, Reg::Rax, off);
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
        Inst::SetField { obj, field, value } => {
            let off = field_offset(module, func, *obj, field)?;
            asm.mov_rm_rbp(Reg::Rax, local_disp(*obj));
            asm.mov_rm_rbp(Reg::R10, local_disp(*value));
            asm.mov_mr(Reg::Rax, off, Reg::R10);
        }
        Inst::MakeRange { dest, start, end } => {
            asm.mov_ri(Reg::Rcx, 16);
            asm.call_label("rt_alloc");
            asm.mov_rm_rbp(Reg::R10, local_disp(*start));
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            asm.mov_rm_rbp(Reg::R10, local_disp(*end));
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
        Inst::MakeIter { dest, src } => {
            emit_make_iter(func, dest, src, asm, uniq);
        }
        Inst::IterNext { value, more, iter } => {
            emit_iter_next(value, more, iter, asm, uniq);
        }
        Inst::MakeMap { dest, keys, values } => {
            let n = keys.len() as i64;
            let cap = n.max(8);
            asm.mov_ri(Reg::Rcx, 16 + 16 * cap);
            asm.call_label("rt_alloc");
            asm.mov_ri(Reg::R10, n);
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            asm.mov_ri(Reg::R10, cap);
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
            for (i, (k, v)) in keys.iter().zip(values.iter()).enumerate() {
                asm.mov_rm_rbp(Reg::R10, local_disp(*k));
                asm.mov_mr(Reg::Rax, 16 + 16 * i as i32, Reg::R10);
                asm.mov_rm_rbp(Reg::R10, local_disp(*v));
                asm.mov_mr(Reg::Rax, 24 + 16 * i as i32, Reg::R10);
            }
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
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

fn load_as_cstr(
    func: &Function,
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
    asm.mov_rm_rbp(Reg::Rcx, local_disp(local));
    match ty {
        IrType::String => {
            asm.mov_rr(Reg::Rax, Reg::Rcx);
        }
        IrType::Int => {
            asm.call_label("rt_itoa");
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
        IrType::List(_) => asm.call_label("rt_display_list"),
        IrType::Map(_, _) => asm.call_label("rt_display_map"),
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
        if let Some(st) = module.structs.iter().find(|s| s.name == *name) {
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

fn emit_make_iter(func: &Function, dest: &LocalId, src: &LocalId, asm: &mut Asm, uniq: &mut u32) {
    let ty = func
        .local(*src)
        .map(|l| l.ty.clone())
        .unwrap_or(IrType::Dyn);
    match ty {
        IrType::Range => {
            asm.mov_ri(Reg::Rcx, 32);
            asm.call_label("rt_alloc");
            asm.mov_rm_rbp(Reg::R11, local_disp(*src));
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
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
        _ => {
            asm.mov_ri(Reg::Rcx, 24);
            asm.call_label("rt_alloc");
            asm.mov_ri(Reg::R11, 2); // kind = list
            asm.mov_mr(Reg::Rax, 0, Reg::R11);
            asm.mov_rm_rbp(Reg::R10, local_disp(*src));
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
            asm.xor_rr(Reg::R10, Reg::R10);
            asm.mov_mr(Reg::Rax, 16, Reg::R10);
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
    }
}

fn emit_iter_next(value: &LocalId, more: &LocalId, iter: &LocalId, asm: &mut Asm, uniq: &mut u32) {
    let id = next_id(uniq);
    asm.mov_rm_rbp(Reg::Rax, local_disp(*iter));
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
    asm.mov_rr(Reg::R11, Reg::R9);
    asm.shl_ri(Reg::R11, 3);
    asm.add_rr(Reg::R8, Reg::R11);
    asm.mov_rm(Reg::R8, Reg::R8, 16);
    asm.mov_mr_rbp(local_disp(*value), Reg::R8);
    asm.add_ri(Reg::R9, 1);
    asm.mov_mr(Reg::Rax, 16, Reg::R9);
    asm.mov_ri(Reg::R8, 1);
    asm.mov_mr_rbp(local_disp(*more), Reg::R8);
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
    asm.mov_mr_rbp(local_disp(*value), Reg::R8);
    asm.add_rr(Reg::R8, Reg::R10);
    asm.mov_mr(Reg::Rax, 8, Reg::R8);
    asm.mov_ri(Reg::R8, 1);
    asm.mov_mr_rbp(local_disp(*more), Reg::R8);
    asm.jmp_label(format!(".nend{id}"));

    asm.label(format!(".ndone{id}"));
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(local_disp(*more), Reg::R8);
    asm.label(format!(".nend{id}"));
}

fn emit_call(
    func: &Function,
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
            emit_native_print(func, dest, args, asm, strings, strs, uniq)
        }
        Callee::Static(name) if name == "len" => emit_native_len(func, dest, args, asm),
        Callee::Static(name) if name == "push" => emit_native_push(dest, args, asm),
        Callee::Static(name) if name == "pop" => emit_native_pop(dest, args, asm),
        Callee::Static(name) if name == "join" => {
            emit_binary_rt("rt_join", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "split" => {
            emit_binary_rt("rt_split", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "abs" => emit_native_abs(dest, args, asm, uniq),
        Callee::Static(name) if name == "min" => emit_native_minmax(dest, args, asm, uniq, true),
        Callee::Static(name) if name == "max" => emit_native_minmax(dest, args, asm, uniq, false),
        Callee::Static(name) if name == "range" => emit_native_range(dest, args, asm),
        Callee::Static(name) if name == "str" => {
            if args.is_empty() {
                return Err(CodegenError::new("str() expected 1 argument"));
            }
            load_as_cstr(func, args[0], asm, strings, strs, uniq);
            store_rax(dest, asm);
            Ok(())
        }
        Callee::Static(name) if name == "int" => emit_native_int(func, dest, args, asm, uniq),
        Callee::Static(name) if name == "type_of" => {
            emit_native_type_of(func, dest, args, asm, strings, strs)
        }
        Callee::Static(name) if name == "assert" => {
            emit_native_assert(dest, args, asm, strings, strs)
        }
        Callee::Static(name) if name == "read_file" => {
            if args.is_empty() {
                return Err(CodegenError::new("read_file() expected 1 argument"));
            }
            asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
            asm.call_label("rt_read_file");
            store_rax(dest, asm);
            Ok(())
        }
        Callee::Static(name) if name == "write_file" => {
            if args.len() < 2 {
                return Err(CodegenError::new("write_file() expected 2 arguments"));
            }
            let spill = local_disp(LocalId(func.locals.len() as u32));
            load_as_cstr(func, args[1], asm, strings, strs, uniq);
            asm.mov_mr_rbp(spill, Reg::Rax);
            asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
            asm.mov_rm_rbp(Reg::Rdx, spill);
            asm.call_label("rt_write_file");
            if let Some(d) = dest {
                asm.xor_rr(Reg::Rax, Reg::Rax);
                asm.mov_mr_rbp(local_disp(*d), Reg::Rax);
            }
            Ok(())
        }
        Callee::Static(name) if name == "starts_with" => {
            emit_binary_rt("rt_starts_with", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "ends_with" => {
            emit_binary_rt("rt_ends_with", dest, args, asm);
            Ok(())
        }
        Callee::Static(name) if name == "contains" => emit_native_contains(func, dest, args, asm),
        Callee::Static(name) if name == "first" => emit_native_first_last(func, dest, args, asm, true),
        Callee::Static(name) if name == "last" => emit_native_first_last(func, dest, args, asm, false),
        Callee::Static(name) if name == "float" => Err(CodegenError::new(
            "native backend does not yet lower float()",
        )),
        Callee::Static(name) => emit_user_call(name, dest, args, asm),
        Callee::Local(_) => Err(CodegenError::new(
            "indirect calls are not yet natively compiled",
        )),
    }
}

fn store_rax(dest: Option<&LocalId>, asm: &mut Asm) {
    if let Some(d) = dest {
        asm.mov_mr_rbp(local_disp(*d), Reg::Rax);
    }
}

fn emit_binary_rt(label: &str, dest: Option<&LocalId>, args: &[LocalId], asm: &mut Asm) {
    asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
    asm.mov_rm_rbp(Reg::Rdx, local_disp(args[1]));
    asm.call_label(label);
    store_rax(dest, asm);
}

fn emit_native_print(
    func: &Function,
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
        asm.mov_rm_rbp(Reg::Rcx, local_disp(*arg));
        match ty {
            IrType::String => asm.call_label("rt_print_cstr"),
            IrType::Int | IrType::Float => asm.call_label("rt_print_i64"),
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
            IrType::List(_) => {
                asm.call_label("rt_display_list");
                asm.mov_rr(Reg::Rcx, Reg::Rax);
                asm.call_label("rt_print_cstr");
            }
            IrType::Map(_, _) => {
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
            _ => asm.call_label("rt_print_i64"),
        }
    }
    asm.call_label("rt_print_nl");
    if let Some(d) = dest {
        asm.xor_rr(Reg::Rax, Reg::Rax);
        asm.mov_mr_rbp(local_disp(*d), Reg::Rax);
    }
    Ok(())
}

fn emit_native_len(
    func: &Function,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
    if args.is_empty() {
        return Err(CodegenError::new("len() expected 1 argument"));
    }
    asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
    match local_ty(func, args[0]) {
        IrType::String => asm.call_label("rt_strlen"),
        _ => asm.mov_rm(Reg::Rax, Reg::Rcx, 0),
    }
    store_rax(dest, asm);
    Ok(())
}

fn emit_native_push(
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
    if args.len() < 2 {
        return Err(CodegenError::new("push() expected 2 arguments"));
    }
    asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
    asm.mov_rm_rbp(Reg::Rdx, local_disp(args[1]));
    asm.call_label("rt_list_push");
    asm.mov_mr_rbp(local_disp(args[0]), Reg::Rax);
    if let Some(d) = dest {
        asm.xor_rr(Reg::Rax, Reg::Rax);
        asm.mov_mr_rbp(local_disp(*d), Reg::Rax);
    }
    Ok(())
}

fn emit_native_pop(
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
    if args.is_empty() {
        return Err(CodegenError::new("pop() expected 1 argument"));
    }
    asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
    asm.call_label("rt_list_pop");
    store_rax(dest, asm);
    Ok(())
}

fn emit_native_abs(
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
    uniq: &mut u32,
) -> Result<(), CodegenError> {
    if args.is_empty() {
        return Err(CodegenError::new("abs() expected 1 argument"));
    }
    let id = next_id(uniq);
    asm.mov_rm_rbp(Reg::Rax, local_disp(args[0]));
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Ge, format!(".abs{id}"));
    asm.bytes.extend_from_slice(&[0x48, 0xF7, 0xD8]);
    asm.label(format!(".abs{id}"));
    store_rax(dest, asm);
    Ok(())
}

fn emit_native_minmax(
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
    uniq: &mut u32,
    is_min: bool,
) -> Result<(), CodegenError> {
    if args.len() < 2 {
        return Err(CodegenError::new("min/max expected at least 2 arguments"));
    }
    asm.mov_rm_rbp(Reg::Rax, local_disp(args[0]));
    for a in &args[1..] {
        let id = next_id(uniq);
        asm.mov_rm_rbp(Reg::R10, local_disp(*a));
        asm.cmp_rr(Reg::Rax, Reg::R10);
        let keep = format!(".mm{id}");
        if is_min {
            asm.jcc_label(Cc::Le, &keep);
        } else {
            asm.jcc_label(Cc::Ge, &keep);
        }
        asm.mov_rr(Reg::Rax, Reg::R10);
        asm.label(keep);
    }
    store_rax(dest, asm);
    Ok(())
}

fn emit_native_range(
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
            asm.mov_rm_rbp(Reg::R10, local_disp(args[0]));
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
        }
        n if n >= 2 => {
            asm.mov_rm_rbp(Reg::R10, local_disp(args[0]));
            asm.mov_mr(Reg::Rax, 0, Reg::R10);
            asm.mov_rm_rbp(Reg::R10, local_disp(args[1]));
            asm.mov_mr(Reg::Rax, 8, Reg::R10);
        }
        _ => return Err(CodegenError::new("range() expected 1 or 2 arguments")),
    }
    store_rax(dest, asm);
    Ok(())
}

fn emit_native_int(
    func: &Function,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
    uniq: &mut u32,
) -> Result<(), CodegenError> {
    if args.is_empty() {
        return Err(CodegenError::new("int() expected 1 argument"));
    }
    asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
    match local_ty(func, args[0]) {
        IrType::String => asm.call_label("rt_atoi"),
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
    store_rax(dest, asm);
    Ok(())
}

fn emit_native_type_of(
    func: &Function,
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
    store_rax(dest, asm);
    Ok(())
}

fn emit_native_assert(
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
    _strings: &mut Vec<Vec<u8>>,
    _strs: &mut Vec<(usize, usize)>,
) -> Result<(), CodegenError> {
    if args.is_empty() {
        return Err(CodegenError::new("assert() expected 1 or 2 arguments"));
    }
    asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
    if args.len() >= 2 {
        asm.mov_rm_rbp(Reg::Rdx, local_disp(args[1]));
    } else {
        asm.xor_rr(Reg::Rdx, Reg::Rdx);
    }
    asm.call_label("rt_assert");
    if let Some(d) = dest {
        asm.xor_rr(Reg::Rax, Reg::Rax);
        asm.mov_mr_rbp(local_disp(*d), Reg::Rax);
    }
    Ok(())
}

fn emit_native_contains(
    func: &Function,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
    if args.len() < 2 {
        return Err(CodegenError::new("contains() expected 2 arguments"));
    }
    match local_ty(func, args[0]) {
        IrType::List(_) => {
            asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
            asm.mov_rm_rbp(Reg::Rdx, local_disp(args[1]));
            asm.call_label("rt_list_contains");
        }
        _ => {
            asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
            asm.mov_rm_rbp(Reg::Rdx, local_disp(args[1]));
            asm.call_label("rt_contains");
        }
    }
    store_rax(dest, asm);
    Ok(())
}

fn emit_native_first_last(
    func: &Function,
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
            asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
            if first {
                asm.xor_rr(Reg::Rdx, Reg::Rdx);
            } else {
                asm.mov_ri(Reg::Rdx, -1);
            }
            asm.call_label("rt_str_index");
        }
        _ => {
            asm.mov_rm_rbp(Reg::Rcx, local_disp(args[0]));
            if first {
                asm.call_label("rt_list_first");
            } else {
                asm.call_label("rt_list_last");
            }
        }
    }
    store_rax(dest, asm);
    Ok(())
}

fn emit_user_call(
    name: &str,
    dest: Option<&LocalId>,
    args: &[LocalId],
    asm: &mut Asm,
) -> Result<(), CodegenError> {
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
            asm.mov_rm_rbp(Reg::Rax, local_disp(*a));
            asm.mov_mr_rsp(32 + 8 * (i as i32 - 4), Reg::Rax);
        }
    }
    for (i, a) in args.iter().enumerate().take(4) {
        asm.mov_rm_rbp(win_args[i], local_disp(*a));
    }
    asm.call_label(name);
    if extra > 0 {
        asm.add_ri(Reg::Rsp, space);
    }
    store_rax(dest, asm);
    Ok(())
}

fn emit_get_index(
    func: &Function,
    dest: &LocalId,
    obj: &LocalId,
    index: &LocalId,
    asm: &mut Asm,
    uniq: &mut u32,
) {
    let obj_ty = local_ty(func, *obj);
    let idx_ty = local_ty(func, *index);
    match obj_ty {
        IrType::Map(_, _) => {
            asm.mov_rm_rbp(Reg::Rcx, local_disp(*obj));
            asm.mov_rm_rbp(Reg::Rdx, local_disp(*index));
            asm.call_label("rt_map_get");
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
        IrType::String => {
            asm.mov_rm_rbp(Reg::Rcx, local_disp(*obj));
            asm.mov_rm_rbp(Reg::Rdx, local_disp(*index));
            asm.call_label("rt_str_index");
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
        IrType::Dyn | IrType::Unknown if is_string_ty(&idx_ty) => {
            asm.mov_rm_rbp(Reg::Rcx, local_disp(*obj));
            asm.mov_rm_rbp(Reg::Rdx, local_disp(*index));
            asm.call_label("rt_map_get");
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
        _ => {
            let id = next_id(uniq);
            asm.mov_rm_rbp(Reg::Rax, local_disp(*obj));
            asm.mov_rm_rbp(Reg::R10, local_disp(*index));
            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::Ge, format!(".idxpos{id}"));
            asm.mov_rm(Reg::R11, Reg::Rax, 0);
            asm.add_rr(Reg::R10, Reg::R11);
            asm.label(format!(".idxpos{id}"));
            asm.shl_ri(Reg::R10, 3);
            asm.add_rr(Reg::Rax, Reg::R10);
            asm.mov_rm(Reg::Rax, Reg::Rax, 16);
            asm.mov_mr_rbp(local_disp(*dest), Reg::Rax);
        }
    }
}

fn emit_set_index(
    func: &Function,
    obj: &LocalId,
    index: &LocalId,
    value: &LocalId,
    asm: &mut Asm,
    uniq: &mut u32,
) {
    let obj_ty = local_ty(func, *obj);
    let idx_ty = local_ty(func, *index);
    match obj_ty {
        IrType::Map(_, _) => {
            asm.mov_rm_rbp(Reg::Rcx, local_disp(*obj));
            asm.mov_rm_rbp(Reg::Rdx, local_disp(*index));
            asm.mov_rm_rbp(Reg::R8, local_disp(*value));
            asm.call_label("rt_map_set");
            asm.mov_mr_rbp(local_disp(*obj), Reg::Rax);
        }
        IrType::Dyn | IrType::Unknown if is_string_ty(&idx_ty) => {
            asm.mov_rm_rbp(Reg::Rcx, local_disp(*obj));
            asm.mov_rm_rbp(Reg::Rdx, local_disp(*index));
            asm.mov_rm_rbp(Reg::R8, local_disp(*value));
            asm.call_label("rt_map_set");
            asm.mov_mr_rbp(local_disp(*obj), Reg::Rax);
        }
        _ => {
            let id = next_id(uniq);
            asm.mov_rm_rbp(Reg::Rax, local_disp(*obj));
            asm.mov_rm_rbp(Reg::R10, local_disp(*index));
            asm.test_rr(Reg::R10, Reg::R10);
            asm.jcc_label(Cc::Ge, format!(".sidxpos{id}"));
            asm.mov_rm(Reg::R11, Reg::Rax, 0);
            asm.add_rr(Reg::R10, Reg::R11);
            asm.label(format!(".sidxpos{id}"));
            asm.shl_ri(Reg::R10, 3);
            asm.add_rr(Reg::Rax, Reg::R10);
            asm.mov_rm_rbp(Reg::R10, local_disp(*value));
            asm.mov_mr(Reg::Rax, 16, Reg::R10);
        }
    }
}

fn local_ty(func: &Function, id: LocalId) -> IrType {
    func.local(id).map(|l| l.ty.clone()).unwrap_or(IrType::Dyn)
}

fn is_string_ty(ty: &IrType) -> bool {
    matches!(ty, IrType::String)
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
        IrType::Range => "Range",
        IrType::Iter => "Iter",
        IrType::Func => "Function",
        IrType::Dyn | IrType::Unknown => "dyn",
    }
}
