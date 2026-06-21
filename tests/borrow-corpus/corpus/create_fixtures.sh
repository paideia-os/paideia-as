#!/bin/bash

# S0906: Mutable and immutable borrows conflict
cat > a_s0906_mut_only.pdx << 'EOF'
; Accept: mutable borrow only
fn test_mut_only() -> u64 {
  let x: u64 = 42;
  let mref: &mut x = &mut x;
  *mref + 1
}
EOF

cat > a_s0906_mut_only.expect << 'EOF'
# Accept: mutable borrow only
EOF

cat > a_s0906_separate_scopes.pdx << 'EOF'
; Accept: immutable then mutable in separate scopes
fn test_separate_scopes() -> u64 {
  let x: u64 = 42;
  {
    let iref: &x = &x;
    let r1 = *iref;
  }
  {
    let mref: &mut x = &mut x;
    *mref + 1
  }
  0
}
EOF

cat > a_s0906_separate_scopes.expect << 'EOF'
# Accept: immutable then mutable in separate scopes
EOF

cat > a_s0906_two_immut.pdx << 'EOF'
; Accept: two immutable borrows (allowed)
fn test_two_immut() -> u64 {
  let x: u64 = 42;
  let iref1: &x = &x;
  let iref2: &x = &x;
  *iref1 + *iref2
}
EOF

cat > a_s0906_two_immut.expect << 'EOF'
# Accept: two immutable borrows
EOF

cat > a_s0906_mut_after_drop.pdx << 'EOF'
; Accept: mutable borrow after immutable borrow is dropped
fn test_mut_after_drop() -> u64 {
  let x: u64 = 42;
  let iref: &x = &x;
  let r1 = *iref;
  let mref: &mut x = &mut x;
  *mref + 1
}
EOF

cat > a_s0906_mut_after_drop.expect << 'EOF'
# Accept: mutable borrow after immutable borrow is dropped
EOF

# S0906 reject fixtures
cat > r_s0906_immut_then_mut.pdx << 'EOF'
; Reject: immutable borrow then mutable borrow (conflict)
fn test_immut_then_mut() -> u64 {
  let x: u64 = 42;
  let iref: &x = &x;
  let mref: &mut x = &mut x;
  *iref + *mref
}
EOF

cat > r_s0906_immut_then_mut.expect << 'EOF'
# Reject: immutable borrow then mutable borrow conflict
S0906
EOF

cat > r_s0906_mut_then_immut.pdx << 'EOF'
; Reject: mutable borrow then immutable borrow (conflict)
fn test_mut_then_immut() -> u64 {
  let x: u64 = 42;
  let mref: &mut x = &mut x;
  let iref: &x = &x;
  *mref + *iref
}
EOF

cat > r_s0906_mut_then_immut.expect << 'EOF'
# Reject: mutable borrow then immutable borrow conflict
S0906
EOF

cat > r_s0906_overlapping_field_borrows.pdx << 'EOF'
; Reject: overlapping field borrows (one mut, one immut)
struct Pair { a: u64, b: u64 }
fn test_field_conflict() -> u64 {
  let p: Pair = Pair { a: 1, b: 2 };
  let iref: &p.a = &p.a;
  let mref: &mut p.b = &mut p.b;
  *iref + *mref
}
EOF

cat > r_s0906_overlapping_field_borrows.expect << 'EOF'
# Reject: overlapping field borrows
S0906
EOF

cat > r_s0906_three_way_conflict.pdx << 'EOF'
; Reject: three-way conflict (immut, then immut, then mut)
fn test_three_way() -> u64 {
  let x: u64 = 42;
  let i1: &x = &x;
  let i2: &x = &x;
  let m: &mut x = &mut x;
  *i1 + *i2 + *m
}
EOF

cat > r_s0906_three_way_conflict.expect << 'EOF'
# Reject: three-way conflict
S0906
EOF

# S0907: Two mutable borrows conflict
cat > a_s0907_mut_single.pdx << 'EOF'
; Accept: single mutable borrow
fn test_mut_single() -> u64 {
  let x: u64 = 42;
  let mref: &mut x = &mut x;
  *mref + 1
}
EOF

cat > a_s0907_mut_single.expect << 'EOF'
# Accept: single mutable borrow
EOF

cat > a_s0907_mut_separate.pdx << 'EOF'
; Accept: mutable borrows in separate scopes
fn test_mut_separate() -> u64 {
  let x: u64 = 42;
  {
    let m1: &mut x = &mut x;
    *m1
  }
  {
    let m2: &mut x = &mut x;
    *m2
  }
  0
}
EOF

cat > a_s0907_mut_separate.expect << 'EOF'
# Accept: mutable borrows in separate scopes
EOF

cat > a_s0907_mut_different_fields.pdx << 'EOF'
; Accept: mutable borrows of different fields
struct Point { x: u64, y: u64 }
fn test_different_fields() -> u64 {
  let p: Point = Point { x: 1, y: 2 };
  let mx: &mut p.x = &mut p.x;
  let my: &mut p.y = &mut p.y;
  *mx + *my
}
EOF

cat > a_s0907_mut_different_fields.expect << 'EOF'
# Accept: mutable borrows of different fields
EOF

cat > a_s0907_immut_only.pdx << 'EOF'
; Accept: immutable borrows (always allowed)
fn test_immut_multiple() -> u64 {
  let x: u64 = 42;
  let i1: &x = &x;
  let i2: &x = &x;
  let i3: &x = &x;
  *i1 + *i2 + *i3
}
EOF

cat > a_s0907_immut_only.expect << 'EOF'
# Accept: immutable borrows
EOF

cat > a_s0907_mut_after_use.pdx << 'EOF'
; Accept: second mutable borrow after first is used
fn test_mut_after_use() -> u64 {
  let x: u64 = 42;
  let m1: &mut x = &mut x;
  let r1 = *m1;
  let m2: &mut x = &mut x;
  r1 + *m2
}
EOF

cat > a_s0907_mut_after_use.expect << 'EOF'
# Accept: second mutable borrow after first is used
EOF

# S0907 reject fixtures
cat > r_s0907_two_mut_overlap.pdx << 'EOF'
; Reject: two mutable borrows of same field
struct Cell { val: u64 }
fn test_two_mut() -> u64 {
  let c: Cell = Cell { val: 42 };
  let m1: &mut c.val = &mut c.val;
  let m2: &mut c.val = &mut c.val;
  *m1 + *m2
}
EOF

cat > r_s0907_two_mut_overlap.expect << 'EOF'
# Reject: two mutable borrows overlap
S0907
EOF

cat > r_s0907_two_mut_simple.pdx << 'EOF'
; Reject: two mutable borrows of same binding
fn test_two_mut_simple() -> u64 {
  let x: u64 = 42;
  let m1: &mut x = &mut x;
  let m2: &mut x = &mut x;
  *m1 + *m2
}
EOF

cat > r_s0907_two_mut_simple.expect << 'EOF'
# Reject: two mutable borrows simple
S0907
EOF

cat > r_s0907_three_mut_chain.pdx << 'EOF'
; Reject: three mutable borrows
fn test_three_mut() -> u64 {
  let x: u64 = 42;
  let m1: &mut x = &mut x;
  let m2: &mut x = &mut x;
  let m3: &mut x = &mut x;
  *m1 + *m2 + *m3
}
EOF

cat > r_s0907_three_mut_chain.expect << 'EOF'
# Reject: three mutable borrows
S0907
EOF

cat > r_s0907_nested_mut_scopes.pdx << 'EOF'
; Reject: nested scopes with mutable overlaps
fn test_nested_mut() -> u64 {
  let x: u64 = 42;
  let m1: &mut x = &mut x;
  {
    let m2: &mut x = &mut x;
    *m2
  }
  *m1
}
EOF

cat > r_s0907_nested_mut_scopes.expect << 'EOF'
# Reject: nested mutable scopes
S0907
EOF

# S0908: Borrow lifetime violation
cat > a_s0908_local_borrow.pdx << 'EOF'
; Accept: local borrow used within scope
fn test_local_borrow() -> u64 {
  let x: u64 = 42;
  let r: &x = &x;
  *r
}
EOF

cat > a_s0908_local_borrow.expect << 'EOF'
# Accept: local borrow
EOF

cat > a_s0908_static_lifetime.pdx << 'EOF'
; Accept: static lifetime source
fn test_static() -> &u64 {
  let static_data: u64 = 42;
  &static_data
}
EOF

cat > a_s0908_static_lifetime.expect << 'EOF'
# Accept: static lifetime
EOF

cat > a_s0908_param_return.pdx << 'EOF'
; Accept: borrow parameter returned
fn borrow_return(x: &u64) -> &u64 {
  x
}
fn test_param_return() -> u64 {
  let local: u64 = 42;
  *borrow_return(&local)
}
EOF

cat > a_s0908_param_return.expect << 'EOF'
# Accept: parameter return
EOF

cat > a_s0908_struct_field.pdx << 'EOF'
; Accept: borrow of struct field with valid lifetime
struct Wrapper { val: u64 }
fn test_field_borrow() -> u64 {
  let w: Wrapper = Wrapper { val: 42 };
  let r: &w.val = &w.val;
  *r
}
EOF

cat > a_s0908_struct_field.expect << 'EOF'
# Accept: struct field borrow
EOF

cat > a_s0908_scope_chain.pdx << 'EOF'
; Accept: valid scope chain
fn test_scope_chain() -> u64 {
  let x: u64 = 42;
  {
    let r: &x = &x;
    *r
  }
  0
}
EOF

cat > a_s0908_scope_chain.expect << 'EOF'
# Accept: scope chain
EOF

# S0908 reject fixtures
cat > r_s0908_return_local_ref.pdx << 'EOF'
; Reject: return reference to local
fn test_return_local() -> &u64 {
  let x: u64 = 42;
  &x
}
EOF

cat > r_s0908_return_local_ref.expect << 'EOF'
# Reject: return local reference
S0908
EOF

cat > r_s0908_inner_scope_return.pdx << 'EOF'
; Reject: inner scope borrow returned
fn test_inner_return() -> &u64 {
  let x: u64 = 42;
  let r: &x = &x;
  r
}
EOF

cat > r_s0908_inner_scope_return.expect << 'EOF'
# Reject: inner scope return
S0908
EOF

cat > r_s0908_drop_then_use.pdx << 'EOF'
; Reject: source drops before borrow used
fn test_drop_then_use() -> u64 {
  let x: u64 = 42;
  let r: &x = &x;
  drop(x);
  *r
}
EOF

cat > r_s0908_drop_then_use.expect << 'EOF'
# Reject: drop then use
S0908
EOF

cat > r_s0908_nested_invalid.pdx << 'EOF'
; Reject: nested scope with invalid return
fn test_nested_invalid() -> &u64 {
  let outer: u64 = 10;
  {
    let inner: u64 = 20;
    let r: &inner = &inner;
    r
  }
  &outer
}
EOF

cat > r_s0908_nested_invalid.expect << 'EOF'
# Reject: nested invalid
S0908
EOF

# S0909: Mutation while borrow exists
cat > a_s0909_no_mutation.pdx << 'EOF'
; Accept: borrow without mutation
fn test_no_mutation() -> u64 {
  let x: u64 = 42;
  let r: &x = &x;
  *r + 1
}
EOF

cat > a_s0909_no_mutation.expect << 'EOF'
# Accept: no mutation
EOF

cat > a_s0909_mutation_after.pdx << 'EOF'
; Accept: mutation after borrow ends
fn test_mutation_after() -> u64 {
  let x: u64 = 42;
  let r: &x = &x;
  let result = *r;
  x = 100;
  result
}
EOF

cat > a_s0909_mutation_after.expect << 'EOF'
# Accept: mutation after
EOF

cat > a_s0909_separate_bindings.pdx << 'EOF'
; Accept: mutation of different binding
fn test_separate_bindings() -> u64 {
  let x: u64 = 42;
  let y: u64 = 10;
  let r: &x = &x;
  y = 20;
  *r + y
}
EOF

cat > a_s0909_separate_bindings.expect << 'EOF'
# Accept: separate bindings
EOF

cat > a_s0909_in_scope_no_mutation.pdx << 'EOF'
; Accept: loop without mutation to borrowed var
fn test_loop_no_mutation() -> u64 {
  let x: u64 = 42;
  let r: &x = &x;
  let mut acc: u64 = 0;
  loop {
    acc = acc + 1;
    if acc > 5 {
      break;
    }
  }
  *r + acc
}
EOF

cat > a_s0909_in_scope_no_mutation.expect << 'EOF'
# Accept: loop no mutation
EOF

cat > a_s0909_different_var_mutation.pdx << 'EOF'
; Accept: assignment to different variable in loop
fn test_loop_diff_var() -> u64 {
  let x: u64 = 42;
  let r: &x = &x;
  let mut temp: u64 = 0;
  loop {
    temp = temp + 1;
    if temp >= 3 {
      break;
    }
  }
  *r + temp
}
EOF

cat > a_s0909_different_var_mutation.expect << 'EOF'
# Accept: loop different var
EOF

# S0909 reject fixtures
cat > r_s0909_mutation_in_loop.pdx << 'EOF'
; Reject: mutation of borrowed var in loop
fn test_mutation_in_loop() -> u64 {
  let x: u64 = 42;
  let r: &x = &x;
  loop {
    x = x + 1;
    if x > 50 {
      break;
    }
  }
  *r
}
EOF

cat > r_s0909_mutation_in_loop.expect << 'EOF'
# Reject: mutation in loop
S0909
EOF

cat > r_s0909_reassign_in_scope.pdx << 'EOF'
; Reject: reassignment while borrow active
fn test_reassign() -> u64 {
  let x: u64 = 42;
  let r: &x = &x;
  {
    x = 100;
  }
  *r
}
EOF

cat > r_s0909_reassign_in_scope.expect << 'EOF'
# Reject: reassign in scope
S0909
EOF

cat > r_s0909_shadowing_mutation.pdx << 'EOF'
; Reject: shadowing with mutation during borrow
fn test_shadowing() -> u64 {
  let x: u64 = 42;
  let r: &x = &x;
  {
    let x: u64 = 50;
    x = 60;
  }
  *r
}
EOF

cat > r_s0909_shadowing_mutation.expect << 'EOF'
# Reject: shadowing mutation
S0909
EOF

cat > r_s0909_nested_mutation.pdx << 'EOF'
; Reject: nested mutation in inner scope
fn test_nested_mutation() -> u64 {
  let x: u64 = 42;
  let r: &x = &x;
  {
    {
      x = 50;
    }
  }
  *r
}
EOF

cat > r_s0909_nested_mutation.expect << 'EOF'
# Reject: nested mutation
S0909
EOF

echo "All fixtures created successfully!"
