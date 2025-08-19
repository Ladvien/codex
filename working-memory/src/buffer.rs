use memmap2::{Mmap, MmapMut, MmapOptions};
use parking_lot::RwLock;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct CircularBuffer {
    mmap: RwLock<MmapMut>,
    size: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
    _file: File,
}

impl CircularBuffer {
    pub fn new(size: usize) -> anyhow::Result<Self> {
        // Create a temporary file for memory mapping
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open("/tmp/working_memory.mmap")?;
        file.set_len(size as u64)?;

        // Create memory map
        let mmap = unsafe { MmapOptions::new().len(size).map_mut(&file)? };

        Ok(Self {
            mmap: RwLock::new(mmap),
            size,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            _file: file,
        })
    }

    pub fn write(&self, data: &[u8]) -> anyhow::Result<usize> {
        let data_len = data.len();
        if data_len > self.size {
            return Err(anyhow::anyhow!("Data too large for buffer"));
        }

        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        // Calculate available space
        let available = if head >= tail {
            self.size - head + tail
        } else {
            tail - head
        };

        if data_len > available {
            return Err(anyhow::anyhow!("Buffer full"));
        }

        // Write data
        let mut mmap = self.mmap.write();
        let new_head = if head + data_len <= self.size {
            // Can write in one go
            mmap[head..head + data_len].copy_from_slice(data);
            (head + data_len) % self.size
        } else {
            // Need to wrap around
            let first_part = self.size - head;
            mmap[head..].copy_from_slice(&data[..first_part]);
            mmap[..data_len - first_part].copy_from_slice(&data[first_part..]);
            data_len - first_part
        };

        self.head.store(new_head, Ordering::Release);
        Ok(data_len)
    }

    pub fn read(&self, len: usize) -> anyhow::Result<Vec<u8>> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        // Calculate available data
        let available = if head >= tail {
            head - tail
        } else {
            self.size - tail + head
        };

        let read_len = len.min(available);
        if read_len == 0 {
            return Ok(Vec::new());
        }

        let mmap = self.mmap.read();
        let mut data = vec![0u8; read_len];

        let new_tail = if tail + read_len <= self.size {
            // Can read in one go
            data.copy_from_slice(&mmap[tail..tail + read_len]);
            (tail + read_len) % self.size
        } else {
            // Need to wrap around
            let first_part = self.size - tail;
            data[..first_part].copy_from_slice(&mmap[tail..]);
            data[first_part..].copy_from_slice(&mmap[..read_len - first_part]);
            read_len - first_part
        };

        self.tail.store(new_tail, Ordering::Release);
        Ok(data)
    }

    pub fn available_read(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        if head >= tail {
            head - tail
        } else {
            self.size - tail + head
        }
    }

    pub fn available_write(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        if head >= tail {
            self.size - head + tail - 1
        } else {
            tail - head - 1
        }
    }

    pub fn clear(&self) {
        self.head.store(0, Ordering::Release);
        self.tail.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circular_buffer_write_read() {
        let buffer = CircularBuffer::new(1024).unwrap();

        let data = b"Hello, World!";
        let written = buffer.write(data).unwrap();
        assert_eq!(written, data.len());

        let read_data = buffer.read(data.len()).unwrap();
        assert_eq!(read_data, data);
    }

    #[test]
    fn test_circular_buffer_wrap_around() {
        let buffer = CircularBuffer::new(10).unwrap();

        // Write to fill most of buffer
        buffer.write(b"12345678").unwrap();
        
        // Read some data
        let _ = buffer.read(5).unwrap();
        
        // Write more data that wraps around
        buffer.write(b"ABCDE").unwrap();
        
        // Read all remaining data
        let data = buffer.read(8).unwrap();
        assert_eq!(data, b"678ABCDE");
    }
}