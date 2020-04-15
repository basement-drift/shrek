import urllib.request
from bs4 import BeautifulSoup
from readability import Document

FAKE_USER_AGENT = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_12_4) AppleWebKit/603.1.30 (KHTML, like Gecko) Version/10.1 Safari/603.1.30'

def extract_text(url):
    req = urllib.request.Request( url, headers={ 'User-Agent': FAKE_USER_AGENT })
    with urllib.request.urlopen(req) as response:
        # Extract the main body of text as HTML
        doc = Document(response.read())
        soup = BeautifulSoup(doc.summary(), features="lxml")

        return soup.get_text()
