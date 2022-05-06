use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use gpt2_client::{GenerateRequest, Gpt2Client};

use std::net::SocketAddr;

async fn handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    // TODO: share common client pool
    let mut client = Gpt2Client::connect("http://van-cleef.self.wg:50080")
        .await
        .unwrap();

    while let Some(msg) = socket.recv().await {
        let msg = if let Ok(msg) = msg {
            msg
        } else {
            // client disconnected
            return;
        };

        let msg = match msg {
            Message::Text(s) => s,
            _ => return,
        };

        let text = client
            .generate_text(GenerateRequest {
                length: 100,
                prompt: msg,
            })
            .await
            .unwrap()
            .into_inner()
            .text;

        if socket.send(Message::Text(text)).await.is_err() {
            // client disconnected
            return;
        }
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/ws", get(handler));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
