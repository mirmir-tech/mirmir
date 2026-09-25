use super::*;

#[test]
fn additive_penalties_are_never_silently_ignored_or_reinterpreted() {
    for field in ["presence_penalty", "frequency_penalty"] {
        for value in [-2.0, -0.5, 0.5, 1.5, 2.0] {
            let mut body =
                serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}]});
            body[field] = serde_json::json!(value);
            let request: ChatRequest = serde_json::from_value(body).expect("wire input");
            assert!(request.into_application().is_err(), "{field}={value} was silently ignored");
        }
        for value in [serde_json::Value::Null, serde_json::json!(0)] {
            let mut body = serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}], "repetition_penalty":1.1});
            body[field] = value;
            let request: ChatRequest = serde_json::from_value(body).expect("wire input");
            let (_, request, _) = request.into_application().expect("neutral control");
            assert_eq!(request.options.repetition_penalty, Some(1.1));
        }
    }
}
