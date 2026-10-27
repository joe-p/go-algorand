;; ApplicationArgs[0] is an address; Fee is the asset reference, FirstValid
;; the expected existence flag, and LastValid the expected balance.
(module
  (import "env" "txn_uint" (func $txn (param i32) (result i64)))
  (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
  (import "env" "asset_balance" (func $balance (param i32 i64 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32) (local $exists i32)
    (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 0) (i32.const 32)))
    (i64.store (i32.const 32) (i64.const 42))
    (local.set $exists (call $balance (i32.const 0) (call $txn (i32.const 1)) (i32.const 32)))
    (i32.and
      (i32.eq (local.get $exists) (i32.wrap_i64 (call $txn (i32.const 2))))
      (i64.eq (i64.load (i32.const 32))
        (if (result i64) (local.get $exists)
          (then (call $txn (i32.const 4))) (else (i64.const 42)))))))
