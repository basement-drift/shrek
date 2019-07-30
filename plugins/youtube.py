from slackbot.bot import respond_to
from googleapiclient.discovery import build
import os

YOUTUBE_API_SERVICE_NAME = 'youtube'
YOUTUBE_API_VERSION = 'v3'

@respond_to('youtube (.*)')
def youtube(message, subject):
    youtube_client = build(
            YOUTUBE_API_SERVICE_NAME,
            YOUTUBE_API_VERSION,
            developerKey=os.environ['YOUTUBE_API_KEY']
    )

    search_response = youtube_client.search().list(
            q=subject,
            part='id',
            type='video',
            maxResults=1
    ).execute()

    search_results = search_response.get('items', [])

    if len(search_results) == 0:
        message.reply("You have poor taste.")
        return

    video_id = search_results[0]['id']['videoId']

    message.send("https://youtube.com/watch?v=" + video_id)
