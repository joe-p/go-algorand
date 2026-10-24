;; A WASM application program that calls `ed25519verify_bare` N times. N is
;; ApplicationArgs[0] (4-byte little-endian); the data, signature, and public key
;; are ApplicationArgs[1..3] (32, 64, 32 bytes), copied into memory once so each
;; loop iteration verifies the same real signature. Used by the host-call cost
;; table; keep N small (the opcode is expensive).
(module
  (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
  (import "env" "ed25519verify_bare" (func $ed (param i32 i32 i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (local $n i32)
    (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 16) (i32.const 4)))
    (local.set $n (i32.load (i32.const 16)))
    ;; arg1 -> data @64, arg2 -> sig @128, arg3 -> pk @256
    (drop (call $arg (i32.const 26) (i32.const 1) (i32.const 64) (i32.const 32)))
    (drop (call $arg (i32.const 26) (i32.const 2) (i32.const 128) (i32.const 64)))
    (drop (call $arg (i32.const 26) (i32.const 3) (i32.const 256) (i32.const 32)))
    (block $done
      (loop $l
        (br_if $done (i32.eqz (local.get $n)))
        (drop (call $ed (i32.const 64) (i32.const 32) (i32.const 128) (i32.const 64)
                        (i32.const 256) (i32.const 32)))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (br $l)))
    (i32.const 1)))
