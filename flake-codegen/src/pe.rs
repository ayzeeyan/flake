//! Minimal PE32+ writer for generated x86-64 code.

use crate::emit::{Compiled, IMPORTS};

const IMAGE_BASE: u64 = 0x0000_0001_4000_0000;
const FILE_ALIGN: u32 = 0x200;
const SECT_ALIGN: u32 = 0x1000;

fn align(n: u32, a: u32) -> u32 {
    (n + a - 1) & !(a - 1)
}

pub fn write_pe(compiled: &Compiled) -> Vec<u8> {
    let mut rdata = Vec::new();
    let n = IMPORTS.len();
    let iat_off = 0u32;
    rdata.extend(std::iter::repeat_n(0u8, (n + 1) * 8));
    let int_off = rdata.len() as u32;
    rdata.extend(std::iter::repeat_n(0u8, (n + 1) * 8));

    let mut hint_offs = Vec::new();
    for name in IMPORTS {
        if rdata.len() % 2 == 1 {
            rdata.push(0);
        }
        hint_offs.push(rdata.len() as u32);
        rdata.extend_from_slice(&0u16.to_le_bytes());
        rdata.extend_from_slice(name.as_bytes());
        rdata.push(0);
    }
    if rdata.len() % 2 == 1 {
        rdata.push(0);
    }
    let dll_off = rdata.len() as u32;
    rdata.extend_from_slice(b"KERNEL32.dll\0");
    while rdata.len() % 4 != 0 {
        rdata.push(0);
    }
    let import_desc_off = rdata.len() as u32;
    // two IMAGE_IMPORT_DESCRIPTOR (20 bytes each), second is null
    rdata.extend(std::iter::repeat_n(0u8, 40));

    let mut str_offs = Vec::new();
    for s in &compiled.strings {
        str_offs.push(rdata.len() as u32);
        rdata.extend_from_slice(s);
    }

    let text_rva = SECT_ALIGN;
    let text_raw = FILE_ALIGN * 2; // 0x400
    let text_vsize = align(compiled.code.len() as u32, SECT_ALIGN);
    let text_raw_size = align(compiled.code.len() as u32, FILE_ALIGN);
    let rdata_rva = text_rva + text_vsize;
    let rdata_raw = text_raw + text_raw_size;
    let rdata_raw_size = align(rdata.len() as u32, FILE_ALIGN);
    let size_of_headers = text_raw;
    let size_of_image = align(
        rdata_rva + align(rdata.len() as u32, SECT_ALIGN),
        SECT_ALIGN,
    );

    // Fill INT and IAT with RVAs of hint/name.
    for (i, hint) in hint_offs.iter().enumerate() {
        let rva = (rdata_rva + *hint) as u64;
        let off_iat = iat_off as usize + i * 8;
        let off_int = int_off as usize + i * 8;
        rdata[off_iat..off_iat + 8].copy_from_slice(&rva.to_le_bytes());
        rdata[off_int..off_int + 8].copy_from_slice(&rva.to_le_bytes());
    }
    // Import descriptor
    let mut desc = Vec::new();
    desc.extend_from_slice(&(rdata_rva + int_off).to_le_bytes()); // OriginalFirstThunk
    desc.extend_from_slice(&0u32.to_le_bytes());
    desc.extend_from_slice(&0u32.to_le_bytes());
    desc.extend_from_slice(&(rdata_rva + dll_off).to_le_bytes());
    desc.extend_from_slice(&(rdata_rva + iat_off).to_le_bytes()); // FirstThunk
    rdata[import_desc_off as usize..import_desc_off as usize + 20].copy_from_slice(&desc);

    let mut code = compiled.code.clone();
    for &(at, imp) in &compiled.iat_patches {
        let insn_end_rva = text_rva + at as u32 + 4;
        let iat_entry = rdata_rva + iat_off + (imp as u32) * 8;
        let rel = iat_entry as i32 - insn_end_rva as i32;
        code[at..at + 4].copy_from_slice(&rel.to_le_bytes());
    }
    for &(at, sidx) in &compiled.str_patches {
        let insn_end_rva = text_rva + at as u32 + 4;
        let s_rva = rdata_rva + str_offs[sidx];
        let rel = s_rva as i32 - insn_end_rva as i32;
        code[at..at + 4].copy_from_slice(&rel.to_le_bytes());
    }

    let mut buf = vec![0u8; (rdata_raw + rdata_raw_size) as usize];

    // DOS header
    buf[0] = b'M';
    buf[1] = b'Z';
    buf[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    // PE at 0x80
    buf[0x80..0x84].copy_from_slice(b"PE\0\0");
    // COFF
    let coff = 0x84;
    buf[coff..coff + 2].copy_from_slice(&0x8664u16.to_le_bytes()); // AMD64
    buf[coff + 2..coff + 4].copy_from_slice(&2u16.to_le_bytes()); // sections
    buf[coff + 16..coff + 18].copy_from_slice(&0x00F0u16.to_le_bytes()); // opt size
    buf[coff + 18..coff + 20].copy_from_slice(&0x0022u16.to_le_bytes()); // executable, large addr

    let opt = coff + 20; // 0x98
    buf[opt..opt + 2].copy_from_slice(&0x020Bu16.to_le_bytes()); // PE32+
    buf[opt + 16..opt + 20].copy_from_slice(&(text_rva + compiled.entry as u32).to_le_bytes());
    buf[opt + 24..opt + 32].copy_from_slice(&IMAGE_BASE.to_le_bytes());
    buf[opt + 32..opt + 36].copy_from_slice(&SECT_ALIGN.to_le_bytes());
    buf[opt + 36..opt + 40].copy_from_slice(&FILE_ALIGN.to_le_bytes());
    buf[opt + 40..opt + 42].copy_from_slice(&6u16.to_le_bytes()); // major OS
    buf[opt + 48..opt + 50].copy_from_slice(&6u16.to_le_bytes()); // major subsys
    buf[opt + 56..opt + 60].copy_from_slice(&size_of_image.to_le_bytes());
    buf[opt + 60..opt + 64].copy_from_slice(&size_of_headers.to_le_bytes());
    buf[opt + 68..opt + 70].copy_from_slice(&3u16.to_le_bytes()); // console
    buf[opt + 70..opt + 72].copy_from_slice(&0x0100u16.to_le_bytes()); // NX
    buf[opt + 72..opt + 80].copy_from_slice(&0x0010_0000u64.to_le_bytes()); // stack reserve
    buf[opt + 80..opt + 88].copy_from_slice(&0x1000u64.to_le_bytes());
    buf[opt + 88..opt + 96].copy_from_slice(&0x0010_0000u64.to_le_bytes()); // heap reserve
    buf[opt + 96..opt + 104].copy_from_slice(&0x1000u64.to_le_bytes());
    buf[opt + 108..opt + 112].copy_from_slice(&16u32.to_le_bytes());
    // DataDirectory[1] import
    let dd = opt + 112;
    buf[dd + 8..dd + 12].copy_from_slice(&(rdata_rva + import_desc_off).to_le_bytes());
    buf[dd + 12..dd + 16].copy_from_slice(&40u32.to_le_bytes());
    // DataDirectory[12] IAT
    buf[dd + 96..dd + 100].copy_from_slice(&(rdata_rva + iat_off).to_le_bytes());
    buf[dd + 100..dd + 104].copy_from_slice(&(((n + 1) * 8) as u32).to_le_bytes());

    // Section table at opt+112+16*8 = opt+240 = 0x98+240 = 0x188
    let sect = opt + 240;
    write_section(
        &mut buf[sect..],
        b".text\0\0\0",
        text_vsize,
        text_rva,
        text_raw_size,
        text_raw,
        0x6000_0020,
    );
    write_section(
        &mut buf[sect + 40..],
        b".rdata\0\0",
        align(rdata.len() as u32, SECT_ALIGN),
        rdata_rva,
        rdata_raw_size,
        rdata_raw,
        0x4000_0040,
    );

    buf[text_raw as usize..text_raw as usize + compiled.code.len()].copy_from_slice(&code);
    buf[rdata_raw as usize..rdata_raw as usize + rdata.len()].copy_from_slice(&rdata);
    buf
}

fn write_section(
    buf: &mut [u8],
    name: &[u8],
    vsize: u32,
    rva: u32,
    raw_size: u32,
    raw_ptr: u32,
    chr: u32,
) {
    buf[..name.len()].copy_from_slice(name);
    buf[8..12].copy_from_slice(&vsize.to_le_bytes());
    buf[12..16].copy_from_slice(&rva.to_le_bytes());
    buf[16..20].copy_from_slice(&raw_size.to_le_bytes());
    buf[20..24].copy_from_slice(&raw_ptr.to_le_bytes());
    buf[36..40].copy_from_slice(&chr.to_le_bytes());
}
