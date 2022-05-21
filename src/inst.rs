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

#[derive(Clone)]
pub enum Inst {
    Accept,
    Uninit,
    // TODO: implement references and pointers
    #[allow(dead_code)]
    Pointer(InstrPointer),
    #[allow(dead_code)]
    Ref(InstrRef),
    Byte(InstByte),
    ByteRange(InstByteRange),
    Split(InstSplit),
    // JoinLast,
    JoinGoto(InstPtr),
}

impl fmt::Debug for Inst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        use Inst::*;
        match self {
            Accept => write!(f, "Accept"),
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
                if let Some(alternate) = range.alternate {
                    write!(f, "alt={}, ", alternate)?;
                }
                write!(f, "0x{:02x}-0x{:02x})",
                    range.range.start, range.range.end)
            }
            Split(ref split) => {
                write!(f, "Split(alt={})", split.alternate)
            }
            JoinGoto(ref addr) => {
                write!(f, "JoinGoto({})", addr)
            }
            // Join => {
            //     write!(f, "Join")
            // }
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


#[derive(Debug, Clone)]
pub enum AcceptState {
    Always,
    NeverReadUninit,
    NeverReadPrivate,
    NeverWritePrivate,
    NeverTagMismatch,
    NeverOutOfRange(RangeInclusive, RangeInclusive),
    NeverUnreachable,
    MaybeCheckRange(RangeInclusive, RangeInclusive),
}

impl AcceptState {
    pub fn always(&self) -> bool {
        matches!(self, AcceptState::Always)
    }
}

#[derive(Debug, Clone)]
pub enum StepByte {
    Uninit,
    Byte(bool, u8),
    ByteRange(bool, RangeInclusive),
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeInclusive {
    pub start: u8,
    pub end: u8,
}

impl core::convert::From<core::ops::RangeInclusive<u8>> for RangeInclusive {
    fn from(src: core::ops::RangeInclusive<u8>) -> Self {
        RangeInclusive {
            start: *src.start(),
            end: *src.end(),
        }
    }
}
impl RangeInclusive {
    pub fn contains_range(&self, small: RangeInclusive) -> bool {
        self.start <= small.start && self.end >= small.end
    }
    pub fn intersects(&self, other: RangeInclusive) -> bool {
        self.end >= other.start && self.start <= other.end
    }
}

fn accept_range(a: RangeInclusive, b: RangeInclusive) -> AcceptState {
    if a.contains_range(b) {
        AcceptState::Always
    } else if a.intersects(b) {
        AcceptState::MaybeCheckRange(a, b)
    } else {
        AcceptState::NeverOutOfRange(a, b)
    }
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
            (&Byte(false, a), &Byte(false, b)) => {
                if a != b {
                    NeverTagMismatch
                } else {
                    Always
                }
            },
            // CoverRange
            (&Byte(false, byte), &ByteRange(false, range)) => {
                accept_range((byte..=byte).into(), range)
            },
            (&ByteRange(false, range), &Byte(false, byte)) => {
                accept_range(range, (byte..=byte).into())
            }
            (&ByteRange(false, a), &ByteRange(false, b)) => {
                accept_range(a, b)
            },
        }
    }
}

#[derive(Clone)]
pub struct ProgFork {
    ip: InstPtr,
    pos: usize,
}

pub enum LayoutStep {
    Byte {
        ip: InstPtr,
        pos: usize,
        byte: StepByte
    },
    Fork(ProgFork),
}

pub struct Program {
    pub insts: Vec<Inst>,
    ip: InstPtr,
    pos: usize,
    name: &'static str,
    took_fork: Option<InstPtr>,
    current: Option<LayoutStep>,
}

// impl Clone for Program {
//     fn clone(&self) -> Self {
//         Self {
//             current: self.current.clone(),
//             ..*self
//         }
//     }
// }

impl Program {
    pub fn new(insts: Vec<Inst>, name: &'static str) -> Self {
        Self {
            insts,
            ip: 0,
            pos: 0,
            name,
            took_fork: None,
            current: None,
        }
    }
    fn positions(&self) -> Vec<usize> {
        let mut positions = (0..self.insts.len()).map(|_| 0_usize).collect::<Vec<_>>();

        let mut pos = 0;
        let mut ip = 0;
        let mut to_visit = Vec::<(InstPtr, usize)>::new();
        let mut stack = Vec::new();
        loop {
            positions[ip as usize] = pos;
            match &self.insts[ip as usize] {
                Inst::Accept => {
                    if let Some((oip, opos)) = to_visit.pop() {
                        pos = opos;
                        ip = oip;
                        continue;
                    } else {
                        break;
                    }
                }
                Inst::Split(_) => {
                    stack.push(pos);
                }
                Inst::JoinGoto(_) => {
                    pos = stack.pop().expect("invalid state");
                }
                Inst::Uninit | Inst::Byte(_) => {
                    pos += 1;
                }
                Inst::ByteRange(range) => {
                    if let Some(alt) = range.alternate {
                        to_visit.push((alt, pos));
                    }
                    pos += 1;
                }
                _ => {}
            }
            ip += 1;
        }
        positions
    }
    pub fn print_dot<W: fmt::Write>(&self, dst: &mut W) -> fmt::Result {
        let name = self.name;
        let positions = self.positions();

        write!(dst, "  {}_accepting [shape=rectangle, label=\"accepting {}\"];\n", name, name)?;
        for (ip, inst) in self.insts.iter().enumerate() {
            let ip = ip as InstPtr;
            let pos = positions[ip as usize];

            write!(dst, "  {}_ip_{} [shape=ellipse, label=\"pos={}, ip{}\"];\n", name, ip, pos, ip)?;
            match inst {
                Inst::Accept => {
                    write!(dst, "  {}_ip_{} -> {}_accepting;\n",
                        name, ip, name)?;
                }
                // Inst::Join => {
                //     write!(dst, "  {}_ip_{} -> {}_ip_{};\n",
                //         name, ip, name, ip + 1)?;
                // }
                Inst::Uninit => {
                    write!(dst, "  {}_ip_{} -> {}_ip_{} [label=\"uninit\"];\n",
                        name, ip, name, ip + 1)?;
                }
                Inst::Byte(byte) => {
                    write!(dst, "  {}_ip_{} -> {}_ip_{} [label=\"byte=0x{:02x}\"];\n",
                        name, ip, name, ip + 1, byte.byte)?;
                }
                Inst::ByteRange(range) => {
                    write!(dst, "  {}_ip_{} -> {}_ip_{} [label=\"range=0x{:02x}-0x{:02x}\"];\n",
                        name, ip, name, ip + 1, range.range.start, range.range.end)?;
                    if let Some(alt) = range.alternate {
                        write!(dst, "  {}_ip_{} -> {}_ip_{} [label=\"fork\"];\n",
                            name, ip, name, alt)?;
                    }
                }
                Inst::Split(split) => {
                    write!(dst, "  {}_ip_{} -> {}_ip_{};\n",
                        name, ip, name, split.alternate)?;
                    write!(dst, "  {}_ip_{} -> {}_ip_{};\n",
                        name, ip, name, ip + 1)?;
                }
                Inst::JoinGoto(addr) => {
                    write!(dst, "  {}_ip_{} -> {}_ip_{} [label=\"goto\"];\n",
                        name, ip, name, addr)?;
                }
                _ => { unimplemented!() }
            }
        }
        Ok(())
    }
    pub fn accept_state(&self, start: usize) -> impl Iterator<Item=AcceptState> + '_ {
        self.insts[start..].iter().map(|inst| match inst {
            Inst::Split(_) | Inst::JoinGoto(_) | Inst::Accept =>
                AcceptState::Always,
            _ => AcceptState::NeverUnreachable,
        })
    }
    pub fn synthetic_fork(&mut self, ip: Option<InstPtr>,
        accepts: AcceptState, marks: &mut Vec<AcceptState>
    ) -> (AcceptState, Option<ProgFork>) {
        let original = accepts.clone();
        let (dst, src) = match accepts {
            AcceptState::MaybeCheckRange(dst, src) => (dst, src),
            _ => { return (original, None); }
        };
        if !dst.intersects(src) {
            return (original, None);
        }
        let ip = match ip {
            Some(ip) => ip,
            _ => { return (original, None); }
        };
        let mut previous = match &self.insts[ip as usize] {
            Inst::ByteRange(range) => range.clone(),
            _ => { return (original, None); }
        };
        if src.start < dst.start {
            let missing_range = (src.start..=(dst.start - 1)).into();
            let location = self.copy_fork(ip);
            let alternate = previous.alternate.replace(location);
            self.insts[location as usize] = Inst::ByteRange(InstByteRange {
                private: previous.private,
                range: missing_range,
                alternate,
            });
            marks.extend(self.accept_state(location as usize));
        }
        if src.end > dst.end {
            let missing_range = ((dst.end + 1)..=src.end).into();
            let location = self.copy_fork(ip);
            let alternate = previous.alternate.replace(location);
            self.insts[location as usize] = Inst::ByteRange(InstByteRange {
                private: previous.private,
                range: missing_range,
                alternate,
            });
            marks.extend(self.accept_state(location as usize));
        }
        previous.range = dst;
        let fork = previous.alternate.map(|ip| ProgFork { ip, pos: self.pos });
        self.insts[ip as usize] = Inst::ByteRange(previous);
        // println!("after synthetic_fork: {:?}", self);
        (AcceptState::Always, fork)
    }
    fn copy_fork(&mut self, start: InstPtr) -> InstPtr {
        let mut depth = 0;
        let dst = self.insts.len();
        let mut pos = start as usize;
        let mut offset = (dst - pos) as InstPtr;
        let mut more_forks = Vec::new();
        loop {
            let mut inst = self.insts[pos].clone();
            match &mut inst {
                Inst::Split(ref mut split) => {
                    depth += 1;
                    split.alternate += offset;
                }
                Inst::JoinGoto(ref mut goto) => {
                    if depth == 0 {
                        pos = *goto as usize;
                        offset = (self.insts.len() - pos) as InstPtr;
                        continue;
                    }
                    depth -= 1;
                    *goto += offset;
                }
                Inst::ByteRange(ref range) => {
                    if let Some(alt) = range.alternate {
                        more_forks.push((pos, alt));
                    }
                }
                Inst::Accept => {
                    self.insts.push(inst);
                    break;
                }
                _ => {  }
            }
            self.insts.push(inst);
            pos += 1;
        }
        for (pos, alt) in more_forks {
            let dst = self.copy_fork(alt);
            match self.insts[pos] {
                Inst::ByteRange(ref mut range) => {
                    range.alternate = Some(dst);
                }
                _ => unreachable!("we should point to ByteRange")
            }
        }
        dst as InstPtr
    }
    pub fn save_fork(&self) -> ProgFork {
        // println!("{} save fork ip={} pos={}", self.name, self.ip, self.pos);
        ProgFork { ip: self.ip, pos: self.pos }
    }
    pub fn restore_fork(&mut self, fork: ProgFork) {
        // println!("{} restore fork ip={} pos={}", self.name, fork.ip, fork.pos);
        self.ip = fork.ip;
        self.pos = fork.pos;
    }
    pub fn next_fork(&mut self) -> Option<ProgFork> {
        if self.current.is_none() {
            self.advance();
        }
        match &self.current {
            Some(LayoutStep::Fork(fork)) => {
                Some(fork.clone())
            }
            _ => None
        }
    }
    pub fn next(&mut self) -> Option<LayoutStep> {
        if self.current.is_none() {
            self.advance();
        }
        self.current.take()
    }
    //  0(2 3|5 6)
    // (1 2|4 5)7
    // pub fn split_byte(&mut self) -> Option<>
    fn advance(&mut self) {
        while let Some(inst) = self.insts.get(self.ip as usize) {
            // print!("{} ip={} inst={:?} ", self.name, self.ip, inst);
            // println!("stack={:?}", self.stack);
            let rv = match inst {
                Inst::Accept => {
                    self.current = None;
                    return;
                },
                Inst::ByteRange(ref range) => {
                    if let Some(alternate) = range.alternate {
                        if self.took_fork.take() == Some(self.ip) {
                            Some(LayoutStep::Byte {
                                ip: self.ip,
                                pos: self.pos,
                                byte: StepByte::ByteRange(range.private, range.range)
                            })
                        } else {
                            self.took_fork = Some(self.ip);
                            self.current = Some(LayoutStep::Fork(ProgFork {
                                ip: alternate,
                                pos: self.pos,
                            }));
                            return;
                        }
                    } else {
                        Some(LayoutStep::Byte {
                            ip: self.ip,
                            pos: self.pos,
                            byte: StepByte::ByteRange(range.private, range.range)
                        })
                    }
                }
                Inst::Uninit => {
                    Some(LayoutStep::Byte {
                        ip: self.ip,
                        pos: self.pos,
                        byte: StepByte::Uninit
                    })
                },
                Inst::Split(ref split) => {
                    Some(LayoutStep::Fork(ProgFork {
                        ip: split.alternate,
                        pos: self.pos,
                    }))
                }
                Inst::Byte(ref byte) => {
                    Some(LayoutStep::Byte {
                        ip: self.ip,
                        pos: self.pos,
                        byte: StepByte::Byte(byte.private, byte.byte)
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
                &Inst::JoinGoto(addr) => {
                    self.ip = addr;
                    continue;
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

impl<'a> fmt::Debug for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "FiniteAutomaton {{")?;
        for (idx, inst) in self.insts.iter().enumerate() {
            writeln!(f, "  {:03} {:?}", idx, inst)?;
        }
        writeln!(f, "}}")?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct InstrPointer {
    pub pointer_size: u32,
    pub data_align: u32,
}

// TODO: implement references and pointers
#[allow(dead_code)]
#[derive(Clone)]
pub enum RefKind {
    Shared,
    Unique,
}

#[derive(Clone)]
pub struct InstrRef {
    pub ref_type: RefKind,
    pub pointer_size: u32,
    pub data_align: u32,
}
#[derive(Clone)]
pub struct InstSplit {     
    pub alternate: InstPtr,
}

#[derive(Clone)]
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


#[derive(Clone)]
pub struct InstByteRange {
    pub private: bool,
    pub range: RangeInclusive,
    pub alternate: Option<InstPtr>,
}
