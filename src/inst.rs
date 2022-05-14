use core::fmt;

use crate::ty::Endian;

fn write_target_uint(endianness: Endian, target: &mut [u8], data: u128) {
    // This u128 holds an "any-size uint" (since smaller uints can fits in it)
    // So we do not write all bytes of the u128, just the "payload".
    let len = target.len();
    match endianness {
        Endian::Little => target.copy_from_slice(&data.to_le_bytes()[..len]),
        Endian::Big => target.copy_from_slice(&data.to_be_bytes()[16 - len..]),
    };
}

pub type InstPtr = u32;

pub enum Inst {
    Uninit,
    // TODO: implement references and pointers
    #[allow(dead_code)]
    Pointer(InstrPointer),
    #[allow(dead_code)]
    Ref(InstrRef),
    Byte(InstByte),
    ByteRange(InstByteRange),
    Split(InstSplit),
    JoinLast,
    JoinGoto(InstPtr),
}

impl fmt::Debug for Inst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        use Inst::*;
        match self {
            Uninit => write!(f, "Uninit"),
            Pointer(ref ptr) => {
                write!(f, "Pointer(pointer_size={}, data_align={})",
                    ptr.data_align, ptr.pointer_size)
            }
            Ref(ref d_ref) => {
                let ref_type = match &d_ref.ref_type {
                    RefKind::Shared => "Shared",
                    RefKind::Unique => "Unique",
                };
                write!(f, "Ref(type={}, data_align={})",
                    ref_type, d_ref.data_align)
            }
            Byte(ref byte) => {
                write!(f, "Byte(")?;
                if byte.private {
                    write!(f, "private, ")?;
                }
                write!(f, "{:02x})", byte.byte)
            }
            ByteRange(ref range) => {
                write!(f, "ByteRange(")?;
                if range.private {
                    write!(f, "private, ")?;
                }
                write!(f, "0x{:02x}-0x{:02x})", range.range.0, range.range.1)
            }
            Split(ref split) => {
                write!(f, "Split(alt={})", split.alternate)
            }
            JoinLast => write!(f, "JoinLast"),
            JoinGoto(ref addr) => {
                write!(f, "JoinGoto({})", addr)
            }
        }
    }
}

// representation of unions:
// split (labelb, end) aaaaaaaa (GroupEnd) bbbbbbbbbbbbb
//                                        ^ labelb      ^ end

impl Inst {
    pub fn new_invalid_split() -> Self {
        Inst::Split(InstSplit {
            alternate: InstPtr::MAX,
        })
    }
    pub fn new_invalid_goto() -> Self {
        Inst::JoinGoto(InstPtr::MAX)
    }
    pub fn patch_split(&mut self, alternate: InstPtr) {
        match self {
            Inst::Split(ref mut split) => {
                split.alternate = alternate;
            }
            _ => panic!("invalid use of patch_split")
        }
    }
    pub fn patch_goto(&mut self, addr: InstPtr) {
        match self {
            Inst::JoinGoto(ref mut goto) => {
                *goto = addr
            }
            _ => panic!("invalid use of patch_goto")
        }
    }
}


#[derive(Debug)]
pub enum AcceptState {
    Always,
    NeverReadUninit,
    NeverReadPrivate,
    NeverWritePrivate,
    NeverTagMismatch,
    NeverOutOfRange,
    MaybeCheckRange,
}

#[derive(Debug, Clone, Copy)]
pub enum StepByte {
    Uninit,
    Byte(bool, u8),
    ByteRange(bool, (u8, u8)),
}

fn range_contain((lo, hi): (u8, u8), byte: u8) -> bool {
    byte >= lo && byte <= hi
}

fn ranges_within(big: (u8, u8), small: (u8, u8)) -> bool {
    big.0 <= small.0 && big.1 >= small.1
}

impl StepByte {
    pub fn accepts(&self, source: &StepByte) -> AcceptState {
        use StepByte::*;
        use AcceptState::*;
        match (self, source) {
            // Uninit bytes can accpet anything
            (Uninit, _) => Always,
            // Nothing can accept uninit
            (_, Uninit) => NeverReadUninit,
            // Cannot write private memory
            (&ByteRange(true, _), _) | (&Byte(true, _), _) => {
                NeverWritePrivate
            }
            // Cannot read private memory
            (_, &ByteRange(true, _)) | (_, &Byte(true, _)) => {
                NeverReadPrivate
            }
            // Constant tags must match
            (Byte(false, a), Byte(false, b)) => {
                if a != b {
                    NeverTagMismatch
                } else {
                    Always
                }
            },
            // CoverRange
            (Byte(false, byte), ByteRange(false, range)) => {
                if range_contain(*range, *byte) {
                    MaybeCheckRange
                } else {
                    NeverOutOfRange
                }
            },
            (ByteRange(false, range), Byte(false, byte)) => {
                if range_contain(*range, *byte) {
                    Always
                } else {
                    NeverOutOfRange
                }
            }
            (ByteRange(false, a), ByteRange(false, b)) => {
                if ranges_within(*a, *b) {
                    Always
                } else {
                    MaybeCheckRange
                }
            },
        }
    }
}

pub enum LayoutStep<P> {
    Byte {
        ip: InstPtr,
        pos: usize,
        byte: StepByte
    },
    Fork(P),
    Join(bool),
}
impl<P> LayoutStep<P> {
    pub fn map_fork<F, D>(self, f: F) -> LayoutStep<D>
        where F: Fn(P) -> D,
    {
        match self {
            LayoutStep::Byte { ip, pos, byte } =>
                LayoutStep::Byte { ip, pos, byte },
            LayoutStep::Fork(p) => LayoutStep::Fork(f(p)),
            LayoutStep::Join(l) => LayoutStep::Join(l),
        }
    }
}

impl<P> Clone for LayoutStep<P>
    where P: Copy
{
    fn clone(&self) -> Self {
        match self {
            &LayoutStep::Byte { ip, pos, byte } =>
                LayoutStep::Byte { ip, pos, byte },
            &LayoutStep::Fork(ip) => LayoutStep::Fork(ip),
            &LayoutStep::Join(l) => LayoutStep::Join(l),
        }
    }
}


// pub type ProgramB<'a> = Program<'a, Box<[u8]>, Box<[(u8, u8)]>>;

pub struct Program<'a> {
    pub insts: &'a [Inst],
    pub ip: InstPtr,
    pub pos: usize,
    name: &'static str,
    current: Option<LayoutStep<InstPtr>>,
}

impl<'a> Clone for Program<'a> {
    fn clone(&self) -> Self {
        Self {
            current: self.current.clone(),
            ..*self
        }
    }
}

impl<'a> Program<'a> {
    pub fn new(insts: &'a[Inst], name: &'static str) -> Self {
        Self {
            insts,
            ip: 0,
            pos: 0,
            name,
            current: None,
        }
    }
    fn fork(&self, ip: InstPtr) -> Self {
        Self {
            insts: self.insts,
            ip,
            pos: self.pos,
            name: self.name,
            current: None,
        }
    }
    pub fn next_fork(&mut self) -> Option<Self> {
        if self.current.is_none() {
            self.advance();
        }
        match &self.current {
            &Some(LayoutStep::Fork(ip)) => {
                self.current = None;
                Some(self.fork(ip))
            }
            _ => None
        }
    }
    pub fn next_join(&mut self) -> Option<bool> {
        if self.current.is_none() {
            self.advance();
        }
        match &self.current {
            &Some(LayoutStep::Join(last)) => {
                self.current = None;
                Some(last)
            }
            _ => None
        }
    }
    // pub fn peek(&mut self, stack: &mut Vec<StackEntry>) -> Option<LayoutStep<Self>> {
    //     if self.current.is_none() {
    //         self.advance(stack);
    //     }
    //     self.current.clone().map(|step| {
    //         step.map_fork(|ip| self.fork(ip))
    //     })
    // }
    pub fn next(&mut self) -> Option<LayoutStep<Self>> {
        if self.current.is_none() {
            self.advance();
        }
        self.current.take().map(|step| {
            step.map_fork(|ip| self.fork(ip))
        })
    }
    //  0(2 3|5 6)
    // (1 2|4 5)7
    // pub fn split_byte(&mut self) -> Option<>
    fn advance(&mut self) {
        while let Some(inst) = self.insts.get(self.ip as usize) {
            // print!("{} ip={} inst={:?} ", self.name, self.ip, inst);
            // println!("stack={:?}", self.stack);
            let rv = match inst {

                Inst::Uninit => {
                    Some(LayoutStep::Byte {
                        ip: self.ip,
                        pos: self.pos,
                        byte: StepByte::Uninit
                    })
                },
                Inst::Split(ref split) => {
                    Some(LayoutStep::Fork(split.alternate))
                }
                Inst::Byte(ref byte) => {
                    Some(LayoutStep::Byte {
                        ip: self.ip,
                        pos: self.pos,
                        byte: StepByte::Byte(byte.private, byte.byte)
                    })
                }
                Inst::ByteRange(ref range) => {
                    Some(LayoutStep::Byte {
                        ip: self.ip,
                        pos: self.pos,
                        byte: StepByte::ByteRange(range.private, range.range)
                    })
                }
                Inst::Ref(ref _ref) => {
                    println!("ref unimplemented");
                    None
                }
                Inst::Pointer(ref _ptr) => {
                    println!("ptr unimplemented");
                    None
                }
                Inst::JoinLast => {
                    Some(LayoutStep::Join(true))
                }
                &Inst::JoinGoto(addr) => {
                    self.ip = addr;
                    Some(LayoutStep::Join(false))
                }
            };
            self.ip += 1;
            if matches!(rv, Some(LayoutStep::Byte {..})) {
                self.pos += 1;
            }
            self.current = rv;
            if self.current.is_some() {
                return;
            }
        }
    }
}

impl<'a> fmt::Debug for Program<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "FiniteAutomaton {{")?;
        for (idx, inst) in self.insts.iter().enumerate() {
            writeln!(f, "  {:03} {:?}", idx, inst)?;
        }
        writeln!(f, "}}")?;
        Ok(())
    }
}

pub struct InstrPointer {
    pub pointer_size: u32,
    pub data_align: u32,
}

// TODO: implement references and pointers
#[allow(dead_code)]
pub enum RefKind {
    Shared,
    Unique,
}

pub struct InstrRef {
    pub ref_type: RefKind,
    pub pointer_size: u32,
    pub data_align: u32,
}
pub struct InstSplit {     
    pub alternate: InstPtr,
}

pub struct InstByte {
    pub private: bool,
    pub byte: u8,
}

impl InstByte {
    pub fn for_literal(
        endian: Endian, size: usize,
        value: u128, private: bool)
    -> impl Iterator<Item=Inst> {
        let mut data = [0_u8; 16];
        let start = data.len() - size;
        write_target_uint(endian, &mut data[start..], value);
        LiteralBytes {
            data,
            private,
            pos: start,
        }
    }
}

struct LiteralBytes {
    data: [u8; 16],
    private: bool,
    pos: usize,
}

impl Iterator for LiteralBytes {
    type Item=Inst;
    fn next(&mut self) -> Option<Self::Item> {
        let byte = *self.data.get(self.pos)?;
        let private = self.private;
        self.pos += 1;
        Some(Inst::Byte(InstByte { private, byte }))
    }
}

pub struct InstByteRange {
    pub private: bool,
    pub range: (u8, u8),
}
