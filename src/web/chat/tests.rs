use super::*;

const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

#[test]
fn converts_dashboard_image_into_the_runtime_contract() {
    let request = ChatRequest {
        model: "vision-model".to_owned(),
        messages: vec![Message {
            role: "user".to_owned(),
            content: "describe it".to_owned(),
            reasoning_content: None,
        }],
        max_tokens: None,
        min_tokens: None,
        ignore_eos: None,
        temperature: None,
        top_p: None,
        top_k: None,
        repetition_penalty: None,
        seed: None,
        image: Some(PNG.to_owned()),
    };

    let request = proto::GenerateRequest::try_from(request).expect("valid image request");

    assert!(request.image.is_some_and(|image| image.starts_with(b"\x89PNG")));
    assert_eq!(
        request.messages[0].content,
        format!("{}\ndescribe it", libmir::IMAGE_PLACEHOLDER)
    );
}
