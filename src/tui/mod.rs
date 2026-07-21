mod app;
mod chat;
mod configuration;
mod confirm;
mod help;
mod load;
mod models;
mod overview;
mod render;
mod startup;
mod terminal;
#[cfg(test)]
mod tests;
mod theme;

use std::time::Duration;

use crossterm::event::EventStream;
use futures_util::StreamExt;
use tokio::time::MissedTickBehavior;

use self::{
    app::{App, ChatStatus, RefreshSnapshot},
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
    let result = dashboard(&mut connection.client, reused).await;
    result.and(connection.shutdown().await)
}

async fn dashboard(client: &mut rpc::Client, server_reused: bool) -> Result<()> {
    let mut terminal = TerminalGuard::new()?;
    let mut events = EventStream::new();
    let mut refresh = tokio::time::interval(Duration::from_secs(1));
    refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut render_tick = tokio::time::interval(Duration::from_millis(16));
    render_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut chat_telemetry = tokio::time::interval(Duration::from_millis(250));
    chat_telemetry.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut app = App::new(server_reused);
    let mut refresh_task = Some(begin_refresh(client));
    let mut chat_telemetry_task = None;
    app.begin_activity(client);
    if !server_reused {
        app.begin_restore(client);
    }

    loop {
        if refresh_task.as_ref().is_some_and(tokio::task::JoinHandle::is_finished) {
            let result = refresh_task
                .take()
                .expect("finished refresh task must exist")
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            app.apply_refresh(result);
        }
        if chat_telemetry_task.as_ref().is_some_and(tokio::task::JoinHandle::is_finished) {
            let result = chat_telemetry_task
                .take()
                .expect("finished chat telemetry task must exist")
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            app.apply_chat_telemetry(result);
        }
        app.poll_operations(client);
        app.poll_chat();
        app.poll_chat_settings();
        app.poll_activity();
        terminal.draw(|frame| render::draw(frame, &mut app))?;
        tokio::select! {
            biased;
            event = events.next() => {
                let event = event.transpose()?;
                if app.handle(event.as_ref(), client).await {
                    break;
                }
            },
            _ = refresh.tick(), if refresh_task.is_none() => {
                refresh_task = Some(begin_refresh(client));
            },
            _ = chat_telemetry.tick(),
                if app.chat_status == ChatStatus::Generating && chat_telemetry_task.is_none() => {
                chat_telemetry_task = Some(begin_chat_telemetry(client));
            },
            _ = render_tick.tick() => app.advance_animation(),
        }
    }
    if let Some(task) = refresh_task {
        task.abort();
    }
    if let Some(task) = chat_telemetry_task {
        task.abort();
    }
    app.stop_activity().await;
    Ok(())
}

fn begin_chat_telemetry(
    client: &rpc::Client,
) -> tokio::task::JoinHandle<std::result::Result<rpc::proto::TelemetrySnapshot, String>> {
    let mut client = client.clone();
    tokio::spawn(async move {
        tokio::time::timeout(
            Duration::from_secs(1),
            client.telemetry(rpc::proto::TelemetryRequest {}),
        )
        .await
        .map_err(|_| "chat telemetry timed out".to_owned())?
        .map(tonic::Response::into_inner)
        .map_err(|error| error.to_string())
    })
}

fn begin_refresh(
    client: &rpc::Client,
) -> tokio::task::JoinHandle<std::result::Result<RefreshSnapshot, String>> {
    let client = client.clone();
    tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(10), App::fetch_refresh(client))
            .await
            .unwrap_or_else(|_| Err("runtime refresh timed out".to_owned()))
    })
}
