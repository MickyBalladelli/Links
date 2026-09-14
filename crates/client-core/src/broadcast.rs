//! MLS broadcast profile boundaries.
//!
//! MLS does not define an application-level "read-only" member bit. Links
//! enforces that policy at the client-core boundary: a broadcast subscriber
//! may join and process authenticated publisher commits, but cannot create a
//! group, create application messages, stage membership changes, or merge a
//! local pending commit.

use crate::{mls::MlsEngine, CoreError};

/// A passive broadcast subscriber. The wrapped engine is intentionally not
/// exposed, so callers cannot bypass the read-only policy through this type.
pub struct BroadcastSubscriber<M> {
    engine: M,
}

impl<M> BroadcastSubscriber<M> {
    pub fn new(engine: M) -> Self {
        Self { engine }
    }

    pub fn into_inner(self) -> M {
        self.engine
    }

    pub fn engine(&self) -> &M {
        &self.engine
    }
}

impl<M: MlsEngine> MlsEngine for BroadcastSubscriber<M> {
    fn create_group(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::Authentication)
    }

    fn join_group(&mut self, conversation_id: &str, welcome: &[u8]) -> Result<(), CoreError> {
        self.engine.join_group(conversation_id, welcome)
    }

    fn process_commit(&mut self, conversation_id: &str, commit: &[u8]) -> Result<(), CoreError> {
        self.engine.process_commit(conversation_id, commit)
    }

    fn encrypt(&mut self, _: &str, _: &str, _: &[u8]) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::Authentication)
    }

    fn decrypt(
        &mut self,
        ciphertext: &[u8],
    ) -> Result<crate::mls::AuthenticatedApplication, CoreError> {
        self.engine.decrypt(ciphertext)
    }

    fn current_epoch(&self, conversation_id: &str) -> Result<u64, CoreError> {
        self.engine.current_epoch(conversation_id)
    }

    fn join_direct_group(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::Authentication)
    }

    fn process_direct_commit(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::Authentication)
    }

    fn ensure_direct_group(
        &mut self,
        _: &str,
        _: &[&[u8]],
    ) -> Result<Option<crate::mls::PendingCommit>, CoreError> {
        Err(CoreError::Authentication)
    }

    fn merge_pending_direct_commit(&mut self, _: &str) -> Result<(), CoreError> {
        Err(CoreError::Authentication)
    }

    fn ensure_group(
        &mut self,
        _: &str,
        _: &[&[u8]],
    ) -> Result<Option<crate::mls::PendingCommit>, CoreError> {
        Err(CoreError::Authentication)
    }

    fn merge_pending_group_commit(&mut self, _: &str) -> Result<(), CoreError> {
        Err(CoreError::Authentication)
    }
}
