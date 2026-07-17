use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    let (model, set_model) = signal(String::from("local-model"));
    let (prompt, set_prompt) = signal(String::new());
    let (output, set_output) = signal(String::from("runtime not connected"));

    let submit = move |_| {
        let text = format!("queued prompt for {}", model.get());
        set_output.set(text);
    };

    view! {
        <main class="shell">
            <section class="toolbar">
                <h1>"Mirmir"</h1>
                <span>"native Rust runtime shell"</span>
            </section>
            <section class="panel">
                <label>
                    "Model"
                    <input
                        prop:value=model
                        on:input=move |event| set_model.set(event_target_value(&event))
                    />
                </label>
                <label>
                    "Prompt"
                    <textarea
                        prop:value=prompt
                        on:input=move |event| set_prompt.set(event_target_value(&event))
                    />
                </label>
                <button on:click=submit>"Send"</button>
            </section>
            <section class="output">
                <pre>{output}</pre>
            </section>
        </main>
    }
}
