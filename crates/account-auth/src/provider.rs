use crate::AuthError;
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Sms,
    Whatsapp,
}
impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sms => "sms",
            Self::Whatsapp => "whatsapp",
        }
    }
}

#[async_trait]
pub trait OtpProvider: Send + Sync {
    async fn start(&self, phone: &str, channel: Channel) -> Result<String, AuthError>;
    async fn check(&self, verification_sid: &str, code: &str) -> Result<bool, AuthError>;
}

/// Live provider; no network requests occur until start/check is invoked.
/// WhatsApp requires an approved/configured Verify sender at Twilio.
pub struct TwilioVerify {
    client: Client,
    account_sid: String,
    auth_token: Zeroizing<String>,
    service_sid: String,
    base_url: String,
}
fn sid(value: &str, prefix: &str) -> bool {
    value.len() == 34
        && value.starts_with(prefix)
        && value[2..].bytes().all(|b| b.is_ascii_hexdigit())
}
#[derive(Deserialize)]
struct VerifyResponse {
    sid: String,
    service_sid: String,
    status: String,
    valid: Option<bool>,
}
impl TwilioVerify {
    pub fn new(
        account_sid: String,
        auth_token: String,
        service_sid: String,
    ) -> Result<Self, AuthError> {
        let auth_token = Zeroizing::new(auth_token);
        if !sid(&account_sid, "AC") || !sid(&service_sid, "VA") || auth_token.len() < 16 {
            return Err(AuthError::Invalid);
        }
        let client = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| AuthError::Unavailable)?;
        Ok(Self {
            client,
            account_sid,
            auth_token,
            service_sid,
            base_url: "https://verify.twilio.com/v2".into(),
        })
    }
    async fn post(
        &self,
        endpoint: &str,
        form: &[(&str, &str)],
    ) -> Result<Option<VerifyResponse>, AuthError> {
        let mut response = self
            .client
            .post(format!(
                "{}/Services/{}/{endpoint}",
                self.base_url, self.service_sid
            ))
            .basic_auth(&self.account_sid, Some(self.auth_token.as_str()))
            .form(form)
            .send()
            .await
            .map_err(|_| AuthError::Unavailable)?;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(AuthError::RateLimited);
        }
        if response.status() == StatusCode::NOT_FOUND && endpoint == "VerificationCheck" {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(AuthError::Unavailable);
        }
        let mut body = Zeroizing::new(Vec::new());
        while let Some(chunk) = response.chunk().await.map_err(|_| AuthError::Unavailable)? {
            if body.len() + chunk.len() > 16 * 1024 {
                return Err(AuthError::Unavailable);
            }
            body.extend_from_slice(&chunk);
        }
        let result: VerifyResponse =
            serde_json::from_slice(&body).map_err(|_| AuthError::Unavailable)?;
        if result.service_sid != self.service_sid || !sid(&result.sid, "VE") {
            return Err(AuthError::Unavailable);
        }
        Ok(Some(result))
    }
}
#[async_trait]
impl OtpProvider for TwilioVerify {
    async fn start(&self, phone: &str, channel: Channel) -> Result<String, AuthError> {
        let response = self
            .post(
                "Verifications",
                &[("To", phone), ("Channel", channel.as_str())],
            )
            .await?
            .ok_or(AuthError::Unavailable)?;
        if response.status != "pending" {
            return Err(AuthError::Unavailable);
        }
        Ok(response.sid)
    }
    async fn check(&self, verification_sid: &str, code: &str) -> Result<bool, AuthError> {
        if !sid(verification_sid, "VE") {
            return Err(AuthError::Denied);
        }
        let Some(response) = self
            .post(
                "VerificationCheck",
                &[("VerificationSid", verification_sid), ("Code", code)],
            )
            .await?
        else {
            return Ok(false);
        };
        if response.sid != verification_sid {
            return Err(AuthError::Denied);
        }
        Ok(response.status == "approved" && response.valid == Some(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::{Form, State},
        routing::post,
        Json, Router,
    };
    use std::{collections::HashMap, sync::Arc};
    #[tokio::test]
    async fn verify_contract_sms_whatsapp_and_approval() {
        async fn send(Form(form): Form<HashMap<String, String>>) -> Json<serde_json::Value> {
            assert_eq!(form["To"], "+12025550123");
            assert!(matches!(form["Channel"].as_str(), "sms" | "whatsapp"));
            Json(
                serde_json::json!({"sid":format!("VE{}", "a".repeat(32)), "service_sid":format!("VA{}", "b".repeat(32)), "status":"pending"}),
            )
        }
        async fn check(
            State(valid): State<Arc<bool>>,
            Form(form): Form<HashMap<String, String>>,
        ) -> Json<serde_json::Value> {
            assert!(form.contains_key("VerificationSid"));
            Json(
                serde_json::json!({"sid":format!("VE{}", "a".repeat(32)), "service_sid":format!("VA{}", "b".repeat(32)), "status":"approved", "valid":*valid && form["Code"] == "123456"}),
            )
        }
        let service = format!("VA{}", "b".repeat(32));
        let app = Router::new()
            .route(&format!("/Services/{service}/Verifications"), post(send))
            .route(
                &format!("/Services/{service}/VerificationCheck"),
                post(check),
            )
            .with_state(Arc::new(true));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let provider = TwilioVerify {
            client: Client::new(),
            account_sid: format!("AC{}", "c".repeat(32)),
            auth_token: Zeroizing::new("test-token-not-real".into()),
            service_sid: service,
            base_url: format!("http://{address}"),
        };
        for channel in [Channel::Sms, Channel::Whatsapp] {
            let id = provider.start("+12025550123", channel).await.unwrap();
            assert!(provider.check(&id, "123456").await.unwrap());
            assert!(!provider.check(&id, "000000").await.unwrap());
        }
        task.abort();
    }
    #[tokio::test]
    async fn provider_errors_and_mismatched_approval_never_authenticate() {
        use axum::response::IntoResponse;
        async fn handler(Form(form): Form<HashMap<String, String>>) -> axum::response::Response {
            match form["Code"].as_str() {
                "404000" => StatusCode::NOT_FOUND.into_response(),
                "429000" => StatusCode::TOO_MANY_REQUESTS.into_response(),
                "500000" => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                "999999" => "x".repeat(17000).into_response(),
                code => Json(serde_json::json!({
                    "sid":format!("VE{}", if code == "111111" { "f" } else { "a" }.repeat(32)),
                    "service_sid":format!("VA{}", "b".repeat(32)),
                    "status":if code == "222222" { "pending" } else { "approved" }, "valid":true
                }))
                .into_response(),
            }
        }
        let service = format!("VA{}", "b".repeat(32));
        let app = Router::new().route(
            &format!("/Services/{service}/VerificationCheck"),
            post(handler),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let provider = TwilioVerify {
            client: Client::new(),
            account_sid: format!("AC{}", "c".repeat(32)),
            auth_token: Zeroizing::new("test-token-not-real".into()),
            service_sid: service,
            base_url: format!("http://{address}"),
        };
        let id = format!("VE{}", "a".repeat(32));
        assert!(!provider.check(&id, "404000").await.unwrap());
        assert!(!provider.check(&id, "222222").await.unwrap());
        assert!(matches!(
            provider.check(&id, "429000").await,
            Err(AuthError::RateLimited)
        ));
        assert!(matches!(
            provider.check(&id, "500000").await,
            Err(AuthError::Unavailable)
        ));
        assert!(matches!(
            provider.check(&id, "111111").await,
            Err(AuthError::Denied)
        ));
        assert!(matches!(
            provider.check(&id, "999999").await,
            Err(AuthError::Unavailable)
        ));
        task.abort();
    }
    #[test]
    fn rejects_unconfigured_provider() {
        assert!(TwilioVerify::new("bad".into(), "short".into(), "bad".into()).is_err());
    }
}
