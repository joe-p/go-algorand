;; Iterative baseline for app_rec_depth.wat: does the same N iterations and the
;; same O(1) work per iteration, but with a loop instead of nested calls. The
;; difference between the two isolates per-call-frame overhead from the cost of
;; the work being performed.
(module
  (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
  (import "env" "app_global_put_uint" (func $put (param i32 i32 i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "depth")
  (func (export "program") (result i32)
    (local $n i32)
    (local $d i32)
    ;; read ApplicationArgs[0] into offset 16 as 4 little-endian bytes
    (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 16) (i32.const 4)))
    (local.set $n (i32.load (i32.const 16)))
    (block $done
      (loop $l
        (br_if $done (i32.eqz (local.get $n)))
        (local.set $d (i32.add (local.get $d) (i32.const 1)))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (br $l)))
    (call $put (i32.const 0) (i32.const 5) (i64.extend_i32_u (local.get $d)))
    (i32.const 1)))
