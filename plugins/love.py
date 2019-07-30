import random
from slackbot.bot import listen_to

@listen_to('')
def love(message):
    if random.random() > 0.90:
        message.send('SHREK IS LOVE, SHREK IS LIFE')
