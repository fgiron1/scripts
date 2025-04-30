use reqwest::{Client, Response};
use anyhow::{Result, Context};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone)]
pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true)  // Be cautious with this in production
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    pub async fn get(&self, url: &str) -> Result<Response> {
        self.client
            .get(url)
            .send()
            .await
            .context("HTTP GET request failed")
    }

    pub async fn post(&self, url: &str, body: &Value) -> Result<Response> {
        self.client
            .post(url)
            .json(body)
            .send()
            .await
            .context("HTTP POST request failed")
    }

    pub async fn check_url_live(&self, url: &str) -> bool {
        match self.get(url).await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
}
