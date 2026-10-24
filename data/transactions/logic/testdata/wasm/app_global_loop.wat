;; A WASM application program that increments the uint64 global "counter" N
;; times. N is ApplicationArgs[0] (field 26), a 4-byte little-endian integer.
(module
  (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
  (import "env" "app_global_get_uint" (func $get (param i32 i32) (result i64)))
  (import "env" "app_global_put_uint" (func $put (param i32 i32 i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "counter")
  (func (export "program") (result i32)
    (local $n i32)
    (local $v i64)
    ;; read ApplicationArgs[0] into offset 16 as 4 little-endian bytes
    (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 16) (i32.const 4)))
    (local.set $n (i32.load (i32.const 16)))
    (local.set $v (call $get (i32.const 0) (i32.const 7)))
    (block $done
      (loop $l
        (br_if $done (i32.eqz (local.get $n)))
        (local.set $v (i64.add (local.get $v) (i64.const 1)))
        (call $put (i32.const 0) (i32.const 7) (local.get $v))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (br $l)))
    (i32.const 1)))
