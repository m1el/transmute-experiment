use core::alloc::Layout;

use crate::ty::*;
use crate::inst::*;

pub struct Compiler {
    pub endian: Endian,
    pub layout: Layout,
    pub insts: Vec<Inst>,
    pub priv_depth: usize,
}

impl Compiler {
    pub fn new(endian: Endian) -> Self {
        Self {
            endian,
            layout: Layout::from_size_align(0, 1).expect("????"),
            insts: Vec::new(),
            priv_depth: 0,
        }
    }
    pub fn extend_from_ty(&mut self, ty: &Ty) {
        let layout = layout_of(ty);
        self.layout = self.layout.align_to(layout.align()).unwrap();
        self.pad_to_align(layout.align());
        match *ty {
            Ty::Void => {
                // let literal = InstBytes::for_literal(Endian::Little, 4, 0x13371337);
                // self.insts.extend(literal.map(Inst::Bytes));
            }
            Ty::Bool => {
                self.repeat_byte(1, (0, 1));
                self.layout = self.layout.extend(layout).unwrap().0;
            }
            Ty::Int(size) => {
                self.repeat_byte(size, (0, 255));
                self.layout = self.layout.extend(layout).unwrap().0;
            }
            Ty::Ptr(ref _ptr) => {
                unimplemented!();
            }
            Ty::Ref(ref _ptr) => {
                unimplemented!();
            }
            Ty::Array(ref array) => {
                for _ in 0..array.count {
                    self.extend_from_ty(&array.element);
                }
            }
            Ty::Struct(ref s_def) => {
                for field in s_def.fields.iter() {
                    let layout = layout_of(&field.ty);
                    self.pad_to_align(layout.align());
                    if field.private { self.priv_depth += 1; }
                    self.extend_from_ty(&field.ty);
                    if field.private { self.priv_depth -= 1; }
                }
                self.pad_to_align(layout.align());
            }
            Ty::Enum(ref e_def) => {
                assert!(!e_def.variants.is_empty(), "zero-variant enum isn't repr-c");
                let mut variants = e_def.variants.iter();
                let last_variant = variants.next_back()
                    .expect("at least one variant is present");
                let mut patches = Vec::with_capacity(e_def.variants.len());
                let mut prev_patch: Option<usize> = None;
                let orig_layout = self.layout;

                for variant in variants {
                    let split = self.insts.len();
                    if let Some(prev_split) = prev_patch {
                        self.insts[prev_split].patch_split(split as InstPtr);
                    }
                    prev_patch = Some(split);
                    self.insts.push(Inst::new_invalid_split());

                    self.extend_enum_variant(e_def, variant);

                    patches.push(self.insts.len());
                    self.insts.push(Inst::new_invalid_goto());
                    self.layout = orig_layout;
                }

                if let Some(last_split) = prev_patch {
                    let ip = self.insts.len() as InstPtr;
                    self.insts[last_split].patch_split(ip);
                }

                self.extend_enum_variant(e_def, last_variant);
                let ip = self.insts.len() as InstPtr;
                self.insts.push(Inst::JoinLast);

                for patch in patches {
                    self.insts[patch].patch_goto(ip);
                }
            }
            Ty::Union(ref u_def) => {
                assert!(!u_def.variants.is_empty(), "zero-variant enum isn't repr-c");
                let mut variants = u_def.variants.iter();
                let last_variant = variants.next_back()
                    .expect("at least one variant is present");
                let mut patches = Vec::with_capacity(u_def.variants.len());
                let mut prev_patch: Option<usize> = None;
                let orig_layout = self.layout;

                for variant in variants {
                    let split = self.insts.len();
                    if let Some(prev_split) = prev_patch {
                        self.insts[prev_split].patch_split(split as InstPtr);
                    }
                    prev_patch = Some(split);
                    self.insts.push(Inst::new_invalid_split());

                    self.extend_union_variant(u_def, variant);
                    patches.push(self.insts.len());
                    self.insts.push(Inst::new_invalid_goto());
                    self.layout = orig_layout;
                }

                if let Some(last_split) = prev_patch {
                    let ip = self.insts.len() as InstPtr;
                    self.insts[last_split].patch_split(ip);
                }

                self.extend_union_variant(u_def, last_variant);
                let ip = self.insts.len() as InstPtr;
                self.insts.push(Inst::JoinLast);

                for patch in patches {
                    self.insts[patch].patch_goto(ip);
                }
            }
        }
    }
    fn extend_union_variant(&mut self, u_def: &Union, variant: &UnionVariant) {
        self.pad_to_align(u_def.layout.align());
        self.priv_depth += variant.private as usize;
        self.extend_from_ty(&variant.ty);
        self.priv_depth -= variant.private as usize;
        let variant_layout = layout_of(&variant.ty);
        self.pad(u_def.layout.size() - variant_layout.size());
    }
    fn extend_enum_variant(&mut self, e_def: &Enum, variant: &EnumVariant) {
        let endian = self.endian;
        let private = self.priv_depth > 0;
        let tag = InstByte::for_literal(
                endian, e_def.tag_layout.size(), variant.disc, private
            );
        self.insts.extend(tag);
        self.layout = self.layout.extend(e_def.tag_layout).unwrap().0;
        self.pad_to_align(e_def.payload_layout.align());
        self.extend_from_ty(&variant.payload);
        let variant_layout = layout_of(&variant.payload);
        self.pad(e_def.payload_layout.size() - variant_layout.size());
    }
    fn repeat_with<F>(&mut self, count: u32, f: F)
        where F: Fn() -> Inst
    {
        for _ in 0..count {
            self.insts.push(f());
        }
    }
    fn pad(&mut self, padding: usize) {
        // println!("i:{}, padding: {}, layout: {:?}", self.insts.len(), padding, self.layout);
        let padding_layout = Layout::from_size_align(padding, 1).unwrap();
        self.layout = self.layout.extend(padding_layout).unwrap().0;
        self.repeat_with(padding as u32, || Inst::Uninit);
    }
    fn pad_to_align(&mut self, align: usize) {
        let padding = self.layout.padding_needed_for(align);
        self.pad(padding);
    }
    fn repeat_byte(&mut self, size: u32, byte_ranges: (u8, u8)) {
        let private = self.priv_depth > 0;
        self.repeat_with(size, || Inst::ByteRange(InstByteRange {
            private,
            range: byte_ranges,
        }));
    }
}