use std::collections::{HashMap, HashSet};

use object::{
    Object, ObjectSection, ObjectSymbol, Relocation, RelocationKind, RelocationTarget,
    SectionFlags, SectionIndex, SectionKind, SymbolIndex, SymbolSection,
    elf::{SHF_ALLOC, SHF_EXECINSTR, SHF_TLS, SHF_WRITE},
    read::elf::ElfFile64,
};

use crate::Error;

use super::{
    arch::Arch,
    os::{Memory, Os},
};

pub(super) struct LoadedImage<M: Memory> {
    _memory: M,
    main: usize,
}

impl<M: Memory> LoadedImage<M> {
    pub(super) fn main(&self) -> usize {
        self.main
    }
}

struct SectionInfo {
    index: SectionIndex,
    align: usize,
    size: usize,
    nobits: bool,
    executable: bool,
    writable: bool,
}

#[derive(Clone, Copy)]
struct SectionLayout {
    offset: usize,
    size: usize,
    executable: bool,
}

struct ImageLayout {
    veneer_offsets: HashMap<SymbolIndex, usize>,
    sections: HashMap<SectionIndex, SectionLayout>,
    text_len: usize,
    total_len: usize,
}

enum SymbolValue {
    Address(usize),
    External(String),
}

pub(super) fn load<A: Arch, O: Os>(object: &[u8]) -> Result<LoadedImage<O::Memory>, Error> {
    let file = ElfFile64::parse(object)
        .map_err(|e| Error::Jit(format!("failed to parse JIT object: {e}")))?;
    A::validate(&file)?;

    let page = O::page_size();
    if page == 0 || !page.is_power_of_two() {
        return Err(error(format!("invalid page size {page}")));
    }

    let infos = collect_sections(&file, page)?;
    let veneers = collect_veneers(&file, &infos)?;
    let image_layout = layout::<A>(&infos, &veneers, page)?;
    let text_len = image_layout.text_len;
    let total = image_layout.total_len;
    let veneer_offsets = image_layout.veneer_offsets;
    let layouts = image_layout.sections;

    let memory = O::allocate(total)?;
    if memory.len() != total {
        return Err(error("JIT memory allocation returned an unexpected size"));
    }
    copy_sections::<O>(&file, &infos, &layouts, &memory)?;

    let base_address = memory.base() as usize;
    let symbols = collect_symbols(&file, &layouts, base_address)?;
    let main = find_main(&file, &layouts, &symbols)?;

    let mut external_cache = HashMap::new();
    write_veneers::<A, O>(&memory, &veneer_offsets, &symbols, &mut external_cache)?;
    apply_relocations::<A, O>(
        &file,
        &infos,
        &layouts,
        &symbols,
        &veneer_offsets,
        &memory,
        &mut external_cache,
    )?;

    let mut memory = memory;
    unsafe {
        memory.publish(text_len)?;
    }
    A::flush_icache(memory.base(), text_len);

    Ok(LoadedImage {
        _memory: memory,
        main,
    })
}

fn collect_sections(file: &ElfFile64<'_>, page: usize) -> Result<Vec<SectionInfo>, Error> {
    let mut infos = Vec::new();
    for section in file.sections() {
        let flags = match section.flags() {
            SectionFlags::Elf { sh_flags } => sh_flags,
            _ => return Err(error("JIT loader requires ELF sections")),
        };
        if flags & u64::from(SHF_ALLOC) == 0 {
            continue;
        }
        if flags & u64::from(SHF_TLS) != 0 {
            return Err(error(format!(
                "TLS section {:?} is not supported by the JIT loader",
                section.name().unwrap_or("<unnamed>")
            )));
        }

        let executable = flags & u64::from(SHF_EXECINSTR) != 0;
        let writable = flags & u64::from(SHF_WRITE) != 0;
        if executable && writable {
            return Err(error(format!(
                "section {:?} is both writable and executable",
                section.name().unwrap_or("<unnamed>")
            )));
        }

        let align = usize::try_from(section.align())
            .map_err(|_| error("section alignment does not fit in usize"))?
            .max(1);
        if !align.is_power_of_two() || align > page {
            return Err(error(format!(
                "unsupported alignment {align} for section {:?}",
                section.name().unwrap_or("<unnamed>")
            )));
        }
        let size = usize::try_from(section.size())
            .map_err(|_| error("section size does not fit in usize"))?;
        let nobits = matches!(
            section.kind(),
            SectionKind::UninitializedData | SectionKind::Common
        );

        infos.push(SectionInfo {
            index: section.index(),
            align,
            size,
            nobits,
            executable,
            writable,
        });
    }
    Ok(infos)
}

fn collect_veneers(file: &ElfFile64<'_>, infos: &[SectionInfo]) -> Result<Vec<SymbolIndex>, Error> {
    let mut required = HashSet::new();
    for info in infos {
        let section = file
            .section_by_index(info.index)
            .map_err(|e| error(format!("failed to read section: {e}")))?;
        for (_, relocation) in section.relocations() {
            if relocation.kind() != RelocationKind::PltRelative {
                continue;
            }
            let RelocationTarget::Symbol(index) = relocation.target() else {
                continue;
            };
            let symbol = file
                .symbol_by_index(index)
                .map_err(|e| error(format!("failed to read symbol: {e}")))?;
            if symbol.is_undefined() {
                required.insert(index);
            }
        }
    }

    let mut required: Vec<_> = required.into_iter().collect();
    required.sort_by_key(|index| index.0);
    Ok(required)
}

fn layout<A: Arch>(
    infos: &[SectionInfo],
    veneers: &[SymbolIndex],
    page: usize,
) -> Result<ImageLayout, Error> {
    if !veneers.is_empty() && A::VENEER_SIZE == 0 {
        return Err(error(format!(
            "{0} does not support external PC-relative veneers",
            A::NAME
        )));
    }

    let mut executable: Vec<_> = infos.iter().filter(|info| info.executable).collect();
    let mut other: Vec<_> = infos.iter().filter(|info| !info.executable).collect();
    executable.sort_by_key(|info| info.index.0);
    other.sort_by_key(|info| (info.writable, info.index.0));

    if executable.is_empty() {
        return Err(error("JIT object has no executable section"));
    }

    let mut layouts = HashMap::new();
    let mut cursor = 0usize;
    for info in &executable {
        cursor = align_up(cursor, info.align)?;
        layouts.insert(
            info.index,
            SectionLayout {
                offset: cursor,
                size: info.size,
                executable: true,
            },
        );
        cursor = cursor
            .checked_add(info.size)
            .ok_or_else(|| error("JIT image layout overflow"))?;
    }

    let mut veneer_offsets = HashMap::new();
    let mut veneer_cursor = align_up(cursor, 16)?;
    for index in veneers {
        veneer_offsets.insert(*index, veneer_cursor);
        veneer_cursor = veneer_cursor
            .checked_add(A::VENEER_SIZE)
            .ok_or_else(|| error("JIT veneer layout overflow"))?;
    }

    let text_len = page_align(veneer_cursor, page)?;
    let mut cursor = text_len;
    for info in &other {
        cursor = align_up(cursor, info.align)?;
        layouts.insert(
            info.index,
            SectionLayout {
                offset: cursor,
                size: info.size,
                executable: false,
            },
        );
        cursor = cursor
            .checked_add(info.size)
            .ok_or_else(|| error("JIT image layout overflow"))?;
    }
    let total = page_align(cursor, page)?;
    if total == 0 {
        return Err(error("JIT image is empty"));
    }

    Ok(ImageLayout {
        veneer_offsets,
        sections: layouts,
        text_len,
        total_len: total,
    })
}

fn copy_sections<O: Os>(
    file: &ElfFile64<'_>,
    infos: &[SectionInfo],
    layouts: &HashMap<SectionIndex, SectionLayout>,
    memory: &O::Memory,
) -> Result<(), Error> {
    let base = memory.base();
    for info in infos {
        let layout = layouts[&info.index];
        let destination = unsafe { base.add(layout.offset) };
        if info.nobits {
            unsafe {
                std::ptr::write_bytes(destination, 0, layout.size);
            }
            continue;
        }

        let section = file
            .section_by_index(info.index)
            .map_err(|e| error(format!("failed to read section: {e}")))?;
        let data = section
            .data()
            .map_err(|e| error(format!("failed to read section data: {e}")))?;
        if data.len() != layout.size {
            return Err(error(format!(
                "section {:?} has {} bytes of data but a {}-byte layout",
                section.name().unwrap_or("<unnamed>"),
                data.len(),
                layout.size
            )));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), destination, data.len());
        }
    }
    Ok(())
}

fn collect_symbols(
    file: &ElfFile64<'_>,
    layouts: &HashMap<SectionIndex, SectionLayout>,
    base_address: usize,
) -> Result<HashMap<SymbolIndex, SymbolValue>, Error> {
    let mut symbols = HashMap::new();
    for symbol in file.symbols() {
        match symbol.section() {
            SymbolSection::Section(section_index) => {
                let Some(layout) = layouts.get(&section_index) else {
                    continue;
                };
                let address = usize::try_from(symbol.address())
                    .map_err(|_| error("symbol address does not fit in usize"))?;
                if address > layout.size {
                    return Err(error(format!(
                        "symbol {:?} is outside its section",
                        symbol.name().unwrap_or("<unnamed>")
                    )));
                }
                symbols.insert(
                    symbol.index(),
                    SymbolValue::Address(base_address + layout.offset + address),
                );
            }
            SymbolSection::Absolute => {
                let address = usize::try_from(symbol.address())
                    .map_err(|_| error("absolute symbol address does not fit in usize"))?;
                symbols.insert(symbol.index(), SymbolValue::Address(address));
            }
            SymbolSection::Undefined => {
                let Ok(name) = symbol.name() else {
                    continue;
                };
                if !name.is_empty() {
                    symbols.insert(symbol.index(), SymbolValue::External(name.to_string()));
                }
            }
            _ => {}
        }
    }
    Ok(symbols)
}

fn find_main(
    file: &ElfFile64<'_>,
    layouts: &HashMap<SectionIndex, SectionLayout>,
    symbols: &HashMap<SymbolIndex, SymbolValue>,
) -> Result<usize, Error> {
    for symbol in file.symbols() {
        if symbol.name().ok() != Some("main") || symbol.is_undefined() {
            continue;
        }
        let Some(section_index) = symbol.section_index() else {
            continue;
        };
        let layout = layouts
            .get(&section_index)
            .ok_or_else(|| error("main belongs to a non-loaded section"))?;
        if !layout.executable {
            return Err(error("main is not in an executable section"));
        }
        if let Some(SymbolValue::Address(address)) = symbols.get(&symbol.index()) {
            return Ok(*address);
        }
    }
    Err(error("JIT object has no defined main symbol"))
}

fn write_veneers<A: Arch, O: Os>(
    memory: &O::Memory,
    veneer_offsets: &HashMap<SymbolIndex, usize>,
    symbols: &HashMap<SymbolIndex, SymbolValue>,
    external_cache: &mut HashMap<String, usize>,
) -> Result<(), Error> {
    let mut offsets: Vec<_> = veneer_offsets.iter().collect();
    offsets.sort_by_key(|(index, _)| index.0);
    for (index, offset) in offsets {
        let Some(SymbolValue::External(name)) = symbols.get(index) else {
            return Err(error("veneers can only target external symbols"));
        };
        let address = resolve_external::<O>(name, external_cache)?;
        A::write_veneer(memory.base(), *offset, address);
    }
    Ok(())
}

fn apply_relocations<A: Arch, O: Os>(
    file: &ElfFile64<'_>,
    infos: &[SectionInfo],
    layouts: &HashMap<SectionIndex, SectionLayout>,
    symbols: &HashMap<SymbolIndex, SymbolValue>,
    veneer_offsets: &HashMap<SymbolIndex, usize>,
    memory: &O::Memory,
    external_cache: &mut HashMap<String, usize>,
) -> Result<(), Error> {
    let base = memory.base();
    let base_address = base as usize;

    for info in infos {
        let layout = layouts[&info.index];
        let section = file
            .section_by_index(info.index)
            .map_err(|e| error(format!("failed to read section: {e}")))?;
        for (offset, relocation) in section.relocations() {
            apply_relocation::<A, O>(
                &layout,
                offset,
                &relocation,
                layouts,
                symbols,
                veneer_offsets,
                base,
                base_address,
                external_cache,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_relocation<A: Arch, O: Os>(
    layout: &SectionLayout,
    offset: u64,
    relocation: &Relocation,
    layouts: &HashMap<SectionIndex, SectionLayout>,
    symbols: &HashMap<SymbolIndex, SymbolValue>,
    veneer_offsets: &HashMap<SymbolIndex, usize>,
    base: *mut u8,
    base_address: usize,
    external_cache: &mut HashMap<String, usize>,
) -> Result<(), Error> {
    if relocation.subtractor().is_some() {
        return Err(error("symbol-subtracting relocations are not supported"));
    }
    if relocation.has_implicit_addend() {
        return Err(error("implicit relocation addends are not supported"));
    }

    let size = relocation.size();
    let width = usize::from(size) / 8;
    let offset =
        usize::try_from(offset).map_err(|_| error("relocation offset does not fit in usize"))?;
    let end = offset
        .checked_add(width)
        .ok_or_else(|| error("relocation offset overflow"))?;
    if end > layout.size {
        return Err(error("relocation is outside its section"));
    }
    let place = layout
        .offset
        .checked_add(offset)
        .ok_or_else(|| error("relocation place overflow"))?;
    let p = base_address
        .checked_add(place)
        .ok_or_else(|| error("relocation address overflow"))?;

    let (mut target, external_symbol) = match relocation.target() {
        RelocationTarget::Symbol(index) => match symbols.get(&index) {
            Some(SymbolValue::Address(address)) => (*address, None),
            Some(SymbolValue::External(name)) => {
                let address = resolve_external::<O>(name, external_cache)?;
                (address, Some(index))
            }
            None => {
                return Err(error(format!("relocation targets unknown symbol {index}")));
            }
        },
        RelocationTarget::Section(index) => {
            let target = layouts
                .get(&index)
                .ok_or_else(|| error("relocation targets a non-loaded section"))?;
            (base_address + target.offset, None)
        }
        RelocationTarget::Absolute => (0, None),
        _ => return Err(error("unsupported relocation target")),
    };

    match relocation.kind() {
        RelocationKind::None => Ok(()),
        RelocationKind::Absolute => {
            let value = (target as i128)
                .checked_add(i128::from(relocation.addend()))
                .and_then(|value| u64::try_from(value).ok())
                .ok_or_else(|| error("absolute relocation value does not fit in 64 bits"))?;
            A::write_absolute(base, place, value, size, relocation.encoding())
        }
        RelocationKind::Relative | RelocationKind::PltRelative => {
            if let Some(index) = external_symbol {
                if relocation.kind() == RelocationKind::PltRelative {
                    let veneer = veneer_offsets
                        .get(&index)
                        .ok_or_else(|| error("external PC-relative relocation has no veneer"))?;
                    target = base_address + veneer;
                } else {
                    return Err(error(
                        "external non-branch PC-relative relocations are not supported",
                    ));
                }
            }
            let value = (target as i128)
                .checked_sub(p as i128)
                .and_then(|value| value.checked_add(i128::from(relocation.addend())))
                .and_then(|value| i64::try_from(value).ok())
                .ok_or_else(|| error("PC-relative relocation value does not fit in 64 bits"))?;
            A::write_pcrel(base, place, value, size, relocation.encoding())
        }
        kind => Err(error(format!(
            "unsupported {0} relocation {kind:?}",
            A::NAME
        ))),
    }
}

fn resolve_external<O: Os>(name: &str, cache: &mut HashMap<String, usize>) -> Result<usize, Error> {
    if let Some(address) = cache.get(name) {
        return Ok(*address);
    }
    let address =
        O::lookup(name).ok_or_else(|| error(format!("unresolved external symbol '{name}'")))?;
    cache.insert(name.to_string(), address);
    Ok(address)
}

fn align_up(value: usize, align: usize) -> Result<usize, Error> {
    value
        .checked_add(align - 1)
        .map(|value| value & !(align - 1))
        .ok_or_else(|| error("JIT image alignment overflow"))
}

fn page_align(value: usize, page: usize) -> Result<usize, Error> {
    align_up(value, page)
}

fn error(message: impl Into<String>) -> Error {
    Error::Jit(message.into())
}
