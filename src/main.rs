#![feature(alloc_layout_extra)]
#![feature(untagged_unions)]
mod compiler;
mod derive;
mod inst;
mod print;
mod ty;
// use print::Printer;
use crate::print::Printer;
use derive::{InspectTy, derive_ty};
use crate::compiler::Compiler;
use crate::inst::{InstPtr, Program, ProgFork, StepByte, LayoutStep, AcceptState};
use crate::ty::*;


struct ExecFork {
    dst: ProgFork,
    src: ProgFork,
}

#[allow(dead_code)]
#[derive(Debug)]
struct Reject {
    src: Option<InstPtr>,
    dst: InstPtr,
    reason: AcceptState,
}

struct Execution {
    forks: Vec<ExecFork>,
    accept: Vec<AcceptState>,
    reject: Vec<Reject>,
    dst: Program,
    src: Program,
}

impl Execution {
    fn new(dst: Program, src: Program) -> Self {
        Self {
            forks: Vec::new(),
            accept: src.accept_state(0).collect(),
            reject: Vec::new(),
            dst,
            src,
        }
    }
    fn pop_fork(&mut self) -> bool {
        if let Some(fork) = self.forks.pop() {
            self.src.restore_fork(fork.src);
            self.dst.restore_fork(fork.dst);
            true
        } else {
            false
        }
    }
    fn check(&mut self) {
        loop {
            let src_fork = self.src.save_fork();
            let dst_fork = self.dst.save_fork();
            if let Some(next_src) = self.src.next_fork() {
                // println!("fork src");
                self.src.next();
                self.forks.push(ExecFork {
                    dst: self.dst.save_fork(),
                    src: next_src,
                });
                continue;
            }
            if let Some(next_dst) = self.dst.next_fork() {
                // println!("fork dst");
                self.dst.next();
                self.forks.push(ExecFork {
                    dst: next_dst,
                    src: src_fork,
                });
                continue;
            }

            let (s_ip, byte_src) = match self.src.next() {
                None => (None, StepByte::Uninit),
                Some(LayoutStep::Byte { ip, byte, .. }) => {
                    (Some(ip), byte)
                }
                _ => unreachable!("peek and next must match")
            };
            let (d_ip, byte_dst) = match self.dst.next() {
                None => {
                    if self.pop_fork() { continue }
                    else { break }
                }
                Some(LayoutStep::Byte { ip, byte, .. }) => (ip, byte),
                _ => unreachable!("peek and next must match")
            };
            println!("dst={}, src={:?}", d_ip, s_ip);
            if s_ip.map_or(false, |ip| self.accept[ip as usize].always()) {
                if self.pop_fork() { continue }
                else { break }
            }
            let accepts = byte_dst.accepts(&byte_src);
            let (accepts, fork) = self.src.synthetic_fork(s_ip, accepts, &mut self.accept);
            if let Some(src_fork) = fork {
                self.forks.push(ExecFork {
                    dst: dst_fork,
                    src: src_fork,
                });
            }
            if let Some(ip) = s_ip {
                println!("accepts={:?}", accepts);
                self.accept[ip as usize] = accepts.clone();
            }
            if !accepts.always() {
                self.reject.push(Reject {
                    src: s_ip,
                    dst: d_ip,
                    reason: accepts,
                });
                if self.pop_fork() { continue }
                else { break }
            }
        }
        println!("dst: {:?}\nsrc: {:?}", self.dst, self.src);
        let mut dot = "digraph q {\n".to_string();
        self.dst.print_dot(&mut dot).unwrap();
        self.src.print_dot(&mut dot).unwrap();
        dot.push_str("}\n");
        println!("{}", dot);
        println!("accept: {:?}", self.accept);
        self.reject.retain(|rej| {
            let src = match rej.src {
                Some(src) => src,
                None => { return false; },
            };
            !self.accept[src as usize].always()
        });
        println!("reject: {:?}", self.reject);
    }
}

#[allow(dead_code)]
fn main() {
    // #[allow(dead_code)]
    // {
    //     use core::alloc::Layout;
    //     derive_ty!(#[repr(C)] struct Foo {
    //         b: u8,
    //         a: [u32; 0]
    //     });
    //     println!("real layout: {:?}", Layout::new::<Foo>());
    //     let ty = Foo::ty_of();
    //     let mut compiler = Compiler::new(Endian::Little);
    //     compiler.extend_from_ty(&ty);
    //     println!("comp layout: {:?}", compiler.layout);
    // }
    if false {
        derive_ty!(
            #[repr(C)] struct Baz {
                a: bool,
                b: u32,
                c: [u8; 4],
            }
        );
        derive_ty!(#[repr(C)] union Uni {
            a: u32,
            b: Baz,
        });
        derive_ty!(#[repr(C)] struct Foo { r: &'static u32 });
        derive_ty!(#[repr(C, u8)] enum Bar {
            A(u32),
            B(Baz),
        });
        let ty_bar = Bar::ty_of();
        let mut printer = Printer::new();
        println!("{}", printer.print_rust(&ty_bar).unwrap());
        println!("{}", printer.print_c(&ty_bar).unwrap());
        let mut compiler = Compiler::new(Endian::Little);
        compiler.extend_from_ty(&ty_bar);
        println!("comp layout: {:?}", compiler.layout);
        let prog_bar = Program::new(compiler.insts, "Bar");
        println!("program for bar: {:?}", prog_bar);
        // std::process::exit(0);
    }
    println!("{:?}", core::mem::size_of::<inst::Inst>());
    // derive_ty!(#[repr(C)] struct Foo {
    //     fiedl0: u8,
    //     fiedl1: u32,
    //     field2: u8,
    // });
    // derive_ty!(#[repr(C)] struct Bar {
    //     fiedl0: u16,
    //     fiedl1: u32,
    //     field2: u8,
    // });
    // println!("sizeof Foo: {}", core::mem::size_of::<Foo>());
    // println!("sizeof Bar: {}", core::mem::size_of::<Bar>());
    // union U {
    //     nothing: (),
    //     something: u128,
    // }
    // safe_transmute::<Pb, Au>
    // safe_transmute<[u8; 16], U>([0; 16]) // goood
    // safe_transmute<U, [u8; 16]>(U { nothing: () }) // bad

    // - all things must be repr(C).
    // - no fat refs/ptrs
    // - cannot observe uninitialized memory (transmuting uninit to MaybeUninit is safe)
    // - cannot read/write private fields (i.e. fields not visible from current context)
    // - cannot construct invalid data (e.g. enums, bool, rustc_layout_scalar_valid_range_start/end)
    // - transmuted references cannot be misaligned OR point to a shorter allocation
    // - source reference lifetimes must outlive transmuted reference lifetimes ('src: 'dst)
    // - elements behind mut references must be transmutible both ways
    // - transmuted slices must have the same element size/align

    derive_ty!(#[repr(C, u8)] enum EnumDst {
        A(bool),
        B(u8),
    });
    derive_ty!(#[repr(C)] struct StructDst {
        a: bool,
        b: EnumDst,
    });

    derive_ty!(#[repr(C, u8)] enum EnumSrc {
        A(bool),
        B(bool),
    });
    derive_ty!(#[repr(C)] struct StructSrc {
        a: EnumSrc,
        b: bool,
    });

    // unsafe trait TransmutableFrom<Src> for Dst {}
    // fn safe_transmute<Src, Dst>(src: Src)
    //     where Dst: TransmutableFrom<Src>
    // { 
    //     unsafe { transmute<Src, Dst>(src) }
    // }

    let endian = if true { Endian::Little } else { Endian::Big };
    let prog_dst = Compiler::compile(&StructDst::ty_of(), endian, "StructDst");
    let prog_src = Compiler::compile(&StructSrc::ty_of(), endian, "StructSrc");
    let mut execution = Execution::new(prog_dst, prog_src);
    execution.check();
    // trait CanTransmuteInto<&Bar> for &Foo {}
    // trait CanTransmuteInto<Bar> for Foo {}
    // 1) implement Rust complier in OCaml
    // 2) once self-sufficient, implement Rust compiler in Rust
    // verion 0.1 can't compile itself (built in OCaml),
    // verion 0.2 can compile itself

    /*
    let mut forks = vec![];

    */
}
