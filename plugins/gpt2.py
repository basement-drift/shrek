import os
import re
import gpt_2_simple as gpt2
import nltk.data
import nltk.tokenize
from slackbot.bot import respond_to
from slackbot.bot import listen_to
from multiprocessing import Pool

# Which model are we running?
gpt2_model_name = os.environ['GPT_2_MODEL']

# Pre-load a sentence tokenizer
sentence_detector = nltk.data.load('tokenizers/punkt/english.pickle')

def subprocess_init():
    # Initialize a global session, since we'll need access to it from
    # subprocess_generate
    global gpt2_sess
    gpt2_sess = gpt2.start_tf_sess()
    gpt2.load_gpt2(gpt2_sess, model_name=gpt2_model_name)

def subprocess_generate(prefix, length):
    global gpt2_sess

    with gpt2_sess.graph.as_default():
        return gpt2.generate(
                gpt2_sess,
                model_name=gpt2_model_name,
                prefix=prefix,
                truncate="<|endoftext|>",
                length=length,
                return_as_list=True,
                )[0]

# Due to some memory leaks in the GPT-2 model's tensorflow usage, we'll need to
# run the generation in a subprocess. This is done via the mutliprocessing.Pool
# object, which is allowed to create a single process that can be invoked 8
# times, then exits and respawns. This may need to be tuned.
worker_pool = Pool(processes=1, initializer=subprocess_init, maxtasksperchild=8)

def gpt2_generate(prefix, length):
    # No locks are needed around generation, because the pool only has a single
    # process.
    return worker_pool.apply(subprocess_generate, (prefix, length))

@respond_to('gpt2 ([0-9]*) (.*)', re.S | re.M)
def gpt2_complete(message, length, prefix):
    global gpt2_sess
    global gpt2_model_name

    # Remove any trailing newline, to allow the completion of sentences.
    prefix = prefix.rstrip()

    text = gpt2_generate(prefix, int(length))
    message.send(text)

@listen_to('(.*)\?')
def gpt2_answer_questions(message, question):
    # Restore the question mark (lost during capture)
    question = question + "?"

    text = gpt2_generate(question, 100)

    # Remove the question
    text = text[len(question):].strip()

    # Remove trailing incomplete sentences
    sentences = sentence_detector.tokenize(text)
    if len(sentences) > 1:
        last_sentence = sentences[-1]

        # If the last sentence doesn't contain a sentence break, it's incomplete
        if not sentence_detector.text_contains_sentbreak(last_sentence):
            text = text[:-len(last_sentence)].strip()

    message.reply(text, in_thread = "yes")
