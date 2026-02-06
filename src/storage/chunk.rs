use crate::storage::interner::StringInterner;
use crate::storage::model::{DataBlock, DataBlockDiff, Delta, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Serialize, Deserialize, Clone)]
pub struct Chunk {
    pub interner: StringInterner,
    pub deltas: Vec<Delta>,
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            interner: StringInterner::new(),
            deltas: Vec::new(),
        }
    }

    pub fn compress(&self) -> Result<Vec<u8>, std::io::Error> {
        let raw_data = bincode::serialize(self).map_err(std::io::Error::other)?;
        zstd::encode_all(&raw_data[..], 3)
    }

    #[allow(dead_code)]
    pub fn decompress(data: &[u8]) -> Result<Self, std::io::Error> {
        let decompressed = zstd::decode_all(data)?;
        bincode::deserialize(&decompressed).map_err(std::io::Error::other)
    }

    /// Collects all string hashes used in this chunk's deltas.
    /// Used to filter the interner before compression.
    pub fn collect_used_hashes(&self) -> HashSet<u64> {
        let mut hashes = HashSet::new();

        for delta in &self.deltas {
            match delta {
                Delta::Full(snapshot) => {
                    Self::collect_hashes_from_snapshot(snapshot, &mut hashes);
                }
                Delta::Diff { blocks, .. } => {
                    Self::collect_hashes_from_diff_blocks(blocks, &mut hashes);
                }
            }
        }

        hashes
    }

    fn collect_hashes_from_snapshot(snapshot: &Snapshot, hashes: &mut HashSet<u64>) {
        for block in &snapshot.blocks {
            match block {
                DataBlock::Processes(procs) => {
                    for p in procs {
                        hashes.insert(p.name_hash);
                        hashes.insert(p.cmdline_hash);
                        hashes.insert(p.cpu.wchan_hash);
                    }
                }
                DataBlock::SystemNet(nets) => {
                    for n in nets {
                        hashes.insert(n.name_hash);
                    }
                }
                DataBlock::SystemDisk(disks) => {
                    for d in disks {
                        hashes.insert(d.device_hash);
                    }
                }
                DataBlock::SystemInterrupts(intrs) => {
                    for i in intrs {
                        hashes.insert(i.irq_hash);
                    }
                }
                DataBlock::SystemSoftirqs(softirqs) => {
                    for s in softirqs {
                        hashes.insert(s.name_hash);
                    }
                }
                DataBlock::PgStatActivity(activities) => {
                    for a in activities {
                        hashes.insert(a.datname_hash);
                        hashes.insert(a.usename_hash);
                        hashes.insert(a.application_name_hash);
                        hashes.insert(a.state_hash);
                        hashes.insert(a.query_hash);
                        hashes.insert(a.wait_event_type_hash);
                        hashes.insert(a.wait_event_hash);
                        hashes.insert(a.backend_type_hash);
                    }
                }
                DataBlock::PgStatStatements(stmts) => {
                    for s in stmts {
                        hashes.insert(s.query_hash);
                    }
                }
                // These blocks don't contain string hashes
                DataBlock::SystemCpu(_)
                | DataBlock::SystemLoad(_)
                | DataBlock::SystemMem(_)
                | DataBlock::SystemPsi(_)
                | DataBlock::SystemVmstat(_)
                | DataBlock::SystemFile(_)
                | DataBlock::SystemStat(_)
                | DataBlock::SystemNetSnmp(_)
                | DataBlock::Cgroup(_) => {}
            }
        }
    }

    fn collect_hashes_from_diff_blocks(blocks: &[DataBlockDiff], hashes: &mut HashSet<u64>) {
        for block in blocks {
            match block {
                DataBlockDiff::Processes { updates, .. } => {
                    for p in updates {
                        hashes.insert(p.name_hash);
                        hashes.insert(p.cmdline_hash);
                        hashes.insert(p.cpu.wchan_hash);
                    }
                }
                DataBlockDiff::SystemNet { updates, .. } => {
                    for n in updates {
                        hashes.insert(n.name_hash);
                    }
                }
                DataBlockDiff::SystemDisk { updates, .. } => {
                    for d in updates {
                        hashes.insert(d.device_hash);
                    }
                }
                DataBlockDiff::SystemInterrupts { updates, .. } => {
                    for i in updates {
                        hashes.insert(i.irq_hash);
                    }
                }
                DataBlockDiff::SystemSoftirqs { updates, .. } => {
                    for s in updates {
                        hashes.insert(s.name_hash);
                    }
                }
                DataBlockDiff::PgStatActivity { updates, .. } => {
                    for a in updates {
                        hashes.insert(a.datname_hash);
                        hashes.insert(a.usename_hash);
                        hashes.insert(a.application_name_hash);
                        hashes.insert(a.state_hash);
                        hashes.insert(a.query_hash);
                        hashes.insert(a.wait_event_type_hash);
                        hashes.insert(a.wait_event_hash);
                        hashes.insert(a.backend_type_hash);
                    }
                }
                DataBlockDiff::PgStatStatements { updates, .. } => {
                    for s in updates {
                        hashes.insert(s.query_hash);
                    }
                }
                // These diffs don't contain string hashes
                DataBlockDiff::SystemCpu { .. }
                | DataBlockDiff::SystemLoad(_)
                | DataBlockDiff::SystemMem(_)
                | DataBlockDiff::SystemPsi(_)
                | DataBlockDiff::SystemVmstat(_)
                | DataBlockDiff::SystemFile(_)
                | DataBlockDiff::SystemStat(_)
                | DataBlockDiff::SystemNetSnmp(_)
                | DataBlockDiff::Cgroup(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::model::{DataBlock, ProcessInfo, Snapshot};

    #[test]
    fn test_chunk_compression_decompression() {
        let mut chunk = Chunk::new();
        let s = Snapshot {
            timestamp: 12345,
            blocks: vec![DataBlock::Processes(vec![ProcessInfo {
                pid: 1,
                name_hash: 111,
                cmdline_hash: 222,
                ..ProcessInfo::default()
            }])],
        };
        chunk.deltas.push(Delta::Full(s.clone()));

        let compressed = chunk.compress().unwrap();
        let decompressed = Chunk::decompress(&compressed).unwrap();

        assert_eq!(decompressed.deltas.len(), 1);
        if let Delta::Full(ds) = &decompressed.deltas[0] {
            assert_eq!(ds.timestamp, 12345);
            if let DataBlock::Processes(procs) = &ds.blocks[0] {
                assert_eq!(procs[0].pid, 1);
            } else {
                panic!("Expected DataBlock::Processes");
            }
        } else {
            panic!("Expected Delta::Full");
        }
    }

    #[test]
    fn test_chunk_serialization_compatibility() {
        let mut chunk = Chunk::new();
        let s = Snapshot {
            timestamp: 999,
            blocks: vec![],
        };
        chunk.deltas.push(Delta::Full(s));

        let raw = bincode::serialize(&chunk).unwrap();
        let compressed = zstd::encode_all(&raw[..], 3).unwrap();

        // Should be able to decompress it back to Chunk
        let decompressed = Chunk::decompress(&compressed).unwrap();
        assert_eq!(decompressed.deltas.len(), 1);
    }
}
