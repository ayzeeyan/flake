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
    emit_ftoa(asm);
    emit_streq(asm);
    emit_strcmp(asm);
    emit_starts_with(asm);
    emit_strndup(asm);
    emit_quote(asm);
    emit_bool_repr(asm);
    emit_repr(asm);
    emit_list_new(asm);
    emit_list_push(asm);
    emit_list_pop(asm);
    emit_list_concat(asm);
    emit_join(asm);
    emit_display_list(asm);
    emit_display_map(asm);
    emit_display_range(asm);
    emit_map_get(asm);
    emit_map_has(asm);
    emit_map_set(asm);
    emit_map_keys(asm);
    emit_map_values(asm);
    emit_map_entries(asm);
    emit_is_empty(asm);
    emit_split(asm);
    emit_str_index(asm);
    emit_atoi(asm);
    emit_assert(asm, iat);
    emit_read_file(asm, iat);
    emit_ends_with(asm);
    emit_contains(asm);
    emit_list_contains(asm);
    emit_range_contains(asm);
    emit_list_first_last(asm);
    emit_write_file(asm, iat);
    emit_trim(asm);
    emit_case(asm, "rt_upper", false);
    emit_case(asm, "rt_lower", true);
    emit_file_exists(asm, iat);
    emit_env(asm, iat);
    emit_cwd(asm, iat);
    emit_remove_file(asm, iat);
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

fn emit_ftoa(asm: &mut Asm) {
    const DIGITS: i32 = 15;
    const SCALE: i64 = 1_000_000_000_000_000;
    const BUFFER: i32 = -96;

    // rcx = f64 bits → rax = fixed-precision decimal with trailing zeroes
    // removed. Fifteen fractional digits preserve practical f64 output while
    // keeping the freestanding runtime compact and deterministic.
    asm.label("rt_ftoa");
    prologue(asm, 144);
    asm.mov_mr_rbp(-8, Reg::Rcx); // absolute f64 bits
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.test_rr(Reg::Rcx, Reg::Rcx);
    asm.jcc_label(Cc::Ge, ".ft_abs_ready");
    asm.mov_ri(Reg::R8, 1);
    asm.mov_ri(Reg::R10, i64::MIN);
    asm.xor_rr(Reg::Rcx, Reg::R10);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.label(".ft_abs_ready");
    asm.mov_mr_rbp(-16, Reg::R8); // sign flag

    asm.movq_xmm_r(0, Reg::Rcx);
    asm.cvttsd2si_r_xmm0(Reg::Rax);
    asm.mov_mr_rbp(-24, Reg::Rax); // integer part

    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.movq_xmm_r(0, Reg::Rcx);
    asm.mov_rm_rbp(Reg::Rax, -24);
    asm.cvtsi2sd_xmm(1, Reg::Rax);
    asm.subsd_xmm0_xmm1();
    asm.mov_ri(Reg::R10, (SCALE as f64).to_bits() as i64);
    asm.movq_xmm_r(1, Reg::R10);
    asm.mulsd_xmm0_xmm1();
    asm.mov_ri(Reg::R10, 0.5f64.to_bits() as i64);
    asm.movq_xmm_r(1, Reg::R10);
    asm.addsd_xmm0_xmm1();
    asm.cvttsd2si_r_xmm0(Reg::Rax);
    asm.mov_ri(Reg::R10, SCALE);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::L, ".ft_no_carry");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.mov_rm_rbp(Reg::R10, -24);
    asm.add_ri(Reg::R10, 1);
    asm.mov_mr_rbp(-24, Reg::R10);
    asm.label(".ft_no_carry");
    asm.mov_mr_rbp(-32, Reg::Rax); // scaled fractional part

    asm.mov_rm_rbp(Reg::Rcx, -24);
    asm.call_label("rt_itoa");
    asm.mov_mr_rbp(-40, Reg::Rax);
    asm.mov_rm_rbp(Reg::R8, -16);
    asm.test_rr(Reg::R8, Reg::R8);
    asm.jcc_label(Cc::Z, ".ft_sign_done");
    asm.mov_m8_imm_rbp(-104, b'-');
    asm.mov_m8_imm_rbp(-103, 0);
    lea_rbp(asm, Reg::Rcx, -104);
    asm.mov_rm_rbp(Reg::Rdx, -40);
    asm.call_label("rt_concat2");
    asm.mov_mr_rbp(-40, Reg::Rax);
    asm.label(".ft_sign_done");

    asm.mov_rm_rbp(Reg::Rax, -32);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".ft_fraction");
    asm.mov_rm_rbp(Reg::Rax, -40);
    epilogue(asm);

    asm.label(".ft_fraction");
    asm.mov_m8_imm_rbp(BUFFER, b'.');
    asm.mov_m8_imm_rbp(BUFFER + DIGITS + 1, 0);
    for digit in (0..DIGITS).rev() {
        asm.cqo();
        asm.mov_ri(Reg::R10, 10);
        asm.idiv(Reg::R10);
        asm.add_ri(Reg::Rdx, i32::from(b'0'));
        asm.mov_m8_rbp(BUFFER + 1 + digit, Reg::Rdx);
    }
    for digit in (1..DIGITS).rev() {
        let offset = BUFFER + 1 + digit;
        asm.cmp_m8_imm_rbp(offset, b'0');
        asm.jcc_label(Cc::Ne, ".ft_fraction_done");
        asm.mov_m8_imm_rbp(offset, 0);
    }
    asm.label(".ft_fraction_done");
    asm.mov_rm_rbp(Reg::Rcx, -40);
    lea_rbp(asm, Reg::Rdx, BUFFER);
    asm.call_label("rt_concat2");
    epilogue(asm);
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
    asm.mov_rm(Reg::Rax, Reg::Rcx, 16); // first item ([len, cap, items...])
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
    asm.mov_rm(Reg::Rdx, Reg::R9, 16);
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

fn emit_streq(asm: &mut Asm) {
    // rcx, rdx → rax = 1 if equal C strings, else 0.
    asm.label("rt_streq");
    asm.label(".eq_loop");
    asm.bytes.extend_from_slice(&[0x8A, 0x01]); // mov al, [rcx]
    asm.bytes.extend_from_slice(&[0x44, 0x8A, 0x02]); // mov r8b, [rdx]
    asm.bytes.extend_from_slice(&[0x44, 0x38, 0xC0]); // cmp al, r8b
    asm.jcc_label(Cc::Ne, ".eq_no");
    asm.bytes.extend_from_slice(&[0x84, 0xC0]); // test al, al
    asm.jcc_label(Cc::Z, ".eq_yes");
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC1]); // inc rcx
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC2]); // inc rdx
    asm.jmp_label(".eq_loop");
    asm.label(".eq_yes");
    asm.mov_ri(Reg::Rax, 1);
    asm.ret();
    asm.label(".eq_no");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.ret();
}

fn emit_strcmp(asm: &mut Asm) {
    // rcx, rdx -> signed lexical ordering for UTF-8 C strings.
    asm.label("rt_strcmp");
    asm.label(".sc_loop");
    asm.bytes.extend_from_slice(&[0x0F, 0xB6, 0x01]); // movzx eax, byte [rcx]
    asm.bytes.extend_from_slice(&[0x44, 0x0F, 0xB6, 0x02]); // movzx r8, byte [rdx]
    asm.cmp_rr(Reg::Rax, Reg::R8);
    asm.jcc_label(Cc::Ne, ".sc_diff");
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".sc_equal");
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC1]); // inc rcx
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC2]); // inc rdx
    asm.jmp_label(".sc_loop");
    asm.label(".sc_diff");
    asm.sub_rr(Reg::Rax, Reg::R8);
    asm.ret();
    asm.label(".sc_equal");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.ret();
}

fn emit_starts_with(asm: &mut Asm) {
    // rcx = s, rdx = prefix → rax = 1 if s starts with prefix.
    asm.label("rt_starts_with");
    asm.label(".sw_loop");
    asm.bytes.extend_from_slice(&[0x44, 0x8A, 0x02]); // mov r8b, [rdx]
    asm.bytes.extend_from_slice(&[0x45, 0x84, 0xC0]); // test r8b, r8b
    asm.jcc_label(Cc::Z, ".sw_yes");
    asm.bytes.extend_from_slice(&[0x8A, 0x01]); // mov al, [rcx]
    asm.bytes.extend_from_slice(&[0x44, 0x38, 0xC0]); // cmp al, r8b
    asm.jcc_label(Cc::Ne, ".sw_no");
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC1]);
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC2]);
    asm.jmp_label(".sw_loop");
    asm.label(".sw_yes");
    asm.mov_ri(Reg::Rax, 1);
    asm.ret();
    asm.label(".sw_no");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.ret();
}

fn emit_strndup(asm: &mut Asm) {
    // rcx = ptr, rdx = len → rax = new C string of that slice.
    asm.label("rt_strndup");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.mov_rr(Reg::Rcx, Reg::Rdx);
    asm.add_ri(Reg::Rcx, 1);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rdx, -8);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    copy_bytes(asm, ".snd");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x00]); // *rax = 0
    asm.mov_rm_rbp(Reg::Rax, -24);
    epilogue(asm);
}

fn emit_quote(asm: &mut Asm) {
    // rcx = C string → rax = "escaped" Debug-style quoted string.
    asm.label("rt_quote");
    prologue(asm, 64);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.add_rr(Reg::Rax, Reg::Rax);
    asm.add_ri(Reg::Rax, 3);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x22]); // *buf = '"'
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC0]); // inc rax
    asm.mov_rm_rbp(Reg::Rdx, -8); // src
    asm.label(".qt_loop");
    asm.bytes.extend_from_slice(&[0x44, 0x0F, 0xB6, 0x02]); // movzx r8, byte [rdx]
    asm.bytes.extend_from_slice(&[0x45, 0x84, 0xC0]); // test r8b, r8b
    asm.jcc_label(Cc::Z, ".qt_end");
    asm.bytes.extend_from_slice(&[0x49, 0x83, 0xF8, 0x22]); // cmp r8, '"'
    asm.jcc_label(Cc::E, ".qt_esc");
    asm.bytes.extend_from_slice(&[0x49, 0x83, 0xF8, 0x5C]); // cmp r8, '\\'
    asm.jcc_label(Cc::Ne, ".qt_put");
    asm.label(".qt_esc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x5C]); // *rax = '\\'
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC0]);
    asm.label(".qt_put");
    asm.bytes.extend_from_slice(&[0x44, 0x88, 0x00]); // [rax] = r8b
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC0]);
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC2]);
    asm.jmp_label(".qt_loop");
    asm.label(".qt_end");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x22]);
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC0]);
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x00]);
    asm.mov_rm_rbp(Reg::Rax, -24);
    epilogue(asm);
}

fn emit_repr(asm: &mut Asm) {
    // rcx = untagged value → rax = display/repr C string.
    // Heuristic: signed value < 0x10000 is an integer, else a C string pointer.
    asm.label("rt_repr");
    asm.mov_ri(Reg::R10, 0x10000);
    asm.cmp_rr(Reg::Rcx, Reg::R10);
    asm.jcc_label(Cc::L, ".rp_int");
    asm.jmp_label("rt_quote");
    asm.label(".rp_int");
    asm.jmp_label("rt_itoa");
}

fn emit_bool_repr(asm: &mut Asm) {
    // rcx = Bool → rax = freshly allocated "true" / "false".
    asm.label("rt_bool_repr");
    prologue(asm, 32);
    asm.test_rr(Reg::Rcx, Reg::Rcx);
    asm.jcc_label(Cc::Z, ".br_false");
    asm.mov_ri(Reg::Rcx, 5);
    asm.call_label("rt_alloc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, b't']);
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x01, b'r']);
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x02, b'u']);
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x03, b'e']);
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x04, 0x00]);
    epilogue(asm);
    asm.label(".br_false");
    asm.mov_ri(Reg::Rcx, 6);
    asm.call_label("rt_alloc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, b'f']);
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x01, b'a']);
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x02, b'l']);
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x03, b's']);
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x04, b'e']);
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x05, 0x00]);
    epilogue(asm);
}

fn emit_list_new(asm: &mut Asm) {
    // rcx = cap → rax = [len=0, cap, items...]
    asm.label("rt_list_new");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.test_rr(Reg::Rcx, Reg::Rcx);
    asm.jcc_label(Cc::NZ, ".ln_cap");
    asm.mov_ri(Reg::Rcx, 8);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.label(".ln_cap");
    asm.mov_rm_rbp(Reg::Rax, -8);
    asm.shl_ri(Reg::Rax, 3);
    asm.add_ri(Reg::Rax, 16);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.call_label("rt_alloc");
    asm.xor_rr(Reg::R10, Reg::R10);
    asm.mov_mr(Reg::Rax, 0, Reg::R10);
    asm.mov_rm_rbp(Reg::R10, -8);
    asm.mov_mr(Reg::Rax, 8, Reg::R10);
    epilogue(asm);
}

fn emit_list_push(asm: &mut Asm) {
    // rcx = list, rdx = val → rax = list (possibly reallocated).
    asm.label("rt_list_push");
    prologue(asm, 64);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.mov_rm(Reg::R8, Reg::Rcx, 0);
    asm.mov_rm(Reg::R9, Reg::Rcx, 8);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::L, ".lp_fit");
    asm.mov_rr(Reg::Rax, Reg::R9);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".lp_dbl");
    asm.mov_ri(Reg::Rax, 8);
    asm.jmp_label(".lp_newcap");
    asm.label(".lp_dbl");
    asm.add_rr(Reg::Rax, Reg::Rax);
    asm.label(".lp_newcap");
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.shl_ri(Reg::Rax, 3);
    asm.add_ri(Reg::Rax, 16);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-32, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rdx, -8);
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 0);
    asm.shl_ri(Reg::Rcx, 3);
    asm.add_ri(Reg::Rcx, 16);
    asm.mov_rm_rbp(Reg::Rax, -32);
    copy_bytes(asm, ".lpcp");
    asm.mov_rm_rbp(Reg::Rax, -32);
    asm.mov_rm_rbp(Reg::R10, -24);
    asm.mov_mr(Reg::Rax, 8, Reg::R10);
    asm.mov_mr_rbp(-8, Reg::Rax);
    asm.label(".lp_fit");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::R8, Reg::Rcx, 0);
    asm.mov_rr(Reg::R9, Reg::R8);
    asm.shl_ri(Reg::R9, 3);
    asm.add_rr(Reg::Rcx, Reg::R9);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.mov_mr(Reg::Rcx, 16, Reg::Rdx);
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr(Reg::Rcx, 0, Reg::R8);
    asm.mov_rr(Reg::Rax, Reg::Rcx);
    epilogue(asm);
}

fn emit_list_pop(asm: &mut Asm) {
    // rcx = list → rax = last item (or 0).
    asm.label("rt_list_pop");
    asm.mov_rm(Reg::R8, Reg::Rcx, 0);
    asm.test_rr(Reg::R8, Reg::R8);
    asm.jcc_label(Cc::NZ, ".po_go");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.ret();
    asm.label(".po_go");
    asm.add_ri(Reg::R8, -1);
    asm.mov_mr(Reg::Rcx, 0, Reg::R8);
    asm.mov_rr(Reg::R9, Reg::R8);
    asm.shl_ri(Reg::R9, 3);
    asm.add_rr(Reg::Rcx, Reg::R9);
    asm.mov_rm(Reg::Rax, Reg::Rcx, 16);
    asm.ret();
}

fn emit_list_concat(asm: &mut Asm) {
    // rcx = list1, rdx = list2 → rax = new combined list
    asm.label("rt_list_concat");
    prologue(asm, 64);
    asm.mov_mr_rbp(-8, Reg::Rcx); // list1
    asm.mov_mr_rbp(-16, Reg::Rdx); // list2
    asm.mov_rm(Reg::R8, Reg::Rcx, 0); // len1
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.mov_rm(Reg::R9, Reg::Rdx, 0); // len2
    asm.mov_mr_rbp(-32, Reg::R9);
    asm.mov_rr(Reg::Rcx, Reg::R8);
    asm.add_rr(Reg::Rcx, Reg::R9); // total len
    asm.mov_mr_rbp(-40, Reg::Rcx);
    asm.call_label("rt_list_new");
    asm.mov_mr_rbp(-48, Reg::Rax); // new list
    asm.mov_rm_rbp(Reg::R10, -40); // total len
    asm.mov_mr(Reg::Rax, 0, Reg::R10); // list.len = total len

    // Copy list1 elements
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-56, Reg::R8); // i = 0
    asm.label(".lc1_loop");
    asm.mov_rm_rbp(Reg::R8, -56);
    asm.mov_rm_rbp(Reg::R9, -24); // len1
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".lc1_done");
    asm.mov_rm_rbp(Reg::Rcx, -8); // list1
    asm.mov_rr(Reg::R10, Reg::R8);
    asm.shl_ri(Reg::R10, 3);
    asm.add_rr(Reg::Rcx, Reg::R10);
    asm.mov_rm(Reg::R11, Reg::Rcx, 16); // item
    asm.mov_rm_rbp(Reg::Rax, -48); // new list
    asm.mov_rr(Reg::R10, Reg::R8);
    asm.shl_ri(Reg::R10, 3);
    asm.add_rr(Reg::Rax, Reg::R10);
    asm.mov_mr(Reg::Rax, 16, Reg::R11);
    asm.mov_rm_rbp(Reg::R8, -56);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-56, Reg::R8);
    asm.jmp_label(".lc1_loop");
    asm.label(".lc1_done");

    // Copy list2 elements
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-56, Reg::R8); // j = 0
    asm.label(".lc2_loop");
    asm.mov_rm_rbp(Reg::R8, -56);
    asm.mov_rm_rbp(Reg::R9, -32); // len2
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".lc2_done");
    asm.mov_rm_rbp(Reg::Rdx, -16); // list2
    asm.mov_rr(Reg::R10, Reg::R8);
    asm.shl_ri(Reg::R10, 3);
    asm.add_rr(Reg::Rdx, Reg::R10);
    asm.mov_rm(Reg::R11, Reg::Rdx, 16); // item
    asm.mov_rm_rbp(Reg::Rax, -48); // new list
    asm.mov_rm_rbp(Reg::R10, -24); // len1
    asm.add_rr(Reg::R10, Reg::R8); // len1 + j
    asm.shl_ri(Reg::R10, 3);
    asm.add_rr(Reg::Rax, Reg::R10);
    asm.mov_mr(Reg::Rax, 16, Reg::R11);
    asm.mov_rm_rbp(Reg::R8, -56);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-56, Reg::R8);
    asm.jmp_label(".lc2_loop");
    asm.label(".lc2_done");

    asm.mov_rm_rbp(Reg::Rax, -48);
    epilogue(asm);
}

fn emit_display_list(asm: &mut Asm) {
    // rcx = list, rdx = element mode (0 dyn, 1 String, 2 Int, 3 Bool, 4 Float)
    // -> rax = "[a, b, c]".
    asm.label("rt_display_list");
    prologue(asm, 80);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-48, Reg::Rdx);
    asm.mov_rm(Reg::Rax, Reg::Rcx, 0);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".dl_go");
    asm.mov_ri(Reg::Rcx, 3);
    asm.call_label("rt_alloc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x5B]); // '['
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x01, 0x5D]); // ']'
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x02, 0x00]);
    epilogue(asm);
    asm.label(".dl_go");
    asm.mov_ri(Reg::Rcx, 2);
    asm.call_label("rt_alloc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x5B]);
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x01, 0x00]);
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.label(".dl_loop");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::R9, Reg::Rcx, 0);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".dl_close");
    asm.test_rr(Reg::R8, Reg::R8);
    asm.jcc_label(Cc::Z, ".dl_item");
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xD8, 0x2C]); // [rbp-40] = ','
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xD9, 0x20]); // ' '
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xDA, 0x00]);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    lea_rbp(asm, Reg::Rdx, -40);
    asm.call_label("rt_concat2");
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.label(".dl_item");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.shl_ri(Reg::R8, 3);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 16);
    asm.mov_rm_rbp(Reg::Rax, -48);
    asm.mov_ri(Reg::R10, 1);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".dl_value_string");
    asm.mov_ri(Reg::R10, 2);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".dl_value_int");
    asm.mov_ri(Reg::R10, 3);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".dl_value_bool");
    asm.mov_ri(Reg::R10, 4);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".dl_value_float");
    // Check if element is a nested list: pointer >= 0x10000 with valid len/cap
    asm.mov_ri(Reg::R10, 0x10000);
    asm.cmp_rr(Reg::Rcx, Reg::R10);
    asm.jcc_label(Cc::L, ".dl_repr");
    asm.mov_rm(Reg::R10, Reg::Rcx, 0); // len
    asm.mov_rm(Reg::R11, Reg::Rcx, 8); // cap
    asm.test_rr(Reg::R10, Reg::R10);
    asm.jcc_label(Cc::L, ".dl_repr");
    asm.mov_ri(Reg::Rax, 0x10000);
    asm.cmp_rr(Reg::R10, Reg::Rax);
    asm.jcc_label(Cc::Ge, ".dl_repr");
    asm.cmp_rr(Reg::R11, Reg::R10);
    asm.jcc_label(Cc::L, ".dl_repr");
    asm.cmp_rr(Reg::R11, Reg::Rax);
    asm.jcc_label(Cc::Ge, ".dl_repr");
    asm.test_rr(Reg::R11, Reg::R11);
    asm.jcc_label(Cc::Z, ".dl_repr");
    // It's a nested list!
    asm.xor_rr(Reg::Rdx, Reg::Rdx);
    asm.call_label("rt_display_list");
    asm.jmp_label(".dl_value_done");
    asm.label(".dl_repr");
    asm.call_label("rt_repr");
    asm.jmp_label(".dl_value_done");
    asm.label(".dl_value_string");
    asm.call_label("rt_quote");
    asm.jmp_label(".dl_value_done");
    asm.label(".dl_value_int");
    asm.call_label("rt_itoa");
    asm.jmp_label(".dl_value_done");
    asm.label(".dl_value_bool");
    asm.call_label("rt_bool_repr");
    asm.jmp_label(".dl_value_done");
    asm.label(".dl_value_float");
    asm.call_label("rt_ftoa");
    asm.label(".dl_value_done");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.call_label("rt_concat2");
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.jmp_label(".dl_loop");
    asm.label(".dl_close");
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xD8, 0x5D]); // ']'
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xD9, 0x00]);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    lea_rbp(asm, Reg::Rdx, -40);
    asm.call_label("rt_concat2");
    epilogue(asm);
}

fn emit_display_map(asm: &mut Asm) {
    // rcx = map, rdx = key mode (0 Int, 1 String, 2 Bool),
    // r8 = value mode (0 dyn, 1 String, 2 Int, 3 Bool, 4 Float).
    asm.label("rt_display_map");
    prologue(asm, 96);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-48, Reg::Rdx);
    asm.mov_mr_rbp(-56, Reg::R8);
    asm.mov_rm(Reg::Rax, Reg::Rcx, 0);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".dm_go");
    asm.mov_ri(Reg::Rcx, 3);
    asm.call_label("rt_alloc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x7B]); // '{'
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x01, 0x7D]); // '}'
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x02, 0x00]);
    epilogue(asm);
    asm.label(".dm_go");
    asm.mov_ri(Reg::Rcx, 2);
    asm.call_label("rt_alloc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x7B]);
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x01, 0x00]);
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.label(".dm_loop");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::R9, Reg::Rcx, 0);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".dm_close");
    asm.test_rr(Reg::R8, Reg::R8);
    asm.jcc_label(Cc::Z, ".dm_item");
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xD8, 0x2C]);
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xD9, 0x20]);
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xDA, 0x00]);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    lea_rbp(asm, Reg::Rdx, -40);
    asm.call_label("rt_concat2");
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.label(".dm_item");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.shl_ri(Reg::R8, 4);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 16); // key
    asm.mov_rm_rbp(Reg::Rax, -48);
    asm.mov_ri(Reg::R10, 1);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".dm_key_string");
    asm.mov_ri(Reg::R10, 2);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".dm_key_bool");
    // If key >= 0x10000, it's a heap pointer (string key)
    asm.mov_ri(Reg::R10, 0x10000);
    asm.cmp_rr(Reg::Rcx, Reg::R10);
    asm.jcc_label(Cc::Ge, ".dm_key_string");
    asm.call_label("rt_itoa");
    asm.jmp_label(".dm_key_done");
    asm.label(".dm_key_string");
    asm.call_label("rt_quote");
    asm.jmp_label(".dm_key_done");
    asm.label(".dm_key_bool");
    asm.call_label("rt_bool_repr");
    asm.label(".dm_key_done");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.call_label("rt_concat2");
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xD8, 0x3A]); // ':'
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xD9, 0x20]);
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xDA, 0x00]);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    lea_rbp(asm, Reg::Rdx, -40);
    asm.call_label("rt_concat2");
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.shl_ri(Reg::R8, 4);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 24); // value
    asm.mov_rm_rbp(Reg::Rax, -56);
    asm.mov_ri(Reg::R10, 1);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".dm_value_string");
    asm.mov_ri(Reg::R10, 2);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".dm_value_int");
    asm.mov_ri(Reg::R10, 3);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".dm_value_bool");
    asm.mov_ri(Reg::R10, 4);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".dm_value_float");
    asm.call_label("rt_repr");
    asm.jmp_label(".dm_value_done");
    asm.label(".dm_value_string");
    asm.call_label("rt_quote");
    asm.jmp_label(".dm_value_done");
    asm.label(".dm_value_int");
    asm.call_label("rt_itoa");
    asm.jmp_label(".dm_value_done");
    asm.label(".dm_value_bool");
    asm.call_label("rt_bool_repr");
    asm.jmp_label(".dm_value_done");
    asm.label(".dm_value_float");
    asm.call_label("rt_ftoa");
    asm.label(".dm_value_done");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.call_label("rt_concat2");
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.jmp_label(".dm_loop");
    asm.label(".dm_close");
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xD8, 0x7D]);
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xD9, 0x00]);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    lea_rbp(asm, Reg::Rdx, -40);
    asm.call_label("rt_concat2");
    epilogue(asm);
}

fn emit_display_range(asm: &mut Asm) {
    // rcx = range [start, end] → rax = "start..end"
    asm.label("rt_display_range");
    prologue(asm, 64);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 0);
    asm.call_label("rt_itoa");
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xE0, 0x2E]); // [rbp-32] = '.'
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xE1, 0x2E]);
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xE2, 0x00]);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    lea_rbp(asm, Reg::Rdx, -32);
    asm.call_label("rt_concat2");
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.mov_rm_rbp(Reg::R11, -8);
    asm.mov_rm(Reg::Rcx, Reg::R11, 8);
    asm.call_label("rt_itoa");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.call_label("rt_concat2");
    epilogue(asm);
}

fn emit_map_get(asm: &mut Asm) {
    // rcx = map, rdx = key, r8 = string-key flag → rax = value or fail.
    asm.label("rt_map_get");
    prologue(asm, 96);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.mov_mr_rbp(-32, Reg::R8);
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.label(".mg_loop");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::R9, Reg::Rcx, 0);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".mg_miss");
    asm.shl_ri(Reg::R8, 4);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm_rbp(Reg::Rax, -32);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".mg_str");
    asm.mov_rm_rbp(Reg::R10, -16);
    asm.mov_ri(Reg::R11, 0x10000);
    asm.cmp_rr(Reg::R10, Reg::R11);
    asm.jcc_label(Cc::L, ".mg_scalar");
    asm.label(".mg_str");
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 16);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.call_label("rt_streq");
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".mg_hit");
    asm.jmp_label(".mg_next");
    asm.label(".mg_scalar");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.shl_ri(Reg::R8, 4);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm(Reg::Rax, Reg::Rcx, 16);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.cmp_rr(Reg::Rax, Reg::Rdx);
    asm.jcc_label(Cc::E, ".mg_hit");
    asm.label(".mg_next");
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.jmp_label(".mg_loop");
    asm.label(".mg_hit");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.shl_ri(Reg::R8, 4);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm(Reg::Rax, Reg::Rcx, 24);
    epilogue(asm);
    asm.label(".mg_miss");
    for (index, byte) in b"map key not found\0".iter().copied().enumerate() {
        asm.mov_m8_imm_rbp(-64 + index as i32, byte);
    }
    asm.xor_rr(Reg::Rcx, Reg::Rcx);
    lea_rbp(asm, Reg::Rdx, -64);
    asm.call_label("rt_assert");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_map_has(asm: &mut Asm) {
    // rcx = map, rdx = key, r8 = string-key flag → rax = Bool.
    asm.label("rt_map_has");
    prologue(asm, 64);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.mov_mr_rbp(-32, Reg::R8);
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.label(".mh_loop");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::R9, Reg::Rcx, 0);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".mh_miss");
    asm.shl_ri(Reg::R8, 4);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm_rbp(Reg::Rax, -32);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".mh_str");
    asm.mov_rm_rbp(Reg::R10, -16);
    asm.mov_ri(Reg::R11, 0x10000);
    asm.cmp_rr(Reg::R10, Reg::R11);
    asm.jcc_label(Cc::L, ".mh_scalar");
    asm.label(".mh_str");
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 16);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.call_label("rt_streq");
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".mh_hit");
    asm.jmp_label(".mh_next");
    asm.label(".mh_scalar");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.shl_ri(Reg::R8, 4);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm(Reg::Rax, Reg::Rcx, 16);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.cmp_rr(Reg::Rax, Reg::Rdx);
    asm.jcc_label(Cc::E, ".mh_hit");
    asm.label(".mh_next");
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.jmp_label(".mh_loop");
    asm.label(".mh_hit");
    asm.mov_ri(Reg::Rax, 1);
    epilogue(asm);
    asm.label(".mh_miss");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_map_keys(asm: &mut Asm) {
    // rcx = map → rax = List of keys
    asm.label("rt_map_keys");
    prologue(asm, 64);
    asm.mov_mr_rbp(-8, Reg::Rcx); // map
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 0); // len
    asm.mov_mr_rbp(-16, Reg::Rcx); // len
    asm.call_label("rt_list_new");
    asm.mov_mr_rbp(-24, Reg::Rax); // list
    asm.mov_rm_rbp(Reg::Rdx, -16); // len
    asm.mov_mr(Reg::Rax, 0, Reg::Rdx); // list.len = len
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-32, Reg::R8); // i = 0
    asm.label(".mk_loop");
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.mov_rm_rbp(Reg::R9, -16);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".mk_done");
    asm.mov_rm_rbp(Reg::Rcx, -8); // map
    asm.mov_rr(Reg::R10, Reg::R8);
    asm.shl_ri(Reg::R10, 4); // i * 16
    asm.add_rr(Reg::Rcx, Reg::R10);
    asm.mov_rm(Reg::R11, Reg::Rcx, 16); // key = map[16 + i*16]
    asm.mov_rm_rbp(Reg::Rax, -24); // list
    asm.mov_rr(Reg::R10, Reg::R8);
    asm.shl_ri(Reg::R10, 3); // i * 8
    asm.add_rr(Reg::Rax, Reg::R10);
    asm.mov_mr(Reg::Rax, 16, Reg::R11); // list[16 + i*8] = key
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-32, Reg::R8);
    asm.jmp_label(".mk_loop");
    asm.label(".mk_done");
    asm.mov_rm_rbp(Reg::Rax, -24);
    epilogue(asm);
}

fn emit_map_values(asm: &mut Asm) {
    // rcx = map → rax = List of values
    asm.label("rt_map_values");
    prologue(asm, 64);
    asm.mov_mr_rbp(-8, Reg::Rcx); // map
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 0); // len
    asm.mov_mr_rbp(-16, Reg::Rcx); // len
    asm.call_label("rt_list_new");
    asm.mov_mr_rbp(-24, Reg::Rax); // list
    asm.mov_rm_rbp(Reg::Rdx, -16); // len
    asm.mov_mr(Reg::Rax, 0, Reg::Rdx); // list.len = len
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-32, Reg::R8); // i = 0
    asm.label(".mv_loop");
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.mov_rm_rbp(Reg::R9, -16);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".mv_done");
    asm.mov_rm_rbp(Reg::Rcx, -8); // map
    asm.mov_rr(Reg::R10, Reg::R8);
    asm.shl_ri(Reg::R10, 4); // i * 16
    asm.add_rr(Reg::Rcx, Reg::R10);
    asm.mov_rm(Reg::R11, Reg::Rcx, 24); // val = map[24 + i*16]
    asm.mov_rm_rbp(Reg::Rax, -24); // list
    asm.mov_rr(Reg::R10, Reg::R8);
    asm.shl_ri(Reg::R10, 3); // i * 8
    asm.add_rr(Reg::Rax, Reg::R10);
    asm.mov_mr(Reg::Rax, 16, Reg::R11); // list[16 + i*8] = val
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-32, Reg::R8);
    asm.jmp_label(".mv_loop");
    asm.label(".mv_done");
    asm.mov_rm_rbp(Reg::Rax, -24);
    epilogue(asm);
}

fn emit_map_entries(asm: &mut Asm) {
    // rcx = map → rax = List of [key, value] pairs
    asm.label("rt_map_entries");
    prologue(asm, 64);
    asm.mov_mr_rbp(-8, Reg::Rcx); // map
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 0); // len
    asm.mov_mr_rbp(-16, Reg::Rcx); // len
    asm.call_label("rt_list_new");
    asm.mov_mr_rbp(-24, Reg::Rax); // outer list
    asm.mov_rm_rbp(Reg::Rdx, -16); // len
    asm.mov_mr(Reg::Rax, 0, Reg::Rdx); // outer.len = len
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-32, Reg::R8); // i = 0
    asm.label(".me_loop");
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.mov_rm_rbp(Reg::R9, -16);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".me_done");

    // Allocate 2-element pair [key, value]
    asm.mov_ri(Reg::Rcx, 2);
    asm.call_label("rt_list_new");
    asm.mov_mr_rbp(-40, Reg::Rax); // pair
    asm.mov_ri(Reg::R10, 2);
    asm.mov_mr(Reg::Rax, 0, Reg::R10); // pair.len = 2

    // Get key and val from map
    asm.mov_rm_rbp(Reg::Rcx, -8); // map
    asm.mov_rm_rbp(Reg::R8, -32); // i
    asm.mov_rr(Reg::R10, Reg::R8);
    asm.shl_ri(Reg::R10, 4); // i * 16
    asm.add_rr(Reg::Rcx, Reg::R10);
    asm.mov_rm(Reg::R11, Reg::Rcx, 16); // key
    asm.mov_rm(Reg::Rdx, Reg::Rcx, 24); // val
    asm.mov_rm_rbp(Reg::Rax, -40); // pair
    asm.mov_mr(Reg::Rax, 16, Reg::R11); // pair[0] = key
    asm.mov_mr(Reg::Rax, 24, Reg::Rdx); // pair[1] = val

    // Store pair into outer list
    asm.mov_rm_rbp(Reg::Rax, -24); // outer list
    asm.mov_rm_rbp(Reg::R8, -32); // i
    asm.mov_rr(Reg::R10, Reg::R8);
    asm.shl_ri(Reg::R10, 3); // i * 8
    asm.add_rr(Reg::Rax, Reg::R10);
    asm.mov_rm_rbp(Reg::R11, -40); // pair
    asm.mov_mr(Reg::Rax, 16, Reg::R11); // outer[i] = pair

    asm.mov_rm_rbp(Reg::R8, -32);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-32, Reg::R8);
    asm.jmp_label(".me_loop");
    asm.label(".me_done");
    asm.mov_rm_rbp(Reg::Rax, -24);
    epilogue(asm);
}

fn emit_is_empty(asm: &mut Asm) {
    // rcx = collection (list, string, or map) → rax = Bool (1 if empty, 0 otherwise)
    asm.label("rt_is_empty");
    prologue(asm, 32);
    asm.test_rr(Reg::Rcx, Reg::Rcx);
    asm.jcc_label(Cc::Z, ".ie_true");
    asm.mov_ri(Reg::R10, 0x10000);
    asm.cmp_rr(Reg::Rcx, Reg::R10);
    asm.jcc_label(Cc::L, ".ie_true");

    // Check if it's a list or map (len is at [rcx + 0])
    // or string (first byte at [rcx + 0] is 0).
    asm.mov_rm(Reg::R8, Reg::Rcx, 0);
    asm.test_rr(Reg::R8, Reg::R8);
    asm.jcc_label(Cc::Z, ".ie_true");

    // Check if first byte is 0 (for C string)
    asm.bytes.extend_from_slice(&[0x80, 0x39, 0x00]); // cmp byte [rcx], 0
    asm.jcc_label(Cc::E, ".ie_true");

    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
    asm.label(".ie_true");
    asm.mov_ri(Reg::Rax, 1);
    epilogue(asm);
}

fn emit_map_set(asm: &mut Asm) {
    // rcx = map, rdx = key, r8 = val, r9 = string-key flag → rax = map.
    asm.label("rt_map_set");
    prologue(asm, 96);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.mov_mr_rbp(-56, Reg::R9);
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-32, Reg::R8); // i
    asm.label(".ms_loop");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::R9, Reg::Rcx, 0);
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".ms_insert");
    asm.shl_ri(Reg::R8, 4);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm_rbp(Reg::Rax, -56);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".ms_str");
    asm.mov_rm_rbp(Reg::R10, -16);
    asm.mov_ri(Reg::R11, 0x10000);
    asm.cmp_rr(Reg::R10, Reg::R11);
    asm.jcc_label(Cc::L, ".ms_scalar");
    asm.label(".ms_str");
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 16);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.call_label("rt_strcmp");
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".ms_hit");
    asm.jcc_label(Cc::G, ".ms_insert");
    asm.jmp_label(".ms_next");
    asm.label(".ms_scalar");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.shl_ri(Reg::R8, 4);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm(Reg::Rax, Reg::Rcx, 16);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.cmp_rr(Reg::Rax, Reg::Rdx);
    asm.jcc_label(Cc::E, ".ms_hit");
    asm.jcc_label(Cc::G, ".ms_insert");
    asm.label(".ms_next");
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-32, Reg::R8);
    asm.jmp_label(".ms_loop");
    asm.label(".ms_hit");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm_rbp(Reg::R8, -32);
    asm.shl_ri(Reg::R8, 4);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm_rbp(Reg::Rdx, -24);
    asm.mov_mr(Reg::Rcx, 24, Reg::Rdx);
    asm.mov_rm_rbp(Reg::Rax, -8);
    epilogue(asm);
    asm.label(".ms_insert");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::R8, Reg::Rcx, 0); // len
    asm.mov_rm(Reg::R9, Reg::Rcx, 8); // cap
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::L, ".ms_store");
    asm.mov_rr(Reg::Rax, Reg::R9);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".ms_dbl");
    asm.mov_ri(Reg::Rax, 8);
    asm.jmp_label(".ms_newcap");
    asm.label(".ms_dbl");
    asm.add_rr(Reg::Rax, Reg::Rax);
    asm.label(".ms_newcap");
    asm.mov_mr_rbp(-40, Reg::Rax); // newcap
    asm.shl_ri(Reg::Rax, 4);
    asm.add_ri(Reg::Rax, 16);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-48, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rdx, -8);
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::Rcx, Reg::Rcx, 0);
    asm.shl_ri(Reg::Rcx, 4);
    asm.add_ri(Reg::Rcx, 16);
    asm.mov_rm_rbp(Reg::Rax, -48);
    copy_bytes(asm, ".mscp");
    asm.mov_rm_rbp(Reg::Rax, -48);
    asm.mov_rm_rbp(Reg::R10, -40);
    asm.mov_mr(Reg::Rax, 8, Reg::R10);
    asm.mov_mr_rbp(-8, Reg::Rax);
    asm.label(".ms_store");
    // Shift the sorted suffix right by one entry. At loop index `i`, the
    // source entry starts at map + i*16 and its destination is 16 bytes on.
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::R8, Reg::Rcx, 0);
    asm.mov_mr_rbp(-64, Reg::R8);
    asm.label(".ms_shift");
    asm.mov_rm_rbp(Reg::R8, -64);
    asm.mov_rm_rbp(Reg::R9, -32);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Le, ".ms_store_item");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rr(Reg::Rdx, Reg::R8);
    asm.shl_ri(Reg::Rdx, 4);
    asm.add_rr(Reg::Rcx, Reg::Rdx);
    asm.mov_rm(Reg::R10, Reg::Rcx, 0);
    asm.mov_mr(Reg::Rcx, 16, Reg::R10);
    asm.mov_rm(Reg::R11, Reg::Rcx, 8);
    asm.mov_mr(Reg::Rcx, 24, Reg::R11);
    asm.sub_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-64, Reg::R8);
    asm.jmp_label(".ms_shift");
    asm.label(".ms_store_item");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm_rbp(Reg::R9, -32);
    asm.shl_ri(Reg::R9, 4);
    asm.add_rr(Reg::Rcx, Reg::R9);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.mov_mr(Reg::Rcx, 16, Reg::Rdx);
    asm.mov_rm_rbp(Reg::Rdx, -24);
    asm.mov_mr(Reg::Rcx, 24, Reg::Rdx);
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::R8, Reg::Rcx, 0);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr(Reg::Rcx, 0, Reg::R8);
    asm.mov_rr(Reg::Rax, Reg::Rcx);
    epilogue(asm);
}

fn emit_split(asm: &mut Asm) {
    // rcx = s, rdx = sep → rax = list of strings.
    asm.label("rt_split");
    prologue(asm, 96);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.mov_rr(Reg::Rcx, Reg::Rdx);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".sp_chars");
    asm.mov_ri(Reg::Rcx, 8);
    asm.call_label("rt_list_new");
    asm.mov_mr_rbp(-32, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rax, -8);
    asm.mov_mr_rbp(-40, Reg::Rax); // start
    asm.mov_mr_rbp(-48, Reg::Rax); // p
    asm.label(".sp_loop");
    asm.mov_rm_rbp(Reg::Rax, -48);
    asm.bytes.extend_from_slice(&[0x80, 0x38, 0x00]); // cmp byte [rax], 0
    asm.jcc_label(Cc::E, ".sp_last");
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.call_label("rt_starts_with");
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".sp_adv");
    asm.mov_rm_rbp(Reg::Rcx, -40);
    asm.mov_rm_rbp(Reg::Rdx, -48);
    asm.sub_rr(Reg::Rdx, Reg::Rcx);
    asm.call_label("rt_strndup");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -32);
    asm.call_label("rt_list_push");
    asm.mov_mr_rbp(-32, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rax, -48);
    asm.mov_rm_rbp(Reg::Rdx, -24);
    asm.add_rr(Reg::Rax, Reg::Rdx);
    asm.mov_mr_rbp(-48, Reg::Rax);
    asm.mov_mr_rbp(-40, Reg::Rax);
    asm.jmp_label(".sp_loop");
    asm.label(".sp_adv");
    asm.mov_rm_rbp(Reg::Rax, -48);
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC0]);
    asm.mov_mr_rbp(-48, Reg::Rax);
    asm.jmp_label(".sp_loop");
    asm.label(".sp_last");
    asm.mov_rm_rbp(Reg::Rcx, -40);
    asm.mov_rm_rbp(Reg::Rdx, -48);
    asm.sub_rr(Reg::Rdx, Reg::Rcx);
    asm.call_label("rt_strndup");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -32);
    asm.call_label("rt_list_push");
    epilogue(asm);
    asm.label(".sp_chars");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.test_rr(Reg::Rcx, Reg::Rcx);
    asm.jcc_label(Cc::NZ, ".spc_cap");
    asm.mov_ri(Reg::Rcx, 1);
    asm.label(".spc_cap");
    asm.call_label("rt_list_new");
    asm.mov_mr_rbp(-32, Reg::Rax);
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-40, Reg::R8);
    asm.label(".spc_loop");
    asm.mov_rm_rbp(Reg::R8, -40);
    asm.mov_rm_rbp(Reg::R9, -24);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".spc_done");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_ri(Reg::Rdx, 1);
    asm.call_label("rt_strndup");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -32);
    asm.call_label("rt_list_push");
    asm.mov_mr_rbp(-32, Reg::Rax);
    asm.mov_rm_rbp(Reg::R8, -40);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-40, Reg::R8);
    asm.jmp_label(".spc_loop");
    asm.label(".spc_done");
    asm.mov_rm_rbp(Reg::Rax, -32);
    epilogue(asm);
}

fn emit_str_index(asm: &mut Asm) {
    // rcx = s, rdx = index → rax = 1-character heap string (or empty string if out of bounds).
    asm.label("rt_str_index");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.mov_rm_rbp(Reg::R10, -16);
    asm.test_rr(Reg::R10, Reg::R10);
    asm.jcc_label(Cc::Ge, ".si_pos");
    asm.add_rr(Reg::R10, Reg::Rax);
    asm.mov_mr_rbp(-16, Reg::R10);
    asm.label(".si_pos");
    asm.test_rr(Reg::R10, Reg::R10);
    asm.jcc_label(Cc::L, ".si_empty");
    asm.mov_rm_rbp(Reg::Rax, -24);
    asm.cmp_rr(Reg::R10, Reg::Rax);
    asm.jcc_label(Cc::Ge, ".si_empty");
    asm.mov_ri(Reg::Rcx, 2);
    asm.call_label("rt_alloc");
    asm.mov_rm_rbp(Reg::Rdx, -8);
    asm.mov_rm_rbp(Reg::R10, -16);
    asm.add_rr(Reg::Rdx, Reg::R10);
    asm.bytes.extend_from_slice(&[0x44, 0x8A, 0x02]); // mov r8b, [rdx]
    asm.bytes.extend_from_slice(&[0x44, 0x88, 0x00]); // [rax] = r8b
    asm.bytes.extend_from_slice(&[0xC6, 0x40, 0x01, 0x00]);
    epilogue(asm);
    asm.label(".si_empty");
    asm.mov_ri(Reg::Rcx, 1);
    asm.call_label("rt_alloc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x00]);
    epilogue(asm);
}

fn emit_atoi(asm: &mut Asm) {
    // rcx = C string → rax = i64 (stops at first non-digit).
    asm.label("rt_atoi");
    asm.xor_rr(Reg::R11, Reg::R11);
    asm.bytes.extend_from_slice(&[0x80, 0x39, 0x2D]); // cmp byte [rcx], '-'
    asm.jcc_label(Cc::Ne, ".at_parse");
    asm.mov_ri(Reg::R11, 1);
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC1]);
    asm.label(".at_parse");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.label(".at_loop");
    asm.bytes.extend_from_slice(&[0x4C, 0x0F, 0xB6, 0x01]); // movzx r8, byte [rcx]
    asm.bytes.extend_from_slice(&[0x45, 0x84, 0xC0]);
    asm.jcc_label(Cc::Z, ".at_done");
    asm.bytes.extend_from_slice(&[0x49, 0x83, 0xE8, 0x30]); // sub r8, '0'
    asm.bytes.extend_from_slice(&[0x49, 0x83, 0xF8, 0x09]); // cmp r8, 9
    asm.jcc_label(Cc::A, ".at_done");
    asm.mov_ri(Reg::R9, 10);
    asm.imul_rr(Reg::Rax, Reg::R9);
    asm.add_rr(Reg::Rax, Reg::R8);
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC1]);
    asm.jmp_label(".at_loop");
    asm.label(".at_done");
    asm.test_rr(Reg::R11, Reg::R11);
    asm.jcc_label(Cc::Z, ".at_pos");
    asm.bytes.extend_from_slice(&[0x48, 0xF7, 0xD8]); // neg rax
    asm.label(".at_pos");
    asm.ret();
}

fn emit_assert(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    // rcx = cond, rdx = message (0 = default).
    asm.label("rt_assert");
    prologue(asm, 48);
    asm.test_rr(Reg::Rcx, Reg::Rcx);
    asm.jcc_label(Cc::NZ, ".as_ok");
    asm.test_rr(Reg::Rdx, Reg::Rdx);
    asm.jcc_label(Cc::NZ, ".as_msg");
    asm.mov_ri(Reg::Rax, 0x6F69_7472_6573_7361); // "assertio"
    asm.mov_mr_rbp(-32, Reg::Rax);
    asm.mov_ri(Reg::Rax, 0x6465_6C69_6166_206E); // "n failed"
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.bytes.extend_from_slice(&[0xC6, 0x45, 0xF0, 0x00]); // [rbp-16] = 0
    lea_rbp(asm, Reg::Rdx, -32);
    asm.label(".as_msg");
    asm.mov_rr(Reg::Rcx, Reg::Rdx);
    asm.call_label("rt_print_cstr");
    asm.call_label("rt_print_nl");
    asm.mov_ri(Reg::Rcx, 1);
    call_import(asm, iat, Import::ExitProcess);
    asm.label(".as_ok");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_read_file(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    // rcx = path → rax = file contents (empty string on failure).
    asm.label("rt_read_file");
    prologue(asm, 96);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_ri(Reg::Rdx, 0x8000_0000); // GENERIC_READ
    asm.mov_ri(Reg::R8, 1); // FILE_SHARE_READ
    asm.xor_rr(Reg::R9, Reg::R9);
    asm.mov_ri(Reg::Rax, 3); // OPEN_EXISTING
    mov_mr_rsp(asm, 32, Reg::Rax);
    asm.mov_ri(Reg::Rax, 0x80); // FILE_ATTRIBUTE_NORMAL
    mov_mr_rsp(asm, 40, Reg::Rax);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    mov_mr_rsp(asm, 48, Reg::Rax);
    call_import(asm, iat, Import::CreateFileA);
    asm.mov_ri(Reg::R10, -1);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".rf_fail");
    asm.mov_mr_rbp(-16, Reg::Rax); // handle
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.xor_rr(Reg::Rdx, Reg::Rdx);
    call_import(asm, iat, Import::GetFileSize);
    asm.mov_mr_rbp(-24, Reg::Rax); // size
    asm.add_ri(Reg::Rax, 1);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-32, Reg::Rax); // buf
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.mov_rm_rbp(Reg::Rdx, -32);
    asm.mov_rm_rbp(Reg::R8, -24);
    lea_rbp(asm, Reg::R9, -40);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    mov_mr_rsp(asm, 32, Reg::Rax);
    call_import(asm, iat, Import::ReadFile);
    asm.mov_rm_rbp(Reg::Rax, -32);
    asm.mov_rm_rbp(Reg::Rdx, -24);
    asm.add_rr(Reg::Rax, Reg::Rdx);
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x00]);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    call_import(asm, iat, Import::CloseHandle);
    asm.mov_rm_rbp(Reg::Rax, -32);
    epilogue(asm);
    asm.label(".rf_fail");
    asm.mov_ri(Reg::Rcx, 1);
    asm.call_label("rt_alloc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x00]);
    epilogue(asm);
}

fn emit_ends_with(asm: &mut Asm) {
    // rcx = s, rdx = suffix → rax = 1/0
    asm.label("rt_ends_with");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-24, Reg::Rax); // slen
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-32, Reg::Rax); // plen
    asm.mov_rm_rbp(Reg::Rdx, -24);
    asm.cmp_rr(Reg::Rax, Reg::Rdx);
    asm.jcc_label(Cc::A, ".ew_no"); // suffix longer
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.add_rr(Reg::Rcx, Reg::Rdx);
    asm.sub_rr(Reg::Rcx, Reg::Rax); // s + slen - plen
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.call_label("rt_streq");
    epilogue(asm);
    asm.label(".ew_no");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_contains(asm: &mut Asm) {
    // rcx = haystack, rdx = needle → rax = 1/0 (substring)
    asm.label("rt_contains");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.call_label("rt_strlen");
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".ct_go");
    asm.mov_ri(Reg::Rax, 1); // empty needle
    epilogue(asm);
    asm.label(".ct_go");
    asm.mov_rm_rbp(Reg::Rax, -8);
    asm.mov_mr_rbp(-24, Reg::Rax); // p
    asm.label(".ct_loop");
    asm.mov_rm_rbp(Reg::Rax, -24);
    asm.bytes.extend_from_slice(&[0x80, 0x38, 0x00]);
    asm.jcc_label(Cc::E, ".ct_no");
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.call_label("rt_starts_with");
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".ct_yes");
    asm.mov_rm_rbp(Reg::Rax, -24);
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC0]);
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.jmp_label(".ct_loop");
    asm.label(".ct_yes");
    asm.mov_ri(Reg::Rax, 1);
    epilogue(asm);
    asm.label(".ct_no");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_list_contains(asm: &mut Asm) {
    // rcx = list, rdx = item (int or string pointer)
    asm.label("rt_list_contains");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.label(".lc_loop");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm(Reg::R9, Reg::Rcx, 0);
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::Ge, ".lc_no");
    asm.shl_ri(Reg::R8, 3);
    asm.add_rr(Reg::Rcx, Reg::R8);
    asm.mov_rm(Reg::Rax, Reg::Rcx, 16);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.cmp_rr(Reg::Rax, Reg::Rdx);
    asm.jcc_label(Cc::E, ".lc_yes");
    // string compare if both look like pointers
    asm.mov_ri(Reg::R10, 0x10000);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::L, ".lc_next");
    asm.cmp_rr(Reg::Rdx, Reg::R10);
    asm.jcc_label(Cc::L, ".lc_next");
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.call_label("rt_streq");
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::NZ, ".lc_yes");
    asm.label(".lc_next");
    asm.mov_rm_rbp(Reg::R8, -24);
    asm.add_ri(Reg::R8, 1);
    asm.mov_mr_rbp(-24, Reg::R8);
    asm.jmp_label(".lc_loop");
    asm.label(".lc_yes");
    asm.mov_ri(Reg::Rax, 1);
    epilogue(asm);
    asm.label(".lc_no");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_range_contains(asm: &mut Asm) {
    // rcx = range [start: i64, end: i64], rdx = target i64 → rax = Bool (1 or 0)
    asm.label("rt_range_contains");
    prologue(asm, 32);
    asm.mov_rm(Reg::R8, Reg::Rcx, 0); // start
    asm.mov_rm(Reg::R9, Reg::Rcx, 8); // end
    asm.cmp_rr(Reg::R8, Reg::R9);
    asm.jcc_label(Cc::G, ".rc_rev");
    // Forward: start <= n && n < end
    asm.cmp_rr(Reg::Rdx, Reg::R8);
    asm.jcc_label(Cc::L, ".rc_false");
    asm.cmp_rr(Reg::Rdx, Reg::R9);
    asm.jcc_label(Cc::Ge, ".rc_false");
    asm.mov_ri(Reg::Rax, 1);
    epilogue(asm);
    asm.label(".rc_rev");
    // Reverse: n <= start && n > end
    asm.cmp_rr(Reg::Rdx, Reg::R8);
    asm.jcc_label(Cc::G, ".rc_false");
    asm.cmp_rr(Reg::Rdx, Reg::R9);
    asm.jcc_label(Cc::Le, ".rc_false");
    asm.mov_ri(Reg::Rax, 1);
    epilogue(asm);
    asm.label(".rc_false");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_list_first_last(asm: &mut Asm) {
    asm.label("rt_list_first");
    asm.mov_rm(Reg::Rax, Reg::Rcx, 0);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".lf_empty");
    asm.mov_rm(Reg::Rax, Reg::Rcx, 16);
    asm.ret();
    asm.label("rt_list_last");
    asm.mov_rm(Reg::Rax, Reg::Rcx, 0);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".lf_empty");
    asm.add_ri(Reg::Rax, -1);
    asm.shl_ri(Reg::Rax, 3);
    asm.add_rr(Reg::Rcx, Reg::Rax);
    asm.mov_rm(Reg::Rax, Reg::Rcx, 16);
    asm.ret();
    asm.label(".lf_empty");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.ret();
}

fn emit_write_file(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    // rcx = path, rdx = contents
    asm.label("rt_write_file");
    prologue(asm, 96);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_ri(Reg::Rdx, 0x4000_0000); // GENERIC_WRITE
    asm.xor_rr(Reg::R8, Reg::R8);
    asm.xor_rr(Reg::R9, Reg::R9);
    asm.mov_ri(Reg::Rax, 2); // CREATE_ALWAYS
    mov_mr_rsp(asm, 32, Reg::Rax);
    asm.mov_ri(Reg::Rax, 0x80);
    mov_mr_rsp(asm, 40, Reg::Rax);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    mov_mr_rsp(asm, 48, Reg::Rax);
    call_import(asm, iat, Import::CreateFileA);
    asm.mov_ri(Reg::R10, -1);
    asm.cmp_rr(Reg::Rax, Reg::R10);
    asm.jcc_label(Cc::E, ".wf_done");
    asm.mov_mr_rbp(-24, Reg::Rax); // handle
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-32, Reg::Rax); // len
    asm.mov_rm_rbp(Reg::Rcx, -24);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.mov_rm_rbp(Reg::R8, -32);
    lea_rbp(asm, Reg::R9, -40);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    mov_mr_rsp(asm, 32, Reg::Rax);
    call_import(asm, iat, Import::WriteFile);
    asm.mov_rm_rbp(Reg::Rcx, -24);
    call_import(asm, iat, Import::CloseHandle);
    asm.label(".wf_done");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn is_ascii_space_al(asm: &mut Asm, yes: &str, no: &str) {
    // al is the byte. Jump to `yes` if space/tab/lf/cr, else `no`.
    asm.bytes.extend_from_slice(&[0x3C, 0x20]); // cmp al, ' '
    asm.jcc_label(Cc::E, yes);
    asm.bytes.extend_from_slice(&[0x3C, 0x09]); // tab
    asm.jcc_label(Cc::E, yes);
    asm.bytes.extend_from_slice(&[0x3C, 0x0A]); // lf
    asm.jcc_label(Cc::E, yes);
    asm.bytes.extend_from_slice(&[0x3C, 0x0D]); // cr
    asm.jcc_label(Cc::E, yes);
    asm.jmp_label(no);
}

fn emit_trim(asm: &mut Asm) {
    // rcx = cstring → rax = trimmed copy
    asm.label("rt_trim");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.label(".tr_lead");
    asm.mov_rm_rbp(Reg::Rax, -8);
    asm.bytes.extend_from_slice(&[0x0F, 0xB6, 0x00]); // movzx eax, byte [rax]
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".tr_empty");
    is_ascii_space_al(asm, ".tr_skip", ".tr_tail");
    asm.label(".tr_skip");
    asm.mov_rm_rbp(Reg::Rax, -8);
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC0]); // inc rax
    asm.mov_mr_rbp(-8, Reg::Rax);
    asm.jmp_label(".tr_lead");
    asm.label(".tr_tail");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-16, Reg::Rax); // remaining len
    asm.label(".tr_trail");
    asm.mov_rm_rbp(Reg::Rax, -16);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".tr_empty");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.add_rr(Reg::Rcx, Reg::Rax);
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC9]); // dec rcx
    asm.bytes.extend_from_slice(&[0x0F, 0xB6, 0x01]); // movzx eax, byte [rcx]
    is_ascii_space_al(asm, ".tr_dec", ".tr_copy");
    asm.label(".tr_dec");
    asm.mov_rm_rbp(Reg::Rax, -16);
    asm.add_ri(Reg::Rax, -1);
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.jmp_label(".tr_trail");
    asm.label(".tr_copy");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.call_label("rt_strndup");
    epilogue(asm);
    asm.label(".tr_empty");
    asm.mov_ri(Reg::Rcx, 1);
    asm.call_label("rt_alloc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x00]);
    epilogue(asm);
}

fn emit_case(asm: &mut Asm, label: &str, to_lower: bool) {
    // rcx = cstring → rax = ASCII-cased copy
    asm.label(label);
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.add_ri(Reg::Rax, 1);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rdx, -8); // src
    asm.mov_rr(Reg::R8, Reg::Rax); // dst
    let loop_l = format!(".{label}_loop");
    let done_l = format!(".{label}_done");
    let conv_l = format!(".{label}_conv");
    let store_l = format!(".{label}_store");
    asm.label(&loop_l);
    asm.bytes.extend_from_slice(&[0x0F, 0xB6, 0x02]); // movzx eax, byte [rdx]
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, &done_l);
    if to_lower {
        asm.bytes.extend_from_slice(&[0x3C, b'A']);
        asm.jcc_label(Cc::B, &store_l);
        asm.bytes.extend_from_slice(&[0x3C, b'Z']);
        asm.jcc_label(Cc::A, &store_l);
        asm.bytes.extend_from_slice(&[0x04, 32]); // add al, 32
    } else {
        asm.bytes.extend_from_slice(&[0x3C, b'a']);
        asm.jcc_label(Cc::B, &store_l);
        asm.bytes.extend_from_slice(&[0x3C, b'z']);
        asm.jcc_label(Cc::A, &store_l);
        asm.bytes.extend_from_slice(&[0x2C, 32]); // sub al, 32
    }
    let _ = conv_l;
    asm.label(&store_l);
    asm.bytes.extend_from_slice(&[0x41, 0x88, 0x00]); // mov [r8], al
    asm.bytes.extend_from_slice(&[0x49, 0xFF, 0xC0]); // inc r8
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC2]); // inc rdx
    asm.jmp_label(&loop_l);
    asm.label(&done_l);
    asm.bytes.extend_from_slice(&[0x41, 0xC6, 0x00, 0x00]); // *r8 = 0
    asm.mov_rm_rbp(Reg::Rax, -24);
    epilogue(asm);
}

fn emit_file_exists(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    asm.label("rt_file_exists");
    prologue(asm, 32);
    call_import(asm, iat, Import::GetFileAttributesA);
    asm.bytes.extend_from_slice(&[0x83, 0xF8, 0xFF]); // cmp eax, -1
    asm.jcc_label(Cc::E, ".fe_fail");
    asm.mov_ri(Reg::Rax, 1);
    asm.jmp_label(".fe_done");
    asm.label(".fe_fail");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.label(".fe_done");
    epilogue(asm);
}

fn emit_env(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    // rcx = name → rax = value or empty
    asm.label("rt_env");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_ri(Reg::Rcx, 1024);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.mov_ri(Reg::R8, 1024);
    call_import(asm, iat, Import::GetEnvironmentVariableA);
    asm.bytes.extend_from_slice(&[0x85, 0xC0]); // test eax, eax
    asm.jcc_label(Cc::NZ, ".env_ok");
    asm.mov_rm_rbp(Reg::Rax, -16);
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x00]);
    asm.label(".env_ok");
    asm.mov_rm_rbp(Reg::Rax, -16);
    epilogue(asm);
}

fn emit_cwd(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    asm.label("rt_cwd");
    prologue(asm, 32);
    asm.mov_ri(Reg::Rcx, 1024);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-8, Reg::Rax);
    asm.mov_ri(Reg::Rcx, 1024);
    asm.mov_rm_rbp(Reg::Rdx, -8);
    call_import(asm, iat, Import::GetCurrentDirectoryA);
    asm.mov_rm_rbp(Reg::Rax, -8);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.label(".cwd_loop");
    asm.bytes.extend_from_slice(&[0x0F, 0xB6, 0x11]); // movzx edx, byte [rcx]
    asm.test_rr(Reg::Rdx, Reg::Rdx);
    asm.jcc_label(Cc::Z, ".cwd_done");
    asm.bytes.extend_from_slice(&[0x80, 0xFA, 0x5C]); // cmp dl, '\\'
    asm.jcc_label(Cc::Ne, ".cwd_next");
    asm.bytes.extend_from_slice(&[0xC6, 0x01, 0x2F]); // mov byte [rcx], '/'
    asm.label(".cwd_next");
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC1]); // inc rcx
    asm.jmp_label(".cwd_loop");
    asm.label(".cwd_done");
    asm.mov_rm_rbp(Reg::Rax, -8);
    epilogue(asm);
}

fn emit_remove_file(asm: &mut Asm, iat: &mut Vec<(usize, usize)>) {
    asm.label("rt_remove_file");
    prologue(asm, 32);
    call_import(asm, iat, Import::DeleteFileA);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}
