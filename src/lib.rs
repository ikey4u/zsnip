pub type Result<T> = anyhow::Result<T, anyhow::Error>;

pub mod cmd;
pub mod fs;
pub mod zip;
pub mod path;
