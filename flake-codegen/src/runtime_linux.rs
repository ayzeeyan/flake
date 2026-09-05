//! Linux x86-64 syscall runtime.
//!
//! Generated Flake functions keep the Windows x64 ABI (rcx, rdx, r8, r9) so
//! `emit.rs` stays shared. OS work uses raw Linux syscalls (`syscall`), not libc
//! and not a Win32 IAT — which ELF images cannot resolve.
//!
//! A 4 KiB globals page is mapped at [`LINUX_GLOBALS`] from `_start`:
//! argc, argv, envp, and the bump-pointer heap cursor.

use crate::x86::{Asm, Cc, Reg};

/// Fixed anonymous mapping used as the Linux runtime globals page.
pub const LINUX_GLOBALS: i64 = 0x0020_0000;

const G_ARGC: i32 = 0;
const G_ARGV: i32 = 8;
const G_ENVP: i32 = 16;
const G_HEAP: i32 = 24;
const G_HEAP_END: i32 = 32;
const G_LIVE_BYTES: i32 = 40;
const G_PEAK_BYTES: i32 = 48;
const G_ALLOC_COUNT: i32 = 56;
const G_FREE_COUNT: i32 = 64;
const G_FREE_LISTS: i32 = 72;

const SYS_READ: i64 = 0;
const SYS_WRITE: i64 = 1;
const SYS_CLOSE: i64 = 3;
const SYS_LSEEK: i64 = 8;
const SYS_MMAP: i64 = 9;
const SYS_DUP2: i64 = 33;
const SYS_FORK: i64 = 57;
const SYS_EXECVE: i64 = 59;
const SYS_EXIT: i64 = 60;
const SYS_WAIT4: i64 = 61;
const SYS_GETCWD: i64 = 79;
const SYS_MKDIR: i64 = 83;
const SYS_UNLINK: i64 = 87;
const SYS_GETDENTS64: i64 = 217;
const SYS_OPENAT: i64 = 257;
const SYS_NEWFSTATAT: i64 = 262;
const SYS_PIPE2: i64 = 293;

const AT_FDCWD: i64 = -100;
const O_RDONLY: i64 = 0;
const O_WRONLY: i64 = 1;
const O_CREAT: i64 = 64;
const O_TRUNC: i64 = 512;
const O_APPEND: i64 = 1024;
const O_DIRECTORY: i64 = 0x1_0000;
const PROT_READ_WRITE: i64 = 3;
const MAP_PRIVATE_ANON: i64 = 0x22;
const MAP_FIXED: i64 = 0x10;
const SEEK_SET: i64 = 0;
const SEEK_END: i64 = 2;
const S_IFMT: i64 = 0xF000;
const S_IFDIR: i64 = 0x4000;
const S_IFREG: i64 = 0x8000;
const MODE_0644: i64 = 0o644;
const MODE_0755: i64 = 0o755;

pub fn emit_linux_start(asm: &mut Asm, gas: &mut String) {
    gas.push_str("    ; Linux _start: mmap globals, save argc/argv/envp, call main, sys_exit\n");
    asm.label("_start");

    // mmap(0x200000, 4096, PROT_READ|WRITE, MAP_PRIVATE|ANON|FIXED, -1, 0)
    asm.mov_ri(Reg::Rdi, LINUX_GLOBALS);
    asm.mov_ri(Reg::Rsi, 4096);
    asm.mov_ri(Reg::Rdx, PROT_READ_WRITE);
    asm.mov_ri(Reg::R10, MAP_PRIVATE_ANON | MAP_FIXED);
    asm.mov_ri(Reg::R8, -1);
    asm.xor_rr(Reg::R9, Reg::R9);
    do_syscall(asm, SYS_MMAP);

    // argc / argv / envp live on the incoming stack.
    // `mov rax, [rsp]` needs the SIB form (rm=100).
    asm.bytes.extend_from_slice(&[0x48, 0x8B, 0x04, 0x24]);
    asm.mov_rr(Reg::Rdx, Reg::Rsp);
    asm.add_ri(Reg::Rdx, 8);
    asm.mov_rr(Reg::Rcx, Reg::Rax);
    asm.add_ri(Reg::Rcx, 1);
    asm.shl_ri(Reg::Rcx, 3);
    asm.mov_rr(Reg::R8, Reg::Rdx);
    asm.add_rr(Reg::R8, Reg::Rcx);

    load_globals(asm, Reg::R9);
    asm.mov_mr(Reg::R9, G_ARGC, Reg::Rax);
    asm.mov_mr(Reg::R9, G_ARGV, Reg::Rdx);
    asm.mov_mr(Reg::R9, G_ENVP, Reg::R8);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.mov_mr(Reg::R9, G_HEAP, Reg::Rax);
    asm.mov_mr(Reg::R9, G_HEAP_END, Reg::Rax);

    // ELF _start: rsp is 16-byte aligned. Keep Windows ABI for `main`.
    asm.sub_ri(Reg::Rsp, 32);
    asm.call_label("main");
    asm.xor_rr(Reg::Rcx, Reg::Rcx);
    asm.call_label("rt_exit");
}

pub fn emit_linux_os_runtime(asm: &mut Asm) {
    emit_print_cstr(asm);
    emit_alloc(asm);
    emit_free(asm);
    emit_exit(asm);
    emit_read_file(asm);
    emit_write_file(asm);
    emit_file_exists(asm);
    emit_env(asm);
    emit_cwd(asm);
    emit_remove_file(asm);
    emit_is_dir(asm);
    emit_is_file(asm);
    emit_create_dir(asm);
    emit_append_file(asm);
    emit_list_dir(asm);
    emit_args(asm);
    emit_run_cmd(asm);
}

fn prologue(asm: &mut Asm, frame: i32) {
    crate::runtime::rt_prologue(asm, frame);
}

fn epilogue(asm: &mut Asm) {
    crate::runtime::rt_epilogue(asm);
}

fn lea_rbp(asm: &mut Asm, dst: Reg, disp: i32) {
    crate::runtime::rt_lea_rbp(asm, dst, disp);
}

fn load_globals(asm: &mut Asm, dst: Reg) {
    asm.mov_ri(dst, LINUX_GLOBALS);
}

fn do_syscall(asm: &mut Asm, nr: i64) {
    asm.mov_ri(Reg::Rax, nr);
    asm.syscall();
}

fn j_syscall_err(asm: &mut Asm, label: &str) {
    asm.xor_rr(Reg::R11, Reg::R11);
    asm.cmp_rr(Reg::Rax, Reg::R11);
    asm.jcc_label(Cc::L, label);
}

fn emit_empty_cstr(asm: &mut Asm) {
    asm.mov_ri(Reg::Rcx, 1);
    asm.call_label("rt_alloc");
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x00]);
}

fn movsxd_rbp(asm: &mut Asm, dst: Reg, disp: i32) {
    let mut rex = 0x48u8;
    if dst as u8 >= 8 {
        rex |= 0x04;
    }
    asm.bytes.push(rex);
    asm.bytes.push(0x63);
    if (-128..128).contains(&disp) {
        asm.bytes.push(((dst as u8 & 7) << 3) | 0b01_000_101);
        asm.bytes.push(disp as i8 as u8);
    } else {
        asm.bytes.push(((dst as u8 & 7) << 3) | 0b10_000_101);
        asm.bytes.extend_from_slice(&disp.to_le_bytes());
    }
}

fn emit_print_cstr(asm: &mut Asm) {
    // rcx = cstring. sys_write(1, buf, len)
    asm.label("rt_print_cstr");
    prologue(asm, 32);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.call_label("rt_strlen");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rsi, -8);
    asm.mov_ri(Reg::Rdi, 1);
    do_syscall(asm, SYS_WRITE);
    epilogue(asm);
}

fn emit_alloc(asm: &mut Asm) {
    // rcx = size → rax = pointer. Free list buckets or bump allocator.
    asm.label("rt_alloc");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_ri(Reg::R8, -1);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.mov_mr_rbp(-16, Reg::Rax);

    asm.cmp_ri(Reg::Rcx, 16);
    asm.jcc_label(Cc::Le, ".lx_al_b0");
    asm.cmp_ri(Reg::Rcx, 24);
    asm.jcc_label(Cc::Le, ".lx_al_b1");
    asm.cmp_ri(Reg::Rcx, 32);
    asm.jcc_label(Cc::Le, ".lx_al_b2");
    asm.cmp_ri(Reg::Rcx, 48);
    asm.jcc_label(Cc::Le, ".lx_al_b3");
    asm.cmp_ri(Reg::Rcx, 64);
    asm.jcc_label(Cc::Le, ".lx_al_b4");
    asm.cmp_ri(Reg::Rcx, 128);
    asm.jcc_label(Cc::Le, ".lx_al_b5");
    asm.cmp_ri(Reg::Rcx, 256);
    asm.jcc_label(Cc::Le, ".lx_al_b6");
    asm.cmp_ri(Reg::Rcx, 512);
    asm.jcc_label(Cc::Le, ".lx_al_b7");
    asm.cmp_ri(Reg::Rcx, 1024);
    asm.jcc_label(Cc::Le, ".lx_al_b8");
    asm.cmp_ri(Reg::Rcx, 2048);
    asm.jcc_label(Cc::Le, ".lx_al_b9");
    asm.cmp_ri(Reg::Rcx, 4096);
    asm.jcc_label(Cc::Le, ".lx_al_b10");
    asm.jmp_label(".lx_al_os");

    asm.label(".lx_al_b0");
    asm.mov_ri(Reg::R8, 0);
    asm.mov_ri(Reg::Rcx, 16);
    asm.jmp_label(".lx_al_lookup");
    asm.label(".lx_al_b1");
    asm.mov_ri(Reg::R8, 1);
    asm.mov_ri(Reg::Rcx, 24);
    asm.jmp_label(".lx_al_lookup");
    asm.label(".lx_al_b2");
    asm.mov_ri(Reg::R8, 2);
    asm.mov_ri(Reg::Rcx, 32);
    asm.jmp_label(".lx_al_lookup");
    asm.label(".lx_al_b3");
    asm.mov_ri(Reg::R8, 3);
    asm.mov_ri(Reg::Rcx, 48);
    asm.jmp_label(".lx_al_lookup");
    asm.label(".lx_al_b4");
    asm.mov_ri(Reg::R8, 4);
    asm.mov_ri(Reg::Rcx, 64);
    asm.jmp_label(".lx_al_lookup");
    asm.label(".lx_al_b5");
    asm.mov_ri(Reg::R8, 5);
    asm.mov_ri(Reg::Rcx, 128);
    asm.jmp_label(".lx_al_lookup");
    asm.label(".lx_al_b6");
    asm.mov_ri(Reg::R8, 6);
    asm.mov_ri(Reg::Rcx, 256);
    asm.jmp_label(".lx_al_lookup");
    asm.label(".lx_al_b7");
    asm.mov_ri(Reg::R8, 7);
    asm.mov_ri(Reg::Rcx, 512);
    asm.jmp_label(".lx_al_lookup");
    asm.label(".lx_al_b8");
    asm.mov_ri(Reg::R8, 8);
    asm.mov_ri(Reg::Rcx, 1024);
    asm.jmp_label(".lx_al_lookup");
    asm.label(".lx_al_b9");
    asm.mov_ri(Reg::R8, 9);
    asm.mov_ri(Reg::Rcx, 2048);
    asm.jmp_label(".lx_al_lookup");
    asm.label(".lx_al_b10");
    asm.mov_ri(Reg::R8, 10);
    asm.mov_ri(Reg::Rcx, 4096);

    asm.label(".lx_al_lookup");
    asm.mov_mr_rbp(-16, Reg::Rcx); // normalized size
    load_globals(asm, Reg::R9);
    asm.shl_ri(Reg::R8, 3);
    asm.add_ri(Reg::R8, G_FREE_LISTS);
    asm.add_rr(Reg::R8, Reg::R9);
    asm.mov_rm(Reg::Rax, Reg::R8, 0);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".lx_al_os");

    // Hit in free list!
    asm.mov_rm(Reg::R10, Reg::Rax, 0);
    asm.mov_mr(Reg::R8, 0, Reg::R10);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.mov_rm(Reg::R10, Reg::R9, G_LIVE_BYTES);
    asm.add_rr(Reg::R10, Reg::Rcx);
    asm.mov_mr(Reg::R9, G_LIVE_BYTES, Reg::R10);
    asm.mov_rm(Reg::R11, Reg::R9, G_PEAK_BYTES);
    asm.cmp_rr(Reg::R10, Reg::R11);
    asm.jcc_label(Cc::Be, ".lx_al_hit_stats");
    asm.mov_mr(Reg::R9, G_PEAK_BYTES, Reg::R10);
    asm.label(".lx_al_hit_stats");
    asm.mov_rm(Reg::R10, Reg::R9, G_ALLOC_COUNT);
    asm.add_ri(Reg::R10, 1);
    asm.mov_mr(Reg::R9, G_ALLOC_COUNT, Reg::R10);
    epilogue(asm);

    asm.label(".lx_al_os");
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.test_rr(Reg::Rcx, Reg::Rcx);
    asm.jcc_label(Cc::NZ, ".lx_al_align");
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.mov_ri(Reg::R11, 8);
    asm.cmp_rr(Reg::Rcx, Reg::R11);
    asm.jcc_label(Cc::Ae, ".lx_al_align");
    asm.mov_ri(Reg::Rcx, 8);
    asm.label(".lx_al_align");
    asm.add_ri(Reg::Rcx, 7);
    asm.mov_ri(Reg::R11, -8);
    asm.and_rr(Reg::Rcx, Reg::R11);
    asm.mov_mr_rbp(-16, Reg::Rcx);

    load_globals(asm, Reg::R9);
    asm.mov_rm(Reg::Rax, Reg::R9, G_HEAP);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".lx_al_chunk");
    asm.mov_rm(Reg::Rdx, Reg::R9, G_HEAP_END);
    asm.mov_rr(Reg::R8, Reg::Rax);
    asm.add_rr(Reg::R8, Reg::Rcx);
    asm.cmp_rr(Reg::R8, Reg::Rdx);
    asm.jcc_label(Cc::Be, ".lx_al_have");

    asm.label(".lx_al_chunk");
    asm.mov_rm_rbp(Reg::Rsi, -16);
    asm.mov_ri(Reg::R11, 65536);
    asm.cmp_rr(Reg::Rsi, Reg::R11);
    asm.jcc_label(Cc::Ae, ".lx_al_pages");
    asm.mov_ri(Reg::Rsi, 65536);
    asm.label(".lx_al_pages");
    asm.add_ri(Reg::Rsi, 4095);
    asm.mov_ri(Reg::R11, -4096);
    asm.and_rr(Reg::Rsi, Reg::R11);
    asm.mov_mr_rbp(-24, Reg::Rsi);
    asm.xor_rr(Reg::Rdi, Reg::Rdi);
    asm.mov_ri(Reg::Rdx, PROT_READ_WRITE);
    asm.mov_ri(Reg::R10, MAP_PRIVATE_ANON);
    asm.mov_ri(Reg::R8, -1);
    asm.xor_rr(Reg::R9, Reg::R9);
    do_syscall(asm, SYS_MMAP);
    j_syscall_err(asm, ".lx_al_fail");
    load_globals(asm, Reg::R9);
    asm.mov_mr(Reg::R9, G_HEAP, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rdx, -24);
    asm.add_rr(Reg::Rdx, Reg::Rax);
    asm.mov_mr(Reg::R9, G_HEAP_END, Reg::Rdx);

    asm.label(".lx_al_have");
    load_globals(asm, Reg::R9);
    asm.mov_rm(Reg::Rax, Reg::R9, G_HEAP);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.add_rr(Reg::Rcx, Reg::Rax);
    asm.mov_mr(Reg::R9, G_HEAP, Reg::Rcx);

    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.mov_rm(Reg::R10, Reg::R9, G_LIVE_BYTES);
    asm.add_rr(Reg::R10, Reg::Rcx);
    asm.mov_mr(Reg::R9, G_LIVE_BYTES, Reg::R10);
    asm.mov_rm(Reg::R11, Reg::R9, G_PEAK_BYTES);
    asm.cmp_rr(Reg::R10, Reg::R11);
    asm.jcc_label(Cc::Be, ".lx_al_bump_stats");
    asm.mov_mr(Reg::R9, G_PEAK_BYTES, Reg::R10);
    asm.label(".lx_al_bump_stats");
    asm.mov_rm(Reg::R10, Reg::R9, G_ALLOC_COUNT);
    asm.add_ri(Reg::R10, 1);
    asm.mov_mr(Reg::R9, G_ALLOC_COUNT, Reg::R10);
    epilogue(asm);

    asm.label(".lx_al_fail");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_free(asm: &mut Asm) {
    // rcx = ptr, rdx = size
    asm.label("rt_free");
    asm.test_rr(Reg::Rcx, Reg::Rcx);
    asm.jcc_label(Cc::Z, ".lx_fr_ret");
    asm.test_rr(Reg::Rdx, Reg::Rdx);
    asm.jcc_label(Cc::Le, ".lx_fr_ret");

    asm.cmp_ri(Reg::Rdx, 16);
    asm.jcc_label(Cc::Le, ".lx_fr_b0");
    asm.cmp_ri(Reg::Rdx, 24);
    asm.jcc_label(Cc::Le, ".lx_fr_b1");
    asm.cmp_ri(Reg::Rdx, 32);
    asm.jcc_label(Cc::Le, ".lx_fr_b2");
    asm.cmp_ri(Reg::Rdx, 48);
    asm.jcc_label(Cc::Le, ".lx_fr_b3");
    asm.cmp_ri(Reg::Rdx, 64);
    asm.jcc_label(Cc::Le, ".lx_fr_b4");
    asm.cmp_ri(Reg::Rdx, 128);
    asm.jcc_label(Cc::Le, ".lx_fr_b5");
    asm.cmp_ri(Reg::Rdx, 256);
    asm.jcc_label(Cc::Le, ".lx_fr_b6");
    asm.cmp_ri(Reg::Rdx, 512);
    asm.jcc_label(Cc::Le, ".lx_fr_b7");
    asm.cmp_ri(Reg::Rdx, 1024);
    asm.jcc_label(Cc::Le, ".lx_fr_b8");
    asm.cmp_ri(Reg::Rdx, 2048);
    asm.jcc_label(Cc::Le, ".lx_fr_b9");
    asm.cmp_ri(Reg::Rdx, 4096);
    asm.jcc_label(Cc::Le, ".lx_fr_b10");
    asm.jmp_label(".lx_fr_large");

    asm.label(".lx_fr_b0");
    asm.mov_ri(Reg::R8, 0);
    asm.mov_ri(Reg::Rdx, 16);
    asm.jmp_label(".lx_fr_put");
    asm.label(".lx_fr_b1");
    asm.mov_ri(Reg::R8, 1);
    asm.mov_ri(Reg::Rdx, 24);
    asm.jmp_label(".lx_fr_put");
    asm.label(".lx_fr_b2");
    asm.mov_ri(Reg::R8, 2);
    asm.mov_ri(Reg::Rdx, 32);
    asm.jmp_label(".lx_fr_put");
    asm.label(".lx_fr_b3");
    asm.mov_ri(Reg::R8, 3);
    asm.mov_ri(Reg::Rdx, 48);
    asm.jmp_label(".lx_fr_put");
    asm.label(".lx_fr_b4");
    asm.mov_ri(Reg::R8, 4);
    asm.mov_ri(Reg::Rdx, 64);
    asm.jmp_label(".lx_fr_put");
    asm.label(".lx_fr_b5");
    asm.mov_ri(Reg::R8, 5);
    asm.mov_ri(Reg::Rdx, 128);
    asm.jmp_label(".lx_fr_put");
    asm.label(".lx_fr_b6");
    asm.mov_ri(Reg::R8, 6);
    asm.mov_ri(Reg::Rdx, 256);
    asm.jmp_label(".lx_fr_put");
    asm.label(".lx_fr_b7");
    asm.mov_ri(Reg::R8, 7);
    asm.mov_ri(Reg::Rdx, 512);
    asm.jmp_label(".lx_fr_put");
    asm.label(".lx_fr_b8");
    asm.mov_ri(Reg::R8, 8);
    asm.mov_ri(Reg::Rdx, 1024);
    asm.jmp_label(".lx_fr_put");
    asm.label(".lx_fr_b9");
    asm.mov_ri(Reg::R8, 9);
    asm.mov_ri(Reg::Rdx, 2048);
    asm.jmp_label(".lx_fr_put");
    asm.label(".lx_fr_b10");
    asm.mov_ri(Reg::R8, 10);
    asm.mov_ri(Reg::Rdx, 4096);

    asm.label(".lx_fr_put");
    load_globals(asm, Reg::R9);
    asm.shl_ri(Reg::R8, 3);
    asm.add_ri(Reg::R8, G_FREE_LISTS);
    asm.add_rr(Reg::R8, Reg::R9);
    asm.mov_rm(Reg::Rax, Reg::R8, 0);
    asm.mov_mr(Reg::Rcx, 0, Reg::Rax);
    asm.mov_mr(Reg::R8, 0, Reg::Rcx);

    asm.mov_rm(Reg::R10, Reg::R9, G_LIVE_BYTES);
    asm.sub_rr(Reg::R10, Reg::Rdx);
    asm.mov_mr(Reg::R9, G_LIVE_BYTES, Reg::R10);
    asm.mov_rm(Reg::R10, Reg::R9, G_FREE_COUNT);
    asm.add_ri(Reg::R10, 1);
    asm.mov_mr(Reg::R9, G_FREE_COUNT, Reg::R10);
    asm.ret();

    asm.label(".lx_fr_large");
    load_globals(asm, Reg::R9);
    asm.mov_rm(Reg::R10, Reg::R9, G_LIVE_BYTES);
    asm.sub_rr(Reg::R10, Reg::Rdx);
    asm.mov_mr(Reg::R9, G_LIVE_BYTES, Reg::R10);
    asm.mov_rm(Reg::R10, Reg::R9, G_FREE_COUNT);
    asm.add_ri(Reg::R10, 1);
    asm.mov_mr(Reg::R9, G_FREE_COUNT, Reg::R10);

    asm.label(".lx_fr_ret");
    asm.ret();
}

fn emit_exit(asm: &mut Asm) {
    // rcx = code. Does not return.
    asm.label("rt_exit");
    asm.mov_rr(Reg::Rdi, Reg::Rcx);
    do_syscall(asm, SYS_EXIT);
    asm.ret();
}

fn emit_read_file(asm: &mut Asm) {
    // rcx = path → rax = contents (empty string on failure)
    asm.label("rt_read_file");
    prologue(asm, 80);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_ri(Reg::Rdi, AT_FDCWD);
    asm.mov_rm_rbp(Reg::Rsi, -8);
    asm.mov_ri(Reg::Rdx, O_RDONLY);
    asm.xor_rr(Reg::R10, Reg::R10);
    do_syscall(asm, SYS_OPENAT);
    j_syscall_err(asm, ".lx_rf_fail");
    asm.mov_mr_rbp(-16, Reg::Rax); // fd

    asm.mov_rr(Reg::Rdi, Reg::Rax);
    asm.xor_rr(Reg::Rsi, Reg::Rsi);
    asm.mov_ri(Reg::Rdx, SEEK_END);
    do_syscall(asm, SYS_LSEEK);
    j_syscall_err(asm, ".lx_rf_closefail");
    asm.mov_mr_rbp(-24, Reg::Rax); // size

    asm.mov_rm_rbp(Reg::Rdi, -16);
    asm.xor_rr(Reg::Rsi, Reg::Rsi);
    asm.mov_ri(Reg::Rdx, SEEK_SET);
    do_syscall(asm, SYS_LSEEK);

    asm.mov_rm_rbp(Reg::Rcx, -24);
    asm.add_ri(Reg::Rcx, 1);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-32, Reg::Rax); // buf

    asm.mov_rm_rbp(Reg::Rdi, -16);
    asm.mov_rm_rbp(Reg::Rsi, -32);
    asm.mov_rm_rbp(Reg::Rdx, -24);
    do_syscall(asm, SYS_READ);
    j_syscall_err(asm, ".lx_rf_closefail");
    asm.mov_rm_rbp(Reg::Rdx, -32);
    asm.add_rr(Reg::Rdx, Reg::Rax);
    asm.bytes.extend_from_slice(&[0xC6, 0x02, 0x00]); // * (buf+nread) = 0

    asm.mov_rm_rbp(Reg::Rdi, -16);
    do_syscall(asm, SYS_CLOSE);
    asm.mov_rm_rbp(Reg::Rax, -32);
    epilogue(asm);

    asm.label(".lx_rf_closefail");
    asm.mov_rm_rbp(Reg::Rdi, -16);
    do_syscall(asm, SYS_CLOSE);
    asm.label(".lx_rf_fail");
    emit_empty_cstr(asm);
    epilogue(asm);
}

fn emit_write_file(asm: &mut Asm) {
    // rcx = path, rdx = contents
    asm.label("rt_write_file");
    prologue(asm, 64);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.mov_ri(Reg::Rdi, AT_FDCWD);
    asm.mov_rm_rbp(Reg::Rsi, -8);
    asm.mov_ri(Reg::Rdx, O_WRONLY | O_CREAT | O_TRUNC);
    asm.mov_ri(Reg::R10, MODE_0644);
    do_syscall(asm, SYS_OPENAT);
    j_syscall_err(asm, ".lx_wf_done");
    asm.mov_mr_rbp(-24, Reg::Rax); // fd
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-32, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rdi, -24);
    asm.mov_rm_rbp(Reg::Rsi, -16);
    asm.mov_rm_rbp(Reg::Rdx, -32);
    do_syscall(asm, SYS_WRITE);
    asm.mov_rm_rbp(Reg::Rdi, -24);
    do_syscall(asm, SYS_CLOSE);
    asm.label(".lx_wf_done");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_append_file(asm: &mut Asm) {
    asm.label("rt_append_file");
    prologue(asm, 64);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_mr_rbp(-16, Reg::Rdx);
    asm.mov_ri(Reg::Rdi, AT_FDCWD);
    asm.mov_rm_rbp(Reg::Rsi, -8);
    asm.mov_ri(Reg::Rdx, O_WRONLY | O_CREAT | O_APPEND);
    asm.mov_ri(Reg::R10, MODE_0644);
    do_syscall(asm, SYS_OPENAT);
    j_syscall_err(asm, ".lx_af_done");
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -16);
    asm.call_label("rt_strlen");
    asm.mov_mr_rbp(-32, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rdi, -24);
    asm.mov_rm_rbp(Reg::Rsi, -16);
    asm.mov_rm_rbp(Reg::Rdx, -32);
    do_syscall(asm, SYS_WRITE);
    asm.mov_rm_rbp(Reg::Rdi, -24);
    do_syscall(asm, SYS_CLOSE);
    asm.label(".lx_af_done");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_stat(asm: &mut Asm, fail: &str) {
    // rcx = path already saved? caller leaves path in rcx. Uses [rbp-176] as statbuf.
    // Caller must have a frame >= 192.
    asm.mov_ri(Reg::Rdi, AT_FDCWD);
    asm.mov_rr(Reg::Rsi, Reg::Rcx);
    lea_rbp(asm, Reg::Rdx, -176);
    asm.xor_rr(Reg::R10, Reg::R10);
    do_syscall(asm, SYS_NEWFSTATAT);
    j_syscall_err(asm, fail);
}

fn emit_file_exists(asm: &mut Asm) {
    asm.label("rt_file_exists");
    prologue(asm, 192);
    emit_stat(asm, ".lx_fe_no");
    asm.mov_ri(Reg::Rax, 1);
    epilogue(asm);
    asm.label(".lx_fe_no");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn load_st_mode(asm: &mut Asm) {
    // statbuf at [rbp-176], st_mode at +24.
    lea_rbp(asm, Reg::Rax, -176);
    asm.bytes.extend_from_slice(&[0x8B, 0x40, 0x18]); // mov eax, [rax+24]
}

fn emit_is_dir(asm: &mut Asm) {
    asm.label("rt_is_dir");
    prologue(asm, 192);
    emit_stat(asm, ".lx_id_no");
    load_st_mode(asm);
    asm.mov_ri(Reg::R11, S_IFMT);
    asm.and_rr(Reg::Rax, Reg::R11);
    asm.mov_ri(Reg::R11, S_IFDIR);
    asm.cmp_rr(Reg::Rax, Reg::R11);
    asm.jcc_label(Cc::Ne, ".lx_id_no");
    asm.mov_ri(Reg::Rax, 1);
    epilogue(asm);
    asm.label(".lx_id_no");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_is_file(asm: &mut Asm) {
    asm.label("rt_is_file");
    prologue(asm, 192);
    emit_stat(asm, ".lx_if_no");
    load_st_mode(asm);
    asm.mov_ri(Reg::R11, S_IFMT);
    asm.and_rr(Reg::Rax, Reg::R11);
    asm.mov_ri(Reg::R11, S_IFREG);
    asm.cmp_rr(Reg::Rax, Reg::R11);
    asm.jcc_label(Cc::Ne, ".lx_if_no");
    asm.mov_ri(Reg::Rax, 1);
    epilogue(asm);
    asm.label(".lx_if_no");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_remove_file(asm: &mut Asm) {
    asm.label("rt_remove_file");
    prologue(asm, 32);
    asm.mov_rr(Reg::Rdi, Reg::Rcx);
    do_syscall(asm, SYS_UNLINK);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_create_dir(asm: &mut Asm) {
    asm.label("rt_create_dir");
    prologue(asm, 32);
    asm.mov_rr(Reg::Rdi, Reg::Rcx);
    asm.mov_ri(Reg::Rsi, MODE_0755);
    do_syscall(asm, SYS_MKDIR);
    j_syscall_err(asm, ".lx_md_fail");
    asm.mov_ri(Reg::Rax, 1);
    epilogue(asm);
    asm.label(".lx_md_fail");
    asm.xor_rr(Reg::Rax, Reg::Rax);
    epilogue(asm);
}

fn emit_cwd(asm: &mut Asm) {
    asm.label("rt_cwd");
    prologue(asm, 32);
    asm.mov_ri(Reg::Rcx, 1024);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-8, Reg::Rax);
    asm.mov_rr(Reg::Rdi, Reg::Rax);
    asm.mov_ri(Reg::Rsi, 1024);
    do_syscall(asm, SYS_GETCWD);
    j_syscall_err(asm, ".lx_cwd_empty");
    asm.mov_rm_rbp(Reg::Rax, -8);
    epilogue(asm);
    asm.label(".lx_cwd_empty");
    emit_empty_cstr(asm);
    epilogue(asm);
}

fn emit_env(asm: &mut Asm) {
    // rcx = name → rax = value or empty
    asm.label("rt_env");
    prologue(asm, 48);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    load_globals(asm, Reg::R9);
    asm.mov_rm(Reg::Rax, Reg::R9, G_ENVP);
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.label(".lx_env_loop");
    asm.mov_rm_rbp(Reg::Rax, -16);
    asm.mov_rm(Reg::Rax, Reg::Rax, 0);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".lx_env_miss");
    asm.mov_mr_rbp(-24, Reg::Rax); // entry
    asm.mov_rm_rbp(Reg::Rcx, -8); // name
    asm.label(".lx_env_cmp");
    asm.bytes.extend_from_slice(&[0x0F, 0xB6, 0x01]); // movzx eax, [rcx]
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".lx_env_endname");
    asm.mov_rm_rbp(Reg::Rdx, -24);
    asm.bytes.extend_from_slice(&[0x0F, 0xB6, 0x12]); // movzx edx, [rdx]
    asm.cmp_rr(Reg::Rax, Reg::Rdx);
    asm.jcc_label(Cc::Ne, ".lx_env_next");
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0x45, 0xE8]); // inc qword [rbp-24]
    asm.bytes.extend_from_slice(&[0x48, 0xFF, 0xC1]); // inc rcx
    asm.jmp_label(".lx_env_cmp");
    asm.label(".lx_env_endname");
    asm.mov_rm_rbp(Reg::Rdx, -24);
    asm.bytes.extend_from_slice(&[0x80, 0x3A, 0x3D]); // cmp byte [rdx], '='
    asm.jcc_label(Cc::Ne, ".lx_env_next");
    asm.add_ri(Reg::Rdx, 1);
    asm.mov_rr(Reg::Rcx, Reg::Rdx);
    asm.call_label("rt_strlen");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    // rcx still points at value
    asm.call_label("rt_strndup");
    epilogue(asm);
    asm.label(".lx_env_next");
    asm.mov_rm_rbp(Reg::Rax, -16);
    asm.add_ri(Reg::Rax, 8);
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.jmp_label(".lx_env_loop");
    asm.label(".lx_env_miss");
    emit_empty_cstr(asm);
    epilogue(asm);
}

fn emit_args(asm: &mut Asm) {
    // Skip argv[0]; collect the rest as owned C strings.
    asm.label("rt_args");
    prologue(asm, 48);
    asm.mov_ri(Reg::Rcx, 8);
    asm.call_label("rt_list_new");
    asm.mov_mr_rbp(-8, Reg::Rax);
    asm.mov_ri(Reg::Rax, 1);
    asm.mov_mr_rbp(-16, Reg::Rax); // i
    asm.label(".lx_ag_loop");
    load_globals(asm, Reg::R9);
    asm.mov_rm(Reg::Rax, Reg::R9, G_ARGC);
    asm.mov_rm_rbp(Reg::Rdx, -16);
    asm.cmp_rr(Reg::Rdx, Reg::Rax);
    asm.jcc_label(Cc::Ge, ".lx_ag_done");
    asm.mov_rm(Reg::Rax, Reg::R9, G_ARGV);
    asm.shl_ri(Reg::Rdx, 3);
    asm.add_rr(Reg::Rax, Reg::Rdx);
    asm.mov_rm(Reg::Rcx, Reg::Rax, 0);
    asm.mov_mr_rbp(-24, Reg::Rcx);
    asm.call_label("rt_strlen");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -24);
    asm.call_label("rt_strndup");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -8);
    asm.call_label("rt_list_push");
    asm.mov_mr_rbp(-8, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rax, -16);
    asm.add_ri(Reg::Rax, 1);
    asm.mov_mr_rbp(-16, Reg::Rax);
    asm.jmp_label(".lx_ag_loop");
    asm.label(".lx_ag_done");
    asm.mov_rm_rbp(Reg::Rax, -8);
    epilogue(asm);
}

fn emit_list_dir(asm: &mut Asm) {
    // rcx = path → rax = sorted list of names
    asm.label("rt_list_dir");
    prologue(asm, 96);
    asm.mov_mr_rbp(-8, Reg::Rcx);
    asm.mov_ri(Reg::Rdi, AT_FDCWD);
    asm.mov_rm_rbp(Reg::Rsi, -8);
    asm.mov_ri(Reg::Rdx, O_RDONLY | O_DIRECTORY);
    asm.xor_rr(Reg::R10, Reg::R10);
    do_syscall(asm, SYS_OPENAT);
    j_syscall_err(asm, ".lx_ld_empty");
    asm.mov_mr_rbp(-16, Reg::Rax); // fd
    asm.mov_ri(Reg::Rcx, 8);
    asm.call_label("rt_list_new");
    asm.mov_mr_rbp(-24, Reg::Rax); // list
    asm.mov_ri(Reg::Rcx, 4096);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-32, Reg::Rax); // buf

    asm.label(".lx_ld_read");
    asm.mov_rm_rbp(Reg::Rdi, -16);
    asm.mov_rm_rbp(Reg::Rsi, -32);
    asm.mov_ri(Reg::Rdx, 4096);
    do_syscall(asm, SYS_GETDENTS64);
    j_syscall_err(asm, ".lx_ld_close");
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".lx_ld_close");
    asm.mov_mr_rbp(-40, Reg::Rax); // nbytes
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.mov_mr_rbp(-48, Reg::Rax); // offset

    asm.label(".lx_ld_ent");
    asm.mov_rm_rbp(Reg::Rax, -48);
    asm.mov_rm_rbp(Reg::Rdx, -40);
    asm.cmp_rr(Reg::Rax, Reg::Rdx);
    asm.jcc_label(Cc::Ge, ".lx_ld_read");
    asm.mov_rm_rbp(Reg::Rcx, -32);
    asm.add_rr(Reg::Rcx, Reg::Rax); // dirent
    asm.mov_mr_rbp(-56, Reg::Rcx);
    // reclen at +16 (u16)
    asm.bytes.extend_from_slice(&[0x0F, 0xB7, 0x41, 0x10]); // movzx eax, word [rcx+16]
    asm.mov_mr_rbp(-64, Reg::Rax); // reclen
    asm.mov_rm_rbp(Reg::Rcx, -56);
    asm.add_ri(Reg::Rcx, 19); // d_name
    // skip "." / ".."
    asm.bytes.extend_from_slice(&[0x0F, 0xB6, 0x01]);
    asm.bytes.extend_from_slice(&[0x3C, 0x2E]);
    asm.jcc_label(Cc::Ne, ".lx_ld_keep");
    asm.bytes.extend_from_slice(&[0x0F, 0xB6, 0x41, 0x01]);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".lx_ld_next");
    asm.bytes.extend_from_slice(&[0x3C, 0x2E]);
    asm.jcc_label(Cc::Ne, ".lx_ld_keep");
    asm.bytes.extend_from_slice(&[0x0F, 0xB6, 0x41, 0x02]);
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".lx_ld_next");
    asm.label(".lx_ld_keep");
    asm.mov_rm_rbp(Reg::Rcx, -56);
    asm.add_ri(Reg::Rcx, 19);
    asm.call_label("rt_strlen");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -56);
    asm.add_ri(Reg::Rcx, 19);
    asm.call_label("rt_strndup");
    asm.mov_rr(Reg::Rdx, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rcx, -24);
    asm.call_label("rt_list_push");
    asm.mov_mr_rbp(-24, Reg::Rax);
    asm.label(".lx_ld_next");
    asm.mov_rm_rbp(Reg::Rax, -48);
    asm.mov_rm_rbp(Reg::Rdx, -64);
    asm.add_rr(Reg::Rax, Reg::Rdx);
    asm.mov_mr_rbp(-48, Reg::Rax);
    asm.jmp_label(".lx_ld_ent");

    asm.label(".lx_ld_close");
    asm.mov_rm_rbp(Reg::Rdi, -16);
    do_syscall(asm, SYS_CLOSE);
    asm.mov_rm_rbp(Reg::Rcx, -24);
    asm.call_label("rt_sort_cstr_list");
    epilogue(asm);

    asm.label(".lx_ld_empty");
    asm.xor_rr(Reg::Rcx, Reg::Rcx);
    asm.call_label("rt_list_new");
    epilogue(asm);
}

fn emit_run_cmd(asm: &mut Asm) {
    // rcx = command → rax = [stdout, stderr, exit_code] or empty list on failure.
    //
    // Frame (336 bytes):
    //   -8   cmd
    //   -16  read fd
    //   -24  write fd
    //   -32  pid
    //   -40  stdout buf
    //   -48  bytes read
    //   -56  wait status
    //   -64  exit code
    //   -72  stderr string
    //   -80  result list
    //   -88  pipefd[2] (two i32)
    //   -136 "/bin/sh\0"
    //   -144 "-c\0"
    //   -176 argv[0]
    //   -168 argv[1]
    //   -160 argv[2]
    //   -152 argv[3]
    asm.label("rt_run_cmd");
    prologue(asm, 336);
    asm.mov_mr_rbp(-8, Reg::Rcx);

    let sh = b"/bin/sh\0";
    for (i, &b) in sh.iter().enumerate() {
        // rbp-136 = 0x78 with 8-bit disp? -136 < -128, use 32-bit C6 85.
        let disp = -136i32 + i as i32;
        asm.bytes.extend_from_slice(&[0xC6, 0x85]);
        asm.bytes.extend_from_slice(&disp.to_le_bytes());
        asm.bytes.push(b);
    }
    let dashc = b"-c\0";
    for (i, &b) in dashc.iter().enumerate() {
        let disp = -144i32 + i as i32;
        asm.bytes.extend_from_slice(&[0xC6, 0x85]);
        asm.bytes.extend_from_slice(&disp.to_le_bytes());
        asm.bytes.push(b);
    }

    lea_rbp(asm, Reg::Rax, -136);
    asm.mov_mr_rbp(-176, Reg::Rax);
    lea_rbp(asm, Reg::Rax, -144);
    asm.mov_mr_rbp(-168, Reg::Rax);
    asm.mov_rm_rbp(Reg::Rax, -8);
    asm.mov_mr_rbp(-160, Reg::Rax);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.mov_mr_rbp(-152, Reg::Rax);

    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.mov_mr_rbp(-88, Reg::Rax);
    lea_rbp(asm, Reg::Rdi, -88);
    asm.xor_rr(Reg::Rsi, Reg::Rsi);
    do_syscall(asm, SYS_PIPE2);
    j_syscall_err(asm, ".lx_rc_fail");
    movsxd_rbp(asm, Reg::Rax, -88);
    asm.mov_mr_rbp(-16, Reg::Rax);
    movsxd_rbp(asm, Reg::Rax, -84);
    asm.mov_mr_rbp(-24, Reg::Rax);

    do_syscall(asm, SYS_FORK);
    j_syscall_err(asm, ".lx_rc_fail");
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".lx_rc_child");
    asm.mov_mr_rbp(-32, Reg::Rax);

    asm.mov_rm_rbp(Reg::Rdi, -24);
    do_syscall(asm, SYS_CLOSE);

    asm.mov_ri(Reg::Rcx, 65536);
    asm.call_label("rt_alloc");
    asm.mov_mr_rbp(-40, Reg::Rax);
    asm.xor_rr(Reg::Rax, Reg::Rax);
    asm.mov_mr_rbp(-48, Reg::Rax);

    asm.label(".lx_rc_read");
    asm.mov_rm_rbp(Reg::Rdi, -16);
    asm.mov_rm_rbp(Reg::Rsi, -40);
    asm.mov_rm_rbp(Reg::Rax, -48);
    asm.add_rr(Reg::Rsi, Reg::Rax);
    asm.mov_ri(Reg::Rdx, 65535);
    asm.sub_rr(Reg::Rdx, Reg::Rax);
    asm.test_rr(Reg::Rdx, Reg::Rdx);
    asm.jcc_label(Cc::Le, ".lx_rc_readdone");
    do_syscall(asm, SYS_READ);
    j_syscall_err(asm, ".lx_rc_readdone");
    asm.test_rr(Reg::Rax, Reg::Rax);
    asm.jcc_label(Cc::Z, ".lx_rc_readdone");
    asm.mov_rm_rbp(Reg::Rdx, -48);
    asm.add_rr(Reg::Rdx, Reg::Rax);
    asm.mov_mr_rbp(-48, Reg::Rdx);
    asm.jmp_label(".lx_rc_read");

    asm.label(".lx_rc_readdone");
    asm.mov_rm_rbp(Reg::Rax, -40);
    asm.mov_rm_rbp(Reg::Rdx, -48);
    asm.add_rr(Reg::Rax, Reg::Rdx);
    asm.bytes.extend_from_slice(&[0xC6, 0x00, 0x00]);

    asm.mov_rm_rbp(Reg::Rdi, -16);
    do_syscall(asm, SYS_CLOSE);

    asm.mov_rm_rbp(Reg::Rdi, -32);
    lea_rbp(asm, Reg::Rsi, -56);
    asm.xor_rr(Reg::Rdx, Reg::Rdx);
    asm.xor_rr(Reg::R10, Reg::R10);
    do_syscall(asm, SYS_WAIT4);
    // (status >> 8) & 0xff
    asm.bytes.extend_from_slice(&[0x8B, 0x45, 0xC8]); // mov eax, [rbp-56]  -56=0xC8
    asm.bytes.extend_from_slice(&[0xC1, 0xE8, 0x08]); // shr eax, 8
    asm.bytes.extend_from_slice(&[0x25, 0xFF, 0x00, 0x00, 0x00]);
    asm.mov_mr_rbp(-64, Reg::Rax);

    emit_empty_cstr(asm);
    asm.mov_mr_rbp(-72, Reg::Rax);

    asm.mov_ri(Reg::Rcx, 3);
    asm.call_label("rt_list_new");
    asm.mov_mr_rbp(-80, Reg::Rax);

    asm.mov_rm_rbp(Reg::Rcx, -80);
    asm.mov_rm_rbp(Reg::Rdx, -40);
    asm.call_label("rt_list_push");
    asm.mov_mr_rbp(-80, Reg::Rax);

    asm.mov_rm_rbp(Reg::Rcx, -80);
    asm.mov_rm_rbp(Reg::Rdx, -72);
    asm.call_label("rt_list_push");
    asm.mov_mr_rbp(-80, Reg::Rax);

    asm.mov_rm_rbp(Reg::Rcx, -80);
    asm.mov_rm_rbp(Reg::Rdx, -64);
    asm.call_label("rt_list_push");
    asm.mov_rm_rbp(Reg::Rax, -80);
    epilogue(asm);

    asm.label(".lx_rc_child");
    asm.mov_rm_rbp(Reg::Rdi, -24);
    asm.mov_ri(Reg::Rsi, 1);
    do_syscall(asm, SYS_DUP2);
    asm.mov_rm_rbp(Reg::Rdi, -24);
    asm.mov_ri(Reg::Rsi, 2);
    do_syscall(asm, SYS_DUP2);
    asm.mov_rm_rbp(Reg::Rdi, -16);
    do_syscall(asm, SYS_CLOSE);
    asm.mov_rm_rbp(Reg::Rdi, -24);
    do_syscall(asm, SYS_CLOSE);
    lea_rbp(asm, Reg::Rdi, -136);
    lea_rbp(asm, Reg::Rsi, -176);
    load_globals(asm, Reg::R9);
    asm.mov_rm(Reg::Rdx, Reg::R9, G_ENVP);
    do_syscall(asm, SYS_EXECVE);
    asm.mov_ri(Reg::Rdi, 127);
    do_syscall(asm, SYS_EXIT);

    asm.label(".lx_rc_fail");
    asm.xor_rr(Reg::Rcx, Reg::Rcx);
    asm.call_label("rt_list_new");
    epilogue(asm);
}
