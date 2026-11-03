// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Resumable, Checksummed, Cancellable Batched File Transfer Protocol (LinkMyMac/Beam Pattern)

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferPhase {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "batchStart")]
    BatchStart,
    #[serde(rename = "fileStart")]
    FileStart,
    #[serde(rename = "transferring")]
    Transferring,
    #[serde(rename = "checksumStart")]
    ChecksumStart,
    #[serde(rename = "checksumDone")]
    ChecksumDone,
    #[serde(rename = "fileDone")]
    FileDone,
    #[serde(rename = "batchDone")]
    BatchDone,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferItem {
    pub file_id: String,
    pub filename: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub checksum_sha256: Option<String>,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferBatch {
    pub batch_id: String,
    pub items: Vec<TransferItem>,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub phase: TransferPhase,
}

impl TransferBatch {
    pub fn new(batch_id: String, files: Vec<(String, u64)>) -> Self {
        let mut total_bytes = 0;
        let mut items = Vec::new();
        for (idx, (filename, size)) in files.into_iter().enumerate() {
            total_bytes += size;
            items.push(TransferItem {
                file_id: format!("file_{}_{}", idx, filename),
                filename,
                total_bytes: size,
                transferred_bytes: 0,
                checksum_sha256: None,
                completed: false,
            });
        }

        Self {
            batch_id,
            items,
            total_bytes,
            transferred_bytes: 0,
            phase: TransferPhase::BatchStart,
        }
    }

    pub fn start_file(&mut self, file_id: &str) -> Result<(), String> {
        let item = self.items.iter_mut().find(|it| it.file_id == file_id)
            .ok_or_else(|| format!("File '{}' not found in batch", file_id))?;
        self.phase = TransferPhase::FileStart;
        log::info!("[Transfer] FILE_START: {} ({} bytes)", item.filename, item.total_bytes);
        self.phase = TransferPhase::Transferring;
        Ok(())
    }

    pub fn update_progress(&mut self, file_id: &str, chunk_size: u64) -> Result<(u64, u64), String> {
        let (current, total) = {
            let item = self.items.iter_mut().find(|it| it.file_id == file_id)
                .ok_or_else(|| format!("File '{}' not found in batch", file_id))?;
            item.transferred_bytes = (item.transferred_bytes + chunk_size).min(item.total_bytes);
            (item.transferred_bytes, item.total_bytes)
        };
        self.transferred_bytes = self.items.iter().map(|it| it.transferred_bytes).sum();
        Ok((current, total))
    }

    pub fn verify_and_complete_file(&mut self, file_id: &str, content: &[u8], expected_checksum: &str) -> Result<bool, String> {
        self.phase = TransferPhase::ChecksumStart;
        log::info!("[Transfer] CHECKSUM_START for file {}", file_id);

        let mut hasher = Sha256::new();
        hasher.update(content);
        let digest = format!("{:x}", hasher.finalize());

        let clean_expected = expected_checksum.trim().to_lowercase();
        if digest != clean_expected {
            self.phase = TransferPhase::Failed;
            log::error!("[Transfer] CHECKSUM_MISMATCH for {}: expected {}, computed {}", file_id, clean_expected, digest);
            return Err(format!("Checksum mismatch for {}: expected {}, got {}", file_id, clean_expected, digest));
        }

        self.phase = TransferPhase::ChecksumDone;
        log::info!("[Transfer] CHECKSUM_DONE: file {} verified OK", file_id);

        let item = self.items.iter_mut().find(|it| it.file_id == file_id)
            .ok_or_else(|| format!("File '{}' not found in batch", file_id))?;
        item.checksum_sha256 = Some(digest);
        item.completed = true;
        item.transferred_bytes = item.total_bytes;
        self.phase = TransferPhase::FileDone;
        log::info!("[Transfer] FILE_DONE: {}", item.filename);

        if self.items.iter().all(|it| it.completed) {
            self.phase = TransferPhase::BatchDone;
            log::info!("[Transfer] BATCH_DONE: batch {} finished successfully", self.batch_id);
        }

        Ok(true)
    }

    pub fn cancel(&mut self) {
        log::warn!("[Transfer] CANCEL_TASK: batch {} cancelled", self.batch_id);
        self.phase = TransferPhase::Cancelled;
    }
}

pub struct TransferManager {
    pub batches: Arc<RwLock<HashMap<String, TransferBatch>>>,
}

impl TransferManager {
    pub fn new() -> Self {
        Self {
            batches: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[tauri::command]
pub fn cancel_transfer_task(
    batch_id: String,
    mgr: State<'_, TransferManager>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Transfer] Cancelling transfer task: {}", batch_id);
    let mut batches = mgr.batches.write();
    if let Some(b) = batches.get_mut(&batch_id) {
        b.cancel();
        let _ = app.emit("transfer-cancelled", &serde_json::json!({ "batch_id": batch_id }));
        Ok(())
    } else {
        Err(format!("Batch '{}' not found", batch_id))
    }
}

#[tauri::command]
pub fn get_transfer_status(
    batch_id: String,
    mgr: State<'_, TransferManager>,
) -> Result<TransferBatch, String> {
    let batches = mgr.batches.read();
    batches.get(&batch_id)
        .cloned()
        .ok_or_else(|| format!("Transfer batch '{}' not found", batch_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_batch_lifecycle_and_checksum() {
        let files = vec![
            ("video.mp4".to_string(), 1000),
            ("notes.txt".to_string(), 500),
        ];
        let mut batch = TransferBatch::new("batch_01".to_string(), files);
        assert_eq!(batch.phase, TransferPhase::BatchStart);
        assert_eq!(batch.total_bytes, 1500);

        // 1. Start file
        let file_0 = batch.items[0].file_id.clone();
        assert!(batch.start_file(&file_0).is_ok());
        assert_eq!(batch.phase, TransferPhase::Transferring);

        // 2. Progress update
        let (current, total) = batch.update_progress(&file_0, 400).unwrap();
        assert_eq!(current, 400);
        assert_eq!(total, 1000);
        assert_eq!(batch.transferred_bytes, 400);

        // 3. Verify checksum
        let dummy_data = vec![0x42u8; 1000];
        let mut hasher = Sha256::new();
        hasher.update(&dummy_data);
        let expected_hash = format!("{:x}", hasher.finalize());

        let verify_ok = batch.verify_and_complete_file(&file_0, &dummy_data, &expected_hash);
        assert!(verify_ok.is_ok());
        assert!(batch.items[0].completed);
        assert_eq!(batch.phase, TransferPhase::FileDone);

        // 4. Mismatch checksum fails
        let file_1 = batch.items[1].file_id.clone();
        assert!(batch.start_file(&file_1).is_ok());
        let bad_verify = batch.verify_and_complete_file(&file_1, b"hello", "000000000000");
        assert!(bad_verify.is_err());
        assert_eq!(batch.phase, TransferPhase::Failed);

        // 5. Cancel task
        batch.cancel();
        assert_eq!(batch.phase, TransferPhase::Cancelled);
    }

    #[test]
    fn test_transfer_manager_live_batch_status_and_cancellation() {
        let mgr = TransferManager::new();
        let batch_id = "live_batch_99".to_string();
        let files = vec![("document.pdf".to_string(), 2048)];
        let mut batch = TransferBatch::new(batch_id.clone(), files);
        let file_id = batch.items[0].file_id.clone();
        assert!(batch.start_file(&file_id).is_ok());

        // Insert into live TransferManager
        mgr.batches.write().insert(batch_id.clone(), batch.clone());

        // Check status on live batch
        {
            let guard = mgr.batches.read();
            let retrieved = guard.get(&batch_id).expect("batch exists");
            assert_eq!(retrieved.total_bytes, 2048);
            assert_eq!(retrieved.phase, TransferPhase::Transferring);
        }

        // Cancel live batch
        {
            let mut guard = mgr.batches.write();
            let retrieved_mut = guard.get_mut(&batch_id).expect("batch exists");
            retrieved_mut.cancel();
            assert_eq!(retrieved_mut.phase, TransferPhase::Cancelled);
        }

        // Missing batch returns None
        {
            let guard = mgr.batches.read();
            assert!(guard.get("missing_batch_id").is_none());
        }
    }
}
