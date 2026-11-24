//! The software ecosystem a checkpoint was published for.

/// The ecosystem a model's files come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ecosystem {
    /// llama.cpp's GGUF files.
    Ggml,
    /// MLX-quantized checkpoints.
    Mlx,
    /// Hugging Face Transformers checkpoints.
    Transformers,
    /// Another library a Hub model names, as the Hub spells it.
    Library(String),
    /// Neither the files nor the Hub name one.
    Unknown,
}

impl Ecosystem {
    /// The ecosystem of a Hub `library_name`.
    #[must_use]
    pub fn of_library(library: Option<&str>) -> Self {
        let Some(library) = library else {
            return Self::Unknown;
        };
        match library.to_ascii_lowercase().as_str() {
            "ggml" | "gguf" => Self::Ggml,
            "mlx" => Self::Mlx,
            "transformers" => Self::Transformers,
            _ => Self::Library(library.to_owned()),
        }
    }

    /// The protocol spelling.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Ggml => "GGML",
            Self::Mlx => "MLX",
            Self::Transformers => "Transformers",
            Self::Library(name) => name,
            Self::Unknown => "Unknown",
        }
    }
}
