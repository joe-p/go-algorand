#![no_std]

use algokit::GlobalStateKey;
use crypto_bigint::U256;

const TOTAL_SUPPLY: GlobalStateKey<U256> = GlobalStateKey::new(b"ts");

fn get_total_supply() -> U256 {
    TOTAL_SUPPLY.get()
}

fn set_total_supply(value: U256) {
    TOTAL_SUPPLY.set(&value);
}

#[unsafe(no_mangle)]
pub fn program() -> u64 {
    let amt = U256::from(100 as u64);
    set_total_supply(amt);
    let current_supply = get_total_supply();

    let new_supply = current_supply + amt;
    set_total_supply(new_supply);

    // Return the new total supply as an example

    let mut u64_bytes = [0 as u8; 8];
    u64_bytes[7] = *get_total_supply().to_be_bytes().last().unwrap();

    u64::from_be_bytes(u64_bytes.try_into().unwrap())
}
