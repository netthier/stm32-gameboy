use crate::gb::mem::SharedMem;

pub struct Ppu {
    mem: SharedMem,
}

impl Ppu {
    pub fn new(mem: SharedMem) -> Self {
        Self { mem }
    }
}
