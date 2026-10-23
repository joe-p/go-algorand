;; A WASM LogicSig that loops forever, used to test fuel exhaustion.
(module
  (func (export "program") (result i32)
    (loop $l
      br $l)
    i32.const 1))
