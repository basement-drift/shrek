mod generator;

use std::borrow::Cow;
use std::env;

use once_cell::unsync::Lazy;
use regex::Regex;
use tokio::{
    sync::{mpsc, oneshot},
    task,
};
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, error, info};

use gpt2_proto::gpt2::gpt2_server as proto;
use gpt2_proto::gpt2::{GenerateRequest, GeneratedText, ScriptRequest};

type Responder = oneshot::Sender<String>;

struct Message {
    length: u32,
    prompt: String,
    responder: Responder,
}

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

        gen_tx
            .send(Message {
                length: payload.length,
                prompt: payload.prompt,
                responder: tx,
            })
            .map_err(|err| {
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

    async fn generate_script(
        &self,
        request: Request<ScriptRequest>,
    ) -> Result<Response<GeneratedText>, Status> {
        let gen_tx = self.generator_tx.clone();
        let (tx, rx) = oneshot::channel::<String>();

        let payload = request.into_inner();

        info!(length=%payload.length, prompt=%payload.script, speaker=%payload.next_speaker, "new script request");

        let prompt = format!(
            "{}\n{}: ",
            payload.script,
            payload.next_speaker.to_uppercase()
        );

        gen_tx
            .send(Message {
                length: payload.length,
                prompt,
                responder: tx,
            })
            .map_err(|err| {
                error!(%err, "failed to send generation request");
                Status::internal("failed to contact internal processing loop")
            })?;

        let text: String = rx.await.map_err(|err| {
            error!(%err, "failed to receive generation response");
            Status::internal("internal processing loop failed to reply")
        })?;

        info!(%text, "gpt2 text generated");

        let raw = strip_weird_unicode(&text);
        let raw = strip_trailing_thoughts(&raw);
        let clean = strip_incomplete_sentences(raw);

        Ok(Response::new(GeneratedText {
            text: clean.to_string(),
        }))
    }
}

fn strip_weird_unicode(input: &str) -> Cow<'_, str> {
    let re = Lazy::new(|| Regex::new("\u{202a}").unwrap());
    re.replace_all(input, " ")
}

// Remove any trailing incomplete sentences, while allowing a standalone sentence fragment
fn strip_incomplete_sentences(input: &str) -> Cow<'_, str> {
    // Define the "punc" named capture group, and have the whole regex match a period, exclamation
    // point, or question mark followed by at least one other character.
    let re = Lazy::new(|| Regex::new(r"(?P<punc>[.!?])[^.!?]+\z").unwrap());

    // This will find the left-most match of the regex above (a sentence-ending punctuation mark
    // followed by some number of non-punctuation characters), and replace it with the found mark.
    // This will truncate incomplete sentences, but leave input that is just a sentence fragment
    // alone (since the regex will only match punctuation).
    re.replace(input, "$punc")
}

// Remove shrek's inner monologue. Since we're generating text by formatting slack history as a
// script or chat-log, the gpt2 model will continue to generate more entries in this fashion. We're
// only interested shrek's next "line". This will will find the boundaries of that line, and remove
// whatever follows.
fn strip_trailing_thoughts(input: &str) -> &str {
    let re = Lazy::new(|| {
        // Scriptlikes, e.g "DONKEY: "
        let scriptlikes = r"(:?\b|^)[[:upper:][:punct:]&&[^:] ]{3,}:";

        // More permissive script-likes, but only at line starts
        let line_start = r"^[[:word:][:punct:]#.']{3,}:";

        // Lines that only have stage direction, e.g. `[Shrek kisses Fiona]`
        let stage_direction = r"^[(\[].*[)\]]$";

        // Stop claiming that people have left the conversation.
        let left_convo = r"has left the conversation.$";

        // All patterns combined into an alternation, with multi-line mode enabled. Note that
        // we cannot use a RegexSet here, since those don't support splitting.
        let combined = format!(
            "(?m){}|{}|{}|{}",
            scriptlikes, line_start, stage_direction, left_convo
        );

        Regex::new(&combined).unwrap()
    });

    // Collect into a vector so we can log it.
    let split: Vec<_> = re.splitn(input, 2).collect();
    debug!(?split);

    split.first().unwrap()
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
