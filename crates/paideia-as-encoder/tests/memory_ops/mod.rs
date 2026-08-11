//! Memory-system instructions: cache ops (wbinvd/invd/clflush),
//! memory fences (mfence/sfence/lfence), prefetch family, PAUSE. Grouped
//! as "instructions that talk to the memory subsystem" rather than
//! moving data.

mod atomic_ordering;
mod cache_ops;
mod mem_fences;
mod pause;
mod prefetch_family;
mod xsave;
