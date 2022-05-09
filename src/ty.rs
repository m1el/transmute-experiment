use core::alloc::Layout;

pub enum Ty {
    Void,
    Bool,
    Int(u32),
    Ptr(Pointer),
    Ref(Reference),
    Struct(Struct),
    Array(Box<Array>),
    Enum(Enum),
    Union(Union),
}
impl Ty {
    pub fn needs_name(&self) -> bool {
        matches!(self, Ty::Struct(_) | Ty::Enum(_) | Ty::Union(_))
    }
}
pub enum RefKind {
    Shared,
    Unique,
}
pub struct Pointer {
    pub kind: RefKind,
    pub align: usize,
}
pub struct Reference {
    pub kind: RefKind,
    pub size: usize,
    pub align: usize,
}
pub struct Struct {
    pub name: &'static str,
    pub layout: Layout,
    pub fields: Vec<Field>,
}
impl Struct {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            layout: Layout::from_size_align(0, 1).unwrap(),
            fields: Vec::new(),
        }
    }
    pub fn add_field(&mut self, private: bool, ty: Ty) {
        self.layout = self.layout.extend(layout_of(&ty)).unwrap().0;
        self.fields.push(Field { private, ty });
    }
}
pub struct Field {
    pub private: bool,
    pub ty: Ty,
}
pub struct Array {
    pub element: Ty,
    pub count: usize,
}
pub struct Enum {
    pub name: &'static str,
    pub layout: Layout,
    pub tag_layout: Layout,
    pub payload_layout: Layout,
    pub variants: Vec<EnumVariant>,
}
impl Enum {
    pub fn new(name: &'static str, disc_size: u32) -> Self {
        let tag_layout = layout_of(&Ty::Int(disc_size));
        Self {
            name,
            layout: tag_layout,
            tag_layout,
            payload_layout: Layout::from_size_align(0, 1).unwrap(),
            variants: Vec::new(),
        }
    }
    pub fn add_variant(&mut self, disc: u128, payload: Ty) {
        let ty_layout = layout_of(&payload);
        self.payload_layout = Layout::from_size_align(
            self.payload_layout.size().max(ty_layout.size()),
            self.payload_layout.align().max(ty_layout.align())
        ).unwrap();
        self.layout = self.tag_layout.extend(self.payload_layout).unwrap().0;
        self.variants.push(EnumVariant { disc, payload });
    }
}
pub struct EnumVariant {
    pub disc: u128,
    pub payload: Ty,
}
pub struct Union {
    pub name: &'static str,
    pub layout: Layout,
    pub variants: Vec<UnionVariant>,
}
impl Union {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            layout: Layout::from_size_align(0, 1).unwrap(),
            variants: Vec::new(),
        }
    }
    pub fn add_variant(&mut self, private: bool, variant: Ty) {
        let ty_layout = layout_of(&variant);
        self.layout = Layout::from_size_align(
            self.layout.size().max(ty_layout.size()),
            self.layout.align().max(ty_layout.align())
        ).unwrap();
        self.variants.push(UnionVariant { private, ty: variant });
    }
}
pub struct UnionVariant {
    pub private: bool,
    pub ty: Ty,
}

pub fn layout_of(ty: &Ty) -> Layout {
    match ty {
        Ty::Void => Layout::from_size_align(0, 1).unwrap(),
        Ty::Bool => Layout::from_size_align(1, 1).unwrap(),
        Ty::Int(size) => {
            let size = *size as usize;
            let align = match size {
                1 => 1,  // u8
                2 => 2,  // u16
                4 => 4,  // u32
                8 => 8,  // u64
                16 => 8, // u128
                _ => { panic!("invalid int size!"); }
            };
            Layout::from_size_align(size, align).unwrap()
        }
        Ty::Ref(_) => unimplemented!(),
        Ty::Ptr(_) => unimplemented!(),
        Ty::Array(ref arr) => {
            layout_of(&arr.element).repeat(arr.count).expect("array too big").0
        }
        Ty::Struct(ref st) => st.layout,
        Ty::Enum(ref en) => en.layout,
        Ty::Union(ref un) => un.layout,
    }
}

#[derive(Clone, Copy)]
pub enum Endian {
    Little,
    Big,
}
