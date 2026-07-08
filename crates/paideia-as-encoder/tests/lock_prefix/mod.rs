//! LOCK-prefixed atomic instructions: lock add/sub, lock and/or/xor,
//! lock bt-family (bts/btr/btc), lock inc + absolute-disp32 forms, and
//! lock xadd. Grouped as every test that exercises the `F0` LOCK prefix.

mod lock_add_sub;
mod lock_and_or_xor;
mod lock_bt_family;
mod lock_inc_add_abs;
mod lock_xadd;
