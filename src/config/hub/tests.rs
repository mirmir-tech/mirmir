use super::*;

#[test]
fn reads_a_repository_and_its_variant() -> Result<()> {
    let reference = "unsloth/gemma-4-E4B-it-GGUF:Q4_K_M".parse::<HubReference>()?;
    assert_eq!(reference.repository(), "unsloth/gemma-4-E4B-it-GGUF");
    assert_eq!(reference.variant(), Some("Q4_K_M"));
    assert_eq!(reference.key(), "unsloth--gemma-4-E4B-it-GGUF--Q4_K_M");
    assert_eq!(reference.to_string(), "unsloth/gemma-4-E4B-it-GGUF:Q4_K_M");
    let plain = "Qwen/Qwen3-4B".parse::<HubReference>()?;
    assert_eq!((plain.variant(), plain.key().as_str()), (None, "Qwen--Qwen3-4B"));
    Ok(())
}

#[test]
fn refuses_traversal_and_keys_that_could_collide() {
    for value in ["../secret", "owner/model/extra", "owner/model:", "owner/model:a/b", "a/b:x--y"] {
        assert!(value.parse::<HubReference>().is_err(), "{value}");
    }
}
