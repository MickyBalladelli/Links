//! Desktop adapters for channel, business, and bot client surfaces.

use crate::session::{DesktopMessagingCore, DesktopSocketFactory, DesktopTextSession};
use links_client_core::{surfaces::SurfaceProfile, CoreError};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopSurfaceClient {
    profile: SurfaceProfile,
}

impl DesktopSurfaceClient {
    pub fn new(profile: SurfaceProfile) -> Result<Self, CoreError> {
        profile.validate()?;
        Ok(Self { profile })
    }

    pub fn profile(&self) -> &SurfaceProfile {
        &self.profile
    }

    pub fn send_text<C, F>(
        &self,
        session: &mut DesktopTextSession<C, F>,
        now: Instant,
        conversation_id: &str,
        text: &str,
    ) -> Result<(), CoreError>
    where
        C: DesktopMessagingCore + 'static,
        F: DesktopSocketFactory,
    {
        session.send_surface_text(now, &self.profile, conversation_id, text)
    }
}
