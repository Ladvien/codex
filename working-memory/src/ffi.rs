use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_float, c_int, c_void};
use std::ptr;
use std::slice;
use uuid::Uuid;

use crate::{MemoryStats, WorkingMemory};

/// Create a new working memory instance
#[no_mangle]
pub extern "C" fn working_memory_new(max_chunks: usize) -> *mut WorkingMemory {
    match WorkingMemory::new(max_chunks) {
        Ok(memory) => Box::into_raw(Box::new(memory)),
        Err(_) => ptr::null_mut(),
    }
}

/// Free a working memory instance
#[no_mangle]
pub extern "C" fn working_memory_free(memory: *mut WorkingMemory) {
    if !memory.is_null() {
        unsafe {
            let _ = Box::from_raw(memory);
        }
    }
}

/// Insert content into working memory
#[no_mangle]
pub extern "C" fn working_memory_insert(
    memory: *mut WorkingMemory,
    content: *const c_char,
    id_out: *mut [u8; 16],
) -> c_int {
    if memory.is_null() || content.is_null() || id_out.is_null() {
        return -1;
    }

    unsafe {
        let memory = &*memory;
        let content_str = match CStr::from_ptr(content).to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        };

        match memory.insert(content_str.to_string()) {
            Ok(id) => {
                let id_bytes = id.as_bytes();
                (*id_out).copy_from_slice(id_bytes);
                0
            }
            Err(_) => -1,
        }
    }
}

/// Get content from working memory
#[no_mangle]
pub extern "C" fn working_memory_get(
    memory: *mut WorkingMemory,
    id: *const [u8; 16],
    content_out: *mut c_char,
    content_len: usize,
) -> c_int {
    if memory.is_null() || id.is_null() || content_out.is_null() {
        return -1;
    }

    unsafe {
        let memory = &*memory;
        let uuid = Uuid::from_bytes(*id);

        match memory.get(uuid) {
            Some(chunk) => {
                let content_cstring = match CString::new(chunk.content.clone()) {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                let content_bytes = content_cstring.as_bytes_with_nul();
                if content_bytes.len() > content_len {
                    return -1;
                }

                ptr::copy_nonoverlapping(
                    content_bytes.as_ptr() as *const c_char,
                    content_out,
                    content_bytes.len(),
                );
                0
            }
            None => -1,
        }
    }
}

/// Search for similar memories
#[no_mangle]
pub extern "C" fn working_memory_search(
    memory: *mut WorkingMemory,
    embedding: *const c_float,
    embedding_len: usize,
    limit: usize,
    results_out: *mut SearchResult,
    results_capacity: usize,
) -> c_int {
    if memory.is_null() || embedding.is_null() || results_out.is_null() {
        return -1;
    }

    unsafe {
        let memory = &*memory;
        let embedding_slice = slice::from_raw_parts(embedding, embedding_len);
        
        let results = memory.search_similar(embedding_slice, limit);
        let count = results.len().min(results_capacity);

        for (i, (id, score)) in results.iter().take(count).enumerate() {
            let result = &mut *results_out.add(i);
            result.id = id.as_bytes().clone();
            result.similarity = *score;
        }

        count as c_int
    }
}

/// Get memory statistics
#[no_mangle]
pub extern "C" fn working_memory_get_stats(
    memory: *mut WorkingMemory,
    stats_out: *mut StatsFFI,
) -> c_int {
    if memory.is_null() || stats_out.is_null() {
        return -1;
    }

    unsafe {
        let memory = &*memory;
        let stats = memory.get_stats();
        
        (*stats_out).total_chunks = stats.total_chunks;
        (*stats_out).total_bytes = stats.total_bytes;
        (*stats_out).cache_hits = stats.cache_hits;
        (*stats_out).cache_misses = stats.cache_misses;
        (*stats_out).cache_hit_rate = stats.cache_hit_rate;
        (*stats_out).pending_embeddings = stats.pending_embeddings;
        
        0
    }
}

#[repr(C)]
pub struct SearchResult {
    pub id: [u8; 16],
    pub similarity: c_float,
}

#[repr(C)]
pub struct StatsFFI {
    pub total_chunks: usize,
    pub total_bytes: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,
    pub pending_embeddings: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_create_and_free() {
        let memory = working_memory_new(1000);
        assert!(!memory.is_null());
        working_memory_free(memory);
    }

    #[test]
    fn test_ffi_insert_and_get() {
        let memory = working_memory_new(1000);
        assert!(!memory.is_null());

        let content = CString::new("Test content").unwrap();
        let mut id = [0u8; 16];
        
        let result = working_memory_insert(memory, content.as_ptr(), &mut id as *mut _);
        assert_eq!(result, 0);

        let mut buffer = vec![0u8; 1024];
        let result = working_memory_get(
            memory,
            &id as *const _,
            buffer.as_mut_ptr() as *mut c_char,
            buffer.len(),
        );
        assert_eq!(result, 0);

        working_memory_free(memory);
    }
}