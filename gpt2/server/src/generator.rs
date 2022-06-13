use eyre::{Result, WrapErr};
use rust_bert::gpt2::GPT2Generator;
use tracing::{debug, info};

use crate::{Message, Receiver};

const MAX_SIZE: usize = 1024;

// GPT2 generation loop. Enforces that only a single generation is running at once. Not async
// (since rust-bert isn't async), and must be run in a blocking-safe task (e.g.
// tokio::task::spawn_blocking).
pub(crate) fn gpt2(mut rx: Receiver) -> Result<()> {
    use rust_bert::pipelines::generation_utils::{GenerateOptions, LanguageGenerator};

    let generator = load_model()?;
    let tokenizer = generator.get_tokenizer();

    while let Some(msg) = rx.blocking_recv() {
        let (prompt, prompt_size) = truncate(tokenizer, &msg);

        let max_length = Some((prompt_size + msg.length as usize) as i64);

        let mut gen = generator.generate_indices(
            Some(&[prompt]),
            Some(GenerateOptions {
                max_length,
                ..GenerateOptions::default()
            }),
        );

        let trimmed: Vec<i64> = gen
            .swap_remove(0)
            .indices
            .into_iter()
            .skip(prompt_size)
            .collect();

        let output = tokenizer.decode(&trimmed, true, true);

        msg.responder.send(output).ok();
    }

    Ok(())
}

fn load_model() -> Result<GPT2Generator> {
    use rust_bert::{
        gpt2::{Gpt2ConfigResources, Gpt2MergesResources, Gpt2ModelResources, Gpt2VocabResources},
        pipelines::generation_utils::GenerateConfig,
        resources::{RemoteResource, Resource},
    };

    info!("loading gpt2 model");
    let generator = GPT2Generator::new(GenerateConfig {
        model_resource: Resource::Remote(RemoteResource::from_pretrained(
            Gpt2ModelResources::GPT2_LARGE,
        )),
        config_resource: Resource::Remote(RemoteResource::from_pretrained(
            Gpt2ConfigResources::GPT2_LARGE,
        )),
        vocab_resource: Resource::Remote(RemoteResource::from_pretrained(
            Gpt2VocabResources::GPT2_LARGE,
        )),
        merges_resource: Resource::Remote(RemoteResource::from_pretrained(
            Gpt2MergesResources::GPT2_LARGE,
        )),
        max_length: 200,
        num_beams: 5,
        temperature: 1.15,
        repetition_penalty: 1.0,
        ..Default::default()
    })
    .wrap_err("failed to load gpt2 model")?;

    // TODO: run priming generation
    info!("gpt2 model loaded");

    Ok(generator)
}

fn truncate<'a>(
    tokenizer: &rust_bert::pipelines::common::TokenizerOption,
    msg: &'a Message,
) -> (&'a str, usize) {
    let tokenized = tokenizer.tokenize_with_offsets(&msg.prompt);
    let tok_size = tokenized.tokens.len();
    let max_size = msg.length as usize + tok_size;
    let overflow = max_size.saturating_sub(MAX_SIZE);

    let offset = tokenized
        .offsets
        .into_iter()
        .skip(overflow)
        .find(|o| o.is_some())
        .flatten()
        .map(|o| o.begin)
        .unwrap_or(0) as usize;

    if offset == 0 {
        return (&msg.prompt, tok_size)
    }

    debug!(%offset, %overflow, "truncating text");

    // Re-tokenize to get the new prompt size
    // TODO: can we avoid this?
    let trunc_prompt = &msg.prompt[offset..];
    let tokenized = tokenizer.tokenize_with_offsets(trunc_prompt);
    let trunc_tok_size = tokenized.tokens.len();

    debug!(%tok_size, %trunc_tok_size, "truncated");

    (trunc_prompt, trunc_tok_size)
}
