use crate::ty::*;

pub trait InspectTy {
    fn ty_of() -> Ty;
    fn ty_of_this(&self) -> Ty {
        <Self as InspectTy>::ty_of()
    }
}
impl InspectTy for ()   { fn ty_of() -> Ty { Ty::Void } }
impl InspectTy for bool { fn ty_of() -> Ty { Ty::Bool } }
impl InspectTy for u8   { fn ty_of() -> Ty { Ty::Int(1) }  }
impl InspectTy for u16  { fn ty_of() -> Ty { Ty::Int(2) }  }
impl InspectTy for u32  { fn ty_of() -> Ty { Ty::Int(4) }  }
impl InspectTy for u64  { fn ty_of() -> Ty { Ty::Int(8) }  }
impl InspectTy for i8   { fn ty_of() -> Ty { Ty::Int(1) }  }
impl InspectTy for i16  { fn ty_of() -> Ty { Ty::Int(2) }  }
impl InspectTy for i32  { fn ty_of() -> Ty { Ty::Int(4) }  }
impl InspectTy for i64  { fn ty_of() -> Ty { Ty::Int(8) }  }
impl<T: Sized> InspectTy for *const T {
    fn ty_of() -> Ty {
        Ty::Ptr(Pointer {
            kind: RefKind::Shared,
            align: core::mem::align_of::<T>(),
        })
    }
}
impl<T: Sized> InspectTy for *mut T {
    fn ty_of() -> Ty {
        Ty::Ptr(Pointer {
            kind: RefKind::Unique,
            align: core::mem::align_of::<T>(),
        })
    }
}
impl<T: Sized> InspectTy for &T {
    fn ty_of() -> Ty {
        Ty::Ref(Reference {
            kind: RefKind::Shared,
            size: core::mem::size_of::<T>(),
            align: core::mem::align_of::<T>(),
        })
    }
}
impl<T: Sized> InspectTy for &mut T {
    fn ty_of() -> Ty {
        Ty::Ref(Reference {
            kind: RefKind::Unique,
            size: core::mem::size_of::<T>(),
            align: core::mem::align_of::<T>(),
        })
    }
}
impl<T, const C: usize> InspectTy for [T; C]
    where T: InspectTy
{
    fn ty_of() -> Ty {
        Ty::Array(Box::new(Array {
            element: <T as InspectTy>::ty_of(),
            count: C,
        }))
    }
}

macro_rules! derive_ty {
    (
        #[repr(C)]
        struct $name:ident {
            $($field:ident: $ty:ty),*
            $(,)?
        }
    ) => {
        #[repr(C)]
        struct $name {
            $($field: $ty),*
        }
        impl $crate::derive::InspectTy for $name {
            fn ty_of() -> Ty {
                let mut st = Struct::new(stringify!($name));
                $(
                    st.add_field(false, <$ty as $crate::derive::InspectTy>::ty_of());
                )*
                Ty::Struct(st)
            }
        }
    };
    (
        #[repr(C, $sz:ty)]
        enum $name:ident {
            $($variant:ident ($payload:ty)),*
            $(,)?
        }
    ) => {
        #[repr(C)]
        enum $name {
            $($variant($payload)),*
        }
        impl $crate::derive::InspectTy for $name {
            fn ty_of() -> Ty {
                let mut en = Enum::new(stringify!($name), core::mem::size_of::<$sz>() as u32);
                let mut pos = 0;
                $(
                    en.add_variant(pos, <$payload as $crate::derive::InspectTy>::ty_of());
                    pos += 1;
                )*
                let _ = pos;
                Ty::Enum(en)
            }
        }
    };
    (
        #[repr(C)]
        union $name:ident {
            $($variant:ident: $payload:ty),*
            $(,)?
        }
    ) => {
        #[repr(C)]
        union $name {
            $($variant: $payload),*
        }
        impl $crate::derive::InspectTy for $name {
            fn ty_of() -> Ty {
                let mut un = Union::new(stringify!($name));
                $(
                    un.add_variant(false, <$payload as $crate::derive::InspectTy>::ty_of());
                )*
                Ty::Union(un)
            }
        }
    };
}

pub(crate) use derive_ty;