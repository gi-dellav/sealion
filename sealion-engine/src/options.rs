#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptLevel {
    /// Minimize compile time by disabling most optimizations.
    None,
    /// Generate the fastest possible code.
    #[default]
    Speed,
    /// Like [`OptLevel::Speed`], but also reduce code size.
    SpeedAndSize,
}

impl OptLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Speed => "speed",
            Self::SpeedAndSize => "speed_and_size",
        }
    }
}

/// Language, edition and code-generation options for
/// [`Engine::compile_with_options`].
#[derive(Debug, Clone)]
pub struct CompileOptions {
    /// Parse the source as TypeScript and remove type syntax before lowering.
    pub typescript: bool,
    /// Apply the ECMAScript 2026 transforms before lowering.
    pub es2026: bool,
    /// Cranelift optimization level.
    pub opt_level: OptLevel,
    /// Enable redundant-load optimizations with alias analysis.
    pub enable_alias_analysis: bool,
    /// Run the Cranelift IR verifier during compilation.
    pub enable_verifier: bool,
    /// Target triple to compile for; defaults to the host.
    pub target: Option<String>,
    /// Target-specific CPU preset, for example `x86-64-v3` or `znver4`.
    pub target_cpu: Option<String>,
    /// Emit position-independent code.
    pub is_pic: bool,
    /// Generate unwind information.
    pub unwind_info: bool,
    /// Preserve frame pointers in generated functions.
    pub preserve_frame_pointers: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            typescript: false,
            es2026: false,
            opt_level: OptLevel::Speed,
            enable_alias_analysis: true,
            enable_verifier: true,
            target: None,
            target_cpu: None,
            is_pic: true,
            unwind_info: true,
            preserve_frame_pointers: false,
        }
    }
}
