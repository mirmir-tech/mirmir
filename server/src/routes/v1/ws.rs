use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
};

pub async fn upgrade(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    let message = Message::Text("mirmir websocket endpoint is online".into());
    let _ignored = socket.send(message).await;
}
