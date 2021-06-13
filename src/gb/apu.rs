use crate::gb::mem::SharedMem;

pub struct Apu {
    mem: SharedMem,
}

impl Apu {
    pub fn new(mem: SharedMem) -> Self {
        Self { mem }
    }
}
