use std::{path::Path, sync::Arc};

use anyhow::{Context as _, Result};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};

use crate::{NoteRecord, QueryHit};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait NotesStore: Send + Sync {
    fn save_note(&self, note: NoteRecord, embedding: Vec<f32>) -> Result<()>;
    fn search_by_vector(&self, query_vector: &[f32], top_k: usize) -> Result<Vec<QueryHit>>;
}

// ---------------------------------------------------------------------------
// Heed-backed implementation
// ---------------------------------------------------------------------------

type BytesDb = heed::Database<heed::types::Bytes, heed::types::Bytes>;

pub struct HeedNotesStore {
    env: heed::Env,
    notes_db: BytesDb,
    embeddings_db: BytesDb,
    write_lock: Mutex<()>,
}

impl HeedNotesStore {
    pub fn open(db_path: &Path) -> Result<Arc<Self>> {
        std::fs::create_dir_all(db_path)
            .with_context(|| format!("creating notes directory {}", db_path.display()))?;

        let env = unsafe {
            heed::EnvOpenOptions::new()
                .map_size(512 * 1024 * 1024) // 512 MiB
                .max_dbs(2)
                .open(db_path)
        }?;

        let mut wtxn = env.write_txn()?;
        let notes_db: BytesDb = env.create_database(&mut wtxn, Some("notes"))?;
        let embeddings_db: BytesDb = env.create_database(&mut wtxn, Some("note_embeddings"))?;
        wtxn.commit()?;

        Ok(Arc::new(Self {
            env,
            notes_db,
            embeddings_db,
            write_lock: Mutex::new(()),
        }))
    }
}

impl NotesStore for HeedNotesStore {
    fn save_note(&self, note: NoteRecord, embedding: Vec<f32>) -> Result<()> {
        let _guard = self.write_lock.lock();
        let note_id = note_id_for(&note.title);
        let mut wtxn = self.env.write_txn()?;
        self.notes_db
            .put(&mut wtxn, &note_id, &serde_json::to_vec(&note)?)?;
        let embedding_bytes = floats_to_bytes(&embedding);
        self.embeddings_db
            .put(&mut wtxn, &note_id, &embedding_bytes)?;
        wtxn.commit()?;
        Ok(())
    }

    fn search_by_vector(&self, query_vector: &[f32], top_k: usize) -> Result<Vec<QueryHit>> {
        let rtxn = self.env.read_txn()?;

        let mut scored: Vec<([u8; 16], f32)> = self
            .embeddings_db
            .iter(&rtxn)?
            .filter_map(|entry| {
                let (key_bytes, val_bytes) = entry.ok()?;
                let id: [u8; 16] = key_bytes.try_into().ok()?;
                let embedding = bytes_to_floats(val_bytes);
                let score = cosine_similarity(query_vector, &embedding);
                Some((id, score))
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        let mut hits = Vec::with_capacity(scored.len());
        for (id, _) in scored {
            if let Some(bytes) = self.notes_db.get(&rtxn, &id)? {
                let note: NoteRecord = serde_json::from_slice(bytes)?;
                hits.push(QueryHit {
                    source: Arc::from(format!("note:{}", note.title).as_str()),
                    excerpt: note.body,
                });
            }
        }
        Ok(hits)
    }
}

// ---------------------------------------------------------------------------
// Helpers shared with index_store (duplicated to keep modules independent)
// ---------------------------------------------------------------------------

fn note_id_for(title: &str) -> [u8; 16] {
    let digest = Sha256::digest(title.as_bytes());
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    id
}

fn floats_to_bytes(floats: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(floats.len() * 4);
    for &f in floats {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

fn bytes_to_floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}
