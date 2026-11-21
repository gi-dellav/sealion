use std::ptr;

use object::{
    Architecture, Endianness, Object, ObjectKind, RelocationEncoding, read::elf::ElfFile64,
};

use crate::Error;

pub(super) trait Arch {
    const NAME: &'static str;
    const VENEER_SIZE: usize;

    fn validate(file: &ElfFile64<'_>) -> Result<(), Error>;

    fn write_absolute(
        base: *mut u8,
        offset: usize,
        value: u64,
        size: u8,
        encoding: RelocationEncoding,
    ) -> Result<(), Error>;

    fn write_pcrel(
        base: *mut u8,
        offset: usize,
        value: i64,
        size: u8,
        encoding: RelocationEncoding,
    ) -> Result<(), Error>;

    fn write_veneer(base: *mut u8, offset: usize, target: usize);

    fn flush_icache(base: *const u8, len: usize);
}

pub(super) struct X86_64;

impl Arch for X86_64 {
    const NAME: &'static str = "x86-64";
    const VENEER_SIZE: usize = 12;

    fn validate(file: &ElfFile64<'_>) -> Result<(), Error> {
        if file.kind() != ObjectKind::Relocatable {
            return Err(Error::Jit(format!(
                "expected a relocatable object, found {:?}",
                file.kind()
            )));
        }
        if file.architecture() != Architecture::X86_64 {
            return Err(Error::Jit(format!(
                "expected an x86-64 object, found {:?}",
                file.architecture()
            )));
        }
        if !file.is_64() {
            return Err(Error::Jit("expected a 64-bit object".to_string()));
        }
        if file.endianness() != Endianness::Little {
            return Err(Error::Jit(
                "expected a little-endian x86-64 object".to_string(),
            ));
        }
        Ok(())
    }

    fn write_absolute(
        base: *mut u8,
        offset: usize,
        value: u64,
        size: u8,
        encoding: RelocationEncoding,
    ) -> Result<(), Error> {
        match (size, encoding) {
            (64, _) => unsafe {
                ptr::write_unaligned(base.add(offset).cast::<u64>(), value);
            },
            (32, RelocationEncoding::X86Signed) => {
                let value = i32::try_from(value as i64).map_err(|_| {
                    Error::Jit(format!(
                        "absolute 32-bit relocation value {value:#x} does not fit"
                    ))
                })?;
                unsafe {
                    ptr::write_unaligned(base.add(offset).cast::<i32>(), value);
                }
            }
            (32, _) => {
                let value = u32::try_from(value).map_err(|_| {
                    Error::Jit(format!(
                        "absolute 32-bit relocation value {value:#x} does not fit"
                    ))
                })?;
                unsafe {
                    ptr::write_unaligned(base.add(offset).cast::<u32>(), value);
                }
            }
            _ => {
                return Err(Error::Jit(format!(
                    "unsupported {0} absolute relocation size {size}",
                    Self::NAME
                )));
            }
        }
        Ok(())
    }

    fn write_pcrel(
        base: *mut u8,
        offset: usize,
        value: i64,
        size: u8,
        encoding: RelocationEncoding,
    ) -> Result<(), Error> {
        let supported = matches!(
            encoding,
            RelocationEncoding::Generic
                | RelocationEncoding::X86Branch
                | RelocationEncoding::X86RipRelative
                | RelocationEncoding::X86RipRelativeMovq
        );
        if size != 32 || !supported {
            return Err(Error::Jit(format!(
                "unsupported {0} PC-relative relocation size {size} encoding {encoding:?}",
                Self::NAME
            )));
        }
        let value = i32::try_from(value).map_err(|_| {
            Error::Jit(format!(
                "PC-relative relocation displacement {value} does not fit in 32 bits"
            ))
        })?;
        unsafe {
            ptr::write_unaligned(base.add(offset).cast::<i32>(), value);
        }
        Ok(())
    }

    fn write_veneer(base: *mut u8, offset: usize, target: usize) {
        let mut bytes = [0u8; Self::VENEER_SIZE];
        bytes[0] = 0x48;
        bytes[1] = 0xb8;
        bytes[2..10].copy_from_slice(&(target as u64).to_le_bytes());
        bytes[10] = 0xff;
        bytes[11] = 0xe0;
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), base.add(offset), bytes.len());
        }
    }

    fn flush_icache(_base: *const u8, _len: usize) {}
}
