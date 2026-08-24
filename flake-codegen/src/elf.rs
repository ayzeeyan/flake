//! Minimal, standalone ELF64 executable writer for x86-64 and AArch64 Linux.
//!
//! Generates valid, static ELF binaries without external tools or linkers.

use crate::emit::Compiled;
use crate::target::TargetArch;

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ELFOSABI_NONE: u8 = 0;

const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const EM_AARCH64: u16 = 183;

const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

const PAGE_SIZE: u64 = 0x1000;
const BASE_VADDR: u64 = 0x400000;

fn align_up(val: u64, align: u64) -> u64 {
    (val + align - 1) & !(align - 1)
}

/// Encode a compiled module into a 64-bit ELF executable binary.
pub fn write_elf(compiled: &Compiled, arch: TargetArch) -> Vec<u8> {
    let mut rdata = Vec::new();
    let mut str_offs = Vec::new();
    for s in &compiled.strings {
        str_offs.push(rdata.len() as u64);
        rdata.extend_from_slice(s);
    }

    let ehdr_size = 64u64;
    let phdr_size = 56u64;
    let phdr_count = 2u64; // Segment 0: Code (.text), Segment 1: Data (.rodata)
    let headers_size = ehdr_size + phdr_size * phdr_count;

    // Code begins right after program headers, aligned to 16 bytes.
    let code_offset = align_up(headers_size, 16);
    let code_vaddr = BASE_VADDR + code_offset;
    let entry_vaddr = code_vaddr + compiled.entry as u64;

    let text_filesz = code_offset + compiled.code.len() as u64;
    let text_memsz = text_filesz;

    let data_offset = align_up(text_filesz, PAGE_SIZE);
    let data_vaddr = BASE_VADDR + data_offset;
    let data_filesz = rdata.len() as u64;
    let data_memsz = data_filesz;

    // Patch RIP-relative string references in machine code.
    let mut code = compiled.code.clone();
    for &(at, sidx) in &compiled.str_patches {
        if sidx < str_offs.len() {
            let str_target_vaddr = data_vaddr + str_offs[sidx];
            let insn_next_ip = code_vaddr + at as u64 + 4;
            let rel = (str_target_vaddr as i64) - (insn_next_ip as i64);
            let rel32 = rel as i32;
            code[at..at + 4].copy_from_slice(&rel32.to_le_bytes());
        }
    }

    let total_file_size = if data_filesz > 0 {
        data_offset + data_filesz
    } else {
        text_filesz
    };
    let mut buf = vec![0u8; total_file_size as usize];

    // --- ELF Header (64 bytes) ---
    buf[0..4].copy_from_slice(&ELF_MAGIC);
    buf[4] = ELFCLASS64;
    buf[5] = ELFDATA2LSB;
    buf[6] = EV_CURRENT;
    buf[7] = ELFOSABI_NONE;
    buf[8] = 0; // ABI Version
    // Bytes 9..16 are padding

    let machine = match arch {
        TargetArch::X86_64 => EM_X86_64,
        TargetArch::Aarch64 => EM_AARCH64,
    };

    buf[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
    buf[18..20].copy_from_slice(&machine.to_le_bytes());
    buf[20..24].copy_from_slice(&(1u32).to_le_bytes()); // EV_CURRENT
    buf[24..32].copy_from_slice(&entry_vaddr.to_le_bytes()); // e_entry
    buf[32..40].copy_from_slice(&ehdr_size.to_le_bytes()); // e_phoff (64)
    buf[40..48].copy_from_slice(&(0u64).to_le_bytes()); // e_shoff (no section headers)
    buf[48..52].copy_from_slice(&(0u32).to_le_bytes()); // e_flags
    buf[52..54].copy_from_slice(&(ehdr_size as u16).to_le_bytes()); // e_ehsize (64)
    buf[54..56].copy_from_slice(&(phdr_size as u16).to_le_bytes()); // e_phentsize (56)
    buf[56..58].copy_from_slice(&(phdr_count as u16).to_le_bytes()); // e_phnum (2)
    buf[58..60].copy_from_slice(&(0u16).to_le_bytes()); // e_shentsize
    buf[60..62].copy_from_slice(&(0u16).to_le_bytes()); // e_shnum
    buf[62..64].copy_from_slice(&(0u16).to_le_bytes()); // e_shstrndx

    // --- Program Header 0: Text Segment (.text + headers) ---
    let ph0_off = ehdr_size as usize;
    buf[ph0_off..ph0_off + 4].copy_from_slice(&PT_LOAD.to_le_bytes()); // p_type
    buf[ph0_off + 4..ph0_off + 8].copy_from_slice(&(PF_R | PF_X).to_le_bytes()); // p_flags
    buf[ph0_off + 8..ph0_off + 16].copy_from_slice(&(0u64).to_le_bytes()); // p_offset
    buf[ph0_off + 16..ph0_off + 24].copy_from_slice(&BASE_VADDR.to_le_bytes()); // p_vaddr
    buf[ph0_off + 24..ph0_off + 32].copy_from_slice(&BASE_VADDR.to_le_bytes()); // p_paddr
    buf[ph0_off + 32..ph0_off + 40].copy_from_slice(&text_filesz.to_le_bytes()); // p_filesz
    buf[ph0_off + 40..ph0_off + 48].copy_from_slice(&text_memsz.to_le_bytes()); // p_memsz
    buf[ph0_off + 48..ph0_off + 56].copy_from_slice(&PAGE_SIZE.to_le_bytes()); // p_align

    // --- Program Header 1: Data Segment (.rodata) ---
    let ph1_off = ph0_off + phdr_size as usize;
    buf[ph1_off..ph1_off + 4].copy_from_slice(&PT_LOAD.to_le_bytes()); // p_type
    buf[ph1_off + 4..ph1_off + 8].copy_from_slice(&(PF_R | PF_W).to_le_bytes()); // p_flags
    buf[ph1_off + 8..ph1_off + 16].copy_from_slice(&data_offset.to_le_bytes()); // p_offset
    buf[ph1_off + 16..ph1_off + 24].copy_from_slice(&data_vaddr.to_le_bytes()); // p_vaddr
    buf[ph1_off + 24..ph1_off + 32].copy_from_slice(&data_vaddr.to_le_bytes()); // p_paddr
    buf[ph1_off + 32..ph1_off + 40].copy_from_slice(&data_filesz.to_le_bytes()); // p_filesz
    buf[ph1_off + 40..ph1_off + 48].copy_from_slice(&data_memsz.to_le_bytes()); // p_memsz
    buf[ph1_off + 48..ph1_off + 56].copy_from_slice(&PAGE_SIZE.to_le_bytes()); // p_align

    // --- Code Copy ---
    let code_start = code_offset as usize;
    buf[code_start..code_start + code.len()].copy_from_slice(&code);

    // --- Data Copy ---
    if data_filesz > 0 {
        let data_start = data_offset as usize;
        buf[data_start..data_start + rdata.len()].copy_from_slice(&rdata);
    }

    buf
}
