use chatbot::Chatbot;
use chrono::{prelude::*, Duration};
use futures::{stream, StreamExt};
use rand::prelude::*;
use tracing::trace;

pub fn add(bot: &Chatbot) {
    let conn = bot.slack();
    let messages = bot.messages();

    tokio::task::spawn(async move {
        let cache = EmojiCache::new(conn.clone());

        let rand = stream::unfold(cache, move |mut cache| async move {
            let emoji = cache.choose().await.to_string();
            Some((emoji, cache))
        });

        // TODO: this is very awkward
        messages
            .filter(|_| async { thread_rng().gen_bool(0.10) })
            .zip(rand)
            .map(|(msg, emoji)| (msg, emoji, &conn))
            .for_each_concurrent(None, |(msg, emoji, conn)| async move {
                trace!(%emoji, "reacting");
                conn.react(&msg, &emoji).await.ok();
            })
            .await;
    });
}

struct EmojiCache {
    age: DateTime<Utc>,
    emoji: Vec<String>,
    slack: slack::Client,
}

impl EmojiCache {
    fn new(slack: slack::Client) -> EmojiCache {
        EmojiCache {
            age: chrono::MIN_DATETIME,
            emoji: vec![],
            slack,
        }
    }

    async fn choose(&mut self) -> &str {
        if self.expired() {
            self.emoji = Self::fetch(&self.slack).await;
            self.age = Utc::now();
        }

        self.emoji.choose(&mut thread_rng()).unwrap()
    }

    async fn fetch(slack: &slack::Client) -> Vec<String> {
        let regular = gh_emoji::all().map(|e| e.0.to_string());
        slack
            .emoji_list()
            .await
            .unwrap()
            .drain(..)
            .chain(regular)
            .collect()
    }

    fn expired(&self) -> bool {
        (Utc::now() - self.age) > Duration::hours(1)
    }
}
