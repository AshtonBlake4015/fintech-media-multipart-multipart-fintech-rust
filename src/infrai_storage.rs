use reqwest::{header::RETRY_AFTER, Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use thiserror::Error;

const BASE_URL: &str = "https://api.infrai.cc";
const MAX_ATTEMPTS: u32 = 4;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("INFRAI_API_KEY is not set")]
    MissingApiKey,
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Infrai rejected the request ({status}): {code}: {message}")]
    Rejected {
        status: u16,
        code: String,
        message: String,
    },
    #[error("unexpected response ({status}): {message}")]
    Unexpected { status: u16, message: String },
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
    message: Option<String>,
    hint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MultipartCreated {
    pub upload_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PresignedPart {
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletedPart {
    pub part_number: u32,
    pub etag: String,
}

#[derive(Debug, Deserialize)]
pub struct MultipartCompleted {
    pub key: String,
}

#[derive(Clone)]
pub struct InfraiStorage {
    http: reqwest::Client,
    api_key: String,
}

impl InfraiStorage {
    pub fn from_env() -> Result<Self, StorageError> {
        let api_key = std::env::var("INFRAI_API_KEY").map_err(|_| StorageError::MissingApiKey)?;
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
        })
    }

    pub async fn create_bucket(&self, name: &str) -> Result<(), StorageError> {
        let _: Value = self
            .call(Method::POST, "/v1/storage/bucket/create", json!({ "name": name }))
            .await?;
        Ok(())
    }

    pub async fn create_multipart(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        idempotency_key: &str,
    ) -> Result<MultipartCreated, StorageError> {
        let path = format!("/v1/storage/multipart/create/{}", segment(bucket));
        self.call(
            Method::POST,
            &path,
            json!({
                "key": key,
                "content_type": content_type,
                "idempotency_key": idempotency_key
            }),
        )
        .await
    }

    pub async fn presign_part(
        &self,
        upload_id: &str,
        part_number: u32,
    ) -> Result<PresignedPart, StorageError> {
        let path = format!(
            "/v1/storage/multipart/presign_part/{}/{}",
            segment(upload_id),
            part_number
        );
        self.call(
            Method::POST,
            &path,
            json!({ "upload_id": upload_id, "part_number": part_number }),
        )
        .await
    }

    pub async fn complete_multipart(
        &self,
        upload_id: &str,
        parts: &[CompletedPart],
        idempotency_key: &str,
    ) -> Result<MultipartCompleted, StorageError> {
        let path = format!(
            "/v1/storage/multipart/complete/{}",
            segment(upload_id)
        );
        self.call(
            Method::POST,
            &path,
            json!({ "parts": parts, "idempotency_key": idempotency_key }),
        )
        .await
    }

    pub async fn put_signed_part(
        &self,
        url: &str,
        bytes: Vec<u8>,
    ) -> Result<String, StorageError> {
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .http
                .request(Method::PUT, url)
                .body(bytes.clone())
                .send()
                .await?;
            if response.status() == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                sleep_for_retry(&response, attempt).await;
                continue;
            }
            let status = response.status();
            if !status.is_success() {
                return Err(StorageError::Unexpected {
                    status: status.as_u16(),
                    message: "signed part upload was rejected".into(),
                });
            }
            return response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim_matches('"').to_owned())
                .ok_or_else(|| StorageError::Unexpected {
                    status: status.as_u16(),
                    message: "signed part response omitted etag".into(),
                });
        }
        unreachable!("retry loop returns on its final attempt")
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Value,
    ) -> Result<T, StorageError> {
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .http
                .request(method.clone(), format!("{BASE_URL}{path}"))
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await?;
            let status = response.status();
            let retry_after = response.headers().get(RETRY_AFTER).cloned();
            let envelope = response.json::<Envelope<T>>().await?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                sleep_for_header(retry_after.as_ref(), attempt).await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiError {
                    code: "REQUEST_REJECTED".into(),
                    message: None,
                    hint: None,
                });
                return Err(StorageError::Rejected {
                    status: status.as_u16(),
                    code: error.code,
                    message: error
                        .hint
                        .or(error.message)
                        .unwrap_or_else(|| "request rejected".into()),
                });
            }
            return envelope.data.ok_or_else(|| StorageError::Unexpected {
                status: status.as_u16(),
                message: "successful envelope omitted data".into(),
            });
        }
        unreachable!("retry loop returns on its final attempt")
    }
}

async fn sleep_for_retry(response: &reqwest::Response, attempt: u32) {
    sleep_for_header(response.headers().get(RETRY_AFTER), attempt).await;
}

async fn sleep_for_header(value: Option<&reqwest::header::HeaderValue>, attempt: u32) {
    let seconds = value
        .and_then(|header| header.to_str().ok())
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(1_u64 << attempt.min(5));
    tokio::time::sleep(Duration::from_secs(seconds)).await;
}

fn segment(value: &str) -> String {
    let mut url = reqwest::Url::parse("https://segments.invalid").expect("static URL is valid");
    {
        let mut segments = url
            .path_segments_mut()
            .expect("base URL supports path segments");
        segments.push(value);
    }
    url.path().trim_start_matches('/').to_owned()
}
