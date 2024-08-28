pub type Result<T> = anyhow::Result<T, anyhow::Error>;

pub mod cmd;
pub mod zip;
