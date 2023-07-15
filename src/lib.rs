use std::io::{self, Read, Stdout, Write};
use std::process;
use std::sync::mpsc::{self, Receiver as Rx, TryRecvError};

#[repr(usize)]
#[derive(Debug, Clone, Copy)]
pub enum Op {
    BR,   /* branch */
    ADD,  /* add  */
    LD,   /* load */
    ST,   /* store */
    JSR,  /* jump register */
    AND,  /* bitwise and */
    LDR,  /* load register */
    STR,  /* store register */
    RTI,  /* return from interrupt */
    NOT,  /* bitwise not */
    LDI,  /* load indirect */
    STI,  /* store indirect */
    JMP,  /* jump */
    RES,  /* reserved (unused) */
    LEA,  /* load effective address */
    TRAP, /* execute trap */
}

use Op::*;
pub(crate) const TO_OP: [Op; 16] = [
    BR,   /* branch */
    ADD,  /* add  */
    LD,   /* load */
    ST,   /* store */
    JSR,  /* jump register */
    AND,  /* bitwise and */
    LDR,  /* load register */
    STR,  /* store register */
    RTI,  /* return from interrupt */
    NOT,  /* bitwise not */
    LDI,  /* load indirect */
    STI,  /* store indirect */
    JMP,  /* jump */
    RES,  /* reserved (unused) */
    LEA,  /* load effective address */
    TRAP, /* execute trap */
];

#[repr(u16)]
#[derive(Clone, Copy)]
enum Cond {
    N = 0b100,
    Z = 0b010,
    P = 0b001,
}

const PC: usize = 8;
const PSR: usize = 9;

const KBSR: u16 = 0xfe00;
const KBDR: u16 = 0xfe02;
const DSR: u16 = 0xfe04;
const DDR: u16 = 0xfe06;
const MCR: u16 = 0xfffe;

const MEM: usize = u16::MAX as usize + 1;

struct Interrupt {
    vector: u16,
    priority: u16,
}

pub struct Vm {
    mem: [u16; MEM],
    reg: [u16; 10],
    status: [usize; 0x100],
    usp: u16,
    rx: (Rx<Interrupt>, Rx<u8>),
    out: Stdout,
}

impl Vm {
    pub fn new() -> Self {
        use std::thread;

        let (sx, rx) = mpsc::channel();
        let (sx_stdin, rx_stdin) = mpsc::channel();

        thread::spawn(move || {
            let mut b = io::stdin().bytes();
            let mut stop = false;

            while !stop {
                stop = sx_stdin.send(b.next().unwrap().unwrap()).is_err()
                    || sx
                        .send(Interrupt {
                            vector: 0x80,
                            priority: 4,
                        })
                        .is_err();
            }
        });

        let mut reg = [0; 10];

        reg[6] = KBSR;
        reg[PC] = 0x3000;
        reg[PSR] = Cond::Z as u16 | (1 << 15);

        let mut mem = [0; MEM];

        mem[DSR as usize] = 1 << 15;
        mem[0xfffe] = 1 << 15;

        let mut status = [0; 0x100];

        status[0x80] = KBSR as usize;

        Self {
            usp: reg[6],
            mem,
            reg,
            status,
            rx: (rx, rx_stdin),
            out: io::stdout(),
        }
    }

    pub fn load_image(&mut self, f: &str) {
        use std::fs::File;

        let mut f = File::open(f).expect("error opening image file").bytes();

        let mut idx = u16::from_be_bytes([f.next().unwrap().unwrap(), f.next().unwrap().unwrap()]);

        let mut left = None;

        for i in f {
            let i = i.expect("error loading image into memory");

            if let Some(x) = left.take() {
                self.store(idx, u16::from_be_bytes([x, i]));
                idx += 1;
            } else {
                left = Some(i);
            }
        }
    }

    pub fn run(&mut self) {
        while self.mem[MCR as usize] >> 15 > 0 {
            let (opcode, operand) = self.load_next_instr();
            let dr = operand >> 9;
            let sr = (operand >> 6) & 7;
            let pcoffset9 = self.calci(operand & 0x1ff, 9);

            match opcode {
                Op::BR => {
                    if dr & self.reg[PSR] > 0 {
                        self.jmp(pcoffset9);
                    }
                }
                Op::ADD => {
                    let sr2 = if (operand >> 5) & 1 > 0 {
                        sext(operand & 0b11111, 5)
                    } else {
                        self.reg(operand)
                    };

                    self.reg[dr as usize] = self.reg(sr).wrapping_add(sr2);

                    self.setcc(dr);
                }
                Op::AND => {
                    let sr2 = if (operand >> 5) & 1 > 0 {
                        sext(operand & 0b11111, 5)
                    } else {
                        self.reg(operand)
                    };

                    self.reg[dr as usize] = self.reg(sr) & sr2;

                    self.setcc(dr);
                }
                Op::NOT => {
                    self.reg[dr as usize] = !self.reg(sr);

                    self.setcc(dr);
                }

                Op::LD => {
                    self.reg[dr as usize] = self.load(pcoffset9);
                    self.setcc(dr);
                }
                Op::LDI => {
                    let addr = self.load(pcoffset9);
                    self.reg[dr as usize] = self.load(addr);
                    self.setcc(dr);
                }
                Op::LDR => {
                    let offset = sext(operand & 0x3f, 6);
                    let addr = self.reg(sr).wrapping_add(offset);

                    self.reg[dr as usize] = self.load(addr);
                    self.setcc(dr);
                }
                Op::LEA => {
                    self.reg[dr as usize] = pcoffset9;
                    self.setcc(dr);
                }

                Op::ST => {
                    self.store(pcoffset9, self.reg(dr));
                }
                Op::STI => {
                    let addr = self.load(pcoffset9);
                    self.store(addr, self.reg(dr));
                }
                Op::STR => {
                    let offset = sext(operand & 0x3f, 6);

                    self.store(self.reg(sr).wrapping_add(offset), self.reg(dr))
                }

                Op::JMP => self.jmp(self.reg(sr)),
                Op::JSR => {
                    self.reg[7] = self.reg[PC];
                    let addr = if operand >> 11 == 0 {
                        self.reg(sr)
                    } else {
                        self.calci(operand & 0x7ff, 11)
                    };

                    self.jmp(addr)
                }
                Op::RTI => {
                    if self.reg[PSR] >> 15 == 0 {
                        self.reg[PC] = self.load(self.reg[6]);
                        self.reg[PSR] = self.load(self.reg[6] + 1);

                        if self.reg[PSR] >> 15 > 0 {
                            self.reg[6] = self.usp;
                        }

                        self.reg[6] += 2;
                    } else {
                        self.interrupt::<false>(0, 0x00);
                    }
                }
                Op::TRAP => {
                    self.reg[7] = self.reg[PC];
                    self.jmp(self.mem[(operand & 0xff) as usize]);
                }

                Op::RES => {
                    self.interrupt::<false>(0, 0x01);
                }
            }

            match self.rx.0.try_recv() {
                Ok(Interrupt { priority, vector }) => {
                    if priority & 7 > (self.reg[PSR] >> 8) & 0x7
                        && self.mem[self.status[vector as usize & 0xff]] & 1 >> 14 > 0
                    {
                        self.interrupt::<true>(priority & 7, vector & 0xff);
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(_disconnected) => process::exit(-1),
            }
        }
    }

    fn interrupt<const I: bool>(&mut self, priority: u16, v: u16) {
        if self.reg[PSR] >> 15 > 0 {
            self.usp = self.reg[6];
            self.reg[6] = 0x3000;
        }

        self.reg[6] -= 1;
        self.store(self.reg[6], self.reg[PSR]);

        self.reg[6] -= 1;
        self.store(self.reg[6], self.reg[PC]);

        self.reg[PSR] = if I {
            (priority & 7) << 8
        } else {
            self.reg[PSR] & !0b111
        } | Cond::Z as u16;

        self.reg[PC] = self.load(v as u16 | 0x100);
    }

    fn load_next_instr(&mut self) -> (Op, u16) {
        let instr = self.load(self.reg[PC]);
        let res = TO_OP[(instr >> 12) as usize];
        self.reg[PC] += 1;
        (res, instr & 0xfff)
    }

    fn setcc(&mut self, r: u16) {
        let r = self.reg(r) as usize;

        self.reg[PSR] &= !0b111;

        self.reg[PSR] |= if r == 0 {
            Cond::Z as u16
        } else {
            [Cond::P, Cond::N][r >> 15] as u16
        };
    }

    fn jmp(&mut self, add: u16) {
        self.reg[PC] = add;
    }

    fn calci(&self, i: u16, s: u16) -> u16 {
        self.reg[PC].wrapping_add(sext(i, s))
    }

    fn reg(&self, r: u16) -> u16 {
        self.reg[(r & 7) as usize]
    }

    fn store(&mut self, addr: u16, i: u16) {
        if addr <= KBSR || addr == MCR {
            self.mem[addr as usize] = i;
        } else if addr == DDR {
            self.out.write_all(&[i as u8]).unwrap();
            self.out.flush().unwrap();
        }
    }

    fn load(&mut self, addr: u16) -> u16 {
        if addr < KBSR {
            self.mem[addr as usize]
        } else {
            if addr == KBSR {
                let available = self.rx.1.try_recv();

                self.mem[KBSR as usize] = match available {
                    Ok(x) => {
                        self.mem[KBDR as usize] = x.into();
                        1 << 15
                    }

                    Err(TryRecvError::Empty) => 0,

                    Err(_disconnected) => process::exit(-1),
                };
            }

            self.mem[addr as usize]
        }
    }
}

fn sext(inp: u16, s: u16) -> u16 {
    if inp >> (s - 1) > 0 {
        inp | (0xffff << s)
    } else {
        inp
    }
}
