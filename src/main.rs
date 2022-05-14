#![feature(alloc_layout_extra)]
#![feature(untagged_unions)]
mod compiler;
mod derive;
mod inst;
mod print;
mod ty;
use print::Printer;
use derive::{InspectTy, derive_ty};
use crate::compiler::Compiler;
use crate::inst::{Program, LayoutStep, StepByte};
use crate::ty::*;

#[derive(Copy, Clone, PartialEq, Eq)]
enum ForkReason {
    Dst,
    Src,
}

struct Fork<'a> {
    dst: Program<'a>,
    src: Program<'a>,
    reason: ForkReason,
}

struct Execution<'a> {
    forks: Vec<Fork<'a>>,
    dst: Program<'a>,
    src: Program<'a>,
}
impl<'a> Execution<'a> {
    fn new(dst: Program<'a>, src: Program<'a>) -> Self {
        Self {
            forks: Vec::new(),
            dst,
            src,
        }
    }
    fn pop_fork(&mut self, reason: ForkReason) -> Option<Fork<'a>> {
        let last = self.forks.last()
            .expect("Asked to join when there was no previous fork");
        if last.reason == reason {
            self.forks.pop()
        } else {
            None
        }
    }
    fn check(&mut self) {
        loop {
            if let Some(next_src) = self.src.next_fork() {
                println!("fork src");
                self.src.next();
                self.forks.push(Fork {
                    dst: self.dst.clone(),
                    src: next_src,
                    reason: ForkReason::Src,
                });
                continue;
            }
            if let Some(next_dst) = self.dst.next_fork() {
                println!("fork dst");
                self.dst.next();
                self.forks.push(Fork {
                    dst: next_dst,
                    src: self.src.clone(),
                    reason: ForkReason::Dst,
                });
                continue;
            }
            if let Some(last) = self.dst.next_join() {
                if last {
                    println!("join dst last");
                    continue;
                }
                if let Some(fork) = self.pop_fork(ForkReason::Dst) {
                    self.dst = fork.dst;
                    self.src = fork.src;
                    println!("join dst");
                }
                continue;
            }
            if let Some(last) = self.src.next_join() {
                if last {
                    println!("join src last");
                    continue;
                }
                if let Some(fork) = self.pop_fork(ForkReason::Src) {
                    self.dst = fork.dst;
                    self.src = fork.src;
                    println!("join src");
                }
                continue;
            }

            let (s_ip, byte_src) = match self.src.next() {
                None => (!0, StepByte::Uninit),
                Some(LayoutStep::Byte { ip, byte, .. }) => {
                    (ip, byte)
                }
                _ => unreachable!("peek and next must match")
            };
            let (d_ip, byte_dst) = match self.dst.next() {
                None => {
                    println!("always match");
                    break;
                }
                Some(LayoutStep::Byte { ip, byte, .. }) => (ip, byte),
                _ => unreachable!("peek and next must match")
            };

            println!("{:?} [{},{}] ({:?} <- {:?})",
                byte_dst.accepts(&byte_src),
                d_ip, s_ip, byte_dst, byte_src
            );
            // println!("{:?} ({:?} -> {:?})",
            //     byte_src.accepts(&byte_dst), byte_dst, byte_src
            // );
        }
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
        derive_ty!(#[repr(C)] struct Baz {
            a: bool,
            b: u32,
            c: [u8; 4],
        });
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
        let prog_bar = Program::new(&compiler.insts, "Bar");
        println!("program for bar: {:?}", prog_bar);
        // std::process::exit(0);
    }
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
    derive_ty!(#[repr(C, u8)] enum Enum {
        A(bool),
        B(bool)
    });
    derive_ty!(#[repr(C)] struct Struct0 {
        a: bool,
        b: Enum,
    });
    derive_ty!(#[repr(C)] struct Struct1 {
        b: Enum,
        a: bool,
    });
    let ty_a = Struct0::ty_of();
    let ty_b = Struct1::ty_of();
    // let mut printer = Printer::new();
    // println!("{}", printer.print_rust(&ty_a).unwrap());
    // let mut printer = Printer::new();
    // println!("{}", printer.print_rust(&ty_a).unwrap());
    // println!("{}", printer.print_rust(&ty_b).unwrap());
    let endian = if true { Endian::Little } else { Endian::Big };
    let mut compiler = Compiler::new(endian);
    compiler.extend_from_ty(&ty_a);
    // println!("comp layout: {:?}", compiler.layout);
    let prog_foo = Program::new(&compiler.insts, "foo");
    // println!("representation of Foo: {:?}", prog_foo);

    let mut compiler = Compiler::new(endian);
    compiler.extend_from_ty(&ty_b);
    // println!("comp layout: {:?}", compiler.layout);
    let prog_bar = Program::new(&compiler.insts, "bar");
    // println!("representation of Bar: {:?}", prog_bar);
    let mut execution = Execution::new(prog_foo, prog_bar);
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
