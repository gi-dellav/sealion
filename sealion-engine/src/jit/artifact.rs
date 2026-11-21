//! Precompiled artifact format and loader.
//!
//! The artifact is an ELF container with a vendor marker. It carries a fully
//! linked image (`.text` and `.data`), engine compatibility metadata
//! (`.sealion.meta`), and runtime metadata including the function table
//! (`.sealion.info`). Internal relocations are resolved during precompilation;
//! the loader only resolves external symbols and rebases absolute references
//! before publishing the image.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::ptr;
use std::str::{self, FromStr};

use object::{
    Architecture, BinaryFormat, Endianness as ObjectEndianness, FileFlags, Object, ObjectKind,
    ObjectSection, RelocationEncoding, SectionFlags, SectionKind,
    elf::{SHF_ALLOC, SHF_EXECINSTR, SHF_TLS, SHF_WRITE},
    read::elf::ElfFile64,
    write,
};
use serde::{Deserialize, Serialize};
use target_lexicon::{Endianness, Triple};

use crate::{CompileOptions, Error};

use super::{
    arch::{Arch, X86_64},
    loader,
    os::{Linux, Memory, Os},
};

/// Container format version. Bump when the byte-level artifact header changes.
pub const ARTIFACT_FORMAT_VERSION: u16 = 1;
/// Metadata schema version. Bump when `ModuleInfo` changes incompatibly.
pub const METADATA_SCHEMA_VERSION: u32 = 1;

const ARTIFACT_MAGIC: [u8; 8] = *b"SEAARTF1";
const ELF_OSABI_SEALION: u8 = 0x53;
const ELF_E_FLAGS_SEALION: u32 = 0x5EA1_0000;
const ELF_KIND_MASK: u32 = 0x0000_00ff;
const OBJECT_KIND_MODULE: u8 = 1;
const MAX_IMAGE_SIZE: usize = 1 << 30;

const ENC_GENERIC: u8 = 0;
const ENC_X86_SIGNED: u8 = 1;
const ENC_X86_BRANCH: u8 = 2;
const ENC_X86_RIP_RELATIVE: u8 = 3;
const ENC_X86_RIP_RELATIVE_MOVQ: u8 = 4;

/// Stable identity for a function in a loaded module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FuncKey(pub u32);

/// Location of a function body inside the executable text section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionLoc {
    pub start: u32,
    pub length: u32,
}

/// A function table entry: stable key, source-level name, and text location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub key: FuncKey,
    pub name: String,
    pub loc: FunctionLoc,
}

/// External symbol fixup recorded during precompilation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRecord {
    pub name: String,
    pub offset: u32,
    pub addend: i64,
    pub size: u8,
    pub encoding: u8,
    pub veneer: bool,
}

/// Absolute relocation that must be rebased at load time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseFixupRecord {
    pub offset: u32,
    pub size: u8,
    pub encoding: u8,
}

/// Engine configuration recorded in the artifact for compatibility checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineMetadata {
    pub engine_version: String,
    pub target: String,
    pub pointer_width: u8,
    /// `0` for little-endian, `1` for big-endian.
    pub endianness: u8,
    pub object_kind: u8,
    pub metadata_schema_version: u32,
    pub opt_level: String,
    pub target_cpu: Option<String>,
    pub is_pic: bool,
    pub unwind_info: bool,
    pub preserve_frame_pointers: bool,
    pub enable_alias_analysis: bool,
    pub enable_verifier: bool,
    pub feature_bits: u64,
}

/// Runtime metadata needed to validate and register a loaded image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub schema_version: u32,
    pub object_kind: u8,
    pub code_alignment: u32,
    pub page_size: u32,
    pub text_len: u32,
    pub image_len: u32,
    pub data_offset: u32,
    pub entry: u32,
    pub source_checksum: u64,
    pub functions: Vec<FunctionInfo>,
    pub imports: Vec<ImportRecord>,
    pub base_fixups: Vec<BaseFixupRecord>,
}

/// A precompiled module that has been validated, published, and registered.
pub struct Module {
    memory: <Linux as Os>::Memory,
    info: ModuleInfo,
    metadata: EngineMetadata,
    artifact: Vec<u8>,
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("entry", &self.entry_offset())
            .field("text_len", &self.info.text_len)
            .field("functions", &self.info.functions.len())
            .finish()
    }
}

impl Module {
    /// Load an artifact from owned bytes.
    pub fn deserialize(_engine: &crate::Engine, bytes: &[u8]) -> Result<Self, Error> {
        load(bytes)
    }

    /// Load an artifact from a file.
    pub fn deserialize_file(
        _engine: &crate::Engine,
        path: impl AsRef<Path>,
    ) -> Result<Self, Error> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| {
            Error::Loader(format!("failed to read artifact {}: {e}", path.display()))
        })?;
        load(&bytes)
    }

    /// Load an artifact from caller-owned memory.
    ///
    /// The bytes are copied into a fresh executable mapping, so the caller may
    /// reuse or drop the buffer after this returns.
    pub fn deserialize_raw(_engine: &crate::Engine, memory: &[u8]) -> Result<Self, Error> {
        load(memory)
    }

    /// The artifact bytes that produced this module.
    pub fn serialize(&self) -> Result<Vec<u8>, Error> {
        Ok(self.artifact.clone())
    }

    pub fn artifact(&self) -> &[u8] {
        &self.artifact
    }

    pub fn engine_metadata(&self) -> &EngineMetadata {
        &self.metadata
    }

    pub fn module_info(&self) -> &ModuleInfo {
        &self.info
    }

    pub fn functions(&self) -> &[FunctionInfo] {
        &self.info.functions
    }

    pub fn function_location(&self, key: FuncKey) -> Option<FunctionLoc> {
        self.info
            .functions
            .get(key.0 as usize)
            .filter(|function| function.key == key)
            .map(|function| function.loc)
    }

    pub fn function_range(&self, key: FuncKey) -> Option<Range<usize>> {
        let loc = self.function_location(key)?;
        let start = loc.start as usize;
        let end = start.checked_add(loc.length as usize)?;
        Some(start..end)
    }

    pub fn function_address(&self, key: FuncKey) -> Option<usize> {
        let range = self.function_range(key)?;
        Some(self.memory.base() as usize + range.start)
    }

    pub fn function(&self, key: FuncKey) -> Option<&[u8]> {
        let range = self.function_range(key)?;
        Some(&self.text()[range])
    }

    pub fn function_by_name(&self, name: &str) -> Option<FuncKey> {
        self.info
            .functions
            .iter()
            .find(|function| function.name == name)
            .map(|function| function.key)
    }

    /// Reverse lookup from a text offset to a function key.
    pub fn function_at_offset(&self, offset: usize) -> Option<FuncKey> {
        let index = self
            .info
            .functions
            .partition_point(|function| function.loc.start as usize <= offset);
        let function = self.info.functions.get(index.checked_sub(1)?)?;
        let end = (function.loc.start as usize).checked_add(function.loc.length as usize)?;
        (offset < end).then_some(function.key)
    }

    /// Reverse lookup from an absolute address to a function key.
    pub fn function_at_address(&self, address: usize) -> Option<FuncKey> {
        let offset = address.checked_sub(self.memory.base() as usize)?;
        self.function_at_offset(offset)
    }

    pub fn entry_offset(&self) -> usize {
        self.info.entry as usize
    }

    pub fn entry(&self) -> usize {
        self.memory.base() as usize + self.entry_offset()
    }

    pub fn text(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.memory.base(),
                usize::try_from(self.info.text_len).unwrap_or(0),
            )
        }
    }

    /// Run the module's entry point with JS-style arguments.
    pub fn run(&self) -> i32 {
        let main: unsafe extern "C" fn(i32, i64) -> i32 =
            unsafe { std::mem::transmute(self.entry()) };
        unsafe { main(0, 0) }
    }
}

impl crate::Engine {
    /// Load a precompiled artifact from bytes.
    pub fn deserialize(&self, bytes: &[u8]) -> Result<Module, Error> {
        Module::deserialize(self, bytes)
    }

    /// Load a precompiled artifact from a file.
    pub fn deserialize_file(&self, path: impl AsRef<Path>) -> Result<Module, Error> {
        Module::deserialize_file(self, path)
    }

    /// Load a precompiled artifact from caller-owned memory.
    pub fn deserialize_raw(&self, memory: &[u8]) -> Result<Module, Error> {
        Module::deserialize_raw(self, memory)
    }
}

pub(super) fn build(
    object: &[u8],
    options: &CompileOptions,
    target: &Triple,
    source_checksum: u64,
) -> Result<Vec<u8>, Error> {
    let page = Linux::page_size();
    let linked = loader::link::<X86_64>(object, page)?;

    let metadata = EngineMetadata {
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        target: target.to_string(),
        pointer_width: target
            .pointer_width()
            .map(|width| width.bits())
            .unwrap_or(64),
        endianness: match target.endianness() {
            Ok(Endianness::Little) => 0,
            Ok(Endianness::Big) => 1,
            Err(_) => 0,
        },
        object_kind: OBJECT_KIND_MODULE,
        metadata_schema_version: METADATA_SCHEMA_VERSION,
        opt_level: options.opt_level.as_str().to_string(),
        target_cpu: options.target_cpu.clone(),
        is_pic: options.is_pic,
        unwind_info: options.unwind_info,
        preserve_frame_pointers: options.preserve_frame_pointers,
        enable_alias_analysis: options.enable_alias_analysis,
        enable_verifier: options.enable_verifier,
        feature_bits: 0,
    };

    let functions: Vec<FunctionInfo> = linked
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| FunctionInfo {
            key: FuncKey(u32::try_from(index).unwrap_or(u32::MAX)),
            name: function.name.clone(),
            loc: FunctionLoc {
                start: function.start,
                length: function.length,
            },
        })
        .collect();

    let imports = linked
        .imports
        .iter()
        .map(|import| {
            Ok(ImportRecord {
                name: import.name.clone(),
                offset: import.offset,
                addend: import.addend,
                size: import.size,
                encoding: encoding_tag(import.encoding)?,
                veneer: import.veneer,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let base_fixups = linked
        .base_fixups
        .iter()
        .map(|fixup| {
            Ok(BaseFixupRecord {
                offset: fixup.offset,
                size: fixup.size,
                encoding: encoding_tag(fixup.encoding)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let info = ModuleInfo {
        schema_version: METADATA_SCHEMA_VERSION,
        object_kind: OBJECT_KIND_MODULE,
        code_alignment: page as u32,
        page_size: page as u32,
        text_len: u32::try_from(linked.text_len)
            .map_err(|_| Error::Loader("text section is too large".to_string()))?,
        image_len: u32::try_from(linked.image.len())
            .map_err(|_| Error::Loader("image is too large".to_string()))?,
        data_offset: u32::try_from(linked.data_offset)
            .map_err(|_| Error::Loader("image is too large".to_string()))?,
        entry: u32::try_from(linked.entry)
            .map_err(|_| Error::Loader("entry point is too large".to_string()))?,
        source_checksum,
        functions,
        imports,
        base_fixups,
    };

    let meta_section = encode_engine_metadata(&metadata)?;
    let info_section =
        postcard::to_allocvec(&info).map_err(|e| Error::Loader(format!("metadata encode: {e}")))?;

    let mut obj = write::Object::new(
        BinaryFormat::Elf,
        Architecture::X86_64,
        ObjectEndianness::Little,
    );
    obj.flags = FileFlags::Elf {
        os_abi: ELF_OSABI_SEALION,
        abi_version: 0,
        e_flags: ELF_E_FLAGS_SEALION | u32::from(OBJECT_KIND_MODULE),
    };

    let text = obj.add_section(Vec::new(), b".text".to_vec(), SectionKind::Text);
    obj.append_section_data(text, &linked.image[..linked.text_len], page as u64);

    let data = obj.add_section(Vec::new(), b".data".to_vec(), SectionKind::Data);
    obj.append_section_data(data, &linked.image[linked.text_len..], 16);

    let meta = obj.add_section(
        Vec::new(),
        b".sealion.meta".to_vec(),
        SectionKind::ReadOnlyData,
    );
    obj.append_section_data(meta, &meta_section, 8);

    let info_section_id = obj.add_section(
        Vec::new(),
        b".sealion.info".to_vec(),
        SectionKind::ReadOnlyData,
    );
    obj.append_section_data(info_section_id, &info_section, 8);

    obj.write()
        .map_err(|e| Error::Loader(format!("failed to write artifact: {e}")))
}

fn encode_engine_metadata(metadata: &EngineMetadata) -> Result<Vec<u8>, Error> {
    let version = metadata.engine_version.as_bytes();
    if version.len() > u16::MAX as usize {
        return Err(Error::Loader(
            "engine version string is too long".to_string(),
        ));
    }
    let payload = postcard::to_allocvec(metadata)
        .map_err(|e| Error::Loader(format!("engine metadata encode: {e}")))?;
    let mut data = Vec::with_capacity(8 + 2 + 2 + version.len() + 4 + payload.len());
    data.extend_from_slice(&ARTIFACT_MAGIC);
    data.extend_from_slice(&ARTIFACT_FORMAT_VERSION.to_le_bytes());
    data.extend_from_slice(&(version.len() as u16).to_le_bytes());
    data.extend_from_slice(version);
    data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    data.extend_from_slice(&payload);
    Ok(data)
}

fn decode_engine_metadata(data: &[u8]) -> Result<EngineMetadata, Error> {
    if data.len() < 8 + 2 + 2 + 4 {
        return Err(error("engine metadata section is truncated"));
    }
    if data[..8] != ARTIFACT_MAGIC {
        return Err(error("bad artifact magic"));
    }
    let format_version = u16::from_le_bytes([data[8], data[9]]);
    if format_version != ARTIFACT_FORMAT_VERSION {
        return Err(error(format!(
            "unsupported artifact format version {format_version}"
        )));
    }
    let version_len = u16::from_le_bytes([data[10], data[11]]) as usize;
    let version_end = 12usize
        .checked_add(version_len)
        .ok_or_else(|| error("engine version length overflow"))?;
    if data.len() < version_end + 4 {
        return Err(error("engine metadata section is truncated"));
    }
    let version = str::from_utf8(&data[12..version_end])
        .map_err(|e| error(format!("engine version is not UTF-8: {e}")))?;
    let payload_len = u32::from_le_bytes([
        data[version_end],
        data[version_end + 1],
        data[version_end + 2],
        data[version_end + 3],
    ]) as usize;
    let payload_start = version_end + 4;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or_else(|| error("engine metadata length overflow"))?;
    if data.len() != payload_end {
        return Err(error("engine metadata section has an unexpected length"));
    }
    let metadata: EngineMetadata = postcard::from_bytes(&data[payload_start..payload_end])
        .map_err(|e| error(format!("failed to decode engine metadata: {e}")))?;
    if metadata.engine_version != version {
        return Err(error("engine version header does not match metadata"));
    }
    Ok(metadata)
}

pub(super) fn load(bytes: &[u8]) -> Result<Module, Error> {
    let file = ElfFile64::parse(bytes)
        .map_err(|e| Error::Loader(format!("failed to parse artifact: {e}")))?;
    check_container(&file)?;

    let page = Linux::page_size();
    let mut text: Option<&[u8]> = None;
    let mut data = None;
    let mut meta = None;
    let mut info_bytes = None;

    for section in file.sections() {
        let name = section.name().unwrap_or("<unnamed>");
        if section.relocations().next().is_some() {
            return Err(error(format!(
                "unexpected load-time relocations in section {name:?}"
            )));
        }
        let flags = match section.flags() {
            SectionFlags::Elf { sh_flags } => sh_flags,
            _ => return Err(error("artifact is not an ELF file")),
        };
        let alloc = flags & u64::from(SHF_ALLOC) != 0;
        if flags & u64::from(SHF_TLS) != 0 {
            return Err(error(format!("TLS section {name:?} is not supported")));
        }
        let executable = flags & u64::from(SHF_EXECINSTR) != 0;
        let writable = flags & u64::from(SHF_WRITE) != 0;
        if executable && writable {
            return Err(error(format!(
                "section {name:?} is both writable and executable"
            )));
        }
        if !alloc {
            continue;
        }

        let align = section.align();
        if align != 0 && (!align.is_power_of_two() || align > page as u64) {
            return Err(error(format!(
                "section {name:?} has unsupported alignment {align}"
            )));
        }

        let contents = section
            .data()
            .map_err(|e| error(format!("failed to read section {name:?}: {e}")))?;
        match name {
            ".text" => text = Some(contents),
            ".data" => data = Some(contents),
            ".sealion.meta" => meta = Some(contents),
            ".sealion.info" => info_bytes = Some(contents),
            _ if name.starts_with(".sealion.") => {
                return Err(error(format!("unexpected artifact section {name:?}")));
            }
            _ => return Err(error(format!("unexpected section {name:?}"))),
        }
    }

    let text = text.ok_or_else(|| error("artifact has no .text section"))?;
    let data = data.ok_or_else(|| error("artifact has no .data section"))?;
    let meta = meta.ok_or_else(|| error("artifact has no .sealion.meta section"))?;
    let info_bytes = info_bytes.ok_or_else(|| error("artifact has no .sealion.info section"))?;

    let metadata = decode_engine_metadata(meta)?;
    let info: ModuleInfo = postcard::from_bytes(info_bytes)
        .map_err(|e| error(format!("failed to decode module info: {e}")))?;

    check_compatible(&metadata)?;
    check_layout(&info, text, data, page)?;

    let image_len = info.image_len as usize;
    let text_len = info.text_len as usize;
    let data_offset = info.data_offset as usize;

    let mut memory = Linux::allocate(image_len)?;
    if memory.len() != image_len {
        return Err(error(
            "artifact memory allocation returned an unexpected size",
        ));
    }
    let base = memory.base();
    unsafe {
        ptr::copy_nonoverlapping(text.as_ptr(), base, text_len);
        ptr::copy_nonoverlapping(data.as_ptr(), base.add(data_offset), data.len());
    }

    apply_imports(&info, base, text_len)?;
    apply_base_fixups(&info, base)?;

    unsafe {
        memory.publish(text_len)?;
    }
    X86_64::flush_icache(base, text_len);

    Ok(Module {
        memory,
        info,
        metadata,
        artifact: bytes.to_vec(),
    })
}

fn check_container(file: &ElfFile64<'_>) -> Result<(), Error> {
    if file.kind() != ObjectKind::Relocatable {
        return Err(error("artifact is not a relocatable ELF container"));
    }
    if file.architecture() != Architecture::X86_64 {
        return Err(error(format!(
            "artifact was compiled for architecture {:?}",
            file.architecture()
        )));
    }
    if !file.is_64() || file.endianness() != ObjectEndianness::Little {
        return Err(error("artifact is not 64-bit little-endian"));
    }
    match file.flags() {
        FileFlags::Elf {
            os_abi,
            abi_version,
            e_flags,
        } => {
            if os_abi != ELF_OSABI_SEALION
                || abi_version != 0
                || e_flags & !ELF_KIND_MASK != ELF_E_FLAGS_SEALION
            {
                return Err(error("file is not a sealion artifact"));
            }
            let kind = (e_flags & ELF_KIND_MASK) as u8;
            if kind != OBJECT_KIND_MODULE {
                return Err(error(format!("unsupported artifact object kind {kind}")));
            }
        }
        _ => return Err(error("file is not a sealion ELF artifact")),
    }
    Ok(())
}

fn check_compatible(metadata: &EngineMetadata) -> Result<(), Error> {
    if metadata.metadata_schema_version != METADATA_SCHEMA_VERSION {
        return Err(error(format!(
            "unsupported metadata schema version {}",
            metadata.metadata_schema_version
        )));
    }
    if metadata.object_kind != OBJECT_KIND_MODULE {
        return Err(error(format!(
            "unsupported object kind {}",
            metadata.object_kind
        )));
    }
    if metadata.engine_version != env!("CARGO_PKG_VERSION") {
        return Err(error(format!(
            "artifact was built by engine version {}, this engine is {}",
            metadata.engine_version,
            env!("CARGO_PKG_VERSION")
        )));
    }

    let artifact_target = Triple::from_str(&metadata.target).map_err(|e| {
        error(format!(
            "invalid artifact target '{}': {e}",
            metadata.target
        ))
    })?;
    let host = Triple::host();
    if artifact_target.architecture != host.architecture {
        return Err(error(format!(
            "artifact was compiled for architecture '{}'",
            artifact_target.architecture
        )));
    }
    if artifact_target.operating_system != host.operating_system {
        return Err(error(format!(
            "artifact was compiled for operating system '{}'",
            artifact_target.operating_system
        )));
    }
    let host_pointer_width = usize::BITS as u8;
    if metadata.pointer_width != host_pointer_width {
        return Err(error(format!(
            "artifact requires {}-bit pointers, this host uses {host_pointer_width}",
            metadata.pointer_width
        )));
    }
    let host_endianness = if cfg!(target_endian = "little") { 0 } else { 1 };
    if metadata.endianness != host_endianness {
        return Err(error("artifact endianness does not match the host"));
    }
    if metadata.feature_bits != 0 {
        return Err(error("artifact requires unsupported runtime features"));
    }
    Ok(())
}

fn check_layout(info: &ModuleInfo, text: &[u8], data: &[u8], page: usize) -> Result<(), Error> {
    if info.schema_version != METADATA_SCHEMA_VERSION {
        return Err(error(format!(
            "unsupported module metadata schema {}",
            info.schema_version
        )));
    }
    if info.object_kind != OBJECT_KIND_MODULE {
        return Err(error(format!(
            "unsupported module object kind {}",
            info.object_kind
        )));
    }
    if info.page_size as usize != page {
        return Err(error(format!(
            "artifact requires page size {}, host uses {page}",
            info.page_size
        )));
    }
    if info.code_alignment == 0 || !info.code_alignment.is_power_of_two() {
        return Err(error("invalid code alignment"));
    }
    let text_len = info.text_len as usize;
    let image_len = info.image_len as usize;
    let data_offset = info.data_offset as usize;
    if image_len == 0 || image_len > MAX_IMAGE_SIZE {
        return Err(error(format!("invalid image size {image_len}")));
    }
    if text_len == 0 || text_len > image_len {
        return Err(error("invalid text size"));
    }
    if data_offset != align_up(text_len, page) {
        return Err(error("data section is not laid out where metadata says"));
    }
    if text.len() != text_len {
        return Err(error(format!(
            "text section has {} bytes, metadata says {text_len}",
            text.len()
        )));
    }
    if data.len() != image_len - data_offset {
        return Err(error(format!(
            "data section has {} bytes, metadata says {}",
            data.len(),
            image_len - data_offset
        )));
    }
    if info.entry as usize >= text_len {
        return Err(error("entry point is outside the text section"));
    }

    let mut previous_end = 0usize;
    for (index, function) in info.functions.iter().enumerate() {
        if function.key.0 as usize != index {
            return Err(error("function keys are not dense and ordered"));
        }
        let start = function.loc.start as usize;
        let end = start
            .checked_add(function.loc.length as usize)
            .ok_or_else(|| error("function range overflow"))?;
        if end > text_len {
            return Err(error(format!(
                "function {:?} extends outside the text section",
                function.name
            )));
        }
        if start < previous_end {
            return Err(error("function table is not sorted by text offset"));
        }
        previous_end = end;
    }

    for import in &info.imports {
        let offset = import.offset as usize;
        if import.veneer {
            if offset
                .checked_add(X86_64::VENEER_SIZE)
                .is_none_or(|end| end > text_len)
            {
                return Err(error("veneer extends outside the text section"));
            }
        } else {
            let width = usize::from(import.size) / 8;
            if width == 0 || offset.checked_add(width).is_none_or(|end| end > image_len) {
                return Err(error("import fixup is outside the image"));
            }
        }
    }
    for fixup in &info.base_fixups {
        let offset = fixup.offset as usize;
        let width = usize::from(fixup.size) / 8;
        if width == 0 || offset.checked_add(width).is_none_or(|end| end > image_len) {
            return Err(error("base fixup is outside the image"));
        }
    }
    Ok(())
}

fn apply_imports(info: &ModuleInfo, base: *mut u8, text_len: usize) -> Result<(), Error> {
    let mut cache = HashMap::new();
    for import in &info.imports {
        let address = resolve_external(&import.name, &mut cache)?;
        let offset = import.offset as usize;
        if import.veneer {
            if offset + X86_64::VENEER_SIZE > text_len {
                return Err(error("veneer extends outside the text section"));
            }
            X86_64::write_veneer(base, offset, address);
        } else {
            let value = (address as i128)
                .checked_add(i128::from(import.addend))
                .and_then(|value| u64::try_from(value).ok())
                .ok_or_else(|| {
                    error(format!(
                        "external symbol '{}' address does not fit in 64 bits",
                        import.name
                    ))
                })?;
            X86_64::write_absolute(
                base,
                offset,
                value,
                import.size,
                encoding_from_tag(import.encoding)?,
            )?;
        }
    }
    Ok(())
}

fn apply_base_fixups(info: &ModuleInfo, base: *mut u8) -> Result<(), Error> {
    let base_address = base as usize as u64;
    for fixup in &info.base_fixups {
        let encoding = encoding_from_tag(fixup.encoding)?;
        let offset = fixup.offset as usize;
        let value = X86_64::read_absolute(base, offset, fixup.size, encoding)?
            .checked_add(base_address)
            .ok_or_else(|| error("base relocation overflow"))?;
        X86_64::write_absolute(base, offset, value, fixup.size, encoding)?;
    }
    Ok(())
}

fn resolve_external(name: &str, cache: &mut HashMap<String, usize>) -> Result<usize, Error> {
    if let Some(address) = cache.get(name) {
        return Ok(*address);
    }
    let address =
        Linux::lookup(name).ok_or_else(|| error(format!("unresolved external symbol {name:?}")))?;
    cache.insert(name.to_string(), address);
    Ok(address)
}

fn encoding_tag(encoding: RelocationEncoding) -> Result<u8, Error> {
    match encoding {
        RelocationEncoding::Generic => Ok(ENC_GENERIC),
        RelocationEncoding::X86Signed => Ok(ENC_X86_SIGNED),
        RelocationEncoding::X86Branch => Ok(ENC_X86_BRANCH),
        RelocationEncoding::X86RipRelative => Ok(ENC_X86_RIP_RELATIVE),
        RelocationEncoding::X86RipRelativeMovq => Ok(ENC_X86_RIP_RELATIVE_MOVQ),
        _ => Err(error(format!(
            "unsupported relocation encoding {encoding:?}"
        ))),
    }
}

fn encoding_from_tag(tag: u8) -> Result<RelocationEncoding, Error> {
    match tag {
        ENC_GENERIC => Ok(RelocationEncoding::Generic),
        ENC_X86_SIGNED => Ok(RelocationEncoding::X86Signed),
        ENC_X86_BRANCH => Ok(RelocationEncoding::X86Branch),
        ENC_X86_RIP_RELATIVE => Ok(RelocationEncoding::X86RipRelative),
        ENC_X86_RIP_RELATIVE_MOVQ => Ok(RelocationEncoding::X86RipRelativeMovq),
        other => Err(error(format!("unknown relocation encoding tag {other}"))),
    }
}

fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn error(message: impl Into<String>) -> Error {
    Error::Loader(message.into())
}
