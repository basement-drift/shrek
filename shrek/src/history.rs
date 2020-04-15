use eyre::{eyre, Result};
use futures::{
    channel::mpsc,
    future::ready,
    sink::SinkExt,
    stream,
    stream::{Stream, StreamExt, TryStreamExt},
};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tracing::debug;

#[derive(Clone)]
pub struct History {
    workspace: Arc<RwLock<Workspace>>,
    users: UserCache,
    changed: broadcast::Sender<()>,
}

impl History {
    pub fn new(slack: slack::Client) -> Self {
        let (changed, _) = broadcast::channel(1);
        Self {
            workspace: Arc::new(RwLock::new(Workspace::default())),
            users: UserCache::new(slack),
            changed,
        }
    }

    pub async fn monitor(&self, bot: &chatbot::Chatbot) -> Result<()> {
        let (tx, rx) = mpsc::unbounded::<Arc<slack::Message>>();

        let workspace = self.workspace.clone();
        let changed = self.changed.clone();

        tokio::task::spawn(async move {
            rx.filter(|msg| ready(!msg.is_mention))
                .for_each(|msg| {
                    workspace.write().unwrap().insert(msg);
                    changed.send(()).ok();
                    ready(())
                })
                .await;
        });

        Self::send_history(&bot.slack(), tx.clone()).await?;

        tokio::task::spawn(bot.raw_messages().map(Ok).forward(tx));

        Ok(())
    }

    async fn send_history(
        slack: &slack::Client,
        tx: mpsc::UnboundedSender<Arc<slack::Message>>,
    ) -> Result<()> {
        let sink = tx.sink_err_into::<eyre::Error>();

        stream::iter(slack.channel_ids().await?)
            .then(|chan| Self::channel_history(slack, chan))
            .try_flatten()
            .and_then(|msg| Self::thread_history(slack, msg))
            .try_flatten()
            .map_ok(Arc::new)
            .forward(sink)
            .await?;

        Ok(())
    }

    async fn channel_history(
        slack: &slack::Client,
        channel: String,
    ) -> Result<impl Stream<Item = Result<slack::Message>>> {
        let hist: Vec<_> = slack.channel_history(&channel).await?;

        Ok(stream::iter(hist).map(Ok))
    }

    async fn thread_history(
        slack: &slack::Client,
        msg: slack::Message,
    ) -> Result<impl Stream<Item = Result<slack::Message>>> {
        let thread = match &msg.thread_ts {
            Some(thread_ts) => slack.replies(&msg.channel, thread_ts).await?,
            None => vec![msg],
        };

        Ok(stream::iter(thread).map(Ok))
    }

    pub async fn script(&self, msg: &slack::Message, length: usize) -> Result<String> {
        self.wait(msg).await;

        // TODO: rework with tokio rwlock
        let messages: Vec<_> = {
            let workspace = self.workspace.read().unwrap();
            let history = workspace
                .history(msg)
                .ok_or_else(|| eyre!("could not retrieve history"))?;

            history.take(length).cloned().collect()
        };

        let mut script = vec![];

        for msg in messages {
            let user = self.users.get(&msg.user).await?;
            script.push(format!("{}: {}", user, msg.text.trim()));
        }

        script.reverse();

        Ok(script.join("\n"))
    }

    async fn wait(&self, msg: &slack::Message) {
        let mut changed = self.changed.subscribe();

        while !self.contains(msg) {
            debug!(ts=%msg.ts, channel=%msg.channel, "message not found in history, waiting");
            changed.recv().await.ok();
        }
    }

    fn contains(&self, msg: &slack::Message) -> bool {
        let workspace = self.workspace.read().unwrap();
        workspace.contains(msg)
    }

    pub fn parent(&self, msg: &slack::Message) -> Option<Arc<slack::Message>> {
        let workspace = self.workspace.read().unwrap();
        workspace.parent(msg)
    }
}

#[derive(Default, Debug)]
struct Workspace {
    channels: HashMap<String, Channel>,
}

impl Workspace {
    fn insert(&mut self, msg: Arc<slack::Message>) {
        let channel = self.channels.entry(msg.channel.clone()).or_default();

        channel.insert(msg);
    }

    fn contains(&self, msg: &slack::Message) -> bool {
        match self.channels.get(&msg.channel) {
            Some(c) => c.contains(msg),
            None => false,
        }
    }

    fn history(&self, msg: &slack::Message) -> Option<impl Iterator<Item = &slack::Message>> {
        self.channels.get(&msg.channel).map(|c| c.history(msg))
    }

    fn parent(&self, msg: &slack::Message) -> Option<Arc<slack::Message>> {
        self.channels.get(&msg.channel).and_then(|c| c.parent(msg))
    }
}

#[derive(Default, Debug)]
struct Channel {
    main: Thread,
    threads: HashMap<slack::Timestamp, Thread>,
}

impl Channel {
    fn insert(&mut self, msg: Arc<slack::Message>) {
        let thread = match &msg.thread_ts {
            Some(ts) if ts == &msg.ts => &mut self.main,
            Some(ts) => self.threads.entry(ts.clone()).or_default(),
            None => &mut self.main,
        };

        thread.insert(msg);
    }

    fn history(&self, msg: &slack::Message) -> impl Iterator<Item = &slack::Message> {
        let (main_ts, thread) = msg
            .thread_ts
            .as_ref()
            .and_then(|thread_ts| self.threads.get_key_value(thread_ts))
            .map(|(thread_ts, thread)| (thread_ts, thread.history(&msg.ts)))
            .unwrap_or_else(|| (&msg.ts, self.main.history(&msg.ts)));

        thread.chain(self.main.history(main_ts))
    }

    fn thread(&self, msg: &slack::Message) -> Option<&Thread> {
        match &msg.thread_ts {
            Some(ts) if ts == &msg.ts => Some(&self.main),
            Some(ts) => self.threads.get(ts),
            None => Some(&self.main),
        }
    }

    fn contains(&self, msg: &slack::Message) -> bool {
        match self.thread(msg) {
            Some(thread) => thread.contains(&msg.ts),
            None => false,
        }
    }

    fn parent(&self, msg: &slack::Message) -> Option<Arc<slack::Message>> {
        msg.thread_ts.as_ref().and_then(|ts| self.main.get(ts))
    }
}

#[derive(Default, Debug)]
struct Thread {
    thread: Vec<Arc<slack::Message>>,
}

impl Thread {
    fn history(&self, ts: &slack::Timestamp) -> impl Iterator<Item = &slack::Message> {
        let idx = self.find(ts).unwrap_or_else(|x| x.saturating_sub(1));
        self.thread[0..=idx].iter().map(|m| m.deref()).rev()
    }

    fn find(&self, ts: &slack::Timestamp) -> Result<usize, usize> {
        self.thread.binary_search_by(|m| m.ts.cmp(ts))
    }

    fn insert(&mut self, msg: Arc<slack::Message>) {
        self.thread
            .insert(self.find(&msg.ts).unwrap_or_else(|x| x), msg);
    }

    fn contains(&self, ts: &slack::Timestamp) -> bool {
        self.find(ts).is_ok()
    }

    fn get(&self, ts: &slack::Timestamp) -> Option<Arc<slack::Message>> {
        self.find(ts).ok().map(|idx| self.thread[idx].clone())
    }
}

// TODO: handle username updates (events?)
#[derive(Clone)]
struct UserCache {
    slack: slack::Client,
    id_map: Arc<RwLock<HashMap<String, String>>>,
}

impl UserCache {
    fn new(slack: slack::Client) -> Self {
        Self {
            id_map: Arc::default(),
            slack,
        }
    }

    async fn get(&self, id: &str) -> Result<String> {
        match self.read(id) {
            Some(d) => Ok(d),
            None => {
                let name = self.slack.display_name(id).await?.to_uppercase();
                self.insert(id.to_string(), name.clone());
                Ok(name)
            }
        }
    }

    fn insert(&self, id: String, name: String) {
        self.id_map.write().unwrap().insert(id, name);
    }

    fn read(&self, id: &str) -> Option<String> {
        self.id_map.read().unwrap().get(id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotenv::dotenv;
    use futures::stream::StreamExt;
    use std::env;

    #[tokio::test]
    async fn history() {
        dotenv().unwrap();

        let client = slack::Client {
            http: reqwest::Client::new(),
            app_token: env::var("APP_TOKEN").unwrap(),
            bot_token: env::var("BOT_TOKEN").unwrap(),
        };

        let (driver, messages) = client.messages();
        tokio::task::spawn(driver);

        let bot = chatbot::Chatbot::new(client).await.unwrap();
        let mut history = History::new(bot.slack());
        history.monitor(&bot).await.unwrap();
        let mut raw = Box::pin(bot.raw_messages());

        tokio::task::spawn(async move { bot.run(messages).await });

        /*
        let msg = slack::Message {
            text: "asdf".into(),
            user: "UL6H0F39R".into(),
            ts: "1636048583.000400".into(),
            thread_ts: Some("1636047059.000300".into()),
            reply_count: 0,
            channel: "CLXKXACCF".into(),
        };
        */

        println!("done waiting for monitor");

        while let Some(msg) = raw.next().await {
            let script = history.script(&msg, 5).await.unwrap();
            println!("{}\n\n", script);
        }
    }
}
