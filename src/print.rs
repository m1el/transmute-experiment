use core::fmt::{self, Write};
use crate::ty::*;

pub struct Printer<'t> {
    pub pos: usize,
    queue: Vec<(&'t Ty, String)>,
}
impl<'t> Printer<'t> {
    pub fn new() -> Self {
        Self {
            pos: 0,
            queue: vec![],
        }
    }
    pub fn c_name_for(&mut self, ty: &'t Ty) -> String {
        match ty {
            Ty::Void => "Zst".into(),
            Ty::Bool => "bool".into(),
            Ty::Int(sz) => format!("u{}", sz * 8),
            Ty::Ptr(_) => unimplemented!("pointer"),
            Ty::Ref(_) => unimplemented!("reference"),
            Ty::Array(_) => {
                panic!("c doesn't have a good type name for arrays");
            }
            Ty::Struct(_) => {
                let id = self.pos;
                self.pos += 1;
                format!("Struct{}", id)
            }
            Ty::Union(_) => {
                let id = self.pos;
                self.pos += 1;
                format!("Union{}", id)
            }
            Ty::Enum(_) => {
                let id = self.pos;
                self.pos += 1;
                format!("TaggedUnion{}", id)
            }
        }
    }
    pub fn rust_name_for(&mut self, ty: &'t Ty) -> String {
        match ty {
            Ty::Void => "()".into(),
            Ty::Bool => "bool".into(),
            Ty::Int(sz) => format!("u{}", sz * 8),
            Ty::Ptr(_) => unimplemented!("pointer"),
            Ty::Ref(_) => unimplemented!("reference"),
            Ty::Array(ref arr) => {
                let ty = self.rust_name_for(&arr.element);
                format!("[{}; {}]", ty, arr.count)
            }
            Ty::Struct(_) => {
                let id = self.pos;
                self.pos += 1;
                format!("Struct{}", id)
            }
            Ty::Union(_) => {
                let id = self.pos;
                self.pos += 1;
                format!("Union{}", id)
            }
            Ty::Enum(_) => {
                let id = self.pos;
                self.pos += 1;
                format!("Enum{}", id)
            }
        }
    }
    pub fn maybe_push(&mut self, ty: &'t Ty, name: String) {
        if ty.needs_name() {
            self.queue.push((ty, name));
        }
    }
    fn write_c_fields(
        &mut self, dst: &mut String, prefix: &str,
        fields: impl Iterator<Item=&'t Ty>
    ) -> Result<(), fmt::Error> {
        for (idx, ty) in fields.enumerate() {
            if let Ty::Array(ref arr) = ty {
                let name = self.c_name_for(&arr.element);
                writeln!(dst, "  {} {}{}[{}];",
                    name, prefix, idx, arr.count)?;
                self.maybe_push(&arr.element, name);
            } else {
                let name = self.c_name_for(ty);
                writeln!(dst, "  {} {}{};", name, prefix, idx)?;
                self.maybe_push(ty, name);
            }
        }
        Ok(())
    }
    pub fn print_c(&mut self, ty: &'t Ty) -> Result<String, fmt::Error> {
        let mut dst = Vec::new();
        self.pos = 0;
        let name = self.c_name_for(ty);
        self.maybe_push(ty, name.clone());
        while let Some((ty, id)) = self.queue.pop() {
            match ty {
                Ty::Struct(ref st) => {
                    let mut tmp = String::new();
                    writeln!(tmp, "typedef struct S_{} {{", id)?;
                    self.write_c_fields(&mut tmp, "field",
                        st.fields.iter().map(|f| &f.ty))?;
                    writeln!(tmp, "}} {};", id)?;
                    dst.push(tmp);
                }
                Ty::Union(ref un) => {
                    let mut tmp = String::new();
                    writeln!(tmp, "typedef union U_{} {{", id)?;
                    self.write_c_fields(&mut tmp, "variant",
                        un.variants.iter().map(|v| &v.ty))?;
                    writeln!(tmp, "}} {};", id)?;
                    dst.push(tmp);
                }
                Ty::Enum(ref en) => {
                    let mut tmp = String::new();
                    writeln!(tmp, "typedef union U_{}_Payload {{", id)?;
                    self.write_c_fields(&mut tmp, "variant",
                        en.variants.iter().map(|v| &v.payload))?;
                    writeln!(tmp, "}} {}_Payload;", id)?;
                    writeln!(tmp, "typedef struct S_{} {{", id)?;
                    writeln!(tmp, "  u{} tag;", en.tag_layout.size() * 8)?;
                    writeln!(tmp, "  {}_Payload payload;", id)?;
                    writeln!(tmp, "}} {};", id)?;
                    dst.push(tmp);
                }
                _ => unimplemented!(),
            }
        }
        let mut dst = dst.into_iter().rev().collect::<String>();
        writeln!(dst, "typedef {} Root;", name)?;
        Ok(dst)
    }
    fn write_rust_fields(
        &mut self, dst: &mut String, prefix: &str,
        fields: impl Iterator<Item=&'t Ty>
    ) -> Result<(), fmt::Error> {
        for (idx, ty) in fields.enumerate() {
            let name = self.rust_name_for(ty);
            writeln!(dst, "  {}{}: {},", prefix, idx, name)?;
            if let Ty::Array(ref arr) = ty {
                let name = self.rust_name_for(&arr.element);
                self.maybe_push(&arr.element, name);
            }
            self.maybe_push(ty, name);
        }
        Ok(())
    }
    pub fn print_rust(&mut self, ty: &'t Ty) -> Result<String, fmt::Error> {
        let mut dst = String::new();
        self.pos = 0;
        let name = self.rust_name_for(ty);
        self.maybe_push(ty, name.clone());
        while let Some((ty, id)) = self.queue.pop() {
            match ty {
                Ty::Struct(ref st) => {
                    writeln!(dst, "#[repr(C)]\nstruct {} {{", id)?;
                    self.write_rust_fields(&mut dst, "field",
                        st.fields.iter().map(|f| &f.ty))?;
                    writeln!(dst, "}}")?;
                }
                Ty::Enum(ref en) => {
                    writeln!(dst, "#[repr(C, u{})]\nenum {} {{",
                        en.tag_layout.size() * 8, id)?;
                    for (idx, variant) in en.variants.iter().enumerate() {
                        let name = self.rust_name_for(&variant.payload);
                        writeln!(dst, "  Var{}({}),", idx, name)?;
                        if let Ty::Array(ref arr) = variant.payload {
                            let name = self.rust_name_for(&arr.element);
                            self.maybe_push(&arr.element, name);
                        }
                        self.maybe_push(&variant.payload, name);
                    }
                    writeln!(dst, "}}")?;
                }
                Ty::Union(ref un) => {
                    writeln!(dst, "#[repr(C)]\nunion {} {{", id)?;
                    self.write_rust_fields(&mut dst, "variant",
                        un.variants.iter().map(|v| &v.ty))?;
                    writeln!(dst, "}}")?;
                }
                _ => unimplemented!(),
            }
        }
        writeln!(&mut dst, "type Root = {};", name)?;
        Ok(dst)
    }
}
