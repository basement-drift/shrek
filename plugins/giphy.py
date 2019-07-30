from slackbot.bot import respond_to
import giphy_client
import os

@respond_to('giphy (.*)')
def giphy(message, subject):
    api_instance = giphy_client.DefaultApi()
    api_key = os.environ['GIPHY_API_KEY']
    api_response = api_instance.gifs_translate_get(api_key, subject)
    message.send("https://media.giphy.com/media/" + api_response.data.id + "/giphy.gif")
