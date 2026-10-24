;; A WASM LogicSig that reads three arguments (data, signature, public key),
;; copies them into linear memory, and approves iff ed25519verify_bare accepts
;; the signature. This exercises the crypto host function against a real
;; signature produced by the Go crypto package.
(module
  (import "env" "arg" (func $arg (param i32 i32 i32) (result i32)))
  (import "env" "ed25519verify_bare"
    (func $verify (param i32 i32 i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (local $dlen i32) (local $slen i32) (local $plen i32)
    ;; data at 0, signature at 4096, public key at 8192.
    (local.set $dlen (call $arg (i32.const 0) (i32.const 0) (i32.const 4096)))
    (local.set $slen (call $arg (i32.const 1) (i32.const 4096) (i32.const 4096)))
    (local.set $plen (call $arg (i32.const 2) (i32.const 8192) (i32.const 4096)))
    ;; An argument that did not fit was not read; reject rather than error.
    (if (i32.or (i32.or (i32.lt_s (local.get $dlen) (i32.const 0))
                        (i32.lt_s (local.get $slen) (i32.const 0)))
                (i32.lt_s (local.get $plen) (i32.const 0)))
      (then (return (i32.const 0))))
    (call $verify
      (i32.const 0) (local.get $dlen)
      (i32.const 4096) (local.get $slen)
      (i32.const 8192) (local.get $plen))))
