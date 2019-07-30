from slackbot.bot import listen_to
import re

@listen_to('^give him the (.*)', re.IGNORECASE)
def giveHimThe(message, stick):
    message.send("DON'T GIVE HIM THE {}".format(stick.upper()))
