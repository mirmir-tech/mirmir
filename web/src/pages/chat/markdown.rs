use leptos::prelude::*;
use pulldown_cmark::{Event, Options, Parser, html};

#[component]
pub fn Markdown(source: Signal<String>) -> impl IntoView {
    view! { <div class="markdown" inner_html=move || render(&source.get())></div> }
}

fn render(source: &str) -> String {
    let mut output = String::new();
    let parser = Parser::new_ext(source, Options::all()).map(|event| match event {
        Event::Html(value) | Event::InlineHtml(value) => Event::Text(value),
        other => other,
    });
    html::push_html(&mut output, parser);
    output
}
