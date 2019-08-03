from bs4 import BeautifulSoup
from readability import Document
from slackbot.bot import listen_to
from slackbot.bot import respond_to
import markovify
import nltk.tokenize
import os
import threading
import urllib.request
import re


persistence_file = os.environ['MARKOV_MODEL_PATH']

text_model = markovify.NewlineText('SHREK IS LOVE, SHREK IS LIFE')
text_model_lock = threading.Lock()

if os.path.isfile(persistence_file):
    with open(os.environ['MARKOV_MODEL_PATH']) as f:
        model_json = f.read()

    text_model = markovify.NewlineText.from_json(model_json)

@listen_to('.*')
def learn(message):
    text = message.body['text']
    if len(text) > 0:
        merge(text)

@respond_to('markov (.*)')
def markov(message, beginning):
    sentence = text_model.make_sentence_with_start(
            beginning, strict = False, tries=1000)

    if sentence is None:
        sentence = text_model.make_sentence_with_start(
                beginning, strict = False, test_output = False)

    if sentence is None:
        sentence = beginning

    message.send(sentence)

@respond_to('markov_dump')
def markov_dump(message):
    message.send(text_model.to_json())


@respond_to("markov_learn_sentences (.*)")
def learn_sentences(message, url):
    sentences = extract_sentences(url)

    merge(sentences)

    message.reply("I learned this:")
    upload_learned(message, url, sentences)


@respond_to("markov_learn_sentences_dry (.*)")
def learn_sentences_dry(message, url):
    sentences = extract_sentences(url)

    # Upload as an attachment instead of loading into the model
    message.send("Here's what I would have learned:")
    upload_learned(message, url, sentences)


@respond_to("markov_learn_paragraphs (.*)")
def learn_paragraphs(message, url):
    paragraphs = extract_paragraphs(url)

    merge(paragraphs)

    message.reply("I learned this:")
    upload_learned(message, url, paragraphs)


@respond_to("markov_learn_paragraphs_dry (.*)")
def learn_paragraphs_dry(message, url):
    paragraphs = extract_paragraphs(url)

    # Upload as an attachment instead of loading into the model
    message.send("Here's what I would have learned:")
    upload_learned(message, url, paragraphs)


# Merge with the current model. Text may be a string or a list of strings
def merge(text):
    # Build a new markov model
    new_model = markovify.NewlineText(text, well_formed = False)

    # Merge it with the old model
    with text_model_lock:
        global text_model
        text_model = markovify.combine(models=[text_model, new_model])

        # Persist
        with open(os.environ['MARKOV_MODEL_PATH'], 'w') as f:
            model_json = f.write(text_model.to_json())


def extract_text(url):
    with urllib.request.urlopen(url) as response:
        # Extract the main body of text as HTML
        doc = Document(response.read())
        soup = BeautifulSoup(doc.summary(), features="lxml")

        return soup.get_text()


def extract_sentences(url):
    raw = extract_text(url)

    # Merge to a single line
    text = " ".join(raw.split())

    # Tokenize it into sentences
    return nltk.tokenize.sent_tokenize(text)


def extract_paragraphs(url):
    raw = extract_text(url)

    # Split on blank lines, then normalize whitespace
    paragraphs = []
    for raw_paragraph in re.split(r"(?:\r?\n){2,}", raw):
        paragraphs.append(" ".join(raw_paragraph.split()))

    return paragraphs

def upload_learned(message, url, text):
    message.channel.upload_content(url, "\n----------\n".join(text))
