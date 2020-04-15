use bytes::Bytes;
use const_format::concatcp;
use serde::Deserialize;
use serde_json::json;
use tokio::time::{sleep, Duration};

const API_URL: &str = "https://api.uberduck.ai/";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
pub struct Client {
    pub http: reqwest::Client,
    pub api_key: String,
    pub api_secret: String,
}

impl Client {
    pub async fn speak(&self, speech: &str) -> Result<String, Error> {
        let req = json!({
            "speech": speech,
            "voice": "shrek",
        });

        #[derive(Deserialize)]
        struct Response {
            uuid: String,
        }

        let body = self
            .request(self.http.post(concatcp!(API_URL, "speak")).json(&req))
            .await?;

        let res: Response = serde_json::from_str(&body)?;

        Ok(res.uuid)
    }

    pub async fn status(&self, uuid: &str) -> Result<Option<String>, Error> {
        #[derive(Deserialize)]
        struct Response {
            path: Option<String>,
        }

        let body = self
            .request(
                self.http
                    .get(concatcp!(API_URL, "speak-status"))
                    .query(&[("uuid", uuid)]),
            )
            .await?;

        let res: Response = serde_json::from_str(&body)?;

        Ok(res.path)
    }

    pub async fn wait(&self, uuid: &str) -> Result<String, Error> {
        loop {
            match self.status(uuid).await? {
                Some(path) => return Ok(path),
                None => sleep(Duration::from_millis(500)).await,
            }
        }
    }

    pub async fn download(&self, url: &str) -> Result<Bytes, Error> {
        Ok(self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?)
    }

    async fn request(&self, req: reqwest::RequestBuilder) -> Result<String, Error> {
        Ok(req
            .basic_auth(&self.api_key, Some(&self.api_secret))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotenv::dotenv;
    use std::env;

    #[tokio::test]
    async fn speak() {
        dotenv().unwrap();

        let client = Client {
            http: reqwest::Client::new(),
            api_key: env::var("UBERDUCK_API_KEY").unwrap(),
            api_secret: env::var("UBERDUCK_API_SECRET").unwrap(),
        };

        println!("{:#?}", client);

        let uuid = client
            .speak("hey it's me, motherfucking shrek")
            .await
            .unwrap();

        let path = client.wait(&uuid).await.unwrap();

        println!("path: {}", path);
    }
}
