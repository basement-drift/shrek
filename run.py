from slackbot.bot import Bot
import logging
import sys


def main():
    logging.basicConfig(stream=sys.stdout, level=logging.DEBUG)

    bot = Bot()
    bot.run()

if __name__ == "__main__":
    main()
