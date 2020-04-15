import os
import datetime
import logging
import random
import re
import gpt_2_simple as gpt2
import nltk.data
import nltk.tokenize
import threading
from slackbot.bot import respond_to
from slackbot.bot import listen_to
from multiprocessing import Pool

from plugins.util import extract_text

# Which model are we running?
gpt2_model_name = os.environ['GPT_2_MODEL']

# All of the models, regardless of size, cannot generate more than 1024 words.
# Prefixes count towards this.
GPT2_MAX_LENGTH = 1024

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
                temperature=0.9,
                )[0]

# Due to some memory leaks in the GPT-2 model's tensorflow usage, we'll need to
# run the generation in a subprocess. This is done via the mutliprocessing.Pool
# object, which is allowed to create a single process that can be invoked 8
# times, then exits and respawns. This may need to be tuned.
worker_pool = Pool(processes=1, initializer=subprocess_init, maxtasksperchild=8)

# Store the previously generated text for continue
last_generated = ""
last_generated_lock = threading.Lock()

# Make shrek shut up
shut_up_until = datetime.datetime.now()
shut_up_lock = threading.Lock()

def gpt2_generate(prefix, length):
    with last_generated_lock:
        global last_generated
        last_generated = worker_pool.apply(subprocess_generate, (prefix, length))
        return last_generated

@respond_to('^shut up$')
def gpt2_shut_up(message):
    with shut_up_lock:
        global shut_up_until
        shut_up_until = datetime.datetime.now() + datetime.timedelta(minutes = 5)

    message.send(":cry:")

@respond_to('^carry on$')
def gpt2_shut_up(message):
    with shut_up_lock:
        global shut_up_until
        shut_up_until = datetime.datetime.now()

    message.send(":shrek:")

def is_shut_up():
    with shut_up_lock:
        global shut_up_until
        if shut_up_until > datetime.datetime.now():
            logging.debug("I am silenced")
            return True

    return False

@respond_to('gpt2 ([0-9]*) (.*)', re.S | re.M)
def gpt2_complete(message, length, prefix):
    global gpt2_sess
    global gpt2_model_name

    # Remove any trailing newline, to allow the completion of sentences.
    prefix = prefix.rstrip()

    text = gpt2_generate(prefix, int(length))
    message.send(text)

@respond_to('gpt2 ([^0-9].*)', re.S | re.M)
def gpt2_complete_default(message, prefix):
    gpt2_complete(message, 200, prefix)

def generate_pretty_response(prompt, length):
    text = gpt2_generate(prompt, length)

    # Remove the prompt
    text = text[len(prompt):].strip()

    # Remove trailing incomplete sentences
    sentences = sentence_detector.tokenize(text)
    if len(sentences) > 1:
        last_sentence = sentences[-1]

        # If the last sentence doesn't contain a sentence break, it's incomplete
        if not sentence_detector.text_contains_sentbreak(last_sentence):
            text = text[:-len(last_sentence)].strip()

    return text

#@listen_to('(.*)\?(\s|$)')
#def gpt2_answer_questions(message, question, _):
    #if is_shut_up():
        #return
#
    #logging.debug("someone asked a question: `%s`", question)
#
    ## questions containing the word "shrek" are handled by "you rang"
    #if re.match(".*shrek.*", question, re.I):
        #logging.debug("ignoring because shrek is in the question")
        #return
#
    ## Restore the question mark (lost during capture)
    #question = question + "?"
#
    #message.send(generate_pretty_response(question, 100))

@listen_to('(.*shrek.*|.*\?$)', re.S | re.M | re.I)
def gpt2_you_rang(message, prompt):
    if is_shut_up():
        return

    logging.debug("someone rang: `%s`", prompt)

    prompt = prompt + "\nSHREK: "

    text = generate_pretty_response(prompt, len(prompt) + 50)

    # split on all-caps follows by a colon (e.g. "DONKEY:") to catch movie script continuations
    text = re.split(r'\b[A-Z]+:', text)[0].strip()

    message.send(text)

@listen_to('(.*[^?]$)', re.S | re.M)
def gpt2_you_did_not_ring(message, prompt):
    if is_shut_up():
        return

    if random.random() >= 0.99:
        logging.debug("someone said a random thing: `%s`", prompt)
        gpt2_you_rang(message, prompt)

@respond_to('tldr (\S+)$')
def gpt2_tldr_default(message, url):
    gpt2_tldr(message, url, '')

def tokenize_words(text):
    # Find all non-overlapping matches of any amount of whitespace followed by
    # any amount of non-whitespace. In other words, match words and keep
    # whitespace.
    return re.findall(r'\s*\S+', text, re.S | re.M)


@respond_to('tldr (\S+)( .*)')
def gpt2_tldr(message, url, forced_prefix):
    logging.debug("tldr with url `%s` and forced_prefix `%s`", url, forced_prefix)

    url_match = re.search(r'(?:http(s)?:\/\/)?[\w.-]+(?:\.[\w\.-]+)+[\w\-\._~:/?#[\]@!\$&\'\(\)\*\+,;=.]+', url)

    if not url_match:
        message.send('`' + url + '` does not contain a URL')
        return

    url = url_match.group(0)

    # How much summary should we generate?
    tldr_length = 50

    # Append this to the text to ask for a summary
    tldr = '\ntl;dr:'

    # The model seems to get confused when more than 500 words of context are
    # given. This often results in an empty tl;dr. So, just like a real tl;dr,
    # only read the beginning of the text.
    max_prefix_length = 500

    text = extract_text(url)

    # Find all non-overlapping matches of any amount of whitespace followed by
    # any amount of non-whitespace. In other words, match words and keep
    # whitespace. Also, truncate at max_prefix_length words.
    words = tokenize_words(text)[:max_prefix_length]

    if len(words) == 0:
        message.send("I couldn't read that")
        return

    prefix = ''.join(words) + tldr + forced_prefix

    # We want to request however much the truncated string was (which might be
    # less than max_prefix_length), plus however much summary we asked for,
    # plus the tl;dr string itself.
    length = len(words) + tldr_length + 1

    generated = gpt2_generate(prefix, length)

    # Grab the tl;dr and everything after it, up to the next newline
    summary = url + re.search(tldr + '[^\n]*', generated, re.S | re.M).group(0)

    # Limit the response to a single sentence
    summary = sentence_detector.tokenize(summary)[0]

    message.send(summary.strip())


@respond_to('^continue$')
def gpt2_continue_default(message):
    gpt2_continue(message, 200)

@respond_to('^continue ([0-9]+)')
def gpt2_continue(message, length):
    global last_generated

    length = int(length)

    # Grab the current most recent, because this might change out from under us
    # and we need to refer to it multiple times
    current_last = last_generated;

    # Make certain if we try to generate more than the max, we use the maximum
    # trailing amount of context. The model seems to get confused with more
    # than about 500 words of context.
    max_prefix_length = 500
    prefix_words = tokenize_words(current_last)[-max_prefix_length:]
    current_last = ''.join(prefix_words)

    total_length = len(prefix_words) + length

    logging.debug("generating with length `%d` and prefix with length `%d` and value `%s`", total_length, len(prefix_words), current_last)

    text = gpt2_generate(current_last, total_length)

    # clip off the prefix
    text = text[len(current_last):].strip()

    if len(text) == 0:
        message.send("I've got nothing else to say about that.")
        return

    message.send(text)

@respond_to('^context$')
def gpt2_context(message):
    global last_generated
    current_last = last_generated;
    message.send(current_last)
