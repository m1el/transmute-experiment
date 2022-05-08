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
        derive_ty!(#[repr(C, u8)] enum Bar {
            A(u32),
            B(Baz),
        });
        let ty_bar = Bar::ty_of();
        let mut compiler = Compiler::new(Endian::Little);
        compiler.extend_from_ty(&ty_bar);
        println!("comp layout: {:?}", compiler.layout);
        let prog_bar = Program::new(&compiler.insts);
        println!("program for bar: {:?}", prog_bar);
        // std::process::exit(0);
    }
    derive_ty!(#[repr(C)] struct Foo {
        fiedl0: u8,
        fiedl1: u32,
        field2: u8,
    });
    derive_ty!(#[repr(C)] struct Bar {
        fiedl0: u16,
        fiedl1: u32,
        field2: u8,
    });
    // println!("sizeof Foo: {}", core::mem::size_of::<Foo>());
    // println!("sizeof Bar: {}", core::mem::size_of::<Bar>());
    // union U {
    //     nothing: (),
    //     something: u128,
    // }
    // safe_transmute::<Pb, Au>
    // safe_transmute<[u8; 16], U>([0; 16]) // goood
    // safe_transmute<U, [u8; 16]>(U { nothing: () }) // bad
    let mut ty_a = Foo::ty_of();
    if let Ty::Struct(ref mut st) = ty_a {
        st.fields[0].private = false;
    }
    let ty_b = Bar::ty_of();
    // let mut printer = Printer::new();
    // println!("{}", printer.print_rust(&ty_a).unwrap());
    let mut printer = Printer::new();
    println!("{}", printer.print_rust(&ty_a).unwrap());
    println!("{}", printer.print_rust(&ty_b).unwrap());
    let endian = if true { Endian::Little } else { Endian::Big };
    let mut compiler = Compiler::new(endian);
    compiler.extend_from_ty(&ty_a);
    println!("comp layout: {:?}", compiler.layout);
    let prog_foo = Program::new(&compiler.insts);
    println!("representation of Foo: {:?}", prog_foo);

    let mut compiler = Compiler::new(endian);
    compiler.extend_from_ty(&ty_b);
    println!("comp layout: {:?}", compiler.layout);
    let prog_bar = Program::new(&compiler.insts);
    println!("representation of Bar: {:?}", prog_bar);
    if false {
       println!("{:?}", prog_bar);
    }
    // trait CanTransmuteInto<&Bar> for &Foo {}
    // trait CanTransmuteInto<Bar> for Foo {}
    let mut current = Some((prog_foo, prog_bar));
    let mut forks = vec![];
    'outer: while let Some((mut prog_a, mut prog_b)) = current {
        let byte_a;
        match prog_a.next_step() {
            Some(LayoutStep::Byte { byte, .. }) => {
                byte_a = byte;
            }
            Some(LayoutStep::Fork(_)) => unimplemented!(),
            Some(LayoutStep::Join) => unimplemented!(),
            None => {
                current = forks.pop();
                continue 'outer;
            },
        }
        let byte_b;
        match prog_b.next_step() {
            Some(LayoutStep::Byte { byte, .. }) => {
                byte_b = byte;
            }
            Some(LayoutStep::Fork(_)) => unimplemented!(),
            Some(LayoutStep::Join) => unimplemented!(),
            None => {
                byte_b = StepByte::Uninit;
            },
        }
        println!("{:?} ({:?} <- {:?})",
            byte_a.accepts(&byte_b), byte_a, byte_b
        );
        println!("{:?} ({:?} -> {:?})",
            byte_b.accepts(&byte_a), byte_a, byte_b
        );
        current = Some((prog_a, prog_b));
    }
}
