;; A WASM LogicSig that approves only if the transaction's FirstValid is 1234.
;; FirstValid is TxnField 2.
(module
  (import "env" "txn_uint" (func $txn_uint (param i32) (result i64)))
  (func (export "program") (result i32)
    (i64.eq (call $txn_uint (i32.const 2)) (i64.const 1234))))
