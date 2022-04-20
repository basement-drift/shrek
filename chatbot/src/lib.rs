use futures::future::{BoxFuture, FutureExt};
use futures::stream::{Stream, StreamExt};
use regex::Regex;
use slack::Message;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, warn};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Regex(#[from] regex::Error),

    #[error(transparent)]
    Slack(#[from] slack::Error),
}

type Sender = broadcast::Sender<Arc<Message>>;

pub struct Chatbot {
    slack: slack::Client,
    tx: Sender,
    raw_tx: Sender,
}

impl Chatbot {
    pub async fn new(slack: slack::Client) -> Result<Self, Error> {
        let (tx, _) = broadcast::channel(256);
        let (raw_tx, _) = broadcast::channel(256);

        Ok(Self { slack, tx, raw_tx })
    }

    pub fn slack(&self) -> slack::Client {
        self.slack.clone()
    }

    pub fn messages(&self) -> impl Stream<Item = Arc<Message>> {
        subscribe(&self.tx)
    }

    pub fn raw_messages(&self) -> impl Stream<Item = Arc<Message>> {
        subscribe(&self.raw_tx)
    }

    pub fn reply_all<F>(&self, reply: F) -> Result<&Self, Error>
    where
        F: 'static + Sync + Send + Fn(&Message) -> Option<String>,
    {
        self.listen(reply, move |reply, client, msg| {
            async move {
                if let Some(rep) = reply(msg) {
                    let p = client.post(&msg.channel, &rep, msg.thread_ts.as_ref());
                    p.await?;
                }

                Ok::<(), slack::Error>(())
            }
            .boxed()
        })
    }

    pub fn reply_with<S, F>(&self, regex: S, reply: F) -> Result<&Self, Error>
    where
        S: AsRef<str>,
        F: 'static + Sync + Send + Fn(&Message, regex::Captures) -> Option<String>,
    {
        let re = Regex::new(regex.as_ref())?;

        self.reply_all(move |msg| {
            let captures = re.captures(&msg.text);
            captures.map(|c| reply(msg, c)).flatten()
        })
    }

    pub fn listen<T, F, E>(&self, context: T, action: F) -> Result<&Self, Error>
    where
        T: Send + Sync + 'static,
        E: std::fmt::Display,
        F: for<'a> Fn(&'a T, &'a slack::Client, &'a Message) -> BoxFuture<'a, Result<(), E>>
            + Send
            + Sync
            + 'static,
    {
        let messages = self.messages();
        let conn = self.slack();

        task::spawn(async move {
            let messages = messages.map(|m| (&action, &context, &conn, m));

            let f = messages.for_each_concurrent(None, |(action, context, conn, msg)| async move {
                match action(context, conn, &msg).await {
                    Ok(_) => (),
                    Err(error) => error!(%error, "failure in listen loop"),
                }
            });

            f.await;
        });

        Ok(self)
    }

    pub fn reply_all_async<F, T>(&self, context: T, reply: F) -> Result<&Self, Error>
    where
        T: Send + Sync + 'static,
        F: for<'a> Fn(&'a T, &'a Message) -> BoxFuture<'a, Option<String>> + 'static + Sync + Send,
    {
        self.listen((context, reply), |(context, reply), conn, msg| {
            async move {
                if let Some(rep) = reply(context, msg).await {
                    conn.post(&msg.channel, &rep, msg.thread_ts.as_ref())
                        .await?
                }

                Ok::<(), slack::Error>(())
            }
            .boxed()
        })
    }

    pub fn reply_with_async<S, F, T>(&self, regex: S, context: T, reply: F) -> Result<&Self, Error>
    where
        S: AsRef<str>,
        T: Send + Sync + 'static,
        F: for<'a> Fn(&'a T, &'a Message, regex::Captures<'a>) -> BoxFuture<'a, Option<String>>
            + 'static
            + Sync
            + Send,
    {
        let re = Regex::new(regex.as_ref())?;

        self.reply_all_async((context, re, reply), |(context, re, reply), msg| {
            async move {
                match re.captures(&msg.text) {
                    Some(capture) => reply(context, msg, capture).await,
                    None => None,
                }
            }
            .boxed()
        })
    }

    pub fn reply(&self, regex: impl AsRef<str>, reply: &'static str) -> Result<&Self, Error> {
        self.reply_with(regex, |_, _| Some(reply.to_string()))
    }

    pub async fn run(&self, messages: impl Stream<Item = Message>) -> Result<(), Error> {
        messages
            .for_each(|m| async move {
                let msg = Arc::new(m);

                // TODO: can we eliminate this clone?
                self.raw_tx.send(msg.clone()).ok();

                if msg.user != self.slack.bot_user_id() {
                    self.tx.send(msg).ok();
                }
            })
            .await;

        Ok(())
    }
}

fn subscribe(tx: &Sender) -> impl Stream<Item = Arc<Message>> {
    BroadcastStream::new(tx.subscribe()).filter_map(|res| async move {
        match res {
            Ok(m) => Some(m),
            Err(err) => {
                warn!(?err, "stream lagged");
                None
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2, 4);
    }
}
