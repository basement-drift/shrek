use eyre::{Context, Result};
use futures::FutureExt;
use gpt2_client::{GenerateRequest, Gpt2Client};
use once_cell::unsync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::env;
use tracing::{debug, error};

use crate::history::History;

pub async fn add(bot: &chatbot::Chatbot, history: History) -> Result<()> {
    let gpt2 = Gpt2::new(bot, history).await?;

    bot.reply_all_async(gpt2, |gpt2, msg| {
        async move {
            if msg.is_mention {
                return None;
            }

            gpt2.reply(msg).await
        }
        .boxed()
    })?;

    Ok(())
}

struct Gpt2 {
    client: Gpt2Client,
    history: History,
    bot_id: String,
}

impl Gpt2 {
    async fn new(bot: &chatbot::Chatbot, history: History) -> Result<Self> {
        let address = env::var("GPT2_ADDRESS").wrap_err("gpt2_server address not specified")?;
        let client = Gpt2Client::connect(address)
            .await
            .wrap_err("could not connect to gpt2_server")?;

        Ok(Self {
            client,
            history,
            bot_id: bot.slack().bot_user_id().into(),
        })
    }

    async fn reply(&self, msg: &slack::Message) -> Option<String> {
        // Is the triggering message in a thread that was started by our bot?
        let bot_reply = match self.history.parent(msg) {
            Some(p) => p.user == self.bot_id,
            None => false,
        };

        if !bot_reply && !should_reply(&msg.text) {
            return None;
        }

        match self.prompt(msg).await {
            Ok(r) => Some(r),
            Err(err) => {
                error!(%err, "failed to generate reply");
                None
            }
        }
    }

    async fn prompt(&self, msg: &slack::Message) -> Result<String> {
        // Get the 20 messages leading up to our trigger message.
        let script = self.history.script(msg, 20).await?;

        let prompt = format!("{}\nSHREK:", script);
        debug!(%prompt, "gpt2 prompt");

        let mut client = self.client.clone();
        let text = client
            .generate_text(GenerateRequest {
                length: 100,
                prompt,
            })
            .await?
            .into_inner()
            .text;

        debug!(%text, "gpt2 text generated");

        let raw = strip_trailing_thoughts(&text);
        let clean = strip_incomplete_sentences(raw);
        Ok(clean.to_string())
    }
}

// Does the input contain a reply trigger?
fn should_reply(input: &str) -> bool {
    // Reply to any message that mentions shrek, or ends in a question mark.
    let re = Lazy::new(|| Regex::new(r"(?i)\?$|shrek").unwrap());
    re.is_match(input)
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
        let combined = format!("(?m){}|{}|{}|{}", scriptlikes, line_start, stage_direction, left_convo);

        Regex::new(&combined).unwrap()
    });

    // Collect into a vector so we can log it.
    let split: Vec<_> = re.splitn(input, 2).collect();
    debug!(?split);

    split.first().unwrap()
}

// TODO: unit test regex
