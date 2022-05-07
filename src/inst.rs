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

pub struct Program {
    pub insts: Vec<InstB>,
}

use core::fmt;
impl fmt::Debug for Program {
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
                    for (idx, &byte) in bytes.bytes.iter().enumerate() {
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
                    for (idx, &(start, end)) in ranges.ranges.iter().enumerate() {
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
