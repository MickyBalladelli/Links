//! Loopback-only gateway for local transport development.
//!
//! This process exercises the real `links.v1` socket adapter with an in-memory
//! ciphertext mailbox. It is not a production gateway and deliberately has no
//! non-loopback bind path.
use async_trait::async_trait;
use links_gateway::{
    websocket::WebSocketAdapter, AuthenticatedDevice, DeviceAuthenticator, Gateway, GatewayConfig,
    GatewayError, PushNotifier, PushWakeup, RegionBus,
};
use links_protocol::{self as protocol, v1};
use links_server_store::{
    ephemeral::MemoryEphemeralState,
    payload::{AppendRequest, AppendResult, EncryptedPayloadStore, ReadRequest},
    StoreError,
};
use prost::Message;
use std::{collections::HashMap, error::Error, net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, sync::Mutex};
use uuid::Uuid;

const LOCAL_ACCESS_TOKEN: &[u8] = b"local-development-token";

struct LocalAuthenticator;

#[async_trait]
impl DeviceAuthenticator for LocalAuthenticator {
    async fn authenticate(
        &self,
        device_id: Uuid,
        access_token: &[u8],
    ) -> Result<AuthenticatedDevice, GatewayError> {
        if access_token != LOCAL_ACCESS_TOKEN {
            return Err(GatewayError::Authentication);
        }
        Ok(AuthenticatedDevice {
            user_id: device_id,
            device_id,
        })
    }
}

struct LocalRegionBus;

#[async_trait]
impl RegionBus for LocalRegionBus {
    async fn forward(
        &self,
        _destination_gateway_id: &str,
        _delivery: links_gateway::ForwardedEnvelope,
    ) -> Result<(), GatewayError> {
        Err(GatewayError::Unavailable)
    }
}

struct LocalPushNotifier;

#[async_trait]
impl PushNotifier for LocalPushNotifier {
    async fn notify(&self, _wakeup: PushWakeup) -> Result<(), GatewayError> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum LocalItemState {
    Live,
    Acknowledged,
    Expired,
}

struct LocalItem {
    cursor: u64,
    envelope: v1::Envelope,
    state: LocalItemState,
}

#[derive(Default)]
struct LocalMailbox {
    items: HashMap<String, Vec<LocalItem>>,
}

struct LocalMailboxState {
    mailbox: Mutex<LocalMailbox>,
}

impl LocalMailboxState {
    fn new() -> Self {
        Self {
            mailbox: Mutex::new(LocalMailbox::default()),
        }
    }
}

#[async_trait]
impl EncryptedPayloadStore for LocalMailboxState {
    async fn append(&self, request: AppendRequest) -> Result<AppendResult, StoreError> {
        let envelope = request.envelope().clone();
        protocol::validate_enqueue(&envelope, request.accepted_at_ms())?;
        let mut mailbox = self.mailbox.lock().await;
        let items = mailbox
            .items
            .entry(envelope.recipient_device_id.clone())
            .or_default();
        if let Some(existing) = items
            .iter()
            .find(|item| item.envelope.envelope_id == envelope.envelope_id)
        {
            if existing.envelope.encode_to_vec() != envelope.encode_to_vec() {
                return Err(StoreError::Conflict);
            }
            return Ok(AppendResult {
                cursor: existing.cursor,
                duplicate: true,
            });
        }
        let cursor = items
            .last()
            .map_or(1, |item| item.cursor.checked_add(1).unwrap_or(u64::MAX));
        if cursor == u64::MAX || cursor > protocol::MAX_CURSOR {
            return Err(StoreError::Conflict);
        }
        items.push(LocalItem {
            cursor,
            envelope,
            state: LocalItemState::Live,
        });
        Ok(AppendResult {
            cursor,
            duplicate: false,
        })
    }

    async fn read(&self, request: ReadRequest) -> Result<v1::SyncBatch, StoreError> {
        let mut mailbox = self.mailbox.lock().await;
        let items = mailbox
            .items
            .entry(request.device_id().to_owned())
            .or_default();
        let high_watermark = items.last().map_or(0, |item| item.cursor);
        if request.after_cursor() > high_watermark {
            return Err(StoreError::Invalid);
        }
        let last_cursor = request
            .after_cursor()
            .saturating_add(u64::from(request.limit()))
            .min(high_watermark);
        let mut batch_items = Vec::new();
        if request.after_cursor() < last_cursor {
            for cursor in (request.after_cursor() + 1)..=last_cursor {
                let item = items
                    .iter_mut()
                    .find(|item| item.cursor == cursor)
                    .ok_or(StoreError::CursorExpired)?;
                let entry = match item.state {
                    LocalItemState::Acknowledged => {
                        Some(v1::queue_item::Entry::Tombstone(v1::Tombstone {
                            reason: v1::tombstone::Reason::Acknowledged as i32,
                        }))
                    }
                    LocalItemState::Expired => {
                        Some(v1::queue_item::Entry::Tombstone(v1::Tombstone {
                            reason: v1::tombstone::Reason::Expired as i32,
                        }))
                    }
                    LocalItemState::Live if item.envelope.expires_at_ms > request.now_ms() => {
                        Some(v1::queue_item::Entry::Envelope(item.envelope.clone()))
                    }
                    LocalItemState::Live => {
                        item.state = LocalItemState::Expired;
                        Some(v1::queue_item::Entry::Tombstone(v1::Tombstone {
                            reason: v1::tombstone::Reason::Expired as i32,
                        }))
                    }
                };
                batch_items.push(v1::QueueItem { cursor, entry });
            }
        }
        let next_cursor = batch_items
            .last()
            .map_or(request.after_cursor(), |item| item.cursor);
        let batch = v1::SyncBatch {
            recipient_device_id: request.device_id().to_owned(),
            after_cursor: request.after_cursor(),
            next_cursor,
            high_watermark,
            items: batch_items,
        };
        protocol::validate_sync_batch(&batch)?;
        Ok(batch)
    }

    async fn acknowledge(
        &self,
        device_id: &str,
        through_cursor: u64,
        _now_ms: u64,
    ) -> Result<(), StoreError> {
        protocol::validate_id(device_id)?;
        let mut mailbox = self.mailbox.lock().await;
        let items = mailbox
            .items
            .get_mut(device_id)
            .ok_or(StoreError::Invalid)?;
        let high_watermark = items.last().map_or(0, |item| item.cursor);
        if through_cursor > high_watermark {
            return Err(StoreError::Invalid);
        }
        for item in items
            .iter_mut()
            .filter(|item| item.cursor <= through_cursor)
        {
            item.state = LocalItemState::Acknowledged;
        }
        Ok(())
    }

    async fn purge_expired(&self, now_ms: u64, limit: u32) -> Result<u64, StoreError> {
        let mut mailbox = self.mailbox.lock().await;
        let mut purged = 0;
        for items in mailbox.items.values_mut() {
            for item in items.iter_mut() {
                if purged >= u64::from(limit) {
                    break;
                }
                if matches!(item.state, LocalItemState::Live)
                    && item.envelope.expires_at_ms <= now_ms
                {
                    item.state = LocalItemState::Expired;
                    purged += 1;
                }
            }
        }
        Ok(purged)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let address: SocketAddr = std::env::var("GATEWAY_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8081".to_owned())
        .parse()?;
    if !address.ip().is_loopback() {
        return Err("links-gateway-local only binds to loopback".into());
    }
    let gateway_id = std::env::var("GATEWAY_ID").unwrap_or_else(|_| "local-gateway".to_owned());
    let region = std::env::var("GATEWAY_REGION").unwrap_or_else(|_| "local".to_owned());
    let gateway = Arc::new(Gateway::new(
        GatewayConfig::new(gateway_id, region)?,
        Arc::new(MemoryEphemeralState::new(1_024)?),
        Arc::new(LocalMailboxState::new()),
        Arc::new(LocalAuthenticator),
        Arc::new(LocalRegionBus),
        Arc::new(LocalPushNotifier),
    ));
    let adapter = WebSocketAdapter::new(gateway);
    let listener = TcpListener::bind(address).await?;
    println!("links-gateway-local listening on ws://{address}/v1/connect");
    adapter.serve_until_shutdown(listener).await?;
    Ok(())
}
