mod generator;

use std::env;
use tokio::{
    sync::{mpsc, oneshot},
    task,
};
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, error, info};

use gpt2_proto::gpt2::gpt2_server as proto;
use gpt2_proto::gpt2::{GenerateRequest, GeneratedText};

type Responder = oneshot::Sender<String>;
type Message = (GenerateRequest, Responder);
type Sender = mpsc::UnboundedSender<Message>;
type Receiver = mpsc::UnboundedReceiver<Message>;

struct Gpt2 {
    generator_tx: Sender,
}

#[tonic::async_trait]
impl proto::Gpt2 for Gpt2 {
    async fn generate_text(
        &self,
        request: Request<GenerateRequest>,
    ) -> Result<Response<GeneratedText>, Status> {
        let gen_tx = self.generator_tx.clone();
        let (tx, rx) = oneshot::channel::<String>();

        let payload = request.into_inner();

        info!(length=%payload.length, prompt=%payload.prompt, "new generation request");

        gen_tx.send((payload, tx)).map_err(|err| {
            error!(%err, "failed to send generation request");
            Status::internal("failed to contact internal processing loop")
        })?;

        let text: String = rx.await.map_err(|err| {
            error!(%err, "failed to receive generation response");
            Status::internal("internal processing loop failed to reply")
        })?;

        info!(%text, "gpt2 text generated");

        Ok(Response::new(GeneratedText { text }))
    }
}

#[tokio::main]
async fn main() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("info,gpt2_server=debug"))
                .unwrap(),
        )
        .init();

    let (tx, rx) = mpsc::unbounded_channel::<Message>();
    task::spawn_blocking(move || {
        debug!("starting gpt2 loop");
        let res = generator::gpt2(rx);
        debug!(?res, "ending gpt2 loop");
    });

    let gpt2 = Gpt2 { generator_tx: tx };

    let address = env::var("APP_ADDR").unwrap_or_else(|_| "127.0.0.1".into());
    let port = env::var("APP_PORT").unwrap_or_else(|_| "80".into());
    let sockaddr = format!("{}:{}", address, port).parse().unwrap();

    Server::builder()
        .add_service(proto::Gpt2Server::new(gpt2))
        .serve(sockaddr)
        .await
        .unwrap();
}
