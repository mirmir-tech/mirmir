use sysinfo::System;

/// Host system RAM reported by the operating system.
///
/// Live telemetry and the history recorder both sample it through
/// [`HostMemory::sample`], so the two RPCs always describe the same pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostMemory {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl HostMemory {
    /// Stable presentation label for HTTP consumers that predate typed pools.
    pub const LABEL: &str = if cfg!(target_os = "macos") {
        "macOS unified system memory"
    } else {
        "host system memory"
    };

    #[must_use]
    pub fn sample() -> Option<Self> {
        let mut system = System::new();
        system.refresh_memory();
        Self::from_totals(system.total_memory(), system.available_memory())
    }

    #[must_use]
    pub fn from_totals(total_bytes: u64, available_bytes: u64) -> Option<Self> {
        (total_bytes > 0).then(|| Self {
            total_bytes,
            available_bytes: available_bytes.min(total_bytes),
        })
    }
}

/// Memory budget owned by the accelerator backend.
///
/// Only the live telemetry snapshot reports it: the history recorder is
/// host-only and never touches the accelerator runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceleratorMemory {
    pub pool: AcceleratorMemoryPool,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceleratorMemoryPool {
    /// Metal's recommended working-set budget within unified memory.
    MetalWorkingSet,
    /// Dedicated CUDA device memory.
    CudaDevice,
    /// Integrated CUDA memory bounded by the shared host pool.
    CudaUnified,
}

impl AcceleratorMemoryPool {
    /// The pool of the backend this binary is built with: Metal on macOS and
    /// CUDA on Linux, matching the `libmir` features selected in `Cargo.toml`.
    const fn compiled(unified: bool) -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::MetalWorkingSet)
        } else if !cfg!(target_os = "linux") {
            None
        } else if unified {
            Some(Self::CudaUnified)
        } else {
            Some(Self::CudaDevice)
        }
    }
}

impl AcceleratorMemory {
    #[must_use]
    pub fn from_backend(snapshot: &libmir::MemorySnapshot) -> Option<Self> {
        let total_bytes = snapshot.total_bytes.filter(|total| *total > 0)?;
        Some(Self {
            pool: AcceleratorMemoryPool::compiled(snapshot.unified)?,
            total_bytes,
            available_bytes: snapshot.available_bytes?.min(total_bytes),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn backend(total: Option<u64>, available: Option<u64>) -> libmir::MemorySnapshot {
        libmir::MemorySnapshot {
            total_bytes: total,
            available_bytes: available,
            active_bytes: 0,
            cached_bytes: 0,
            allocation_reserve_bytes: 0,
            source: String::new(),
            unified: true,
        }
    }

    #[test]
    fn host_memory_requires_a_reported_total() {
        assert_eq!(HostMemory::from_totals(0, 4), None);
        assert_eq!(
            HostMemory::from_totals(64 * GIB, 80 * GIB),
            Some(HostMemory {
                total_bytes: 64 * GIB,
                available_bytes: 64 * GIB
            })
        );
    }

    #[test]
    fn accelerator_memory_is_typed_by_the_compiled_backend() {
        let memory = AcceleratorMemory::from_backend(&backend(Some(48 * GIB), Some(40 * GIB)));
        let expected = if cfg!(target_os = "macos") {
            Some(AcceleratorMemoryPool::MetalWorkingSet)
        } else if cfg!(target_os = "linux") {
            Some(AcceleratorMemoryPool::CudaUnified)
        } else {
            None
        };
        assert_eq!(memory.map(|memory| memory.pool), expected);
        assert_eq!(AcceleratorMemory::from_backend(&backend(None, Some(1))), None);
        assert_eq!(AcceleratorMemory::from_backend(&backend(Some(0), Some(0))), None);
    }
}
