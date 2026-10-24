;; A WASM LogicSig that asks for the Sender (TxnField 0) as a uint. Sender is a
;; byte string, so the host traps.
(module
  (import "env" "txn_uint" (func $txn_uint (param i32) (result i64)))
  (func (export "program") (result i32)
    (i32.wrap_i64 (call $txn_uint (i32.const 0)))))
