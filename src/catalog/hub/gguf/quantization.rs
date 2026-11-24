//! Bits per weight of GGUF quantizations, to estimate a variant's size from
//! the model's parameter count before its file sizes are listed.
//!
//! From llama.cpp's `tools/quantize/quantize.cpp`: the IQ and TQ types state
//! their bits per weight; the others state Llama-3-8B's size in GiB, here
//! converted over its 8.03 billion parameters.

/// Types by name, longest first among those sharing a prefix (`Q4_K_S`
/// before `Q4_K`), each with its bits per weight.
const BITS: [(&str, f64); 32] = [
    ("IQ1_S", 1.56),
    ("IQ1_M", 1.75),
    ("IQ2_XXS", 2.06),
    ("IQ2_XS", 2.31),
    ("IQ2_S", 2.5),
    ("IQ2_M", 2.7),
    ("IQ3_XXS", 3.06),
    ("IQ3_XS", 3.3),
    ("IQ3_S", 3.44),
    ("IQ3_M", 3.66),
    ("IQ4_XS", 4.25),
    ("IQ4_NL", 4.5),
    ("TQ1_0", 1.69),
    ("TQ2_0", 2.06),
    ("Q1_0", 1.125),
    ("Q2_0", 2.25),
    ("Q2_K", 3.17),
    ("Q3_K_S", 3.65),
    ("Q3_K_L", 4.31),
    ("Q3_K", 4.0),
    ("Q4_0", 4.64),
    ("Q4_1", 5.11),
    ("Q4_K_S", 4.67),
    ("Q4_K", 4.9),
    ("Q5_0", 5.57),
    ("Q5_1", 6.04),
    ("Q5_K_S", 5.57),
    ("Q5_K", 5.7),
    ("Q6_K", 6.57),
    ("Q8_0", 8.51),
    ("Q8_K", 8.51),
    ("MXFP4", 4.25),
];

/// The bits per weight of a variant label such as `Q4_K_M`, `UD-Q4_K_XL` or
/// `BF16`; `None` when it names no known type.
pub(super) fn bits_per_weight(label: &str) -> Option<f64> {
    let upper = label.to_ascii_uppercase();
    let name = upper.strip_prefix("UD-").unwrap_or(&upper);
    match name {
        _ if name.starts_with("BF16") || name.starts_with("F16") => return Some(16.0),
        _ if name.starts_with("F32") => return Some(32.0),
        _ => {},
    }
    BITS.iter().find(|(prefix, _)| name.starts_with(prefix)).map(|&(_, bits)| bits)
}

/// The bytes `parameters` weights take at `bits` per weight.
pub(super) fn bytes(parameters: u64, bits: f64) -> u64 {
    // Parameter counts fit f64 exactly well past any model's size.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bytes = (parameters as f64 * bits / 8.0).round() as u64;
    bytes
}
