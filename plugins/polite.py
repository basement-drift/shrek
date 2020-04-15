from slackbot.bot import listen_to
import re

@listen_to('^(fuck|thank).*shrek', re.IGNORECASE)
def thankShrek(message, capture):
    message.send("You're welcome!")
