mod message;

use std::time::Duration;

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use tokio::time::MissedTickBehavior;

use self::message::ServerMessage;
use super::{configuration::Configuration, security_headers, session, session::WebError, types};
use crate::{
    application::{ActivityEvent, Application},
    http::{ApiState, DashboardUpdate},
};

pub async fn updates(
    State(state): State<ApiState>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, WebError> {
    session::validate_origin(&headers)?;
    state.sessions().authenticate(&headers)?;
    let mut response = upgrade.on_upgrade(move |socket| serve(socket, state)).into_response();
    response.headers_mut().extend(security_headers());
    Ok(response)
}

async fn serve(mut socket: WebSocket, state: ApiState) {
    if send_initial(&mut socket, state.application()).await.is_err() {
        return;
    }
    let mut startup = state.application().startup_updates();
    let mut activity = state.application().activity_updates();
    let mut shutdown = state.shutdown();
    let mut updates = state.updates();
    let mut telemetry = tokio::time::interval(Duration::from_secs(1));
    telemetry.set_missed_tick_behavior(MissedTickBehavior::Skip);
    telemetry.tick().await;
    loop {
        let event = tokio::select! {
            _ = telemetry.tick() => Next::Telemetry,
            result = startup.changed() => Next::Startup(result.is_ok()),
            event = activity.recv() => Next::Activity(event),
            update = updates.recv() => Next::Dashboard(update),
            incoming = socket.recv() => Next::Incoming(incoming),
            _ = shutdown.changed() => Next::Shutdown,
        };
        if !handle(event, &mut socket, &state).await {
            break;
        }
    }
}

enum Next {
    Telemetry,
    Startup(bool),
    Activity(Result<ActivityEvent, tokio::sync::broadcast::error::RecvError>),
    Dashboard(Result<DashboardUpdate, tokio::sync::broadcast::error::RecvError>),
    Incoming(Option<Result<Message, axum::Error>>),
    Shutdown,
}

async fn handle(event: Next, socket: &mut WebSocket, state: &ApiState) -> bool {
    let result = match event {
        Next::Telemetry => send_overview(socket, state.application()).await,
        Next::Startup(true) => send_startup(socket, state.application()).await,
        Next::Startup(false)
        | Next::Shutdown
        | Next::Activity(Err(tokio::sync::broadcast::error::RecvError::Closed))
        | Next::Dashboard(Err(tokio::sync::broadcast::error::RecvError::Closed))
        | Next::Incoming(Some(Ok(Message::Close(_)) | Err(_)) | None) => return false,
        Next::Activity(Ok(event)) => send_activity(socket, state.application(), event).await,
        Next::Dashboard(
            Ok(DashboardUpdate::Configuration)
            | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)),
        ) => send_configuration(socket, state.application()).await,
        Next::Incoming(Some(Ok(Message::Ping(data)))) => socket.send(Message::Pong(data)).await,
        Next::Activity(Err(tokio::sync::broadcast::error::RecvError::Lagged(_)))
        | Next::Incoming(Some(Ok(_))) => Ok(()),
    };
    result.is_ok()
}

async fn send_initial(
    socket: &mut WebSocket,
    application: &Application,
) -> Result<(), axum::Error> {
    send_startup(socket, application).await?;
    send_overview(socket, application).await?;
    send_models(socket, application).await?;
    send_configuration(socket, application).await?;
    for event in application.activity_history() {
        send(socket, ServerMessage::Activity { activity: event.into() }).await?;
    }
    Ok(())
}

async fn send_startup(
    socket: &mut WebSocket,
    application: &Application,
) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(
            serde_json::to_string(&ServerMessage::Startup {
                startup: application.startup_snapshot().into(),
            })
            .expect("websocket startup message should serialize")
            .into(),
        ))
        .await
}

async fn send_overview(
    socket: &mut WebSocket,
    application: &Application,
) -> Result<(), axum::Error> {
    let snapshot = application.telemetry_snapshot().map_err(|error| error.to_string());
    match snapshot {
        Ok(snapshot) => {
            send(socket, ServerMessage::Overview { overview: Box::new(snapshot.into()) }).await
        },
        Err(message) => send(socket, ServerMessage::Error { message }).await,
    }
}

async fn send_models(socket: &mut WebSocket, application: &Application) -> Result<(), axum::Error> {
    match application.local_models() {
        Ok(models) => {
            send(
                socket,
                ServerMessage::Models {
                    models: types::Models {
                        models: models.into_iter().map(Into::into).collect(),
                    },
                },
            )
            .await
        },
        Err(error) => send(socket, ServerMessage::Error { message: error.to_string() }).await,
    }
}

async fn send_configuration(
    socket: &mut WebSocket,
    application: &Application,
) -> Result<(), axum::Error> {
    match application.configuration() {
        Ok(configuration) => {
            send(
                socket,
                ServerMessage::Configuration {
                    configuration: Configuration::from(configuration),
                },
            )
            .await
        },
        Err(error) => send(socket, ServerMessage::Error { message: error.to_string() }).await,
    }
}

async fn send_activity(
    socket: &mut WebSocket,
    application: &Application,
    event: ActivityEvent,
) -> Result<(), axum::Error> {
    let refresh_models = event.state.is_terminal() && event.kind.changes_models();
    send(socket, ServerMessage::Activity { activity: event.into() }).await?;
    if refresh_models {
        send_models(socket, application).await?;
    }
    Ok(())
}

async fn send(socket: &mut WebSocket, message: ServerMessage) -> Result<(), axum::Error> {
    let json = serde_json::to_string(&message).expect("websocket server message should serialize");
    socket.send(Message::Text(json.into())).await
}
