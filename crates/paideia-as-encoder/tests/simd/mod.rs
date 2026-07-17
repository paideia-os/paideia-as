//! AVX2 SIMD instructions: Vpxor, Vpcmpeqb, Vpmovmskb, Vmovdqu.
//! Phase R18 PA-R18-011 (issue #1004): VEX-prefix substrate for hash-table probe baseline.

mod vex_prefix;
mod vpxor;
mod vpcmpeqb;
mod vpmovmskb;
mod vmovdqu;
