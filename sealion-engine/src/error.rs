use std::fmt;

#[derive(Debug)]
pub enum Error {
    Parse(String),
    Scope(String),
    Unsupported(String),
    Codegen(String),
    Io(std::io::Error),
    Linker(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(m) => write!(f, "parse error: {m}"),
            Self::Scope(m) => write!(f, "scope error: {m}"),
            Self::Unsupported(m) => write!(f, "unsupported feature: {m}"),
            Self::Codegen(m) => write!(f, "codegen error: {m}"),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Linker(m) => write!(f, "linker error: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<boa_parser::Error> for Error {
    fn from(value: boa_parser::Error) -> Self {
        Self::Parse(value.to_string())
    }
}

impl From<cranelift_module::ModuleError> for Error {
    fn from(value: cranelift_module::ModuleError) -> Self {
        Self::Codegen(value.to_string())
    }
}
