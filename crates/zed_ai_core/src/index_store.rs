use std::{collections::HashSet, path::Path, sync::Arc};

use anyhow::{Context as _, Result};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};

use crate::{CodeChunk, IndexStats, QueryHit};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

type BytesDb = heed::Database<heed::types::Bytes, heed::types::Bytes>;

#[allow(dead_code)]
pub struct HeedIndexStore {
    env: heed::Env,
    chunks_db: BytesDb,
    embeddings_db: BytesDb,
    write_lock: Mutex<()>,
}

impl HeedIndexStore {
    pub fn open(db_path: &Path) -> Result<Arc<Self>> {
        std::fs::create_dir_all(db_path)
            .with_context(|| format!("creating index directory {}", db_path.display()))?;

        let env = unsafe {
            heed::EnvOpenOptions::new()
                .map_size(512 * 1024 * 1024) // 512 MiB
                .max_dbs(2)
                .open(db_path)
        }?;

        let mut wtxn = env.write_txn()?;
        let chunks_db: BytesDb = env.create_database(&mut wtxn, Some("chunks"))?;
        let embeddings_db: BytesDb = env.create_database(&mut wtxn, Some("embeddings"))?;
        wtxn.commit()?;

        Ok(Arc::new(Self {
            env,
            chunks_db,
            embeddings_db,
            write_lock: Mutex::new(()),
        }))
    }
}

pub trait IndexStore: Send + Sync {
    /// Upsert a batch of (chunk, embedding) pairs.
    fn upsert_chunks(&self, items: &[(CodeChunk, Vec<f32>)]) -> Result<()>;

    /// Find the top-k most similar chunks to `query_vector`.
    fn search_by_vector(&self, query_vector: &[f32], top_k: usize) -> Result<Vec<QueryHit>>;

    /// Remove all chunks associated with a file path.
    fn remove_file(&self, path: &Path) -> Result<()>;

    /// Get the set of content hashes for chunks associated with a file path.
    fn get_file_hashes(&self, path: &Path) -> Result<HashSet<Vec<u8>>>;

    /// Drop all stored data.
    fn clear(&self) -> Result<()>;

    /// Get statistics about the index.
    fn stats(&self) -> Result<IndexStats>;
}

impl IndexStore for HeedIndexStore {
    fn upsert_chunks(&self, _items: &[(CodeChunk, Vec<f32>)]) -> Result<()> {
        unimplemented!("upsert_chunks not implemented yet")
    }

    fn search_by_vector(&self, _query_vector: &[f32], _top_k: usize) -> Result<Vec<QueryHit>> {
        unimplemented!("search_by_vector not implemented yet")
    }

    fn remove_file(&self, _path: &Path) -> Result<()> {
        unimplemented!("remove_file not implemented yet")
    }

    fn get_file_hashes(&self, _path: &Path) -> Result<HashSet<Vec<u8>>> {
        unimplemented!("get_file_hashes not implemented yet")
    }

    fn clear(&self) -> Result<()> {
        unimplemented!("clear not implemented yet")
    }

    fn stats(&self) -> Result<IndexStats> {
        unimplemented!("stats not implemented yet")
    }
}

#[allow(dead_code)]
fn chunk_id_for(path: &Path, symbol_path: Option<&str>) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(b"::");
    hasher.update(symbol_path.unwrap_or("").as_bytes());
    let digest = hasher.finalize();
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    id
}

#[allow(dead_code)]
fn path_hash_for(path: &Path) -> [u8; 16] {
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    let mut hash = [0u8; 16];
    hash.copy_from_slice(&digest[..16]);
    hash
}

#[allow(dead_code)]
fn floats_to_bytes(floats: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(floats.len() * 4);
    for &f in floats {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

#[allow(dead_code)]
fn bytes_to_floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical_vectors() {
        let v = vec![1.0f32, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-5);
    }

    #[test]
    fn float_bytes_round_trip() {
        let floats = vec![1.5f32, -0.5, 3.14];
        let bytes = floats_to_bytes(&floats);
        let recovered = bytes_to_floats(&bytes);
        assert_eq!(floats, recovered);
    }
}
