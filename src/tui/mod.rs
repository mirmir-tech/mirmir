mod activity;
mod app;
mod benchmarks;
mod chat;
mod configuration;
mod confirm;
mod help;
mod load;
mod models;
mod overview;
mod render;
mod terminal;
#[cfg(test)]
mod tests;
mod theme;

use std::time::Duration;

use crossterm::event::EventStream;
use futures_util::StreamExt;
use tokio::time::MissedTickBehavior;

use self::{
    app::{App, ChatStatus},
    terminal::TerminalGuard,
};
use crate::{
    config::{AppConfig, Paths},
    daemon,
    error::Result,
    rpc,
};

pub async fn run(paths: Paths, config: AppConfig) -> Result<()> {
    let mut connection = daemon::connect_or_start(&paths, &config).await?;
    let reused = connection.reused();
    let result =
        dashboard(&mut connection.client, reused, paths.state_dir.join("benchmarks")).await;
    result.and(connection.shutdown().await)
}

async fn dashboard(
    client: &mut rpc::Client,
    server_reused: bool,
    benchmark_dir: std::path::PathBuf,
) -> Result<()> {
    let mut terminal = TerminalGuard::new()?;
    let mut events = EventStream::new();
    let mut refresh = tokio::time::interval(Duration::from_secs(1));
    refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut render_tick = tokio::time::interval(Duration::from_millis(16));
    render_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut chat_telemetry = tokio::time::interval(Duration::from_millis(250));
    chat_telemetry.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut app = App::new(server_reused);
    app.set_benchmark_export_dir(benchmark_dir);
    app.refresh(client).await;
    app.begin_activity(client);
    if !server_reused {
        app.begin_restore(client).await;
    }

    loop {
        app.poll_operations(client);
        app.poll_chat();
        app.poll_chat_settings();
        app.poll_benchmark();
        app.poll_activity();
        terminal.draw(|frame| render::draw(frame, &mut app))?;
        tokio::select! {
            _ = refresh.tick() => app.refresh(client).await,
            _ = chat_telemetry.tick(), if app.chat_status == ChatStatus::Generating => {
                app.refresh_chat_telemetry(client).await;
            },
            _ = render_tick.tick() => app.advance_animation(),
            event = events.next() => {
                let event = event.transpose()?;
                if app.handle(event.as_ref(), client).await {
                    break;
                }
            },
        }
    }
    app.stop_activity().await;
    Ok(())
}
