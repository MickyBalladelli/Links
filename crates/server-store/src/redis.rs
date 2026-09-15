//! Redis-backed implementation of [`crate::ephemeral::EphemeralState`].
//!
//! The adapter keeps every operation to one Redis key, so it works in Redis
//! Cluster without cross-slot transactions. Lua scripts use Redis `TIME` and
//! perform the compare-and-set or token-bucket update atomically. A concrete
//! Redis client implements `RedisScriptExecutor` and must map its native reply
//! values without logging keys or values.
use crate::{
    ephemeral::{BucketPolicy, EphemeralState, RateDecision, SessionLease, MAX_SESSION_TTL_MS},
    StoreError,
};
use async_trait::async_trait;
use links_protocol::validate_id;
use std::sync::Arc;

const SESSION_BIND_SCRIPT: &str = r#"
local t = redis.call('TIME')
local now = tonumber(t[1]) * 1000 + math.floor(tonumber(t[2]) / 1000)
local expiry = tonumber(ARGV[4])
if not expiry or expiry <= now or expiry - now > 120000 then return -1 end
if redis.call('EXISTS', KEYS[1]) == 1 then
  local current_expiry = tonumber(redis.call('HGET', KEYS[1], 'expires_at_ms'))
  if not current_expiry then return -3 end
  if current_expiry > now then return -2 end
  redis.call('DEL', KEYS[1])
end
redis.call('HSET', KEYS[1],
  'device_id', ARGV[1],
  'session_id', ARGV[2],
  'gateway_id', ARGV[3],
  'expires_at_ms', ARGV[4])
redis.call('PEXPIREAT', KEYS[1], expiry)
return 1
"#;

const SESSION_RENEW_SCRIPT: &str = r#"
local t = redis.call('TIME')
local now = tonumber(t[1]) * 1000 + math.floor(tonumber(t[2]) / 1000)
local expiry = tonumber(ARGV[3])
if not expiry or expiry <= now or expiry - now > 120000 then return -1 end
if redis.call('EXISTS', KEYS[1]) == 0 then return 0 end
if redis.call('HGET', KEYS[1], 'session_id') ~= ARGV[2] then return 0 end
local current_expiry = tonumber(redis.call('HGET', KEYS[1], 'expires_at_ms'))
if not current_expiry or current_expiry <= now then
  redis.call('DEL', KEYS[1])
  return 0
end
redis.call('HSET', KEYS[1], 'expires_at_ms', ARGV[3])
redis.call('PEXPIREAT', KEYS[1], expiry)
return 1
"#;

const SESSION_UNBIND_SCRIPT: &str = r#"
if redis.call('HGET', KEYS[1], 'session_id') ~= ARGV[1] then return 0 end
redis.call('DEL', KEYS[1])
return 1
"#;

const SESSION_ROUTE_SCRIPT: &str = r#"
local t = redis.call('TIME')
local now = tonumber(t[1]) * 1000 + math.floor(tonumber(t[2]) / 1000)
if redis.call('EXISTS', KEYS[1]) == 0 then return {} end
local expiry = tonumber(redis.call('HGET', KEYS[1], 'expires_at_ms'))
if not expiry or expiry <= now then
  redis.call('DEL', KEYS[1])
  return {}
end
return {
  redis.call('HGET', KEYS[1], 'device_id'),
  redis.call('HGET', KEYS[1], 'session_id'),
  redis.call('HGET', KEYS[1], 'gateway_id'),
  expiry
}
"#;

const BUCKET_CONSUME_SCRIPT: &str = r#"
local t = redis.call('TIME')
local now = tonumber(t[1]) * 1000 + math.floor(tonumber(t[2]) / 1000)
local capacity = tonumber(ARGV[1])
local refill = tonumber(ARGV[2])
local cost = tonumber(ARGV[3])
local stored_capacity = redis.call('HGET', KEYS[1], 'capacity')
local stored_refill = redis.call('HGET', KEYS[1], 'refill_per_second')
if stored_capacity and (tonumber(stored_capacity) ~= capacity or tonumber(stored_refill) ~= refill) then
  return -2
end
local milli = tonumber(redis.call('HGET', KEYS[1], 'milli_tokens'))
local updated = tonumber(redis.call('HGET', KEYS[1], 'updated_at_ms'))
if not milli or not updated then
  milli = capacity * 1000
  updated = now
end
if now < updated then now = updated end
milli = math.min(capacity * 1000, milli + (now - updated) * refill)
local required = cost * 1000
local allowed = 0
local retry = 0
if milli >= required then
  milli = milli - required
  allowed = 1
else
  retry = math.ceil((required - milli) / refill)
end
redis.call('HSET', KEYS[1],
  'capacity', capacity,
  'refill_per_second', refill,
  'milli_tokens', milli,
  'updated_at_ms', now)
local idle = math.ceil((capacity * 1000) / refill)
redis.call('PEXPIREAT', KEYS[1], now + idle)
return { allowed, math.floor(milli / 1000), retry }
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RedisReply {
    Nil,
    Integer(i64),
    Bytes(Vec<u8>),
    Simple(String),
    Array(Vec<RedisReply>),
}

/// Minimal client boundary. The implementation should call Redis `EVALSHA` or
/// `EVAL` with the one supplied key and arguments. Redis Cluster requires the
/// key to be passed in `KEYS[1]`, never interpolated into a script.
#[async_trait]
pub trait RedisScriptExecutor: Send + Sync {
    async fn eval(
        &self,
        script: &'static str,
        key: &str,
        args: &[String],
    ) -> Result<RedisReply, StoreError>;
}

pub struct RedisEphemeralState<E> {
    executor: Arc<E>,
    namespace: String,
}

impl<E> RedisEphemeralState<E>
where
    E: RedisScriptExecutor + 'static,
{
    pub fn new(executor: Arc<E>, namespace: String) -> Result<Self, StoreError> {
        if !valid_namespace(&namespace) {
            return Err(StoreError::Invalid);
        }
        Ok(Self {
            executor,
            namespace,
        })
    }

    fn session_key(&self, device_id: &str) -> String {
        format!("{}:{{session:{device_id}}}", self.namespace)
    }

    fn bucket_key(&self, key: &str) -> String {
        format!("{}:{{bucket:{key}}}", self.namespace)
    }
}

#[async_trait]
impl<E> EphemeralState for RedisEphemeralState<E>
where
    E: RedisScriptExecutor + 'static,
{
    async fn bind(&self, lease: SessionLease, now_ms: u64) -> Result<(), StoreError> {
        validate_id(&lease.device_id)?;
        validate_id(&lease.session_id)?;
        if !valid_key(&lease.gateway_id) || !valid_expiry(lease.expires_at_ms, now_ms) {
            return Err(StoreError::Invalid);
        }
        let reply = self
            .executor
            .eval(
                SESSION_BIND_SCRIPT,
                &self.session_key(&lease.device_id),
                &[
                    lease.device_id,
                    lease.session_id,
                    lease.gateway_id,
                    lease.expires_at_ms.to_string(),
                ],
            )
            .await?;
        match integer(&reply)? {
            1 => Ok(()),
            -1 => Err(StoreError::Invalid),
            -2 => Err(StoreError::Conflict),
            _ => Err(StoreError::Unavailable),
        }
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
        let reply = self
            .executor
            .eval(
                SESSION_RENEW_SCRIPT,
                &self.session_key(device_id),
                &[
                    device_id.to_owned(),
                    session_id.to_owned(),
                    expires_at_ms.to_string(),
                ],
            )
            .await?;
        match integer(&reply)? {
            1 => Ok(true),
            0 => Ok(false),
            -1 => Err(StoreError::Invalid),
            _ => Err(StoreError::Unavailable),
        }
    }

    async fn unbind(&self, device_id: &str, session_id: &str) -> Result<bool, StoreError> {
        validate_id(device_id)?;
        validate_id(session_id)?;
        let reply = self
            .executor
            .eval(
                SESSION_UNBIND_SCRIPT,
                &self.session_key(device_id),
                &[session_id.to_owned()],
            )
            .await?;
        match integer(&reply)? {
            1 => Ok(true),
            0 => Ok(false),
            _ => Err(StoreError::Unavailable),
        }
    }

    async fn route(
        &self,
        device_id: &str,
        now_ms: u64,
    ) -> Result<Option<SessionLease>, StoreError> {
        validate_id(device_id)?;
        let reply = self
            .executor
            .eval(
                SESSION_ROUTE_SCRIPT,
                &self.session_key(device_id),
                &[now_ms.to_string()],
            )
            .await?;
        let values = match reply {
            RedisReply::Array(values) => values,
            RedisReply::Nil => return Ok(None),
            _ => return Err(StoreError::Unavailable),
        };
        if values.is_empty() {
            return Ok(None);
        }
        if values.len() != 4 {
            return Err(StoreError::Unavailable);
        }
        let stored_device = text(&values[0])?.to_owned();
        let session_id = text(&values[1])?.to_owned();
        let gateway_id = text(&values[2])?.to_owned();
        let expires_at_ms = unsigned(&values[3])?;
        if stored_device != device_id
            || validate_id(&session_id).is_err()
            || !valid_key(&gateway_id)
        {
            return Err(StoreError::Unavailable);
        }
        Ok(Some(SessionLease {
            device_id: stored_device,
            session_id,
            gateway_id,
            expires_at_ms,
        }))
    }

    async fn consume(
        &self,
        key: &str,
        policy: BucketPolicy,
        cost: u32,
        _now_ms: u64,
    ) -> Result<RateDecision, StoreError> {
        if !valid_key(key)
            || policy.capacity == 0
            || policy.refill_per_second == 0
            || cost == 0
            || cost > policy.capacity
        {
            return Err(StoreError::Invalid);
        }
        let reply = self
            .executor
            .eval(
                BUCKET_CONSUME_SCRIPT,
                &self.bucket_key(key),
                &[
                    policy.capacity.to_string(),
                    policy.refill_per_second.to_string(),
                    cost.to_string(),
                ],
            )
            .await?;
        if integer(&reply).ok() == Some(-2) {
            return Err(StoreError::Conflict);
        }
        let values = match reply {
            RedisReply::Array(values) if values.len() == 3 => values,
            _ => return Err(StoreError::Unavailable),
        };
        let allowed = integer(&values[0])?;
        let remaining = unsigned(&values[1])?;
        let retry_after_ms = unsigned(&values[2])?;
        if allowed > 1 || remaining > u64::from(policy.capacity) {
            return Err(StoreError::Unavailable);
        }
        Ok(RateDecision {
            allowed: allowed == 1,
            remaining: remaining as u32,
            retry_after_ms,
        })
    }
}

fn integer(reply: &RedisReply) -> Result<i64, StoreError> {
    match reply {
        RedisReply::Integer(value) => Ok(*value),
        _ => Err(StoreError::Unavailable),
    }
}

fn unsigned(reply: &RedisReply) -> Result<u64, StoreError> {
    let value = integer(reply)?;
    u64::try_from(value).map_err(|_| StoreError::Unavailable)
}

fn text(reply: &RedisReply) -> Result<&str, StoreError> {
    match reply {
        RedisReply::Bytes(value) => std::str::from_utf8(value).map_err(|_| StoreError::Unavailable),
        RedisReply::Simple(value) => Ok(value),
        _ => Err(StoreError::Unavailable),
    }
}

fn valid_expiry(expires_at_ms: u64, now_ms: u64) -> bool {
    expires_at_ms > now_ms && expires_at_ms - now_ms <= MAX_SESSION_TTL_MS
}

fn valid_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-'))
}

fn valid_namespace(value: &str) -> bool {
    valid_key(value) && !value.contains('{') && !value.contains('}')
}
