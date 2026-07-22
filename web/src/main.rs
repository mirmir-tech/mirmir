#![allow(clippy::cast_precision_loss)]
#![allow(clippy::future_not_send)]
#![allow(clippy::struct_excessive_bools, clippy::struct_field_names)]
#![cfg_attr(feature = "capture", allow(dead_code))]

mod api;
mod app;
mod cleanup;
mod components;
mod connection;
#[cfg(feature = "capture")]
mod demo;
mod pages;
mod state;
mod types;

use app::App;
use leptos::prelude::*;

fn main() {
    mount_to_body(|| view! { <App /> });
}
