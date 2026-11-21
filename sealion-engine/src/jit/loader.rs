use std::collections::{HashMap, HashSet};

use object::{
    Object, ObjectSection, ObjectSymbol, Relocation, RelocationEncoding, RelocationKind,
    RelocationTarget, SectionFlags, SectionIndex, SectionKind, SymbolIndex, SymbolKind,
    SymbolSection,
    elf::{SHF_ALLOC, SHF_EXECINSTR, SHF_TLS, SHF_WRITE},
    read::elf::ElfFile64,
};

use crate::Error;

use super::arch::Arch;

/// A fully linked image, produced during precompilation.
///
/// All relocations between sections of this object have been resolved. The only
/// work left for the loader is to copy the image into executable memory, apply
/// external symbol fixups, and publish it.
pub(super) struct LinkedImage {
    pub image: Vec<u8>,
    pub text_len: usize,
    pub data_offset: usize,
    pub entry: usize,
    pub functions: Vec<LinkedFunction>,
    pub imports: Vec<ImportFixup>,
    pub base_fixups: Vec<BaseFixup>,
}

pub(super) struct LinkedFunction {
    pub name: String,
    pub start: u32,
    pub length: u32,
}

/// An external symbol reference that must be resolved at load time.
pub(super) struct ImportFixup {
    pub name: String,
    pub offset: u32,
    pub addend: i64,
    pub size: u8,
    pub encoding: RelocationEncoding,
    /// When true, `offset` points at a veneer slot in the text section and the
    /// loader writes a jump to the external symbol there.
    pub veneer: bool,
}

/// A base-relative absolute relocation. The image stores the offset from the
/// start of the image and the loader adds the final load address.
pub(super) struct BaseFixup {
    pub offset: u32,
    pub size: u8,
    pub encoding: RelocationEncoding,
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
    got_offsets: HashMap<SymbolIndex, usize>,
    sections: HashMap<SectionIndex, SectionLayout>,
    text_len: usize,
    total_len: usize,
}

enum SymbolValue {
    ImageOffset(usize),
    Absolute(u64),
    External(String),
}

pub(super) fn link<A: Arch>(object: &[u8], page: usize) -> Result<LinkedImage, Error> {
    let file = ElfFile64::parse(object)
        .map_err(|e| Error::Loader(format!("failed to parse object: {e}")))?;
    A::validate(&file)?;

    if page == 0 || !page.is_power_of_two() {
        return Err(error(format!("invalid page size {page}")));
    }

    let infos = collect_sections(&file, page)?;
    let veneers = collect_veneers(&file, &infos)?;
    let got_symbols = collect_got_symbols(&file, &infos)?;
    let image_layout = layout::<A>(&infos, &veneers, &got_symbols, page)?;
    let text_len = image_layout.text_len;
    let total_len = image_layout.total_len;
    let layouts = image_layout.sections;
    let veneer_offsets = image_layout.veneer_offsets;
    let got_offsets = image_layout.got_offsets;

    let mut image = vec![0u8; total_len];
    copy_sections(&file, &infos, &layouts, &mut image)?;

    let symbols = collect_symbols(&file, &layouts)?;
    let entry = find_main(&file, &layouts, &symbols)?;

    let mut imports = Vec::new();
    let mut base_fixups = Vec::new();
    fill_got::<A>(
        &symbols,
        &got_offsets,
        &mut image,
        &mut imports,
        &mut base_fixups,
    )?;
    apply_relocations::<A>(
        &file,
        &infos,
        &layouts,
        &symbols,
        &veneer_offsets,
        &got_offsets,
        &mut image,
        &mut imports,
        &mut base_fixups,
    )?;

    append_veneer_imports(&symbols, &veneer_offsets, &mut imports)?;
    imports.sort_by_key(|import| import.offset);
    base_fixups.sort_by_key(|fixup| fixup.offset);

    let functions = collect_functions(&file, &layouts, text_len)?;
    if functions.is_empty() {
        return Err(error("object has no function symbols"));
    }

    Ok(LinkedImage {
        image,
        text_len,
        data_offset: text_len,
        entry,
        functions,
        imports,
        base_fixups,
    })
}

fn collect_sections(file: &ElfFile64<'_>, page: usize) -> Result<Vec<SectionInfo>, Error> {
    let mut infos = Vec::new();
    for section in file.sections() {
        let flags = match section.flags() {
            SectionFlags::Elf { sh_flags } => sh_flags,
            _ => return Err(error("loader requires ELF sections")),
        };
        if flags & u64::from(SHF_ALLOC) == 0 {
            continue;
        }
        if flags & u64::from(SHF_TLS) != 0 {
            return Err(error(format!(
                "TLS section {:?} is not supported",
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

fn collect_got_symbols(
    file: &ElfFile64<'_>,
    infos: &[SectionInfo],
) -> Result<Vec<SymbolIndex>, Error> {
    let mut required = HashSet::new();
    for info in infos {
        let section = file
            .section_by_index(info.index)
            .map_err(|e| error(format!("failed to read section: {e}")))?;
        for (_, relocation) in section.relocations() {
            if relocation.kind() != RelocationKind::GotRelative {
                continue;
            }
            let RelocationTarget::Symbol(index) = relocation.target() else {
                continue;
            };
            required.insert(index);
        }
    }

    let mut required: Vec<_> = required.into_iter().collect();
    required.sort_by_key(|index| index.0);
    Ok(required)
}

fn layout<A: Arch>(
    infos: &[SectionInfo],
    veneers: &[SymbolIndex],
    got_symbols: &[SymbolIndex],
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
        return Err(error("object has no executable section"));
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
            .ok_or_else(|| error("image layout overflow"))?;
    }

    let mut veneer_offsets = HashMap::new();
    let mut veneer_cursor = align_up(cursor, 16)?;
    for index in veneers {
        veneer_offsets.insert(*index, veneer_cursor);
        veneer_cursor = veneer_cursor
            .checked_add(A::VENEER_SIZE)
            .ok_or_else(|| error("veneer layout overflow"))?;
    }

    let text_len = page_align(veneer_cursor, page)?;
    let mut got_offsets = HashMap::new();
    let mut cursor = text_len;
    for index in got_symbols {
        cursor = align_up(cursor, 8)?;
        got_offsets.insert(*index, cursor);
        cursor = cursor
            .checked_add(8)
            .ok_or_else(|| error("GOT layout overflow"))?;
    }
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
            .ok_or_else(|| error("image layout overflow"))?;
    }
    let total_len = page_align(cursor, page)?;
    if total_len == 0 {
        return Err(error("image is empty"));
    }

    Ok(ImageLayout {
        veneer_offsets,
        got_offsets,
        sections: layouts,
        text_len,
        total_len,
    })
}

fn copy_sections(
    file: &ElfFile64<'_>,
    infos: &[SectionInfo],
    layouts: &HashMap<SectionIndex, SectionLayout>,
    image: &mut [u8],
) -> Result<(), Error> {
    for info in infos {
        let layout = layouts[&info.index];
        if info.nobits {
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
        image[layout.offset..layout.offset + layout.size].copy_from_slice(data);
    }
    Ok(())
}

fn collect_symbols(
    file: &ElfFile64<'_>,
    layouts: &HashMap<SectionIndex, SectionLayout>,
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
                    SymbolValue::ImageOffset(layout.offset + address),
                );
            }
            SymbolSection::Absolute => {
                symbols.insert(symbol.index(), SymbolValue::Absolute(symbol.address()));
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
        if let Some(SymbolValue::ImageOffset(address)) = symbols.get(&symbol.index()) {
            return Ok(*address);
        }
    }
    Err(error("object has no defined main symbol"))
}

fn append_veneer_imports(
    symbols: &HashMap<SymbolIndex, SymbolValue>,
    veneer_offsets: &HashMap<SymbolIndex, usize>,
    imports: &mut Vec<ImportFixup>,
) -> Result<(), Error> {
    let mut offsets: Vec<_> = veneer_offsets.iter().collect();
    offsets.sort_by_key(|(index, _)| index.0);
    for (index, offset) in offsets {
        let Some(SymbolValue::External(name)) = symbols.get(index) else {
            return Err(error("veneers can only target external symbols"));
        };
        imports.push(ImportFixup {
            name: name.clone(),
            offset: u32::try_from(*offset)
                .map_err(|_| error("veneer offset does not fit in 32 bits"))?,
            addend: 0,
            size: 0,
            encoding: RelocationEncoding::Generic,
            veneer: true,
        });
    }
    Ok(())
}

fn fill_got<A: Arch>(
    symbols: &HashMap<SymbolIndex, SymbolValue>,
    got_offsets: &HashMap<SymbolIndex, usize>,
    image: &mut [u8],
    imports: &mut Vec<ImportFixup>,
    base_fixups: &mut Vec<BaseFixup>,
) -> Result<(), Error> {
    let mut slots: Vec<_> = got_offsets.iter().collect();
    slots.sort_by_key(|(index, _)| index.0);
    for (index, offset) in slots {
        match symbols.get(index) {
            Some(SymbolValue::ImageOffset(value)) => {
                A::write_absolute(
                    image.as_mut_ptr(),
                    *offset,
                    *value as u64,
                    64,
                    RelocationEncoding::Generic,
                )?;
                base_fixups.push(BaseFixup {
                    offset: u32::try_from(*offset)
                        .map_err(|_| error("GOT offset does not fit in 32 bits"))?,
                    size: 64,
                    encoding: RelocationEncoding::Generic,
                });
            }
            Some(SymbolValue::Absolute(value)) => {
                A::write_absolute(
                    image.as_mut_ptr(),
                    *offset,
                    *value,
                    64,
                    RelocationEncoding::Generic,
                )?;
            }
            Some(SymbolValue::External(name)) => {
                imports.push(ImportFixup {
                    name: name.clone(),
                    offset: u32::try_from(*offset)
                        .map_err(|_| error("GOT offset does not fit in 32 bits"))?,
                    addend: 0,
                    size: 64,
                    encoding: RelocationEncoding::Generic,
                    veneer: false,
                });
            }
            None => {
                return Err(error(format!("GOT entry targets unknown symbol {index}")));
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_relocations<A: Arch>(
    file: &ElfFile64<'_>,
    infos: &[SectionInfo],
    layouts: &HashMap<SectionIndex, SectionLayout>,
    symbols: &HashMap<SymbolIndex, SymbolValue>,
    veneer_offsets: &HashMap<SymbolIndex, usize>,
    got_offsets: &HashMap<SymbolIndex, usize>,
    image: &mut [u8],
    imports: &mut Vec<ImportFixup>,
    base_fixups: &mut Vec<BaseFixup>,
) -> Result<(), Error> {
    for info in infos {
        let layout = layouts[&info.index];
        let section = file
            .section_by_index(info.index)
            .map_err(|e| error(format!("failed to read section: {e}")))?;
        for (offset, relocation) in section.relocations() {
            apply_relocation::<A>(
                &layout,
                offset,
                &relocation,
                layouts,
                symbols,
                veneer_offsets,
                got_offsets,
                image,
                imports,
                base_fixups,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_relocation<A: Arch>(
    layout: &SectionLayout,
    offset: u64,
    relocation: &Relocation,
    layouts: &HashMap<SectionIndex, SectionLayout>,
    symbols: &HashMap<SymbolIndex, SymbolValue>,
    veneer_offsets: &HashMap<SymbolIndex, usize>,
    got_offsets: &HashMap<SymbolIndex, usize>,
    image: &mut [u8],
    imports: &mut Vec<ImportFixup>,
    base_fixups: &mut Vec<BaseFixup>,
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

    let kind = relocation.kind();
    if kind == RelocationKind::None {
        return Ok(());
    }

    let (target, external, base_relative) = resolve_target(&relocation.target(), layouts, symbols)?;
    let addend = relocation.addend();

    match kind {
        RelocationKind::Absolute => {
            if let Some(name) = external {
                imports.push(ImportFixup {
                    name: name.to_string(),
                    offset: u32::try_from(place)
                        .map_err(|_| error("relocation place does not fit in 32 bits"))?,
                    addend,
                    size,
                    encoding: relocation.encoding(),
                    veneer: false,
                });
                return Ok(());
            }
            let value = target
                .checked_add(i128::from(addend))
                .and_then(|value| u64::try_from(value).ok())
                .ok_or_else(|| error("absolute relocation value does not fit in 64 bits"))?;
            A::write_absolute(
                image.as_mut_ptr(),
                place,
                value,
                size,
                relocation.encoding(),
            )?;
            if base_relative {
                base_fixups.push(BaseFixup {
                    offset: u32::try_from(place)
                        .map_err(|_| error("relocation place does not fit in 32 bits"))?,
                    size,
                    encoding: relocation.encoding(),
                });
            }
            Ok(())
        }
        RelocationKind::Relative | RelocationKind::PltRelative => {
            let mut target = target;
            if let Some(name) = external {
                if kind == RelocationKind::PltRelative {
                    let RelocationTarget::Symbol(index) = relocation.target() else {
                        unreachable!();
                    };
                    let veneer = veneer_offsets.get(&index).ok_or_else(|| {
                        error(format!(
                            "external PC-relative relocation to '{name}' has no veneer"
                        ))
                    })?;
                    target = *veneer as i128;
                } else {
                    return Err(error(
                        "external non-branch PC-relative relocations are not supported",
                    ));
                }
            }
            let value = target
                .checked_sub(place as i128)
                .and_then(|value| value.checked_add(i128::from(addend)))
                .and_then(|value| i64::try_from(value).ok())
                .ok_or_else(|| error("PC-relative relocation value does not fit in 64 bits"))?;
            A::write_pcrel(
                image.as_mut_ptr(),
                place,
                value,
                size,
                relocation.encoding(),
            )
        }
        RelocationKind::GotRelative => {
            let RelocationTarget::Symbol(index) = relocation.target() else {
                return Err(error("GOT relocation does not target a symbol"));
            };
            let slot = got_offsets
                .get(&index)
                .ok_or_else(|| error("GOT relocation has no slot"))?;
            let value = (*slot as i128)
                .checked_sub(place as i128)
                .and_then(|value| value.checked_add(i128::from(addend)))
                .and_then(|value| i64::try_from(value).ok())
                .ok_or_else(|| error("GOT-relative relocation value does not fit in 64 bits"))?;
            A::write_pcrel(
                image.as_mut_ptr(),
                place,
                value,
                size,
                relocation.encoding(),
            )
        }
        kind => Err(error(format!(
            "unsupported {0} relocation {kind:?}",
            A::NAME
        ))),
    }
}

fn resolve_target<'a>(
    target: &RelocationTarget,
    layouts: &HashMap<SectionIndex, SectionLayout>,
    symbols: &'a HashMap<SymbolIndex, SymbolValue>,
) -> Result<(i128, Option<&'a str>, bool), Error> {
    match target {
        RelocationTarget::Symbol(index) => match symbols.get(index) {
            Some(SymbolValue::ImageOffset(value)) => Ok((*value as i128, None, true)),
            Some(SymbolValue::Absolute(value)) => Ok((*value as i128, None, false)),
            Some(SymbolValue::External(name)) => Ok((0, Some(name.as_str()), false)),
            None => Err(error(format!("relocation targets unknown symbol {index}"))),
        },
        RelocationTarget::Section(index) => {
            let target = layouts
                .get(index)
                .ok_or_else(|| error("relocation targets a non-loaded section"))?;
            Ok((target.offset as i128, None, true))
        }
        RelocationTarget::Absolute => Ok((0, None, false)),
        _ => Err(error("unsupported relocation target")),
    }
}

fn collect_functions(
    file: &ElfFile64<'_>,
    layouts: &HashMap<SectionIndex, SectionLayout>,
    text_len: usize,
) -> Result<Vec<LinkedFunction>, Error> {
    let mut functions = Vec::new();
    for symbol in file.symbols() {
        if symbol.kind() != SymbolKind::Text || symbol.is_undefined() {
            continue;
        }
        let Ok(name) = symbol.name() else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let Some(section_index) = symbol.section_index() else {
            continue;
        };
        let Some(layout) = layouts.get(&section_index) else {
            continue;
        };
        if !layout.executable {
            continue;
        }
        let address = usize::try_from(symbol.address())
            .map_err(|_| error("function address does not fit in usize"))?;
        let length = usize::try_from(symbol.size())
            .map_err(|_| error("function size does not fit in usize"))?;
        let start = layout
            .offset
            .checked_add(address)
            .ok_or_else(|| error("function start overflow"))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| error("function end overflow"))?;
        if end > text_len || end > layout.offset + layout.size {
            return Err(error(format!(
                "function {name:?} extends outside the text section"
            )));
        }
        functions.push(LinkedFunction {
            name: name.to_string(),
            start: u32::try_from(start).map_err(|_| error("function offset overflow"))?,
            length: u32::try_from(length).map_err(|_| error("function size overflow"))?,
        });
    }
    functions.sort_by_key(|function| (function.start, function.name.clone()));
    Ok(functions)
}

fn align_up(value: usize, align: usize) -> Result<usize, Error> {
    value
        .checked_add(align - 1)
        .map(|value| value & !(align - 1))
        .ok_or_else(|| error("image alignment overflow"))
}

fn page_align(value: usize, page: usize) -> Result<usize, Error> {
    align_up(value, page)
}

fn error(message: impl Into<String>) -> Error {
    Error::Loader(message.into())
}
