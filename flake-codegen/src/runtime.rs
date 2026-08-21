//! Hand-written x86-64 runtime (Windows x64 ABI) used by generated code.

use crate::emit::Import;
use crate::x86::{Asm, Cc, Reg};

pub fn emit_runtime(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    emit_strlen(asm);
    emit_print_cstr(asm, iat);
    emit_print_i64(asm, iat);
    emit_print_nl(asm);
    emit_concat(asm, iat);
    emit_alloc(asm, iat);
    emit_itoa(asm, iat);
    emit_join(asm);
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

fn lea_rbp(asm: &mut Asm, dst: Reg, disp: i32) {
    let mut rex = 0x48;
    if dst as u8 >= 8 {
        rex |= 0x04;
    }
    asm.bytes.push(rex);
    asm.bytes.push(0x8D);
    if (-128..128).contains(&disp) {
        asm.bytes.push(((dst as u8 & 7) << 3) | 0b01_000_101);
        asm.bytes.push(disp as i8 as u8);
    } else {
        asm.bytes.push(((dst as u8 & 7) << 3) | 0b10_000_101);
        asm.bytes.extend_from_slice(&disp.to_le_bytes());
    }
}

fn mov_mr_rsp(asm: &mut Asm, disp: i32, src: Reg) {
    let mut rex = 0x48;
    if src as u8 >= 8 {
        rex |= 0x04;
    }
    asm.bytes.push(rex);
    asm.bytes.push(0x89);
    asm.bytes.push(((src as u8 & 7) << 3) | 0b01_000_100);
    asm.bytes.push(0x24);
    asm.bytes.push(disp as i8 as u8);
}

fn call_import(asm: &mut Asm, iat: &mut Vec<(usize, usize)>, imp: Import) {
    let p = asm.call_indirect_rip();
    iat.push((p, imp as usize));
}

fn emit_strlen(asm: &mut Asm) {
    // rcx = ptr, rax = length. Destroys rax only besides flags.
    asm.label("rt_strlen");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.label(".sl_loop");
    // cmp byte ptr [rcx+rax], 0
    asm.bytes.extend_from_slice(&[0x80, 0x3C, 0x01, 0x00]);
    asm.jcc_label(Cc::E, ".sl_done");
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC0]); // inc rax
    asm.jmp_label(".sl_loop");
    asm.label(".sl_done");
    asm.ret();
}

fn emit_print_cstr(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    // rcx = cstring
    asm.label("rt_print_cstr");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.call_label("rt_strlen");
    asm.mov_rr(Reg::R8, Reg::Rax); // len
    asm.mov_rm_rbp(Reg::Rdx, -8); // buf
    asm.mov_ri(Reg::Rcx, -11); // STD_OUTPUT_HANDLE
    call_import(asm, iat, Import::GetStdHandle);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    lea_rbp(asm, Reg::R9, -16);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    mov_mr_rsp(asm, 32, Reg::Rax);
    call_import(asm, iat, Import::WriteFile);
    epilogue(asm);
}

fn emit_print_nl(asm: &mut Asm) {
    asm.label("rt_print_nl");
    prologue(asm, 40);
    // lea rcx, [rip+nl] patched as string index 3 ("\n\0")
    let at = asm.lea_rip(Reg::Rcx);
    // special: we record this in emit.rs via a convention — store in a well-known
    // We cannot access str_patches here easily; use a tiny on-stack newline instead.
    let _ = at;
    // mov byte [rbp-8], 10; mov byte [rbp-7], 0; lea rcx, [rbp-8]
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xF8, 0x0A]); // [rbp-8] = '\n'
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xF9, 0x00]);
    lea_rbp(asm, Reg::Rcx, -8);
    asm.call_label("rt_print_cstr");
    epilogue(asm);
}

fn emit_print_i64(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    // rcx = number. Build decimal at [rbp-40 .. rbp-16] backwards.
    asm.label("rt_print_i64");
    prologue(asm, 80);
    asm.mov_rr(Reg::Rax, Reg::Rcx);
    asm.xor_rr(Reg::R11, Reg::R11);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Ge, ".itoa_pos");
    asm.mov_ri(Reg::R11, 1);
    asm.bytes.extend_from_slice(&[0x48, 0xF7, 0xD8]); // neg rax
    asm.label(".itoa_pos");
    lea_rbp(asm, Reg::R9, -16); // write cursor
    asm.xor_rr(Reg::Rcx, Reg::Rcx); // length
    asm.label(".itoa_loop");
    asm.cqo();
    asm.mov_ri(Reg::R8, 10);
    asm.idiv(Reg::R8);
    asm.bytes.extend_from_slice(&[0x48, 0x83, 0xC2, 0x30]); // add rdx, '0'
    asm.bytes.extend_from_slice(&[0x49, 0xFF, 0xC9]); // dec r9
    asm.bytes.extend_from_slice(&[0x41, 0x88, 0x11]); // mov [r9], dl
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC1]); // inc rcx
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".itoa_loop");
    asm.test_rr(Reg::R11, Reg::R11);
    asm.jcc_label(Cc::Z, ".itoa_emit");
    asm.bytes.extend_from_slice(&[0x49, 0xFF, 0xC9]); // dec r9
    asm.bytes.extend_from_slice(&[0x41, 0xC6, 0x01, 0x2D]); // mov byte [r9], '-'
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC1]); // inc rcx
    asm.label(".itoa_emit");
    asm.mov_rr(Reg::R8, Reg::Rcx); // len
    asm.mov_rr(Reg::Rdx, Reg::R9); // buf
    asm.mov_ri(Reg::Rcx, -11);
    call_import(asm, iat, Import::GetStdHandle);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    lea_rbp(asm, Reg::R9, -8);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    mov_mr_rsp(asm, 32, Reg::Rax);
    call_import(asm, iat, Import::WriteFile);
    epilogue(asm);
}

fn emit_concat(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    // rcx = a, rdx = b → rax = malloc(a+b)
    asm.label("rt_concat2");
    prologue(asm, 80);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-32, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rdx, -24);
    asm.add_rr(Reg::Rax, Reg::Rdx);
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC0]);
    asm.mov_mr_rbp(-40, Reg::Rax);
    call_import(asm, iat, Import::GetProcessHeap);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.xor_rr(Reg::Rdx, Reg::Rdx);
    asm.mov_rm_rbp(Reg::R8, -40);
    call_import(asm, iat, Import::HeapAlloc);
    asm.mov_mr_rbp(-48, Reg::Rax);
    // copy a
    asm.mov_rm_rbp(Reg::Rax, -48);
    asm.mov_rm_rbp(Reg::Rdx, -8);
    asm.mov_rm_rbp(Reg::Rcx, -24);
    copy_bytes(asm, ".cp1");
    // copy b (rax advanced by copy_bytes)
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.mov_rm_rbp(Reg::Rcx, -32);
    copy_bytes(asm, ".cp2");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x00]); // *rax = 0
    asm.mov_rm_rbp(Reg::Rax, -48);
    epilogue(asm);
}

fn emit_alloc(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    // rcx = size → rax = pointer
    asm.label("rt_alloc");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    call_import(asm, iat, Import::GetProcessHeap);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.xor_rr(Reg::Rdx, Reg::Rdx);
    asm.mov_rm_rbp(Reg::R8, -8);
    call_import(asm, iat, Import::HeapAlloc);
    epilogue(asm);
}

fn emit_itoa(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    // rcx = i64 → rax = pointer to decimal cstring (inside a 32-byte heap buffer).
    asm.label("rt_itoa");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx); // n
    asm.mov_ri(Reg::Rcx, 32);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-16, Reg::Rax); // buf
    // r10 = cursor at buf+31
    asm.mov_rr(Reg::R10, Reg::Rax);
    asm.add_ri(Reg::R10, 31);
    asm.bytes.extend_from_slice(&[0x41, 0xC6, 0x02, 0x00]); // *r10 = 0
    asm.mov_rm_rbp(Reg::Rax, -8);
    asm.xor_rr(Reg::R11, Reg::R11);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Ge, ".itoa2_pos");
    asm.mov_ri(Reg::R11, 1);
    asm.bytes.extend_from_slice(&[0x48, 0xF7, 0xD8]); // neg rax
    asm.label(".itoa2_pos");
    asm.label(".itoa2_loop");
    asm.cqo();
    asm.mov_ri(Reg::Rcx, 10);
    asm.idiv(Reg::Rcx); // rax=quot rdx=rem
    asm.bytes.extend_from_slice(&[0x48, 0x83, 0xC2, 0x30]); // rem + '0'
    asm.bytes.extend_from_slice(&[0x49, 0xFF, 0xCA]); // dec r10
    asm.bytes.extend_from_slice(&[0x41, 0x88, 0x12]); // [r10] = dl
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".itoa2_loop");
    asm.test_rr(Reg::R11, Reg::R11);
    asm.jcc_label(Cc::Z, ".itoa2_done");
    asm.bytes.extend_from_slice(&[0x49, 0xFF, 0xCA]);
    asm.bytes.extend_from_slice(&[0x41, 0xC6, 0x02, 0x2D]); // '-'
    asm.label(".itoa2_done");
    asm.mov_rr(Reg::Rax, Reg::R10);
    epilogue(asm);
    let _ = iat;
}

fn emit_join(asm: &mut Asm) {
    // rcx = list, rdx = sep → rax = string
    asm.label("rt_join");
    prologue(asm, 80);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.mov_rm(Reg::Rax, Reg::Rcx, 0); // len
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".join_go");
    asm.mov_ri(Reg::Rcx, 1);
    asm.call_label("rt_alloc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x00]);
    epilogue(asm);
    asm.label(".join_go");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::Rax, Reg::Rcx, 8); // first item
    asm.mov_mr_rbp(-24, Reg::Rax); // acc
    asm.mov_ri(Reg::R8, 1); // i
    asm.mov_mr_rbp(-32, Reg::R8);
    asm.label(".join_loop");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::R9, Reg::Rcx, 0); // len
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".join_done");
    asm.mov_rm_rbp(Reg::Rcx, -24);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.call_label("rt_concat2");
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.shl_ri(Reg::R8, 3);
    asm.mov_rm_rbp(Reg::R9, -8);
    asm.add_rr(Reg::R9, Reg::R8);
    asm.mov_rm(Reg::Rdx, Reg::R9, 8);
    asm.call_label("rt_concat2");
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-32, Reg::R8);
    asm.jmp_label(".join_loop");
    asm.label(".join_done");
    asm.mov_rm_rbp(Reg::Rax, -24);
    epilogue(asm);
}

fn copy_bytes(asm: &mut Asm, prefix: &str) {
    let loop_l = format!("{prefix}_loop");
    let done_l = format!("{prefix}_done");
    asm.label(&loop_l);
    asm.test_rr(Reg::Rcx, Reg::Rcx);
    asm.jcc_label(Cc::Z, &done_l);
    asm.bytes.extend_from_slice(&[0x44, 0x8A, 0x1A]); // mov r11b, [rdx]
    asm.bytes.extend_from_slice(&[0x44, 0x88, 0x18]); // mov [rax], r11b
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC0]); // inc rax
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC2]); // inc rdx
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC9]); // dec rcx
    asm.jmp_label(loop_l);
    asm.label(&done_l);
}
