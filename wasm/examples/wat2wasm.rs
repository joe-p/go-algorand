// Copyright (C) 2019-2026 Algorand Foundation Ltd.
// This file is part of go-algorand
//
// go-algorand is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// go-algorand is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with go-algorand.  If not, see <https://www.gnu.org/licenses/>.

//! Compiles a `.wat` file to a `.wasm` file. Used to (re)generate the LogicSig
//! test fixtures under data/transactions/logic/testdata/wasm.
//!
//! Usage: cargo run --example wat2wasm -- <input.wat> <output.wasm>

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: wat2wasm <input.wat> <output.wasm>");
        std::process::exit(2);
    }
    let wat = std::fs::read_to_string(&args[1]).expect("read input");
    let wasm = wat::parse_str(&wat).expect("parse wat");
    std::fs::write(&args[2], wasm).expect("write output");
}
