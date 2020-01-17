import os
import nltk
import gpt_2_simple as gpt2

nltk.download('punkt')
gpt2.download_gpt2(model_name=os.environ['GPT_2_MODEL'])
