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

pub enum Inst<B, R>
    where B: AsRef<[u8]>,
          R: AsRef<[(u8, u8)]>
{
    GroupEnd,
    Uninit,
    // TODO: implement references and pointers
    #[allow(dead_code)]
    Pointer(InstrPointer),
    #[allow(dead_code)]
    Ref(InstrRef),
    Bytes(InstBytes<B>),
    ByteRanges(InstByteRanges<R>),
    Split(InstSplit),
    Repeat(InstRepeat),
}

// representation of unions:
// split (labelb, end) aaaaaaaa (GroupEnd) bbbbbbbbbbbbb
//                                        ^ labelb      ^ end

impl<B, R> Inst<B, R>
    where B: AsRef<[u8]>,
          R: AsRef<[(u8, u8)]>
{
    pub fn new_invalid_split() -> Self {
        Inst::Split(InstSplit {
            alternate: u32::MAX,
            end: u32::MAX,
        })
    }
    pub fn patch_split(&mut self, alternate: InstPtr) {
        match self {
            Inst::Split(ref mut split) => {
                split.alternate = alternate;
            }
            _ => panic!("invalid use of patch_split")
        }
    }
    pub fn patch_split_end(&mut self, end: InstPtr) {
        match self {
            Inst::Split(ref mut split) => {
                split.end = end;
            }
            _ => panic!("invalid use of patch_split")
        }
    }
}

pub type InstB = Inst<Box<[u8]>, Box<[(u8, u8)]>>;

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

#[derive(Debug)]
pub enum StepByte<'a> {
    Uninit,
    Bytes(bool, &'a [u8]),
    ByteRanges(bool, &'a [(u8, u8)]),
}

fn ranges_contain(ranges: &[(u8, u8)], byte: u8) -> bool {
    ranges.iter().any(|&(start, end)| byte >= start && byte <= end)
}

fn ranges_within(big: &[(u8, u8)], small: &[(u8, u8)]) -> bool {
    small.iter().all(|(small_s, small_e)| {
        big.iter().any(|(big_s, big_e)| {
            big_s <= small_s && big_e >= small_e
        })
    })
}

impl<'a> StepByte<'a> {
    pub fn accepts<'b>(&self, source: &StepByte<'b>) -> AcceptState {
        use StepByte::*;
        use AcceptState::*;
        match (self, source) {
            // Uninit bytes can accpet anything
            (Uninit, _) => Always,
            // Nothing can accept uninit
            (_, Uninit) => NeverReadUninit,
            // Cannot write private memory
            (&ByteRanges(true, _), _) | (&Bytes(true, _), _) => {
                NeverWritePrivate
            }
            // Cannot read private memory
            (_, &ByteRanges(true, _)) | (_, &Bytes(true, _)) => {
                NeverReadPrivate
            }
            // Constant tags must match
            (Bytes(false, a), Bytes(false, b)) => {
                if a != b {
                    NeverTagMismatch
                } else {
                    Always
                }
            },
            // CoverRange
            (Bytes(false, bytes), ByteRanges(false, ranges)) => {
                if ranges_contain(ranges, bytes[0]) {
                    MaybeCheckRange
                } else {
                    NeverOutOfRange
                }
            },
            (ByteRanges(false, ranges), Bytes(false, bytes)) => {
                if ranges_contain(ranges, bytes[0]) {
                    Always
                } else {
                    NeverOutOfRange
                }
            }
            (ByteRanges(false, a), ByteRanges(false, b)) => {
                if ranges_within(a, b) {
                    Always
                } else {
                    MaybeCheckRange
                }
            },
        }
    }
}

pub enum LayoutStep<'a> {
    Byte {
        ip: InstPtr,
        pos: usize,
        byte: StepByte<'a>
    },
    Fork(InstPtr),
    Join,
}

#[derive(Clone)]
enum StackEntry {
    Repeat { start: InstPtr, remaining: u32 },
    Split { end: InstPtr },
    Join,
}

pub struct Program<'a, B, R>
    where B: AsRef<[u8]>,
          R: AsRef<[(u8, u8)]>
{
    pub insts: &'a [Inst<B, R>],
    pub ip: InstPtr,
    pub pos: usize,
    pending: &'a [u8],
    stack: Vec<StackEntry>,
}
impl<'a, B, R> Program<'a, B, R>
    where B: AsRef<[u8]>,
          R: AsRef<[(u8, u8)]>
{
    pub fn new(insts: &'a[Inst<B,R>]) -> Self {
        Self {
            insts,
            ip: 0,
            pos: 0,
            stack: Vec::new(),
            pending: &[],
        }
    }
    pub fn fork(&self, ip: InstPtr) -> Self {
        let mut fork = Self {
            insts: self.insts,
            ip,
            pos: self.pos,
            stack: self.stack.clone(),
            pending: self.pending,
        };
        fork.stack.push(StackEntry::Join);
        fork
    }
    // pub fn split_byte(&mut self) -> Option<>
    pub fn next_step(&mut self) -> Option<LayoutStep> {
        while let Some(inst) = self.insts.get(self.ip as usize) {
            let rv = match inst {
                Inst::GroupEnd => {
                    use StackEntry::*;
                    let stack_entry = self.stack.pop()
                        .expect("invalid state");
                    match stack_entry {
                        Repeat { start, remaining } => {
                            if remaining > 1 {
                                self.ip = start;
                                self.stack.push(Repeat {
                                    start,
                                    remaining: remaining - 1,
                                });
                            }
                            None
                        }
                        Split { end } => {
                            self.ip = end;
                            Some(LayoutStep::Join)
                        }
                        Join => Some(LayoutStep::Join),
                    }
                }
                Inst::Uninit => {
                    Some(LayoutStep::Byte {
                        ip: self.ip,
                        pos: self.pos,
                        byte: StepByte::Uninit
                    })
                },
                Inst::Repeat(ref repeat) => {
                    use StackEntry::*;
                    self.stack.push(Repeat {
                        start: self.ip,
                        remaining: repeat.count,
                    });
                    None
                }
                Inst::Split(ref split) => {
                    use StackEntry::*;
                    self.stack.push(Split { end: split.end });
                    Some(LayoutStep::Fork(split.alternate))
                }
                Inst::Bytes(ref bytes) => {
                    self.pending = bytes.bytes.as_ref();
                    Some(LayoutStep::Byte {
                        ip: self.ip,
                        pos: self.pos,
                        byte: StepByte::Bytes(bytes.private, bytes.bytes.as_ref())
                    })
                }
                Inst::ByteRanges(ref ranges) => {
                    Some(LayoutStep::Byte {
                        ip: self.ip,
                        pos: self.pos,
                        byte: StepByte::ByteRanges(ranges.private, ranges.ranges.as_ref())
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
            };
            self.ip += 1;
            if matches!(rv, Some(LayoutStep::Byte {..})) {
                self.pos += 1;
            }
            if rv.is_some() {
                return rv;
            }
        }
        None
    }
}

use core::fmt;
impl<'a, B, R> fmt::Debug for Program<'a, B, R>
    where B: AsRef<[u8]>,
          R: AsRef<[(u8, u8)]>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "FiniteAutomaton {{")?;
        for (idx, inst) in self.insts.iter().enumerate() {
            use Inst::*;
            write!(f, "  {:03} ", idx)?;
            match inst {
                GroupEnd => { writeln!(f, "GroupEnd")?; },
                Uninit => { writeln!(f, "Uninit")?; },
                Pointer(ref ptr) => {
                    writeln!(f, "Pointer(pointer_size={}, data_align={})",
                        ptr.data_align, ptr.pointer_size)?;
                }
                Ref(ref d_ref) => {
                    let ref_type = match &d_ref.ref_type {
                        RefKind::Shared => "Shared",
                        RefKind::Unique => "Unique",
                    };
                    writeln!(f, "Ref(type={}, data_align={})",
                        ref_type, d_ref.data_align)?;
                }
                Bytes(ref bytes) => {
                    write!(f, "Bytes(")?;
                    if bytes.private {
                        write!(f, "private, ")?;
                    }
                    for (idx, &byte) in bytes.bytes.as_ref().iter().enumerate() {
                        let sep = if idx != 0 { " " } else { "" };
                        write!(f, "{}{:02x}", sep, byte)?;
                    }
                    writeln!(f, ")")?;
                }
                ByteRanges(ref ranges) => {
                    write!(f, "ByteRanges(")?;
                    if ranges.private {
                        write!(f, "private, ")?;
                    }
                    for (idx, &(start, end)) in ranges.ranges.as_ref().iter().enumerate() {
                        let sep = if idx != 0 { ", " } else { "" };
                        write!(f, "{}0x{:02x}-0x{:02x}", sep, start, end)?;
                    }
                    writeln!(f, ")")?;
                }
                Split(ref split) => {
                    writeln!(f, "Split(alt={}, end={})",
                        split.alternate, split.end)?;
                }
                Repeat(ref repeat) => {
                    writeln!(f, "Repeat({})", repeat.count)?;
                }
            };
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

pub struct InstBytes<B>
    where B: AsRef<[u8]>
{
    pub private: bool,
    pub bytes: B,
}
impl<B> InstBytes<B>
    where B: AsRef<[u8]>
{
    pub fn with_private(mut self, private: bool) -> Self {
        self.private = private;
        self
    }
}

impl<B> InstBytes<B>
    where B: From<Vec<u8>> + AsRef<[u8]>
{
    pub fn for_literal(endian: Endian, size: usize, value: u128) -> impl Iterator<Item=Self> {
        let mut data = [0_u8; 16];
        write_target_uint(endian, &mut data[..size], value);
        core::iter::once(InstBytes {
            private: false,
            bytes: data[..size].to_vec().into(),
        }).chain(None)
    }
}

pub struct InstByteRanges<R>
    where R: AsRef<[(u8, u8)]>
{
    pub private: bool,
    pub ranges: R,
}

pub struct InstSplit {
    pub alternate: InstPtr,
    pub end: InstPtr,
}

pub struct InstRepeat {
    pub count: u32,
}
