use leptos::{portal::Portal, prelude::*};
use wasm_bindgen::JsCast;

#[derive(Clone, Copy)]
struct TooltipState {
    text: RwSignal<String>,
    position: RwSignal<(f64, f64)>,
}

impl TooltipState {
    fn update(self, event: &web_sys::PointerEvent) {
        let text = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .and_then(|element| element.closest("[data-tooltip]").ok().flatten())
            .and_then(|element| element.get_attribute("data-tooltip"))
            .unwrap_or_default();
        self.text.set(text);
        let viewport_width = window()
            .inner_width()
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or(640.0);
        let left = f64::from(event.client_x()).clamp(172.0, (viewport_width - 172.0).max(172.0));
        self.position.set((left, f64::from(event.client_y() - 10)));
    }

    fn hide(self) {
        self.text.set(String::new());
    }
}

#[component]
pub fn TooltipSurface(children: Children) -> impl IntoView {
    let state = TooltipState {
        text: RwSignal::new(String::new()),
        position: RwSignal::new((0.0, 0.0)),
    };
    view! {
        <div
            class="tooltip-surface"
            on:pointerover=move |event| state.update(&event)
            on:pointermove=move |event| state.update(&event)
            on:pointerleave=move |_| state.hide()
        >
            {children()}
        </div>
        <Portal>
            <span
                class="floating-tooltip"
                hidden=move || state.text.get().is_empty()
                style=move || {
                    let (left, top) = state.position.get();
                    format!("left: {left}px; top: {top}px")
                }
                role="tooltip"
            >
                {move || state.text.get()}
            </span>
        </Portal>
    }
}
