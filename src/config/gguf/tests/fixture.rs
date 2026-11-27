//! GGUF files of no tensors whose metadata a test chooses.

use std::{fs, path::Path};

/// A GGUF v3 file's metadata, written in the order it was added.
#[derive(Default)]
pub(super) struct Metadata {
    entries: Vec<(String, u32, Vec<u8>)>,
}

fn string(value: &str) -> Vec<u8> {
    let mut bytes = (value.len() as u64).to_le_bytes().to_vec();
    bytes.extend(value.as_bytes());
    bytes
}

impl Metadata {
    pub(super) fn text(mut self, key: &str, value: &str) -> Self {
        self.entries.push((key.to_owned(), 8, string(value)));
        self
    }

    pub(super) fn unsigned(mut self, key: &str, value: u32) -> Self {
        self.entries.push((key.to_owned(), 4, value.to_le_bytes().to_vec()));
        self
    }

    pub(super) fn strings(mut self, key: &str, values: &[&str]) -> Self {
        let mut bytes = 8_u32.to_le_bytes().to_vec();
        bytes.extend((values.len() as u64).to_le_bytes());
        for value in values {
            bytes.extend(string(value));
        }
        self.entries.push((key.to_owned(), 9, bytes));
        self
    }

    pub(super) fn integers(mut self, key: &str, values: &[i32]) -> Self {
        let mut bytes = 5_u32.to_le_bytes().to_vec();
        bytes.extend((values.len() as u64).to_le_bytes());
        for value in values {
            bytes.extend(value.to_le_bytes());
        }
        self.entries.push((key.to_owned(), 9, bytes));
        self
    }

    /// A small Llama with a byte-level vocabulary, which imports.
    pub(super) fn llama() -> Self {
        Self::default()
            .text("general.architecture", "llama")
            .unsigned("llama.block_count", 1)
            .unsigned("llama.context_length", 64)
            .unsigned("llama.embedding_length", 8)
            .unsigned("llama.feed_forward_length", 16)
            .unsigned("llama.attention.head_count", 2)
            .unsigned("llama.attention.head_count_kv", 2)
            .text("tokenizer.ggml.model", "gpt2")
            .text("tokenizer.ggml.pre", "qwen2")
            .strings("tokenizer.ggml.tokens", &["a", "b", "ab"])
            .integers("tokenizer.ggml.token_type", &[1, 1, 1])
            .strings("tokenizer.ggml.merges", &["a b"])
    }

    /// Shard `number` of `count` of a split model, numbered from 1.
    pub(super) fn shard(self, number: u32, count: u32) -> Self {
        let shard = self.unsigned("split.no", number - 1).unsigned("split.count", count);
        if number == 1 {
            shard.unsigned("split.tensors.count", 0)
        } else {
            shard
        }
    }

    pub(super) fn write(&self, path: &Path) -> std::io::Result<()> {
        let mut bytes = b"GGUF".to_vec();
        bytes.extend(3_u32.to_le_bytes());
        bytes.extend(0_u64.to_le_bytes());
        bytes.extend((self.entries.len() as u64).to_le_bytes());
        for (key, kind, value) in &self.entries {
            bytes.extend(string(key));
            bytes.extend(kind.to_le_bytes());
            bytes.extend(value);
        }
        fs::write(path, bytes)
    }
}
