;; A WASM application program that calls `app_global_put_bytes` N times, where N
;; is ApplicationArgs[0] (4-byte little-endian). Used by the host-call cost table.
(module
  (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
  (import "env" "app_global_put_bytes" (func $putb (param i32 i32 i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "b")
  (data (i32.const 8) "x")
  (func (export "program") (result i32)
    (local $n i32)
    (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 16) (i32.const 4)))
    (local.set $n (i32.load (i32.const 16)))
    (block $done
      (loop $l
        (br_if $done (i32.eqz (local.get $n)))
        (call $putb (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 1))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (br $l)))
    (i32.const 1)))
