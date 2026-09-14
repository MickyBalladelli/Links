//! Shared rules for channel, business, and bot client surfaces.
//!
//! A surface is an account-facing client mode. It does not change the MLS or
//! envelope format: the platform host passes the surface identity into the
//! existing shared-core send/receive coordinator.

use crate::{protocol, CoreError};

pub const MAX_SURFACE_NAME_BYTES: usize = 80;
pub const MAX_SURFACE_TEXT_BYTES: usize = protocol::MAX_MESSAGE_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceKind {
    Channel,
    Business,
    Bot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceRole {
    Owner,
    Admin,
    Member,
    Subscriber,
    Bot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceProfile {
    surface_id: String,
    kind: SurfaceKind,
    role: SurfaceRole,
    display_name: String,
    verified: bool,
}

impl SurfaceProfile {
    pub fn new(
        surface_id: impl Into<String>,
        kind: SurfaceKind,
        role: SurfaceRole,
        display_name: impl Into<String>,
        verified: bool,
    ) -> Result<Self, CoreError> {
        let profile = Self {
            surface_id: surface_id.into(),
            kind,
            role,
            display_name: display_name.into(),
            verified,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        protocol::validate_id(&self.surface_id)?;
        if self.display_name.is_empty()
            || self.display_name.len() > MAX_SURFACE_NAME_BYTES
            || self.display_name.chars().any(char::is_control)
        {
            return Err(protocol::ProtocolError::Invalid("surface display name").into());
        }
        let role_allowed = match self.kind {
            SurfaceKind::Channel => {
                matches!(self.role, SurfaceRole::Owner | SurfaceRole::Admin | SurfaceRole::Subscriber)
            }
            SurfaceKind::Business => {
                matches!(self.role, SurfaceRole::Owner | SurfaceRole::Admin | SurfaceRole::Member)
            }
            SurfaceKind::Bot => self.role == SurfaceRole::Bot,
        };
        if !role_allowed {
            return Err(protocol::ProtocolError::Invalid("surface role").into());
        }
        Ok(())
    }

    pub fn surface_id(&self) -> &str {
        &self.surface_id
    }

    pub fn kind(&self) -> SurfaceKind {
        self.kind
    }

    pub fn role(&self) -> SurfaceRole {
        self.role
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn verified(&self) -> bool {
        self.verified
    }

    pub fn can_send(&self) -> bool {
        !matches!(self.kind, SurfaceKind::Channel) || self.role != SurfaceRole::Subscriber
    }

    pub fn can_publish(&self) -> bool {
        match self.kind {
            SurfaceKind::Channel => matches!(self.role, SurfaceRole::Owner | SurfaceRole::Admin),
            SurfaceKind::Business => matches!(self.role, SurfaceRole::Owner | SurfaceRole::Admin),
            SurfaceKind::Bot => false,
        }
    }

    pub fn can_manage(&self) -> bool {
        matches!(self.role, SurfaceRole::Owner | SurfaceRole::Admin)
    }

    pub fn validate_text(text: &str) -> Result<(), CoreError> {
        if text.is_empty() || text.len() > MAX_SURFACE_TEXT_BYTES {
            return Err(protocol::ProtocolError::Invalid("surface text").into());
        }
        Ok(())
    }
}
