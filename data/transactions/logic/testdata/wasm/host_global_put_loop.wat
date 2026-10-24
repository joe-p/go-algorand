;; A WASM application program that calls `app_global_put_uint` N times, where N
;; is ApplicationArgs[0] (4-byte little-endian). The value changes each
;; iteration so the write is not elided. Used by the host-call cost table.
(module
  (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
  (import "env" "app_global_put_uint" (func $put (param i32 i32 i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "counter")
  (func (export "program") (result i32)
    (local $n i32)
    (local $v i64)
    (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 16) (i32.const 4)))
    (local.set $n (i32.load (i32.const 16)))
    (block $done
      (loop $l
        (br_if $done (i32.eqz (local.get $n)))
        (local.set $v (i64.add (local.get $v) (i64.const 1)))
        (call $put (i32.const 0) (i32.const 7) (local.get $v))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (br $l)))
    (i32.const 1)))
