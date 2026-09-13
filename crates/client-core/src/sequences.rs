//! Client-owned conversation sequence allocation.
//!
//! Conversation IDs are private MLS metadata, so the server cannot assign a
//! conversation counter. Each sender device owns a strictly increasing
//! sequence in each conversation. The host persists the updated counter with
//! its outbox/MLS transaction before treating the send as durable.
use crate::{protocol, CoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSequence {
    conversation_id: String,
    sender_device_id: String,
    last_sequence_id: u64,
}

impl ConversationSequence {
    pub fn new(conversation_id: String, sender_device_id: String) -> Result<Self, CoreError> {
        Self::restore(conversation_id, sender_device_id, 0)
    }

    pub fn restore(
        conversation_id: String,
        sender_device_id: String,
        last_sequence_id: u64,
    ) -> Result<Self, CoreError> {
        protocol::validate_id(&conversation_id)?;
        protocol::validate_id(&sender_device_id)?;
        if last_sequence_id > protocol::MAX_CURSOR {
            return Err(CoreError::InvalidSequence);
        }
        Ok(Self {
            conversation_id,
            sender_device_id,
            last_sequence_id,
        })
    }

    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }

    pub fn sender_device_id(&self) -> &str {
        &self.sender_device_id
    }

    pub fn last_sequence_id(&self) -> u64 {
        self.last_sequence_id
    }

    pub fn reserve_next(&mut self) -> Result<u64, CoreError> {
        let next = self
            .last_sequence_id
            .checked_add(1)
            .filter(|value| *value <= protocol::MAX_CURSOR)
            .ok_or(CoreError::InvalidSequence)?;
        self.last_sequence_id = next;
        Ok(next)
    }

    /// Accept an authenticated remote sequence after its message and MLS
    /// state are durable. Gaps are allowed for failed sends; reuse is not.
    pub fn accept(&mut self, sequence_id: u64) -> Result<(), CoreError> {
        if sequence_id == 0
            || sequence_id > protocol::MAX_CURSOR
            || sequence_id <= self.last_sequence_id
        {
            return Err(CoreError::InvalidSequence);
        }
        self.last_sequence_id = sequence_id;
        Ok(())
    }
}
