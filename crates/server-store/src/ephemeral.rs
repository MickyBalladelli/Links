//! Redis-shaped ephemeral state API and bounded, single-process reference adapter.
//! The memory adapter is for tests/local development, not distributed deployment.
use crate::StoreError;
use async_trait::async_trait;
use links_protocol::validate_id;
use std::collections::HashMap;
use tokio::sync::Mutex;

pub const MAX_SESSION_TTL_MS: u64 = 120_000;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLease {
    pub device_id: String,
    pub session_id: String,
    pub gateway_id: String, // Locator, never a serialized socket or bearer token.
    pub expires_at_ms: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BucketPolicy {
    pub capacity: u32,
    pub refill_per_second: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateDecision {
    pub allowed: bool,
    pub remaining: u32,
    pub retry_after_ms: u64,
}

#[async_trait]
pub trait EphemeralState: Send + Sync {
    /// Atomically claim the device route. An active lease cannot be replaced;
    /// duplicate device binds return [`StoreError::Conflict`]. Redis
    /// implementation must use one key/hash slot per device.
    async fn bind(&self, lease: SessionLease, now_ms: u64) -> Result<(), StoreError>;
    /// CAS by session ID; cannot resurrect expired leases or alter another
    /// active lease.
    async fn renew(
        &self,
        device_id: &str,
        session_id: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<bool, StoreError>;
    async fn unbind(&self, device_id: &str, session_id: &str) -> Result<bool, StoreError>;
    async fn route(&self, device_id: &str, now_ms: u64)
        -> Result<Option<SessionLease>, StoreError>;
    /// Atomic token bucket. Keys must be namespaced opaque digests, never phone
    /// numbers, tokens, handles, or raw IPs. now_ms is trusted server time.
    /// Redis uses TIME + a Lua script/transaction, not client read/modify/write.
    async fn consume(
        &self,
        key: &str,
        policy: BucketPolicy,
        cost: u32,
        now_ms: u64,
    ) -> Result<RateDecision, StoreError>;
}

struct Bucket {
    policy: BucketPolicy,
    milli_tokens: u64,
    updated_at_ms: u64,
}
impl Bucket {
    fn idle_expiry_ms(&self) -> u64 {
        self.updated_at_ms.saturating_add(
            (u64::from(self.policy.capacity) * 1000)
                .div_ceil(u64::from(self.policy.refill_per_second)),
        )
    }
}
#[derive(Default)]
struct State {
    sessions: HashMap<String, SessionLease>,
    buckets: HashMap<String, Bucket>,
}
pub struct MemoryEphemeralState {
    state: Mutex<State>,
    max_entries: usize,
}
impl MemoryEphemeralState {
    pub fn new(max_entries: usize) -> Result<Self, StoreError> {
        if max_entries == 0 {
            return Err(StoreError::Invalid);
        }
        Ok(Self {
            state: Mutex::new(State::default()),
            max_entries,
        })
    }
}
fn valid_expiry(expires: u64, now: u64) -> bool {
    expires > now && expires - now <= MAX_SESSION_TTL_MS
}
fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'))
}
#[async_trait]
impl EphemeralState for MemoryEphemeralState {
    async fn bind(&self, lease: SessionLease, now_ms: u64) -> Result<(), StoreError> {
        validate_id(&lease.device_id)?;
        validate_id(&lease.session_id)?;
        if !valid_key(&lease.gateway_id) || !valid_expiry(lease.expires_at_ms, now_ms) {
            return Err(StoreError::Invalid);
        }
        let mut state = self.state.lock().await;
        state
            .sessions
            .retain(|_, lease| lease.expires_at_ms > now_ms);
        if state.sessions.contains_key(&lease.device_id) {
            return Err(StoreError::Conflict);
        }
        if state.sessions.len() >= self.max_entries {
            return Err(StoreError::Unavailable);
        }
        state.sessions.insert(lease.device_id.clone(), lease);
        Ok(())
    }
    async fn renew(
        &self,
        device_id: &str,
        session_id: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<bool, StoreError> {
        validate_id(device_id)?;
        validate_id(session_id)?;
        if !valid_expiry(expires_at_ms, now_ms) {
            return Err(StoreError::Invalid);
        }
        let mut state = self.state.lock().await;
        if let Some(lease) = state.sessions.get_mut(device_id) {
            if lease.session_id == session_id && lease.expires_at_ms > now_ms {
                lease.expires_at_ms = expires_at_ms;
                return Ok(true);
            }
        }
        Ok(false)
    }
    async fn unbind(&self, device_id: &str, session_id: &str) -> Result<bool, StoreError> {
        validate_id(device_id)?;
        validate_id(session_id)?;
        let mut state = self.state.lock().await;
        if state
            .sessions
            .get(device_id)
            .is_some_and(|lease| lease.session_id == session_id)
        {
            state.sessions.remove(device_id);
            return Ok(true);
        }
        Ok(false)
    }
    async fn route(
        &self,
        device_id: &str,
        now_ms: u64,
    ) -> Result<Option<SessionLease>, StoreError> {
        validate_id(device_id)?;
        let mut state = self.state.lock().await;
        if state
            .sessions
            .get(device_id)
            .is_some_and(|lease| lease.expires_at_ms <= now_ms)
        {
            state.sessions.remove(device_id);
        }
        Ok(state.sessions.get(device_id).cloned())
    }
    async fn consume(
        &self,
        key: &str,
        policy: BucketPolicy,
        cost: u32,
        now_ms: u64,
    ) -> Result<RateDecision, StoreError> {
        if !valid_key(key)
            || policy.capacity == 0
            || policy.refill_per_second == 0
            || cost == 0
            || cost > policy.capacity
        {
            return Err(StoreError::Invalid);
        }
        let mut state = self.state.lock().await;
        state
            .buckets
            .retain(|_, bucket| bucket.idle_expiry_ms() > now_ms);
        if !state.buckets.contains_key(key) && state.buckets.len() >= self.max_entries {
            return Err(StoreError::Unavailable);
        }
        let bucket = state.buckets.entry(key.into()).or_insert(Bucket {
            policy,
            milli_tokens: u64::from(policy.capacity) * 1000,
            updated_at_ms: now_ms,
        });
        if bucket.policy != policy {
            return Err(StoreError::Conflict);
        }
        let now = now_ms.max(bucket.updated_at_ms); // Backwards clock cannot mint tokens.
        let replenished = u128::from(bucket.milli_tokens)
            + u128::from(now - bucket.updated_at_ms) * u128::from(policy.refill_per_second);
        bucket.milli_tokens = replenished.min(u128::from(policy.capacity) * 1000) as u64;
        bucket.updated_at_ms = now;
        let required = u64::from(cost) * 1000;
        let allowed = bucket.milli_tokens >= required;
        let retry_after_ms = if allowed {
            bucket.milli_tokens -= required;
            0
        } else {
            (required - bucket.milli_tokens).div_ceil(u64::from(policy.refill_per_second))
        };
        Ok(RateDecision {
            allowed,
            remaining: (bucket.milli_tokens / 1000) as u32,
            retry_after_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lease(session: u8) -> SessionLease {
        SessionLease {
            device_id: "00000000-0000-4000-8000-000000000001".into(),
            session_id: format!("00000000-0000-4000-8000-{session:012}"),
            gateway_id: "gateway-1".into(),
            expires_at_ms: 1000,
        }
    }
    #[tokio::test]
    async fn duplicate_active_session_conflicts_without_replacing_route() {
        let state = MemoryEphemeralState::new(10).unwrap();
        let a = lease(2);
        let b = lease(3);
        state.bind(a.clone(), 1).await.unwrap();
        assert!(matches!(
            state.bind(b.clone(), 2).await,
            Err(StoreError::Conflict)
        ));
        assert_eq!(
            state.route(&a.device_id, 999).await.unwrap(),
            Some(a.clone())
        );
        assert!(!state.unbind(&a.device_id, &b.session_id).await.unwrap());
        assert!(!state
            .renew(&b.device_id, &b.session_id, 1100, 100)
            .await
            .unwrap());
        assert!(state.renew(&a.device_id, &a.session_id, 999, 100).await.unwrap());
        assert!(state.route(&b.device_id, 1000).await.unwrap().is_none());
    }
    #[tokio::test]
    async fn refill_retry_rounding_and_backwards_clock() {
        let state = MemoryEphemeralState::new(10).unwrap();
        let p = BucketPolicy {
            capacity: 2,
            refill_per_second: 3,
        };
        assert!(
            state
                .consume("send:opaque", p, 2, 100)
                .await
                .unwrap()
                .allowed
        );
        let denied = state.consume("send:opaque", p, 1, 100).await.unwrap();
        assert!(!denied.allowed);
        assert_eq!(denied.retry_after_ms, 334);
        assert!(
            !state
                .consume("send:opaque", p, 1, 99)
                .await
                .unwrap()
                .allowed
        );
        assert!(
            !state
                .consume("send:opaque", p, 1, 433)
                .await
                .unwrap()
                .allowed
        );
        assert!(
            state
                .consume("send:opaque", p, 1, 434)
                .await
                .unwrap()
                .allowed
        );
        assert!(
            state
                .consume("send:opaque", p, 2, 2000)
                .await
                .unwrap()
                .allowed
        );
    }
    #[tokio::test]
    async fn concurrent_consumption_never_overspends() {
        let state = std::sync::Arc::new(MemoryEphemeralState::new(1).unwrap());
        let mut tasks = vec![];
        for _ in 0..20 {
            let state = state.clone();
            tasks.push(tokio::spawn(async move {
                state
                    .consume(
                        "shared",
                        BucketPolicy {
                            capacity: 5,
                            refill_per_second: 1,
                        },
                        1,
                        0,
                    )
                    .await
                    .unwrap()
                    .allowed
            }));
        }
        let mut allowed = 0;
        for task in tasks {
            if task.await.unwrap() {
                allowed += 1;
            }
        }
        assert_eq!(allowed, 5);
    }
    #[tokio::test]
    async fn bounded_memory_and_invalid_input() {
        let state = MemoryEphemeralState::new(1).unwrap();
        let p = BucketPolicy {
            capacity: 1,
            refill_per_second: 1,
        };
        assert!(state.consume("a", p, 1, 0).await.is_ok());
        assert!(matches!(
            state.consume("b", p, 1, 0).await,
            Err(StoreError::Unavailable)
        ));
        assert!(state.consume("b", p, 1, 1000).await.is_ok());
        assert!(state.consume("b", p, 0, 1000).await.is_err());
        assert!(state.bind(lease(2), 1000).await.is_err());
    }
}
