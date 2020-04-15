use chatbot::Chatbot;
use dotenv::dotenv;
use eyre::{eyre, Result};
use futures::FutureExt;
use rand::prelude::*;
use std::env;
use tracing::debug;

mod emoji;
mod gpt2;
mod history;

use history::History;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv()?;

    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("shrek=trace,chatbot=trace,slack=debug"))
                .unwrap(),
        )
        .init();

    let client = slack::Client::new(env::var("APP_TOKEN")?, env::var("BOT_TOKEN")?).await?;

    let (driver, messages) = client.messages();
    tokio::task::spawn(driver);

    let chatbot = chatbot::Chatbot::new(client.clone()).await?;

    let history = History::new(chatbot.slack());
    history.monitor(&chatbot).await?;

    configure(&chatbot, &history).await?;
    chatbot.run(messages).await?;

    Ok(())
}

async fn configure(chatbot: &Chatbot, history: &History) -> Result<()> {
    chatbot
        .reply("(?i)^(?:fuck|thank).*shrek", "You're welcome!")?
        .reply("(?i)^shrek no$", "SHREK YES")?;

    chatbot.reply_with("echo (.*)", |_, cap| Some(cap[1].to_string()))?;

    chatbot.reply_with("(?i)give (him|her|them) the (.*)", |_, cap| {
        Some(format!(
            "DON'T GIVE {} THE {}",
            cap[1].to_uppercase(),
            cap[2].to_uppercase()
        ))
    })?;

    chatbot.reply_all(|_| {
        thread_rng()
            .gen_bool(0.01)
            .then(|| "SHREK IS LOVE, SHREK IS LIFE".into())
    })?;

    chatbot.reply_all(cronk)?;

    emoji::add(chatbot);
    gpt2::add(chatbot, history.clone()).await?;
    uberduck(chatbot, history.clone())?;

    Ok(())
}

fn uberduck(bot: &Chatbot, history: History) -> Result<()> {
    let uber = uberduck::Client {
        http: reqwest::Client::new(),
        api_key: env::var("UBERDUCK_API_KEY").unwrap(),
        api_secret: env::var("UBERDUCK_API_SECRET").unwrap(),
    };

    bot.listen((uber, history), |(uber, history), slack, msg| {
        async move {
            if !msg.is_mention || !msg.text.contains("speak") {
                return Ok(());
            }

            let parent = history
                .parent(msg)
                .ok_or_else(|| eyre!("couldn't find parent"))?;

            debug!(text=%parent.text, "speaking");

            let uuid = uber.speak(&parent.text).await?;
            let url = uber.wait(&uuid).await?;
            let wav = uber.download(&url).await?;

            let filename = format!("{:0.20}.wav", &parent.text);
            slack.upload_reply(&parent, &filename, wav).await?;

            Ok::<(), eyre::Error>(())
        }
        .boxed()
    })?;

    Ok(())
}

fn cronk(_: &slack::Message) -> Option<String> {
    let mut rng = thread_rng();

    let cronk = [
        "Cronk.",
        "Cronk is good.",
        "Buy Cronk.",
        "Drink Cronk.",
        "Dr. Cronk.",
        ":point_right: Who said Cronk was dead?",
        ":point_right: Drink Cronk and be happy",
    ];

    rng.gen_bool(0.01)
        .then(|| cronk.choose(&mut rng))?
        .map(|c| c.to_string())
}
