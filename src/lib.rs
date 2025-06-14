#[derive(rust2go::R2G, Clone, Copy)]
pub struct AddResult {
    pub overflow: bool,
    pub result: u64,
}

#[rust2go::g2r]
pub trait G2RCall {
    fn add(a: u64, b: u64) -> AddResult;
}

impl G2RCall for G2RCallImpl {
    fn add(a: u64, b: u64) -> AddResult {
        if let Some(result) = a.checked_add(b) {
            AddResult {
                overflow: false,
                result,
            }
        } else {
            AddResult {
                overflow: true,
                result: 0,
            }
        }
    }
}
