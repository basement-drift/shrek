from slackbot.bot import respond_to
import re

@respond_to('^echo (.*)', re.S | re.M)
def echo(message, echo):
    message.send(echo)
