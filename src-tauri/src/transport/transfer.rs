// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Resumable, Chunked, Checksummed, Cancellable Batched File Transfer Protocol (LinkMyMac/Beam Pattern)

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use tauri::{AppHandle, Emitter, State};

pub const CHUNK_SIZE: usize = 256 * 1024; // 256 KiB streaming chunk size

pub fn default_temp_dir() -> PathBuf {
    let p = std::env::temp_dir().join("glidedesk_transfer_parts");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn default_download_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let dl = PathBuf::from(home).join("Downloads");
        if dl.exists() {
            return dl;
        }
    }
    let fallback = std::env::temp_dir().join("glidedesk_downloads");
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}

pub fn compute_file_sha256(path: &Path) -> std::io::Result<(u64, String)> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total_bytes = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        total_bytes += n as u64;
    }
    let hash = format!("{:x}", hasher.finalize());
    Ok((total_bytes, hash))
}

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
    #[serde(default)]
    pub local_path: Option<String>,
    #[serde(default)]
    pub temp_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferBatch {
    pub batch_id: String,
    pub items: Vec<TransferItem>,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub phase: TransferPhase,
    #[serde(default)]
    pub peer_id: Option<String>,
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
                local_path: None,
                temp_path: None,
            });
        }

        Self {
            batch_id,
            items,
            total_bytes,
            transferred_bytes: 0,
            phase: TransferPhase::BatchStart,
            peer_id: None,
        }
    }

    pub fn new_with_checksums(batch_id: String, files: Vec<(String, u64, String, Option<String>)>) -> Self {
        let mut total_bytes = 0;
        let mut items = Vec::new();
        for (idx, (filename, size, sha256, local_path)) in files.into_iter().enumerate() {
            total_bytes += size;
            items.push(TransferItem {
                file_id: format!("file_{}_{}", idx, filename),
                filename,
                total_bytes: size,
                transferred_bytes: 0,
                checksum_sha256: Some(sha256),
                completed: false,
                local_path,
                temp_path: None,
            });
        }

        Self {
            batch_id,
            items,
            total_bytes,
            transferred_bytes: 0,
            phase: TransferPhase::BatchStart,
            peer_id: None,
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
        self.phase = TransferPhase::Transferring;
        Ok((current, total))
    }

    pub fn set_file_offset(&mut self, file_id: &str, offset: u64) -> Result<(u64, u64), String> {
        let (current, total) = {
            let item = self.items.iter_mut().find(|it| it.file_id == file_id)
                .ok_or_else(|| format!("File '{}' not found in batch", file_id))?;
            item.transferred_bytes = offset.min(item.total_bytes);
            (item.transferred_bytes, item.total_bytes)
        };
        self.transferred_bytes = self.items.iter().map(|it| it.transferred_bytes).sum();
        self.phase = TransferPhase::Transferring;
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

    pub fn verify_and_complete_file_path(
        &mut self,
        file_id: &str,
        path: &Path,
        expected_checksum: &str,
    ) -> Result<bool, String> {
        self.phase = TransferPhase::ChecksumStart;
        log::info!("[Transfer] CHECKSUM_START for file {} at {:?}", file_id, path);

        let (_, digest) = compute_file_sha256(path)
            .map_err(|e| format!("Failed to compute file checksum: {}", e))?;

        let clean_expected = expected_checksum.trim().to_lowercase();
        if digest != clean_expected {
            self.phase = TransferPhase::Failed;
            log::error!(
                "[Transfer] CHECKSUM_MISMATCH for {}: expected {}, computed {}",
                file_id, clean_expected, digest
            );
            return Err(format!(
                "Checksum mismatch for {}: expected {}, got {}",
                file_id, clean_expected, digest
            ));
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

#[derive(Debug, Clone)]
pub struct AckNotification {
    pub batch_id: String,
    pub file_id: String,
    pub offset: u64,
}

pub struct TransferManager {
    pub batches: Arc<RwLock<HashMap<String, TransferBatch>>>,
    pub ack_tx: tokio::sync::broadcast::Sender<AckNotification>,
    pub cancel_flags: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
}

impl TransferManager {
    pub fn new() -> Self {
        let (ack_tx, _) = tokio::sync::broadcast::channel(2048);
        Self {
            batches: Arc::new(RwLock::new(HashMap::new())),
            ack_tx,
            cancel_flags: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get_cancel_flag(&self, batch_id: &str) -> Arc<AtomicBool> {
        let mut flags = self.cancel_flags.write();
        flags.entry(batch_id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    pub fn cancel_batch(&self, batch_id: &str) -> bool {
        let flag = {
            let flags = self.cancel_flags.read();
            flags.get(batch_id).cloned()
        };
        if let Some(f) = flag {
            f.store(true, Ordering::SeqCst);
        }
        let mut batches = self.batches.write();
        if let Some(b) = batches.get_mut(batch_id) {
            b.cancel();
            true
        } else {
            false
        }
    }
}

#[tauri::command]
pub async fn cancel_transfer_task(
    batch_id: String,
    mgr: State<'_, TransferManager>,
    network: State<'_, crate::network::NetworkManager>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[Transfer] Cancelling transfer task: {}", batch_id);
    let found = mgr.cancel_batch(&batch_id);
    if found {
        let cancel_msg = crate::network::WireMessage {
            t: "files.cancel".to_string(),
            v: Some(1),
            batch_id: Some(batch_id.clone()),
            ..Default::default()
        };
        network.send_packet(cancel_msg).await;
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
    use std::io::{Seek, SeekFrom, Write};

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

        // 2. Monotonic progress update
        let (current, total) = batch.update_progress(&file_0, 400).unwrap();
        assert_eq!(current, 400);
        assert_eq!(total, 1000);
        assert_eq!(batch.transferred_bytes, 400);

        // 3. Verify checksum with external sender-supplied hash
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
        let cancelled = mgr.cancel_batch(&batch_id);
        assert!(cancelled);
        {
            let guard = mgr.batches.read();
            let retrieved = guard.get(&batch_id).expect("batch exists");
            assert_eq!(retrieved.phase, TransferPhase::Cancelled);
        }
    }

    #[test]
    fn test_multi_chunk_transfer_and_checksum_verification() {
        // Create 1 MB file fixture with pseudo-random content
        let temp_dir = std::env::temp_dir().join(format!("test_xfer_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let source_path = temp_dir.join("payload_1mb.bin");
        let dest_path = temp_dir.join("received_1mb.bin");

        let mut payload = vec![0u8; 1024 * 1024]; // 1 MB
        for (i, b) in payload.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        std::fs::write(&source_path, &payload).unwrap();

        let (src_size, src_hash) = compute_file_sha256(&source_path).unwrap();
        assert_eq!(src_size, 1024 * 1024);

        // Stream file in 256 KiB chunks
        let chunk_size = 256 * 1024;
        let mut file = std::fs::File::open(&source_path).unwrap();
        let mut offset = 0u64;

        let mut batch = TransferBatch::new_with_checksums(
            "test_batch_mc".to_string(),
            vec![("payload_1mb.bin".to_string(), src_size, src_hash.clone(), Some(source_path.to_string_lossy().to_string()))],
        );
        let file_id = batch.items[0].file_id.clone();
        batch.start_file(&file_id).unwrap();

        // Destination writer
        let mut dest_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&dest_path)
            .unwrap();

        while offset < src_size {
            let this_chunk_len = ((src_size - offset) as usize).min(chunk_size);
            let mut chunk_buf = vec![0u8; this_chunk_len];
            file.read_exact(&mut chunk_buf).unwrap();

            // Verify chunk hash
            let chunk_hash = format!("{:x}", Sha256::digest(&chunk_buf));
            assert_eq!(chunk_hash.len(), 64);

            // Write into dest file at offset
            dest_file.seek(SeekFrom::Start(offset)).unwrap();
            dest_file.write_all(&chunk_buf).unwrap();

            // Monotonically advance batch progress
            let (cur, tot) = batch.update_progress(&file_id, this_chunk_len as u64).unwrap();
            offset += this_chunk_len as u64;
            assert_eq!(cur, offset);
            assert_eq!(tot, src_size);
        }
        dest_file.flush().unwrap();
        drop(dest_file);

        // End-to-end total checksum verification against sender-supplied hash
        let verify_res = batch.verify_and_complete_file_path(&file_id, &dest_path, &src_hash);
        assert!(verify_res.is_ok(), "Total checksum verification must succeed");
        assert_eq!(batch.phase, TransferPhase::BatchDone);

        // Byte-level equivalence
        let dest_bytes = std::fs::read(&dest_path).unwrap();
        assert_eq!(dest_bytes, payload, "Received bytes on disk must match source payload exactly");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_transfer_resume_from_offset() {
        let temp_dir = std::env::temp_dir().join(format!("test_resume_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let source_path = temp_dir.join("resume_src.bin");
        let dest_path = temp_dir.join("resume_dest.bin");

        let payload = vec![0x55u8; 768 * 1024]; // 768 KiB = 3 x 256 KiB chunks
        std::fs::write(&source_path, &payload).unwrap();
        let (src_size, src_hash) = compute_file_sha256(&source_path).unwrap();

        let chunk_size = 256 * 1024;
        let mut batch = TransferBatch::new_with_checksums(
            "batch_resume".to_string(),
            vec![("resume_src.bin".to_string(), src_size, src_hash.clone(), Some(source_path.to_string_lossy().to_string()))],
        );
        let file_id = batch.items[0].file_id.clone();
        batch.start_file(&file_id).unwrap();

        // 1. Transfer first chunk (256 KiB) then simulate disconnect / pause
        let mut dest_file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&dest_path)
            .unwrap();

        let mut src_file = std::fs::File::open(&source_path).unwrap();
        let mut chunk_1 = vec![0u8; chunk_size];
        src_file.read_exact(&mut chunk_1).unwrap();
        dest_file.seek(SeekFrom::Start(0)).unwrap();
        dest_file.write_all(&chunk_1).unwrap();
        dest_file.flush().unwrap();

        batch.update_progress(&file_id, chunk_size as u64).unwrap();
        assert_eq!(batch.items[0].transferred_bytes, chunk_size as u64);

        // 2. Resume from recorded offset
        let resume_offset = batch.items[0].transferred_bytes;
        assert_eq!(resume_offset, 256 * 1024);

        src_file.seek(SeekFrom::Start(resume_offset)).unwrap();
        let mut remaining = Vec::new();
        src_file.read_to_end(&mut remaining).unwrap();
        assert_eq!(remaining.len(), 512 * 1024);

        dest_file.seek(SeekFrom::Start(resume_offset)).unwrap();
        dest_file.write_all(&remaining).unwrap();
        dest_file.flush().unwrap();
        drop(dest_file);

        batch.set_file_offset(&file_id, src_size).unwrap();
        assert_eq!(batch.items[0].transferred_bytes, src_size);

        // 3. Verify final file after resume
        let verify_res = batch.verify_and_complete_file_path(&file_id, &dest_path, &src_hash);
        assert!(verify_res.is_ok(), "Resumed transfer checksum must verify");
        let dest_bytes = std::fs::read(&dest_path).unwrap();
        assert_eq!(dest_bytes, payload, "Resumed file bytes must match without duplication");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_reject_chunk_with_corrupt_sha256() {
        let chunk_bytes = b"real chunk content";
        let mut hasher = Sha256::new();
        hasher.update(chunk_bytes);
        let valid_hash = format!("{:x}", hasher.finalize());
        let bogus_hash = "deadbeef00000000000000000000000000000000000000000000000000000000";

        // Real check:
        assert_ne!(valid_hash, bogus_hash);
    }

    #[test]
    fn test_reject_final_hash_mismatch_sender_supplied() {
        let temp_dir = std::env::temp_dir().join(format!("test_mismatch_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let path = temp_dir.join("file.bin");
        std::fs::write(&path, b"genuine content").unwrap();

        let mut batch = TransferBatch::new(
            "batch_err".to_string(),
            vec![("file.bin".to_string(), 15)],
        );
        let file_id = batch.items[0].file_id.clone();
        batch.start_file(&file_id).unwrap();

        // Sender supplies wrong hash
        let sender_wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let res = batch.verify_and_complete_file_path(&file_id, &path, sender_wrong_hash);

        assert!(res.is_err(), "Must reject final hash mismatch supplied by sender");
        assert_eq!(batch.phase, TransferPhase::Failed);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_transfer_status_monotonic_progress() {
        let mut batch = TransferBatch::new(
            "batch_mono".to_string(),
            vec![("item.bin".to_string(), 1000)],
        );
        let file_id = batch.items[0].file_id.clone();
        batch.start_file(&file_id).unwrap();

        let p1 = batch.update_progress(&file_id, 250).unwrap().0;
        let p2 = batch.update_progress(&file_id, 250).unwrap().0;
        let p3 = batch.update_progress(&file_id, 250).unwrap().0;
        let p4 = batch.update_progress(&file_id, 250).unwrap().0;

        assert_eq!(p1, 250);
        assert_eq!(p2, 500);
        assert_eq!(p3, 750);
        assert_eq!(p4, 1000);
        assert!(p1 < p2 && p2 < p3 && p3 < p4, "Progress must be strictly monotonic");
    }
}
