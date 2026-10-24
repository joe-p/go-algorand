;; A WASM LogicSig that reads the transaction's Sender (TxnField 0) and
;; approves only if it is the expected 32 bytes.
(module
  (import "env" "txn_bytes" (func $txn_bytes (param i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (i32.eq (call $txn_bytes (i32.const 0) (i32.const 0) (i32.const 32))
            (i32.const 32))))
