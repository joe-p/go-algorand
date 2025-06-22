#![no_std]

use algokit::BigInt;
use algokit::GlobalStateKey;

const TOTAL_SUPPLY: GlobalStateKey<BigInt> = GlobalStateKey::new(b"ts");

fn get_total_supply() -> BigInt {
    TOTAL_SUPPLY.get()
}

fn set_total_supply(value: &BigInt) {
    TOTAL_SUPPLY.set(value);
}

#[unsafe(no_mangle)]
pub fn program() -> u64 {
    let amt = BigInt::from(100 as u64);
    set_total_supply(&amt);
    let current_supply = get_total_supply();

    let new_supply = current_supply + amt;
    set_total_supply(&new_supply);

    // Return the new total supply as an example

    if get_total_supply() == BigInt::from(200 as u64) {
        1 // Indicating success
    } else {
        0 // Indicating failure
    }
}
