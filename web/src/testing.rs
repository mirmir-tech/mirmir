//! Browser test support: a stand-in for the server's HTTP API that records
//! each request and answers it from routes, holding an answer until the
//! test releases it, and helpers to mount a view and wait on the page.

use gloo_timers::future::TimeoutFuture;
use leptos::{mount::mount_to, prelude::*};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

/// Replaces `window.fetch`. A route answers requests whose method matches
/// and whose path starts with its prefix; each answer is used once, the
/// last one for every request after it.
const FETCH: &str = r"
const state = { routes: JSON.parse(routes), requests: [], held: [] };
window.__mirmir = state;
window.fetch = async (input, init) => {
  const request = input instanceof Request ? input : new Request(input, init);
  const url = new URL(request.url);
  const path = url.pathname + url.search;
  const body = request.method === 'GET' ? null : await request.text();
  state.requests.push({ method: request.method, path, body });
  const route = state.routes.find(
    (route) => route.method === request.method && path.startsWith(route.prefix));
  if (!route) return new Response('{}', { status: 404 });
  const answer = route.answers.length > 1 ? route.answers.shift() : route.answers[0];
  if (answer.hold) await new Promise((resolve) => state.held.push({ path, resolve }));
  return new Response(JSON.stringify(answer.body),
    { status: answer.status, headers: { 'content-type': 'application/json' } });
};
";

#[derive(Serialize)]
pub struct Route {
    pub method: &'static str,
    pub prefix: &'static str,
    pub answers: Vec<Answer>,
}

#[derive(Serialize)]
pub struct Answer {
    pub status: u16,
    pub body: serde_json::Value,
    /// Held until [`release`] names the request's path.
    pub hold: bool,
}

impl Answer {
    pub const fn ok(body: serde_json::Value) -> Self {
        Self { status: 200, body, hold: false }
    }

    pub const fn held(body: serde_json::Value) -> Self {
        Self { status: 200, body, hold: true }
    }

    pub fn status(status: u16) -> Self {
        Self {
            status,
            body: serde_json::json!({"error": {"message": format!("HTTP {status}")}}),
            hold: false,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Recorded {
    pub method: String,
    /// Below the API's base, with the query.
    pub path: String,
    pub body: Option<String>,
}

pub fn serve(routes: &[Route]) {
    let routes = serde_json::to_string(routes).expect("routes serialize");
    let install = js_sys::Function::new_with_args("routes", FETCH);
    install.call1(&JsValue::NULL, &routes.into()).expect("fetch is replaced");
}

fn state() -> JsValue {
    js_sys::Reflect::get(&window(), &"__mirmir".into()).expect("serve ran")
}

/// The requests the page sent, in order, with `/api/mirmir/v1` dropped.
pub fn requests() -> Vec<Recorded> {
    let list = js_sys::Reflect::get(&state(), &"requests".into()).expect("requests");
    let text = js_sys::JSON::stringify(&list).expect("requests serialize");
    let mut recorded: Vec<Recorded> =
        serde_json::from_str(&String::from(text)).expect("requests decode");
    for request in &mut recorded {
        if let Some(path) = request.path.strip_prefix("/api/mirmir/v1") {
            request.path = path.to_owned();
        }
    }
    recorded
}

/// Answers the held requests whose path contains `part`.
pub fn release(part: &str) {
    let held = js_sys::Reflect::get(&state(), &"held".into()).expect("held");
    let held = held.unchecked_into::<js_sys::Array>();
    for entry in held.iter() {
        let path = js_sys::Reflect::get(&entry, &"path".into()).expect("path");
        if path.as_string().is_some_and(|path| path.contains(part)) {
            let resolve = js_sys::Reflect::get(&entry, &"resolve".into()).expect("resolve");
            resolve
                .unchecked_into::<js_sys::Function>()
                .call0(&JsValue::NULL)
                .expect("released");
        }
    }
}

/// Waits until `condition` holds, for at most two seconds.
pub async fn until(what: &str, condition: impl Fn() -> bool) {
    for _ in 0..200 {
        if condition() {
            return;
        }
        TimeoutFuture::new(10).await;
    }
    panic!("timed out waiting until {what}");
}

/// Lets pending answers, timers and effects run for `ms`.
pub async fn settle(ms: u32) {
    TimeoutFuture::new(ms).await;
}

/// Mounts `view` in a fresh element of the page; dropping the handle
/// unmounts it.
pub fn mount<N: IntoView + 'static>(
    view: impl FnOnce() -> N + 'static,
) -> (web_sys::HtmlElement, impl Sized) {
    let document = window().document().expect("a document");
    let root = document
        .create_element("div")
        .expect("an element")
        .unchecked_into::<web_sys::HtmlElement>();
    document.body().expect("a body").append_child(&root).expect("mounted");
    let handle = mount_to(root.clone(), view);
    (root, handle)
}

pub fn find<T: JsCast>(root: &web_sys::HtmlElement, selector: &str) -> T {
    root.query_selector(selector)
        .expect("a valid selector")
        .unwrap_or_else(|| panic!("no {selector}"))
        .unchecked_into()
}

/// Dispatches a bubbling `kind` event at `target`.
pub fn dispatch(target: &web_sys::EventTarget, kind: &str) {
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    let event = web_sys::Event::new_with_event_init_dict(kind, &init).expect("an event");
    target.dispatch_event(&event).expect("dispatched");
}
