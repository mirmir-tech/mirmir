use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("dotenv error: {0}")]
    Dotenv(#[from] dotenvy::Error),
}
