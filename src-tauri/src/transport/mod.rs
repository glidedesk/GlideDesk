// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Transfer Route, TLS Local Transport, and Resumable Transfer Protocol

pub mod bt;
pub mod route;
pub mod tls;
pub mod transfer;
pub mod wifi;

pub use route::{RouteCandidate, RouteMode, RoutePicker, SelectedRoute};
pub use tls::{LocalTlsIdentity, PinnedCertVerifier, format_beam_auth_token};
pub use transfer::{TransferBatch, TransferItem, TransferManager, TransferPhase, cancel_transfer_task, get_transfer_status};
