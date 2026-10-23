;; A WASM LogicSig that rejects unconditionally.
(module
  (func (export "program") (result i32)
    i32.const 0))
