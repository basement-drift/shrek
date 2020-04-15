from slackbot.bot import listen_to
import re

@listen_to('^shrek no$', re.IGNORECASE)
def shrekNo(message):
    message.send("SHREK YES")
