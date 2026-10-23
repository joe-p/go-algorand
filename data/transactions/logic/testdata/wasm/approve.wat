;; A WASM LogicSig that approves unconditionally.
(module
  (func (export "program") (result i32)
    i32.const 1))
