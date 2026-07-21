use leptos::prelude::*;

#[component]
pub fn Icon(name: &'static str) -> impl IntoView {
    let paths = match name {
        "load" => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zm-2 5 6 4-6 4z",
        "unload" => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM9 9h6v6H9z",
        "download" => "M12 3v12m0 0 4-4m-4 4-4-4M5 20h14",
        "downloaded" => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zm-4 9 2.5 2.5L16 9",
        "add" => "M12 5v14M5 12h14",
        "remove" => "M4 7h16M9 7V4h6v3m-8 0 1 14h8l1-14",
        "cancel" => "M6 6l12 12M18 6 6 18",
        "tools" => "M14 6l4-4 4 4-4 4m-2-2L7 17m-3 4 3-4 3 3-4 3z",
        "thinking" => "M9 18h6m-5 3h4M8 14a6 6 0 1 1 8 0c-1 1-1 2-1 2H9s0-1-1-2z",
        "vision" => {
            "M2 12s4-6 10-6 10 6 10 6-4 6-10 6S2 12 2 12zm10 3a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"
        },
        _ => "M5 12h14",
    };
    view! { <svg aria-hidden="true" data-icon=name viewBox="0 0 24 24"><path d=paths /></svg> }
}
