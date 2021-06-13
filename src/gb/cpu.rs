use crate::gb::mem::SharedMem;

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

    pub fn step(&mut self) {
        let instr = self.get_instr_nibbles();

        self.current_instr = instr;
        let step = match instr {
            [0x0, 0x0] => 1, // NOP
            [0x0, 0x8] => self.ld_u16p_sp(),
            [0x1, 0x0] => 2, //TODO STOP
            [0x1, 0x8] => self.jr(),
            [0x2 | 0x3, 0x0 | 0x8] => self.jr_cond(),
            [0x0..=0x3, 0x1] => self.ld_r16_u16(),
            [0x0..=0x3, 0x9] => self.add_hl_r16(),
            [0x0..=0x3, 0x2] => self.ld_r16p_a(),
            [0x0..=0x3, 0xA] => self.ld_a_r16p(),
            [0x0..=0x3, 0x3] => self.inc_r16(),
            [0x0..=0x3, 0xB] => self.dec_r16(),
            [0x0..=0x3, 0x4 | 0xC] => self.inc_r8(),
            [0x0..=0x3, 0x5 | 0xD] => self.dec_r8(),
            [0x0..=0x3, 0x6 | 0xE] => self.ld_r8_u8(),
            [0x0..=0x3, 0x7 | 0xF] => self.af_ops(),
            [0x7, 0x6] => unimplemented!(), //TODO HALT
            [0x4..=0x7, 0x0..=0xF] => self.ld_r8_r8(),
            [0x8..=0xB, 0x0..=0xF] => self.alu_a_r8(),
            [0xC | 0xD, 0x0 | 0x8] => self.ret_cond(),
            [0xE, 0x0] => self.ld_io_u8_a(),
            [0xE, 0x8] => self.add_sp_i8(),
            [0xF, 0x0] => self.ld_a_io_u8(),
            [0xF, 0x8] => self.ld_hl_sp_i8(),
            [0xC..=0xF, 0x1] => self.pop_r16(),
            [0xC, 0x9] => self.ret(),
            [0xD, 0x9] => self.reti(),
            [0xE, 0x9] => self.jp_hl(),
            [0xF, 0x9] => self.ld_sp_hl(),
            [0xC | 0xD, 0x2 | 0xA] => self.jp_cond(),
            [0xE, 0x2] => self.ld_io_c_a(),
            [0xE, 0xA] => self.ld_u16p_a(),
            [0xF, 0x2] => self.ld_a_io_c(),
            [0xF, 0xA] => self.ld_a_u16p(),
            [0xC, 0x3] => self.jp_u16(),
            [0xC, 0xB] => self.cb(),
            [0xF, 0x3] => self.di(),
            [0xF, 0xB] => self.ei(),
            [0xC | 0xD, 0x4 | 0xC] => self.call_cond(),
            [0xC..=0xF, 0x5] => self.push_r16(),
            [0xC, 0xD] => self.call_u16(),
            [0xC..=0xF, 0x6 | 0xE] => self.alu_a_u8(),
            [0xC..=0xF, 0x7 | 0xF] => self.rst(),
            _ => {
                hprintln!("Unimplemented instr: {:X}", self.get_instr());
                panic!();
            }
        };

        self.set_pc(PcMode::Step(step));
    }

    fn ld_u16p_sp(&mut self) -> u16 {
        let mut mem = self.mem.borrow_mut();
        let dest = mem.read_dword(self.pc + 1);
        mem.write_dword(dest, self.sp);
        3
    }

    fn jr(&mut self) -> u16 {
        let offset = {
            let mut mem = self.mem.borrow_mut();
            mem.cycles += 4;
            mem.read_word(self.pc + 1) as i8
        };

        self.set_pc(PcMode::RelJump(offset));
        2
    }

    fn jr_cond(&mut self) -> u16 {
        let offset = self.mem.borrow_mut().read_word(self.pc + 1) as i8;
        if self.decode_condition() {
            self.mem.borrow_mut().cycles += 4;
            self.set_pc(PcMode::RelJump(offset));
        }
        2
    }

    fn ld_r16_u16(&mut self) -> u16 {
        let val = self.mem.borrow_mut().read_dword(self.pc + 1);
        *self.mut_decoded_r16_1() = val;
        3
    }

    fn add_hl_r16(&mut self) -> u16 {
        let val = *self.hl.as_16bit();
        let rhs = *self.mut_decoded_r16_1();
        self.set_flag(Flag::N, false);
        self.set_flag(Flag::H, ((val & 0x0F00) + (rhs & 0x0F00)) & 0x10 == 0x10);
        let (res, wrap) = val.overflowing_add(rhs);
        self.set_flag(Flag::C, wrap);
        *self.hl.as_16bit() = res;
        self.mem.borrow_mut().cycles += 4;
        1
    }

    fn ld_r16p_a(&mut self) -> u16 {
        let val = self.af.as_8bit()[1];
        self.set_decoded_r16_2_mem(val);
        1
    }

    fn ld_a_r16p(&mut self) -> u16 {
        self.af.as_8bit()[1] = self.get_decoded_r16_2_mem();
        1
    }

    fn inc_r16(&mut self) -> u16 {
        *self.mut_decoded_r16_1() = self.mut_decoded_r16_1().wrapping_add(1);
        self.mem.borrow_mut().cycles += 4;
        1
    }

    fn dec_r16(&mut self) -> u16 {
        *self.mut_decoded_r16_1() = self.mut_decoded_r16_1().wrapping_sub(1);
        self.mem.borrow_mut().cycles += 4;
        1
    }

    fn inc_r8(&mut self) -> u16 {
        let val = self.get_decoded_high_r8();
        self.set_flag(Flag::Z, val == 0xFF);
        self.set_flag(Flag::N, false);
        self.set_flag(Flag::H, (val & 0xF) == 0xF);
        self.set_decoded_high_r8(val.wrapping_add(1));
        1
    }

    fn dec_r8(&mut self) -> u16 {
        let val = self.get_decoded_high_r8();
        self.set_flag(Flag::Z, val == 0x01);
        self.set_flag(Flag::N, true);
        self.set_flag(Flag::H, (val & 0xF) == 0x0);
        self.set_decoded_high_r8(val.wrapping_sub(1));
        1
    }

    fn ld_r8_u8(&mut self) -> u16 {
        let val = self.mem.borrow_mut().read_word(self.pc + 1);
        self.set_decoded_high_r8(val);
        2
    }

    fn af_ops(&mut self) -> u16 {
        let bits = self.calc_high_bits();
        let [_, a] = *self.af.as_8bit();
        match bits {
            0x0..=0x3 => {
                self.set_flag(Flag::Z, false);
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, false);

                self.af.as_8bit()[1] = match bits {
                    0x0 => a.rotate_left(1),
                    0x1 => a.rotate_right(1),
                    0x2 => (a << 1) | self.get_flag(Flag::C) as u8,
                    0x3 => (a >> 1) | (self.get_flag(Flag::C) as u8).rotate_right(1),
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
                self.af.as_8bit()[1] = res;
            }
            0x5 => {
                self.set_flag(Flag::N, true);
                self.set_flag(Flag::H, true);
                self.af.as_8bit()[1] = !a
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

    fn ld_r8_r8(&mut self) -> u16 {
        let val = self.get_decoded_low_r8();
        self.set_decoded_high_r8(val);
        1
    }

    fn alu_a_r8(&mut self) -> u16 {
        let rhs = self.get_decoded_low_r8();
        self.af.as_8bit()[1] = self.alu(rhs);
        1
    }

    fn ret_cond(&mut self) -> u16 {
        self.mem.borrow_mut().cycles += 4;
        if self.decode_condition() {
            self.mem.borrow_mut().cycles += 4;
            let dest = self.pop();
            self.set_pc(PcMode::Jump(dest));
            0
        } else {
            1
        }
    }

    fn ld_io_u8_a(&mut self) -> u16 {
        let mut mem = self.mem.borrow_mut();
        let offset = mem.read_word(self.pc + 1) as u16;
        mem.write_word(0xFF00 + offset, self.af.as_8bit()[1]);
        2
    }

    fn add_sp_i8(&mut self) -> u16 {
        let val = self.mem.borrow_mut().read_word(self.pc + 1) as i32;
        let (res, wrap) = (self.sp as i32).overflowing_add(val);
        self.set_flag(Flag::Z, false);
        self.set_flag(Flag::N, false);
        self.set_flag(
            Flag::H,
            if val >= 0 {
                (self.sp & 0xF) + (val as u16 & 0xF) > 0xF
            } else {
                false
            },
        );
        self.set_flag(Flag::C, wrap);

        self.sp = res as u16;
        2
    }

    fn ld_a_io_u8(&mut self) -> u16 {
        let mut mem = self.mem.borrow_mut();
        let offset = mem.read_word(self.pc + 1) as u16;
        self.af.as_8bit()[1] = mem.read_word(0xFF00 + offset);
        2
    }

    fn ld_hl_sp_i8(&mut self) -> u16 {
        let val = self.mem.borrow_mut().read_word(self.pc + 1) as i32;
        let (res, wrap) = (self.sp as i32).overflowing_add(val);
        self.set_flag(Flag::Z, false);
        self.set_flag(Flag::N, false);
        self.set_flag(
            Flag::H,
            if val >= 0 {
                (self.sp & 0xF) + (val as u16 & 0xF) > 0xF
            } else {
                false
            },
        );
        self.set_flag(Flag::C, wrap);

        *self.hl.as_16bit() = res as u16;

        2
    }

    fn ld_sp_hl(&mut self) -> u16 {
        self.mem.borrow_mut().cycles += 4;
        self.sp = *self.hl.as_16bit();
        1
    }

    fn jp_hl(&mut self) -> u16 {
        let dest = *self.hl.as_16bit();
        self.set_pc(PcMode::Jump(dest));
        0
    }

    fn pop_r16(&mut self) -> u16 {
        *self.mut_decoded_r16_3() = self.pop();

        // Edge case - POP AF does not write lower 4 bits of F
        self.af.as_8bit()[0] &= 0xF0;

        1
    }

    fn ret(&mut self) -> u16 {
        self.mem.borrow_mut().cycles += 4;
        let dest = self.pop();
        self.set_pc(PcMode::Jump(dest));
        0
    }

    fn reti(&mut self) -> u16 {
        self.ime = true;
        self.mem.borrow_mut().cycles += 4;
        let dest = self.pop();
        self.set_pc(PcMode::Jump(dest));
        0
    }

    fn jp_cond(&mut self) -> u16 {
        let dest = self.mem.borrow_mut().read_dword(self.pc + 1);
        if self.decode_condition() {
            self.mem.borrow_mut().cycles += 4;
            self.set_pc(PcMode::Jump(dest));
            0
        } else {
            3
        }
    }

    fn ld_io_c_a(&mut self) -> u16 {
        let offset = self.bc.as_8bit()[0] as u16;
        self.mem
            .borrow_mut()
            .write_word(0xFF00 + offset, self.af.as_8bit()[1]);
        1
    }

    fn ld_u16p_a(&mut self) -> u16 {
        let mut mem = self.mem.borrow_mut();
        let dest = mem.read_dword(self.pc + 1);
        mem.write_word(dest, self.af.as_8bit()[1]);
        3
    }

    fn ld_a_io_c(&mut self) -> u16 {
        let offset = self.bc.as_8bit()[0] as u16;
        self.af.as_8bit()[1] = self.mem.borrow_mut().read_word(0xFF00 + offset);
        1
    }

    fn ld_a_u16p(&mut self) -> u16 {
        let mut mem = self.mem.borrow_mut();
        let src = mem.read_dword(self.pc + 1);
        self.af.as_8bit()[1] = mem.read_word(src);
        3
    }

    fn jp_u16(&mut self) -> u16 {
        let dest = {
            let mut mem = self.mem.borrow_mut();
            mem.cycles += 4;
            mem.read_dword(self.pc + 1)
        };

        self.set_pc(PcMode::Jump(dest));
        0
    }

    fn cb(&mut self) -> u16 {
        self.set_pc(PcMode::Step(1));
        self.current_instr = self.get_instr_nibbles();

        let bits = self.calc_high_bits();
        let val = self.get_decoded_low_r8();

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

                self.set_decoded_low_r8(res);
            }
            0x4..=0x7 => {
                self.set_flag(Flag::Z, (val & (0x1 << bits)) == 0x0);
                self.set_flag(Flag::N, false);
                self.set_flag(Flag::H, true);
            }
            0x8..=0xB => self.set_decoded_low_r8(val & !(0x1 << bits)),
            0xC..=0xF => self.set_decoded_low_r8(val | (0x1 << bits)),
            _ => unreachable!(),
        }
        1
    }

    fn di(&mut self) -> u16 {
        self.ime = false;
        1
    }

    fn ei(&mut self) -> u16 {
        self.ime = true;
        1
    }

    fn call_cond(&mut self) -> u16 {
        let dest = self.mem.borrow_mut().read_dword(self.pc + 1);
        if self.decode_condition() {
            self.mem.borrow_mut().cycles += 4;
            self.push(self.pc + 3);
            self.set_pc(PcMode::Jump(dest));
            0
        } else {
            3
        }
    }

    fn push_r16(&mut self) -> u16 {
        self.mem.borrow_mut().cycles += 4;
        let val = *self.mut_decoded_r16_3();
        self.push(val);
        1
    }

    fn call_u16(&mut self) -> u16 {
        let dest = self.mem.borrow_mut().read_dword(self.pc + 1);
        self.mem.borrow_mut().cycles += 4;
        self.push(self.pc + 3);
        self.set_pc(PcMode::Jump(dest));
        0
    }

    fn alu_a_u8(&mut self) -> u16 {
        let rhs = self.mem.borrow_mut().read_word(self.pc + 1);
        self.af.as_8bit()[1] = self.alu(rhs);
        2
    }

    fn rst(&mut self) -> u16 {
        let bits = self.calc_high_bits() as u16;
        self.mem.borrow_mut().cycles += 4;
        self.push(self.pc + 1);
        self.set_pc(PcMode::Jump(bits << 3));
        0
    }

    fn alu(&mut self, rhs: u8) -> u8 {
        let bits = self.calc_high_bits();
        let [_, a] = *self.af.as_8bit();
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

    fn get_decoded_low_r8(&mut self) -> u8 {
        let bits = self.current_instr[1] & 0x7;
        self._get_decoded_r8(bits)
    }

    fn set_decoded_low_r8(&mut self, val: u8) {
        let bits = self.current_instr[1] & 0x7;
        self._set_decoded_r8(bits, val);
    }

    fn get_decoded_high_r8(&mut self) -> u8 {
        let bits = self.calc_high_bits();
        self._get_decoded_r8(bits)
    }

    fn set_decoded_high_r8(&mut self, val: u8) {
        let bits = self.calc_high_bits();
        self._set_decoded_r8(bits, val);
    }

    /// DO NOT USE
    fn _get_decoded_r8(&mut self, bits: u8) -> u8 {
        if bits == 6 {
            self.mem.borrow_mut().read_word(*self.hl.as_16bit())
        } else {
            *self._mut_decoded_r8(bits)
        }
    }

    /// DO NOT USE
    fn _set_decoded_r8(&mut self, bits: u8, val: u8) {
        if bits == 6 {
            self.mem.borrow_mut().write_word(*self.hl.as_16bit(), val);
        } else {
            *self._mut_decoded_r8(bits) = val;
        }
    }

    /// DO NOT USE
    fn _mut_decoded_r8(&mut self, bits: u8) -> &mut u8 {
        let idx = (bits as usize + 1) % 2;
        match bits {
            0x0 | 0x1 => &mut self.bc.as_8bit()[idx],
            0x2 | 0x3 => &mut self.de.as_8bit()[idx],
            0x4 | 0x5 => &mut self.hl.as_8bit()[idx],
            0x6 => panic!("Tried to get mutable reference to memory"),
            0x7 => &mut self.af.as_8bit()[1],
            _ => unreachable!(),
        }
    }

    fn mut_decoded_r16_1(&mut self) -> &mut u16 {
        let bits = self.current_instr[0] & 0x3;
        match bits {
            0x0 => self.bc.as_16bit(),
            0x1 => self.de.as_16bit(),
            0x2 => self.hl.as_16bit(),
            0x3 => &mut self.sp,
            _ => unreachable!(),
        }
    }

    fn get_decoded_r16_2_mem(&mut self) -> u8 {
        let decoded = self._get_decoded_r16_2();
        self.mem.borrow_mut().read_word(decoded)
    }

    fn set_decoded_r16_2_mem(&mut self, val: u8) {
        let decoded = self._get_decoded_r16_2();
        self.mem.borrow_mut().write_word(decoded, val);
    }

    /// DO NOT USE
    fn _get_decoded_r16_2(&mut self) -> u16 {
        let bits = self.current_instr[0] & 0x3;
        match bits {
            0x0 => *self.bc.as_16bit(),
            0x1 => *self.de.as_16bit(),
            0x2 => {
                let old = *self.hl.as_16bit();
                *self.hl.as_16bit() = old.wrapping_add(1);
                old
            }
            0x3 => {
                let old = *self.hl.as_16bit();
                *self.hl.as_16bit() = old.wrapping_sub(1);
                old
            }
            _ => unreachable!(),
        }
    }

    fn mut_decoded_r16_3(&mut self) -> &mut u16 {
        let bits = self.current_instr[0] & 0x3;
        match bits {
            0x0 => self.bc.as_16bit(),
            0x1 => self.de.as_16bit(),
            0x2 => self.hl.as_16bit(),
            0x3 => self.af.as_16bit(),
            _ => unreachable!(),
        }
    }

    fn calc_high_bits(&mut self) -> u8 {
        ((self.current_instr[0] & 0x3) << 1) + ((self.current_instr[1] & 0x8) >> 3)
    }

    fn decode_condition(&mut self) -> bool {
        let bits = ((self.current_instr[0] & 0x1) << 1) + ((self.current_instr[1] & 0xF) >> 3);
        match bits {
            0 => !self.get_flag(Flag::Z),
            1 => self.get_flag(Flag::Z),
            2 => !self.get_flag(Flag::C),
            3 => self.get_flag(Flag::C),
            _ => unreachable!(),
        }
    }

    fn push(&mut self, val: u16) {
        let mut mem = self.mem.borrow_mut();
        self.sp -= 1;
        mem.write_word(self.sp, (val & 0xFF) as u8);
        self.sp -= 1;
        mem.write_word(self.sp, ((val & 0xFF00) >> 8) as u8);
    }

    fn pop(&mut self) -> u16 {
        let mut mem = self.mem.borrow_mut();
        let high = mem.read_word(self.sp) as u16;
        self.sp += 1;
        let low = mem.read_word(self.sp) as u16;
        self.sp += 1;

        (high << 8) + low
    }

    fn set_pc(&mut self, mode: PcMode) {
        match mode {
            PcMode::Step(e) => self.pc += e,
            PcMode::Jump(dest) => self.pc = dest,
            PcMode::RelJump(dest) => self.pc = (self.pc as i32 + dest as i32) as u16,
        }
    }

    fn get_instr(&self) -> u8 {
        self.mem.borrow_mut().read_word(self.pc)
    }

    fn get_instr_nibbles(&self) -> [u8; 2] {
        let instr = self.get_instr();
        [(instr & 0xF0) >> 4, instr & 0x0F]
    }

    fn set_flag(&mut self, flag: Flag, val: bool) {
        let z = &mut self.af.as_8bit()[0];
        if val {
            *z |= flag as u8;
        } else {
            *z &= !(flag as u8);
        }
    }

    fn get_flag(&mut self, flag: Flag) -> bool {
        let mut z = self.af.as_8bit()[0];
        z &= flag as u8;
        z == flag as u8
    }
}

// haha yes, cursed code. on LE, a[0] is the least-significant byte.
#[derive(Copy, Clone)]
#[repr(C)]
union Register {
    a: [u8; 2],
    b: u16,
}

impl Register {
    pub fn new(b: u16) -> Self {
        Self { b }
    }

    pub fn as_8bit(&mut self) -> &mut [u8; 2] {
        unsafe { &mut self.a }
    }

    pub fn as_16bit(&mut self) -> &mut u16 {
        unsafe { &mut self.b }
    }
}
