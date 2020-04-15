import regex
import random
import logging
import json
import threading
import slacker
from slackbot.bot import listen_to

with open('emoji.json') as emoji_file:
    emoji_set = {emoji['short_name'] for emoji in json.loads(emoji_file.read())}

UPDATE_EMOJI_INTERVAL = 100
update_emoji_counter = UPDATE_EMOJI_INTERVAL
emoji_set_lock = threading.Lock()

@listen_to('')
def emoji(message):
    if random.random() > 0.10:
        return

    global update_emoji_counter
    if update_emoji_counter >= UPDATE_EMOJI_INTERVAL:
        update_custom_emoji(message._client.webapi)
        update_emoji_counter = 0

    # Until a correct list of emoji can be found, just retry if an invalid
    # reaction is attempted
    while True:
        try:
            emoji = random.choice(tuple(emoji_set))

            # Blacklist flag emoji (there are a ton of them)
            if regex.match('flag', emoji):
                continue

            logging.info("reacting with emoji :%s:", emoji)
            message.react(emoji)
            return

        except slacker.Error as e:
            if "invalid_name" not in e.args:
                raise

            logging.info(":%s: was invalid, retrying", emoji)



def update_custom_emoji(webapi):
    logging.info("updating custom emoji")

    with emoji_set_lock:
        global emoji_set
        emoji_set |= set(webapi.emoji.list().body['emoji'].keys())
