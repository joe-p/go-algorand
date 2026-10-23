I want to add a new type of LogicSig that is WASM bytecode rather than AVM bytecode/TEAL. To start, these WASM lsigs should just have access to args, no other information yet. In the future we will expose access to opcodes/txns/etc.

For WASM execution, use wasmi
