use js_sys::Array;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::ServiceWorkerRegistration;

pub fn remove_legacy_service_workers() {
    let Some(window) = web_sys::window() else {
        return;
    };
    spawn_local(async move {
        let container = window.navigator().service_worker();
        let Ok(registrations) = JsFuture::from(container.get_registrations()).await else {
            return;
        };
        for registration in Array::from(&registrations).iter() {
            let Ok(registration) = registration.dyn_into::<ServiceWorkerRegistration>() else {
                continue;
            };
            if let Ok(promise) = registration.unregister() {
                drop(JsFuture::from(promise).await);
            }
        }
    });
}
