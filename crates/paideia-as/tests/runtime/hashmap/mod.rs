// v0.18 #996: Runtime canaries for HashMapU64U64

mod new_returns_empty;
mod put_get_roundtrip;
mod get_missing_returns_none;
mod contains_present;
mod collision_probes_sum;
