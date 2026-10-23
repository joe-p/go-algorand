;; A valid WASM module with no `program` export, which is not a valid LogicSig.
(module
  (func (export "not_program") (result i32)
    i32.const 1))
