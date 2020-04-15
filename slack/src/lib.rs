use bytes::Bytes;
use const_format::concatcp;
use futures::channel::mpsc;
use futures::sink::SinkExt;
use futures::stream::{StreamExt, TryStreamExt};
use futures::Future;
use serde::Deserialize;
use serde_json::json;
use std::borrow::Cow;
use std::collections::HashMap;
use tracing::{debug, trace, warn};

const API_URL: &str = "https://slack.com/api/";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    Serialization(#[from] serde_json::Error),

    #[error("slack API error: {0}")]
    Api(String),

    #[error(transparent)]
    WebSocket(#[from] async_tungstenite::tungstenite::error::Error),
}

/// Slack uses message timestamps as IDs. These are formatted somewhat strangely, as Unix epoch
/// timestamps with microseconds represented as a decimal (e.g. "1636048583.000400"). These have
/// been left in String format, since they would require a custom deserializer and a lexicographical
/// sort still puts them in chronological order.
pub type Timestamp = String;

#[derive(Debug, Deserialize, Clone)]
pub struct Message {
    pub text: String,

    pub user: String,
    pub ts: Timestamp,
    pub thread_ts: Option<Timestamp>,

    #[serde(default)]
    pub reply_count: u32,

    #[serde(default)]
    pub channel: String,

    #[serde(default)]
    pub is_mention: bool,
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    app_token: String,
    bot_token: String,
    bot_user_id: String,
}

impl Client {
    pub async fn new(app_token: String, bot_token: String) -> Result<Self, Error> {
        let http = reqwest::Client::new();

        Ok(Self {
            bot_user_id: bot_user_id(&http, &bot_token).await?,
            http,
            app_token,
            bot_token,
        })
    }

    pub async fn post(
        &self,
        channel: &str,
        text: &str,
        parent: Option<&Timestamp>,
    ) -> Result<(), Error> {
        let req = json!({
            "channel": channel,
            "text": text,
            "thread_ts": parent,
        });

        let body = self
            .http
            .post(concatcp!(API_URL, "chat.postMessage"))
            .bearer_auth(&self.bot_token)
            .json(&req)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        deserialize(&body)?;

        Ok(())
    }

    pub fn bot_user_id(&self) -> &str {
        &self.bot_user_id
    }

    pub async fn event_url(&self) -> Result<String, Error> {
        #[derive(Debug, Deserialize)]
        struct Response {
            url: String,
        }

        let body = self
            .http
            .post(concatcp!(API_URL, "apps.connections.open"))
            .bearer_auth(&self.app_token)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        let res: Response = deserialize(&body)?;

        Ok(res.url)
    }

    pub fn messages(&self) -> (impl Future<Output = ()>, mpsc::UnboundedReceiver<Message>) {
        let (tx, rx) = mpsc::unbounded();

        let cli = self.clone();
        let driver = async move {
            // TODO: backoff
            loop {
                let result = cli.process_messages(tx.clone()).await;
                warn!(?result, "websocket loop ended, restarting");
            }
        };

        (driver, rx)
    }

    // TODO: simply/break out to more fns
    async fn process_messages(&self, mut tx: mpsc::UnboundedSender<Message>) -> Result<(), Error> {
        use async_tungstenite::tungstenite;

        #[allow(dead_code)]
        #[derive(Debug, Deserialize)]
        #[serde(tag = "type")]
        #[serde(rename_all = "snake_case")]
        enum Response {
            EventsApi {
                envelope_id: String,
                payload: Payload,
            },
            Hello {
                num_connections: u32,
            },
            Disconnect {
                reason: String,
            },
        }

        #[derive(Debug, Deserialize)]
        struct Payload {
            event: Event,
        }

        #[derive(Debug, Deserialize)]
        #[serde(tag = "type")]
        #[serde(rename_all = "snake_case")]
        enum Event {
            Message {
                #[serde(flatten)]
                message: Option<Message>,
            },
            AppMention,
        }

        let url = self.event_url().await?;

        debug!(%url, "connecting to websocket");
        let (ws, _) = async_tungstenite::tokio::connect_async(url).await?;

        let (mut sink, mut stream) = ws.split();

        while let Some(response) = stream.try_next().await? {
            let text = response.into_text()?;

            trace!(%text, "websocket message received");

            if text.starts_with("Ping") {
                continue;
            }

            let payload = match serde_json::from_str::<Response>(&text)? {
                Response::EventsApi {
                    envelope_id,
                    payload,
                } => {
                    let ack = json!({ "envelope_id": envelope_id });
                    trace!(%ack, "sending websocket ack");
                    sink.send(tungstenite::Message::text(ack.to_string()))
                        .await?;
                    payload
                }
                Response::Disconnect { reason } => {
                    debug!(%reason, "websocket disconnect sent");
                    return Err(Error::Api("websocket_disconnect".into()));
                }
                _ => continue,
            };

            if let Event::Message {
                message: Some(mut msg),
            } = payload.event
            {
                if msg.text.contains(&self.bot_user_id) {
                    msg.is_mention = true;
                }

                if !msg.text.is_empty() {
                    // TODO: we don't care if we drop a few messages, but log this
                    tx.send(msg).await.ok();
                }
            }
        }

        Ok(())
    }

    pub async fn emoji_list(&self) -> Result<Vec<String>, Error> {
        #[derive(Deserialize)]
        struct Response<'a> {
            emoji: HashMap<Cow<'a, str>, Cow<'a, str>>,
        }

        let body = self
            .http
            .get(concatcp!(API_URL, "emoji.list"))
            .bearer_auth(&self.bot_token)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        Ok(deserialize::<Response>(&body)?
            .emoji
            .into_keys()
            .map(|k| k.to_string())
            .collect())
    }

    pub async fn react(&self, message: &Message, emoji: &str) -> Result<(), Error> {
        let req = json!({
            "channel": message.channel,
            "timestamp": message.ts,
            "name": emoji,
        });

        let body = self
            .http
            .post(concatcp!(API_URL, "reactions.add"))
            .bearer_auth(&self.bot_token)
            .json(&req)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        deserialize(&body)?;

        Ok(())
    }

    /// Returns the IDs for all channels that the bot is currently a member of.
    pub async fn channel_ids(&self) -> Result<Vec<String>, Error> {
        #[derive(Debug, Deserialize)]
        struct Response {
            channels: Vec<Channel>,
        }

        #[derive(Debug, Deserialize)]
        struct Channel {
            id: String,
        }

        let body = self
            .http
            .get(concatcp!(API_URL, "users.conversations"))
            .bearer_auth(&self.bot_token)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        Ok(deserialize::<Response>(&body)?
            .channels
            .into_iter()
            .map(|c| c.id)
            .collect())
    }

    pub async fn channel_history(&self, channel_id: &str) -> Result<Vec<Message>, Error> {
        #[derive(Debug, Deserialize)]
        struct Response {
            messages: Vec<Message>,
        }

        let body = self
            .http
            .get(concatcp!(API_URL, "conversations.history"))
            .bearer_auth(&self.bot_token)
            .query(&[("channel", channel_id)])
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        Ok(add_channel(
            deserialize::<Response>(&body)?.messages,
            channel_id,
        ))
    }

    pub async fn replies(&self, channel_id: &str, ts: &str) -> Result<Vec<Message>, Error> {
        #[derive(Debug, Deserialize)]
        struct Response {
            messages: Vec<Message>,
        }

        let body = self
            .http
            .get(concatcp!(API_URL, "conversations.replies"))
            .bearer_auth(&self.bot_token)
            .query(&[("channel", channel_id), ("ts", ts)])
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        Ok(add_channel(
            deserialize::<Response>(&body)?.messages,
            channel_id,
        ))
    }

    pub async fn display_name(&self, user_id: &str) -> Result<String, Error> {
        #[derive(Deserialize)]
        struct Response {
            profile: Profile,
        }

        #[derive(Deserialize, Debug)]
        struct Profile {
            display_name: String,
            real_name: String,
        }

        let body = self
            .http
            .get(concatcp!(API_URL, "users.profile.get"))
            .bearer_auth(&self.bot_token)
            .query(&[("user", user_id)])
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        let profile = deserialize::<Response>(&body)?.profile;

        if profile.display_name.is_empty() {
            Ok(profile.real_name)
        } else {
            Ok(profile.display_name)
        }
    }

    pub async fn upload_reply(
        &self,
        parent: &Message,
        filename: &str,
        content: Bytes,
    ) -> Result<(), Error> {
        use reqwest::multipart::{Form, Part};

        let form = Form::new()
            .text("channels", parent.channel.clone())
            .text("thread_ts", parent.ts.clone())
            .text("title", filename.to_string())
            .part(
                "file",
                Part::stream(content).file_name(filename.to_string()),
            );

        let body = self
            .http
            .post(concatcp!(API_URL, "files.upload"))
            .bearer_auth(&self.bot_token)
            //.query(&[("channels", &parent.channel), ("thread_ts", &parent.ts)])
            //.query(&[("filename", filename)])
            .multipart(form)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        deserialize(&body)?;

        Ok(())
    }
}

fn add_channel(messages: Vec<Message>, channel: &str) -> Vec<Message> {
    messages
        .into_iter()
        .map(|mut msg| {
            msg.channel = channel.to_string();
            msg
        })
        .collect()
}

// TODO: find a better name for this
fn deserialize<'de, T: Deserialize<'de>>(input: &'de str) -> Result<T, Error> {
    // Slack's API returns HTTP 200 on application failure, and stashes error information directly in
    // JSON responses to apparently successful calls.
    #[derive(Deserialize, Debug)]
    struct Response<T> {
        ok: bool,
        error: Option<String>,

        #[serde(flatten)]
        payload: Option<T>,
    }

    match serde_json::from_str::<Response<T>>(input)? {
        Response {
            ok: true,
            payload: Some(t),
            ..
        } => Ok(t),
        Response {
            ok: false,
            error: Some(e),
            ..
        } => Err(Error::Api(e)),
        _ => Err(Error::Api(format!("unexpected format: {}", input))),
    }
}

async fn bot_user_id(http: &reqwest::Client, bot_token: &str) -> Result<String, Error> {
    #[derive(Debug, Deserialize)]
    struct Response {
        user_id: String,
    }

    let body = http
        .get(concatcp!(API_URL, "auth.test"))
        .bearer_auth(bot_token)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let res: Response = deserialize(&body)?;

    Ok(res.user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotenv::dotenv;
    use std::env;

    #[tokio::test]
    async fn posting() {
        dotenv().unwrap();

        let client = Client::new(
            env::var("APP_TOKEN").unwrap(),
            env::var("BOT_TOKEN").unwrap(),
        )
        .await
        .unwrap();

        let msg = Message {
            text: "asdf".into(),
            user: "UL6H0F39R".into(),
            ts: "1636048583.000400".into(),
            thread_ts: Some("1636047059.000300".into()),
            reply_count: 0,
            channel: "CLXKXACCF".into(),
            is_mention: false,
        };

        let file = Bytes::from("hey there");

        println!("{:#?}", client.upload_reply(&msg, "test.txt", file).await);
    }
}
