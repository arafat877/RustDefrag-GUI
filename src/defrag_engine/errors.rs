use std::fmt;

#[derive(Debug)]
pub enum DefragError {
    InsufficientPrivileges,
    InvalidVolume(String),
    UnsupportedFilesystem(String),
    VolumeLocked,
    ApiFailure { api: &'static str, code: u32 },
    FileAccessDenied(String),
    MoveFileFailed { path: String, code: u32 },
    NoFreeRegion { required: u64 },
    Io(std::io::Error),
}

impl fmt::Display for DefragError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DefragError::InsufficientPrivileges =>
                write!(f, "Administrator privileges required."),
            DefragError::InvalidVolume(v) =>
                write!(f, "Cannot open volume '{}'.", v),
            DefragError::UnsupportedFilesystem(fs) =>
                write!(f, "Filesystem '{}' is not supported. Requires NTFS.", fs),
            DefragError::VolumeLocked =>
                write!(f, "Volume is locked by another process."),
            DefragError::ApiFailure { api, code } =>
                write!(f, "API '{}' failed: 0x{:08X}", api, code),
            DefragError::FileAccessDenied(p) =>
                write!(f, "Access denied: '{}'", p),
            DefragError::MoveFileFailed { path, code } =>
                write!(f, "Move failed for '{}' (0x{:08X})", path, code),
            DefragError::NoFreeRegion { required } =>
                write!(f, "No contiguous free region of {} clusters.", required),
            DefragError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for DefragError {}

impl From<std::io::Error> for DefragError {
    fn from(e: std::io::Error) -> Self { DefragError::Io(e) }
}

pub type DefragResult<T> = Result<T, anyhow::Error>;
