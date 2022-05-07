#![feature(alloc_layout_extra)]
#![feature(untagged_unions)]
use core::alloc::Layout;

mod compiler;
mod derive;
mod inst;
mod print;
mod ty;

use print::Printer;
use derive::{InspectTy, derive_ty};
use crate::compiler::Compiler;
use crate::inst::Program;
use crate::ty::*;

#[allow(dead_code)]
fn main() {
    #[allow(dead_code)]
    {
        derive_ty!(#[repr(C)] struct Foo {
            b: u8,
            a: [u32; 0]
        });
        println!("real layout: {:?}", Layout::new::<Foo>());
        let ty = Foo::ty_of();
        let mut compiler = Compiler::new(Endian::Little);
        compiler.extend_from_ty(&ty);
        println!("comp layout: {:?}", compiler.layout);
    }
    derive_ty!(#[repr(C)] struct Foo {
        fiedl0: bool,
        fiedl1: u32,
    });
    derive_ty!(#[repr(C)] union Baz {
        a: bool,
        b: u32,
        c: [u8; 4],
    });
    derive_ty!(#[repr(C, u8)] enum Bar {
        A(Foo),
        B(Baz),
        C(())
    });
    // union U {
    //     nothing: (),
    //     something: u128,
    // }
    // safe_transmute::<Pb, Au>
    // safe_transmute<[u8; 16], U>([0; 16]) // goood
    // safe_transmute<U, [u8; 16]>(U { nothing: () }) // bad
    let ty = Bar::ty_of();
    // let mut printer = Printer::new();
    // println!("{}", printer.print_rust(&ty).unwrap());
    let mut printer = Printer::new();
    println!("{}", printer.print_c(&ty).unwrap());
    println!("{}", printer.print_rust(&ty).unwrap());
    let endian = if true { Endian::Little } else { Endian::Big };
    let mut compiler = Compiler::new(endian);
    compiler.extend_from_ty(&ty);
    println!("comp layout: {:?}", compiler.layout);
    let prog = Program { insts: compiler.insts };
    println!("{:?}", prog);
}
