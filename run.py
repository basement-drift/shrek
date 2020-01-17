from slackbot.bot import Bot
import logging
import sys
import re


def main():
    logging.basicConfig(stream=sys.stdout, level=logging.DEBUG)

    bot = Bot()
    # make @respond_to receive multi-line messages
    bot._dispatcher.AT_MESSAGE_MATCHER = re.compile(
        bot._dispatcher.AT_MESSAGE_MATCHER.pattern, re.S | re.M
    )
    bot.run()

if __name__ == "__main__":
    main()
