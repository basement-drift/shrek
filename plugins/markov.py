from slackbot.bot import listen_to
from slackbot.bot import respond_to
import markovify
import os

persistence_file = os.environ['MARKOV_MODEL_PATH']

text_model = markovify.NewlineText('SHREK IS LOVE, SHREK IS LIFE')
if os.path.isfile(persistence_file):
    with open(os.environ['MARKOV_MODEL_PATH']) as f:
        model_json = f.read()

    text_model = markovify.Text.from_json(model_json)

@listen_to('.*')
def learn(message):
    global text_model
    new_model = markovify.NewlineText(message.body['text'], well_formed = False)
    text_model = markovify.combine(models=[text_model, new_model])

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
