use thiserror::Error;

/// Result type alias for converter operations.
pub type Result<T> = std::result::Result<T, ConverterError>;

/// Errors that can occur during JAR-to-RMOD conversion, archive handling, and JVM bytecode parsing.
#[derive(Debug, Error)]
pub enum ConverterError {
    #[error("Invalid JVM classfile magic: 0x{0:08X}, expected 0xCAFEBABE")]
    InvalidMagic(u32),

    #[error("Unsupported or invalid JVM class version: {major}.{minor}")]
    InvalidVersion { major: u16, minor: u16 },

    #[error("Unexpected end of file while reading classfile at offset {0}")]
    UnexpectedEof(usize),

    #[error("Invalid constant pool tag: {0}")]
    InvalidConstantTag(u8),

    #[error("Constant pool index {index} is out of bounds (pool size: {size})")]
    ConstantPoolOutOfBounds { index: u16, size: usize },

    #[error("Expected constant pool entry {index} to be {expected}, but found {actual}")]
    UnexpectedConstantType {
        index: u16,
        expected: &'static str,
        actual: &'static str,
    },

    #[error("Invalid UTF-8 string in classfile: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    #[error("Invalid zip archive: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("Unrecognized mod format: {0}")]
    UnrecognizedModFormat(String),

    #[error("Conversion error: {0}")]
    Other(String),
}
