use std::fmt;

/// A convenient result type wrapping [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct SlotError {}

#[derive(Debug)]
pub struct LookupError {
    pub entry: String,
}

#[derive(Debug)]
pub struct LoadingError {
    pub entry: String,
    pub path: String,
}

impl fmt::Display for SlotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ran out of slots!")
    }
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Could not find requested entry {} in database!",
            self.entry
        )
    }
}

impl fmt::Display for LoadingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Failed to load requested entry {} in database! Attempted path: {}",
            self.entry, self.path
        )
    }
}

impl std::error::Error for SlotError {}

impl std::error::Error for LookupError {}

impl std::error::Error for LoadingError {}

#[derive(Debug)]
pub enum Error {
    LookupError(LookupError),
    LoadingError(LoadingError),
    SlotError(),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::LookupError(err) => err.fmt(f),
            Error::LoadingError(err) => err.fmt(f),
            Error::SlotError() => SlotError {}.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::LookupError(err) => Some(err),
            Error::LoadingError(err) => Some(err),
            Error::SlotError() => None,
        }
    }
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        return Error::LoadingError(LoadingError {
            entry: "[UNKNOWN]".to_string(),
            path: value,
        });
    }
}

impl From<image::ImageError> for Error {
    fn from(value: image::ImageError) -> Self {
        return Error::LoadingError(LoadingError {
            entry: "[UNKNOWN]".to_string(),
            path: value.to_string(),
        });
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        return Error::LoadingError(LoadingError {
            entry: "IO Loading Error".to_string(),
            path: value.to_string(),
        });
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        return Error::LoadingError(LoadingError {
            entry: "JSON FILE".to_string(),
            path: value.to_string(),
        });
    }
}
//impl From<ash::vk::Result> for GPUError {
//    fn from(res: ash::vk::Result) -> Self {
//        return GPUError::VulkanError(VulkanError { res });
//    }
//}
//
//impl From<ash::LoadingError> for GPUError {
//    fn from(res: ash::LoadingError) -> Self {
//        return GPUError::LoadingError(res);
//    }
//}
