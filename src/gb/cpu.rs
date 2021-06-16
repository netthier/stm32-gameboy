use crate::gb::mem::SharedMem;

use crate::coroutines::yield_now;
use cortex_m_semihosting::hprintln;

pub struct Cpu {
    af: Register,
    bc: Register,
    de: Register,
    hl: Register,
    sp: u16,
    pc: u16,
    ime: bool,
    mem: SharedMem,

    current_instr: [u8; 2], // Caches the current instruction to avoid memory accesses
}

#[derive(Copy, Clone)]
enum Flag {
    Z = 0b1000_0000,
    N = 0b0100_0000,
    H = 0b0010_0000,
    C = 0b0001_0000,
}

enum PcMode {
    Step(u16),
    Jump(u16),
    RelJump(i8),
}

impl Cpu {
    pub fn new(mem: SharedMem) -> Self {
        Self {
            af: Register::new(0x01B0),
            bc: Register::new(0x0013),
            de: Register::new(0x00D8),
            hl: Register::new(0x014D),
            sp: 0xFFFE,
            pc: 0x100,
            ime: false,
            mem,

            current_instr: [0; 2],
        }
    }

    pub async fn step(&mut self) {
        let instr = self.get_instr_nibbles().await;

        self.current_instr = instr;
        let step = match instr {
            [0x0, 0x0] => 1, // NOP
            [0x0, 0x8] => self.ld_u16p_sp().await,
            [0x1, 0x0] => 2,
            [0x1, 0x8] => self.jr().await,
            [0x2 | 0x3, 0x0 | 0x8] => self.jr_cond().await,
            [0x0..=0x3, 0x1] => self.ld_r16_u16().await,
            [0x0..=0x3, 0x9] => self.add_hl_r16().await,
            [0x0..=0x3, 0x2] => self.ld_r16p_a().await,
            [0x0..=0x3, 0xA] => self.ld_a_r16p().await,
            [0x0..=0x3, 0x3] => self.inc_r16().await,
            [0x0..=0x3, 0xB] => self.dec_r16().await,
            [0x0..=0x3, 0x4 | 0xC] => self.inc_r8().await,
            [0x0..=0x3, 0x5 | 0xD] => self.dec_r8().await,
            [0x0..=0x3, 0x6 | 0xE] => self.ld_r8_u8().await,
            [0x0..=0x3, 0x7 | 0xF] => self.af_ops().await,
            [0x7, 0x6] => unimplemented!(), //TODO HALT
            [0x4..=0x7, 0x0..=0xF] => self.ld_r8_r8().await,
            [0x8..=0xB, 0x0..=0xF] => self.alu_a_r8().await,
            [0xC | 0xD, 0x0 | 0x8] => self.ret_cond().await,
            [0xE, 0x0] => self.ld_io_u8_a().await,
            [0xE, 0x8] => self.add_sp_i8().await,
            [0xF, 0x0] => self.ld_a_io_u8().await,
            [0xF, 0x8] => self.ld_hl_sp_i8().await,
            [0xC..=0xF, 0x1] => self.pop_r16().await,
            [0xC, 0x9] => self.ret().await,
            [0xD, 0x9] => self.reti().await,
            [0xE, 0x9] => self.jp_hl().await,
            [0xF, 0x9] => self.ld_sp_hl().await,
            [0xC | 0xD, 0x2 | 0xA] => self.jp_cond().await,
            [0xE, 0x2] => self.ld_io_c_a().await,
            [0xE, 0xA] => self.ld_u16p_a().await,
            [0xF, 0x2] => self.ld_a_io_c().await,
            [0xF, 0xA] => self.ld_a_u16p().await,
            [0xC, 0x3] => self.jp_u16().await,
            [0xC, 0xB] => self.cb().await,
            [0xF, 0x3] => self.di().await,
            [0xF, 0xB] => self.ei().await,
            [0xC | 0xD, 0x4 | 0xC] => self.call_cond().await,
            [0xC..=0xF, 0x5] => self.push_r16().await,
            [0xC, 0xD] => self.call_u16().await,
            [0xC..=0xF, 0x6 | 0xE] => self.alu_a_u8().await,
            [0xC..=0xF, 0x7 | 0xF] => self.rst().await,
            _ => unimplemented!(),
        };

        self.set_pc(PcMode::Step(step));
    }

    async fn ld_u16p_sp(&mut self) -> u16 {
        let dest = self.read_dword(self.pc + 1).await;
        self.write_dword(dest, self.sp).await;
        3
    }

    async fn jr(&mut self) -> u16 {
        let offset = {
            yield_now().await;
            self.read_word(self.pc + 1).await as i8
        };

        self.set_pc(PcMode::RelJump(offset));
        2
    }

    async fn jr_cond(&mut self) -> u16 {
        let offset = self.read_word(self.pc + 1).await as i8;
        if self.decode_condition() {
            yield_now().await;
            self.set_pc(PcMode::RelJump(offset));
        }
        2
    }

    async fn ld_r16_u16(&mut self) -> u16 {
        let val = self.read_dword(self.pc + 1).await;
        *self.mut_decoded_r16_1() = val;
        3
    }

    async fn add_hl_r16(&mut self) -> u16 {
        let val = *self.hl;
        let rhs = *self.mut_decoded_r16_1();
        self.set_flag(Flag::N, false);
        self.set_flag(
            Flag::H,
            ((val & 0x0FFF) + (rhs & 0x0FFF)) & 0x1000 == 0x1000,
        );
        let (res, wrap) = val.overflowing_add(rhs);
        self.set_flag(Flag::C, wrap);
        *self.hl = res;
        yield_now().await;
        1
    }

    async fn ld_r16p_a(&mut self) -> u16 {
        let val = self.af[0];
        self.set_decoded_r16_2_mem(val).await;
        1
    }

    async fn ld_a_r16p(&mut self) -> u16 {
        self.af[0] = self.get_decoded_r16_2_mem().await;
        1
    }

    async fn inc_r16(&mut self) -> u16 {
        *self.mut_decoded_r16_1() = self.mut_decoded_r16_1().wrapping_add(1);
        yield_now().await;
        1
    }

    async fn dec_r16(&mut self) -> u16 {
        *self.mut_decoded_r16_1() = self.mut_decoded_r16_1().wrapping_sub(1);
        yield_now().await;
        1
    }

    async fn inc_r8(&mut self) -> u16 {
        let val = self.get_decoded_high_r8().await;
        self.set_flag(Flag::Z, val == 0xFF);
        self.set_flag(Flag::N, false);
        self.set_flag(Flag::H, (val & 0xF) == 0xF);
        self.set_decoded_high_r8(val.wrapping_add(1)).await;
        1
    }

    async fn dec_r8(&mut self) -> u16 {
        let val = self.get_decoded_high_r8().await;
        self.set_flag(Flag::Z, val == 0x01);
        self.set_flag(Flag::N, true);
        self.set_flag(Flag::H, (val & 0xF) == 0x0);
        self.set_decoded_high_r8(val.wrapping_sub(1)).await;
        1
    }

    async fn ld_r8_u8(&mut self) -> u16 {
        let val = self.read_word(self.pc + 1).await;
        self.set_decoded_high_r8(val).await;
        2
    }

    async fn af_ops(&mut self) -> u16 {
        let bits = self.calc_high_bits();
        let a = self.af[0];
        match bits {
            0x0..=0x3 => {
                self.set_flag(Flag::Z, false);
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, false);

                self.af[0] = match bits {
                    0x0 => a.rotate_left(1),
                    0x1 => a.rotate_right(1),
                    0x2 => (a << 1) | self.get_flag(Flag::C) as u8,
                    0x3 => (a >> 1) | ((self.get_flag(Flag::C) as u8) << 7),
                    _ => unreachable!(),
                };

                if bits % 2 == 0 {
                    self.set_flag(Flag::C, a & 0x80 == 0x80);
                } else {
                    self.set_flag(Flag::C, a & 0x1 == 0x1);
                }
            }
            0x4 => {
                let mut u = 0;
                if self.get_flag(Flag::H) || (!self.get_flag(Flag::N) && (a & 0xF) > 9) {
                    u = 6;
                }
                if self.get_flag(Flag::C) || (!self.get_flag(Flag::N) && a > 0x99) {
                    u |= 0x60;
                    self.set_flag(Flag::C, true);
                }

                let res = if self.get_flag(Flag::N) {
                    a.wrapping_sub(u)
                } else {
                    a.wrapping_add(u)
                };

                self.set_flag(Flag::Z, res == 0);
                self.set_flag(Flag::H, false);
                self.af[0] = res;
            }
            0x5 => {
                self.set_flag(Flag::N, true);
                self.set_flag(Flag::H, true);
                self.af[0] = !a
            }
            0x6 => {
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, false);
                self.set_flag(Flag::C, true);
            }
            0x7 => {
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, false);
                let c = self.get_flag(Flag::C);
                self.set_flag(Flag::C, !c);
            }
            _ => unreachable!(),
        }
        1
    }

    async fn ld_r8_r8(&mut self) -> u16 {
        let val = self.get_decoded_low_r8().await;
        self.set_decoded_high_r8(val).await;
        1
    }

    async fn alu_a_r8(&mut self) -> u16 {
        let rhs = self.get_decoded_low_r8().await;
        self.af[0] = self.alu(rhs);
        1
    }

    async fn ret_cond(&mut self) -> u16 {
        yield_now().await;
        if self.decode_condition() {
            yield_now().await;
            let dest = self.pop().await;
            self.set_pc(PcMode::Jump(dest));
            0
        } else {
            1
        }
    }

    async fn ld_io_u8_a(&mut self) -> u16 {
        let offset = self.read_word(self.pc + 1).await as u16;
        self.write_word(0xFF00 + offset, self.af[0]).await;
        2
    }

    async fn add_sp_i8(&mut self) -> u16 {
        let val = self.read_word(self.pc + 1).await as i8 as u16;
        let res = self.sp.wrapping_add(val);
        self.set_flag(Flag::Z, false);
        self.set_flag(Flag::N, false);
        self.set_flag(Flag::H, (self.sp & 0xF) + (val & 0xF) > 0xF);
        self.set_flag(Flag::C, (self.sp & 0xFF) + (val & 0xFF) > 0xFF);

        self.sp = res;
        2
    }

    async fn ld_a_io_u8(&mut self) -> u16 {
        let offset = self.read_word(self.pc + 1).await as u16;
        self.af[0] = self.read_word(0xFF00 + offset).await;
        2
    }

    async fn ld_hl_sp_i8(&mut self) -> u16 {
        let val = self.read_word(self.pc + 1).await as i8 as u16;
        let res = self.sp.wrapping_add(val);
        self.set_flag(Flag::Z, false);
        self.set_flag(Flag::N, false);
        self.set_flag(Flag::H, (self.sp & 0xF) + (val & 0xF) > 0xF);
        self.set_flag(Flag::C, (self.sp & 0xFF) + (val & 0xFF) > 0xFF);

        *self.hl = res;

        2
    }

    async fn ld_sp_hl(&mut self) -> u16 {
        yield_now().await;
        self.sp = *self.hl;
        1
    }

    async fn jp_hl(&mut self) -> u16 {
        let dest = *self.hl;
        self.set_pc(PcMode::Jump(dest));
        0
    }

    async fn pop_r16(&mut self) -> u16 {
        *self.mut_decoded_r16_3() = self.pop().await;

        // Edge case - POP AF does not write lower 4 bits of F
        self.af[1] &= 0xF0;

        1
    }

    async fn ret(&mut self) -> u16 {
        yield_now().await;
        let dest = self.pop().await;
        self.set_pc(PcMode::Jump(dest));
        0
    }

    async fn reti(&mut self) -> u16 {
        self.ime = true;
        yield_now().await;
        let dest = self.pop().await;
        self.set_pc(PcMode::Jump(dest));
        0
    }

    async fn jp_cond(&mut self) -> u16 {
        let dest = self.read_dword(self.pc + 1).await;
        if self.decode_condition() {
            yield_now().await;
            self.set_pc(PcMode::Jump(dest));
            0
        } else {
            3
        }
    }

    async fn ld_io_c_a(&mut self) -> u16 {
        let offset = self.bc[1] as u16;
        self.write_word(0xFF00 + offset, self.af[0]).await;
        1
    }

    async fn ld_u16p_a(&mut self) -> u16 {
        let dest = self.read_dword(self.pc + 1).await;
        self.write_word(dest, self.af[0]).await;
        3
    }

    async fn ld_a_io_c(&mut self) -> u16 {
        let offset = self.bc[1] as u16;
        self.af[0] = self.read_word(0xFF00 + offset).await;
        1
    }

    async fn ld_a_u16p(&mut self) -> u16 {
        let src = self.read_dword(self.pc + 1).await;
        self.af[0] = self.read_word(src).await;
        3
    }

    async fn jp_u16(&mut self) -> u16 {
        let dest = {
            yield_now().await;
            self.read_dword(self.pc + 1).await
        };

        self.set_pc(PcMode::Jump(dest));
        0
    }

    async fn cb(&mut self) -> u16 {
        self.set_pc(PcMode::Step(1));
        self.current_instr = self.get_instr_nibbles().await;

        let bits = self.calc_high_bits();
        let val = self.get_decoded_low_r8().await;

        match self.current_instr[0] {
            0x0..=0x3 => {
                let res = match bits {
                    0x0..=0x5 | 0x7 => {
                        let res = match bits {
                            0x0 => val.rotate_left(1),
                            0x1 => val.rotate_right(1),
                            0x2 => (val << 1) | self.get_flag(Flag::C) as u8,
                            0x3 => (val >> 1) | ((self.get_flag(Flag::C) as u8) << 7),
                            0x4 => (val << 1),
                            0x5 => (val >> 1) | (val & 0x80),
                            0x7 => (val >> 1),
                            _ => unreachable!(),
                        };

                        if bits % 2 == 0 {
                            self.set_flag(Flag::C, val & 0x80 == 0x80);
                        } else {
                            self.set_flag(Flag::C, val & 0x1 == 0x1);
                        }

                        res
                    }
                    0x6 => {
                        self.set_flag(Flag::C, false);
                        val.rotate_right(4)
                    }
                    _ => unreachable!(),
                };

                self.set_flag(Flag::Z, res == 0);
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, false);

                self.set_decoded_low_r8(res).await;
            }
            0x4..=0x7 => {
                self.set_flag(Flag::Z, (val & (0x1 << bits)) == 0x0);
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, true);
            }
            0x8..=0xB => self.set_decoded_low_r8(val & !(0x1 << bits)).await,
            0xC..=0xF => self.set_decoded_low_r8(val | (0x1 << bits)).await,
            _ => unreachable!(),
        }
        1
    }

    async fn di(&mut self) -> u16 {
        self.ime = false;
        1
    }

    async fn ei(&mut self) -> u16 {
        self.ime = true;
        1
    }

    async fn call_cond(&mut self) -> u16 {
        let dest = self.read_dword(self.pc + 1).await;
        if self.decode_condition() {
            yield_now().await;
            self.push(self.pc + 3).await;
            self.set_pc(PcMode::Jump(dest));
            0
        } else {
            3
        }
    }

    async fn push_r16(&mut self) -> u16 {
        yield_now().await;
        let val = *self.mut_decoded_r16_3();
        self.push(val).await;
        1
    }

    async fn call_u16(&mut self) -> u16 {
        let dest = self.read_dword(self.pc + 1).await;
        yield_now().await;
        self.push(self.pc + 3).await;
        self.set_pc(PcMode::Jump(dest));
        0
    }

    async fn alu_a_u8(&mut self) -> u16 {
        let rhs = self.read_word(self.pc + 1).await;
        self.af[0] = self.alu(rhs);
        2
    }

    async fn rst(&mut self) -> u16 {
        let bits = self.calc_high_bits() as u16;
        yield_now().await;
        self.push(self.pc + 1).await;
        self.set_pc(PcMode::Jump(bits << 3));
        0
    }

    fn alu(&mut self, rhs: u8) -> u8 {
        let bits = self.calc_high_bits();
        let a = self.af[0];
        match bits {
            0x0 => {
                let (res, wrap) = a.overflowing_add(rhs);
                self.set_flag(Flag::Z, res == 0);
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, (a & 0xF) + (rhs & 0xF) > 0xF);
                self.set_flag(Flag::C, wrap);
                res
            }
            0x1 => {
                let c = self.get_flag(Flag::C) as u8;
                let (res, wrap_0) = a.overflowing_add(rhs);
                let (res, wrap_1) = res.overflowing_add(c);
                self.set_flag(Flag::Z, res == 0);
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, (a & 0xF) + (rhs & 0xF) + c > 0xF);
                self.set_flag(Flag::C, wrap_0 | wrap_1);
                res
            }
            0x2 => {
                let (res, wrap) = a.overflowing_sub(rhs);
                self.set_flag(Flag::Z, res == 0);
                self.set_flag(Flag::N, true);
                self.set_flag(Flag::H, (a & 0xF) < (rhs & 0xF));
                self.set_flag(Flag::C, wrap);
                res
            }
            0x3 => {
                let c = self.get_flag(Flag::C) as u8;
                let (res, wrap_0) = a.overflowing_sub(rhs);
                let (res, wrap_1) = res.overflowing_sub(c);
                self.set_flag(Flag::Z, res == 0);
                self.set_flag(Flag::N, true);
                self.set_flag(Flag::H, (a & 0xF) < (rhs & 0xF) + c);
                self.set_flag(Flag::C, wrap_0 | wrap_1);
                res
            }
            0x4 => {
                let res = a & rhs;
                self.set_flag(Flag::Z, res == 0);
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, true);
                self.set_flag(Flag::C, false);
                res
            }
            0x5 => {
                let res = a ^ rhs;
                self.set_flag(Flag::Z, res == 0);
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, false);
                self.set_flag(Flag::C, false);
                res
            }
            0x6 => {
                let res = a | rhs;
                self.set_flag(Flag::Z, res == 0);
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, false);
                self.set_flag(Flag::C, false);
                res
            }
            0x7 => {
                let (res, wrap) = a.overflowing_sub(rhs);
                self.set_flag(Flag::Z, res == 0);
                self.set_flag(Flag::N, true);
                self.set_flag(Flag::H, (a & 0xF) < (rhs & 0xF));
                self.set_flag(Flag::C, wrap);
                a
            }
            _ => unreachable!(),
        }
    }

    async fn get_decoded_low_r8(&mut self) -> u8 {
        let bits = self.current_instr[1] & 0x7;
        self._get_decoded_r8(bits).await
    }

    async fn set_decoded_low_r8(&mut self, val: u8) {
        let bits = self.current_instr[1] & 0x7;
        self._set_decoded_r8(bits, val).await;
    }

    async fn get_decoded_high_r8(&mut self) -> u8 {
        let bits = self.calc_high_bits();
        self._get_decoded_r8(bits).await
    }

    async fn set_decoded_high_r8(&mut self, val: u8) {
        let bits = self.calc_high_bits();
        self._set_decoded_r8(bits, val).await;
    }

    /// DO NOT USE
    async fn _get_decoded_r8(&mut self, bits: u8) -> u8 {
        if bits == 6 {
            self.read_word(*self.hl).await
        } else {
            *self._mut_decoded_r8(bits)
        }
    }

    /// DO NOT USE
    async fn _set_decoded_r8(&mut self, bits: u8, val: u8) {
        if bits == 6 {
            self.write_word(*self.hl, val).await;
        } else {
            *self._mut_decoded_r8(bits) = val;
        }
    }

    /// DO NOT USE
    fn _mut_decoded_r8(&mut self, bits: u8) -> &mut u8 {
        let idx = (bits) % 2;
        match bits {
            0x0 | 0x1 => &mut self.bc[idx],
            0x2 | 0x3 => &mut self.de[idx],
            0x4 | 0x5 => &mut self.hl[idx],
            0x6 => panic!("Tried to get mutable reference to memory"),
            0x7 => &mut self.af[0],
            _ => unreachable!(),
        }
    }

    fn mut_decoded_r16_1(&mut self) -> &mut u16 {
        let bits = self.current_instr[0] & 0x3;
        match bits {
            0x0 => &mut self.bc,
            0x1 => &mut self.de,
            0x2 => &mut self.hl,
            0x3 => &mut self.sp,
            _ => unreachable!(),
        }
    }

    async fn get_decoded_r16_2_mem(&mut self) -> u8 {
        let decoded = self._get_decoded_r16_2();
        self.read_word(decoded).await
    }

    async fn set_decoded_r16_2_mem(&mut self, val: u8) {
        let decoded = self._get_decoded_r16_2();
        self.write_word(decoded, val).await;
    }

    /// DO NOT USE
    fn _get_decoded_r16_2(&mut self) -> u16 {
        let bits = self.current_instr[0] & 0x3;
        match bits {
            0x0 => *self.bc,
            0x1 => *self.de,
            0x2 => {
                let old = *self.hl;
                *self.hl = old.wrapping_add(1);
                old
            }
            0x3 => {
                let old = *self.hl;
                *self.hl = old.wrapping_sub(1);
                old
            }
            _ => unreachable!(),
        }
    }

    fn mut_decoded_r16_3(&mut self) -> &mut u16 {
        let bits = self.current_instr[0] & 0x3;
        match bits {
            0x0 => &mut self.bc,
            0x1 => &mut self.de,
            0x2 => &mut self.hl,
            0x3 => &mut self.af,
            _ => unreachable!(),
        }
    }

    fn calc_high_bits(&mut self) -> u8 {
        ((self.current_instr[0] & 0x3) << 1) + ((self.current_instr[1] & 0x8) >> 3)
    }

    fn decode_condition(&mut self) -> bool {
        let bits = ((self.current_instr[0] & 0x1) << 1) + ((self.current_instr[1] & 0x8) >> 3);
        match bits {
            0 => !self.get_flag(Flag::Z),
            1 => self.get_flag(Flag::Z),
            2 => !self.get_flag(Flag::C),
            3 => self.get_flag(Flag::C),
            _ => unreachable!(),
        }
    }

    async fn push(&mut self, val: u16) {
        self.sp -= 1;
        self.write_word(self.sp, ((val & 0xFF00) >> 8) as u8).await;
        self.sp -= 1;
        self.write_word(self.sp, (val & 0xFF) as u8).await;
    }

    async fn pop(&mut self) -> u16 {
        let low = self.read_word(self.sp).await as u16;
        self.sp += 1;
        let high = self.read_word(self.sp).await as u16;
        self.sp += 1;

        (high << 8) + low
    }

    fn set_pc(&mut self, mode: PcMode) {
        match mode {
            PcMode::Step(e) => self.pc += e,
            PcMode::Jump(dest) => self.pc = dest,
            PcMode::RelJump(dest) => self.pc = self.pc.wrapping_add(dest as u16),
        }
    }

    async fn get_instr(&mut self) -> u8 {
        self.read_word(self.pc).await
    }

    async fn get_instr_nibbles(&mut self) -> [u8; 2] {
        let instr = self.get_instr().await;
        [(instr & 0xF0) >> 4, instr & 0x0F]
    }

    fn set_flag(&mut self, flag: Flag, val: bool) {
        let z = &mut self.af[1];
        if val {
            *z |= flag as u8;
        } else {
            *z &= !(flag as u8);
        }
    }

    fn get_flag(&mut self, flag: Flag) -> bool {
        let mut z = self.af[1];
        z &= flag as u8;
        z == flag as u8
    }

    // Required so that memory accesses yield, and double-word accesses dont panic
    async fn read_word(&mut self, addr: u16) -> u8 {
        let val = self.mem.borrow_mut().read_word(addr);
        yield_now().await;
        val
    }

    async fn write_word(&mut self, addr: u16, val: u8) {
        self.mem.borrow_mut().write_word(addr, val);
        yield_now().await;
    }

    async fn read_dword(&mut self, addr: u16) -> u16 {
        let high = self.read_word(addr + 1).await as u16;
        let low = self.read_word(addr).await as u16;
        (high << 8) + low
    }

    async fn write_dword(&mut self, addr: u16, val: u16) {
        self.write_word(addr, (val & 0x00FF) as u8).await;
        self.write_word(addr + 1, ((val & 0xFF00) >> 8) as u8).await;
    }
}

// no rust project is complete without some unsafe!.
// indexing returns registers in big-endian order, assuming a little endian host. this means af[0] returns a.
#[derive(Copy, Clone)]
#[repr(C)]
union Register {
    a: [u8; 2],
    b: u16,
}

impl core::ops::Deref for Register {
    type Target = u16;

    fn deref(&self) -> &Self::Target {
        unsafe { &self.b }
    }
}

impl core::ops::DerefMut for Register {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut self.b }
    }
}

impl core::ops::Index<u8> for Register {
    type Output = u8;

    fn index(&self, index: u8) -> &Self::Output {
        unsafe {
            match index {
                0 => &self.a[1],
                1 => &self.a[0],
                _ => unreachable!(),
            }
        }
    }
}

impl core::ops::IndexMut<u8> for Register {
    fn index_mut(&mut self, index: u8) -> &mut Self::Output {
        unsafe {
            match index {
                0 => &mut self.a[1],
                1 => &mut self.a[0],
                _ => unreachable!(),
            }
        }
    }
}

impl Register {
    pub fn new(b: u16) -> Self {
        Self { b }
    }
}
