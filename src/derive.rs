use crate::ty::*;

pub trait MakeTy {
    fn ty_of() -> Ty;
    fn ty_of_this(&self) -> Ty {
        <Self as MakeTy>::ty_of()
    }
}
impl MakeTy for ()   { fn ty_of() -> Ty { Ty::Void } }
impl MakeTy for bool { fn ty_of() -> Ty { Ty::Bool } }
impl MakeTy for u8   { fn ty_of() -> Ty { Ty::Int(1) }  }
impl MakeTy for u16  { fn ty_of() -> Ty { Ty::Int(2) }  }
impl MakeTy for u32  { fn ty_of() -> Ty { Ty::Int(4) }  }
impl MakeTy for u64  { fn ty_of() -> Ty { Ty::Int(8) }  }
impl<T, const C: usize> MakeTy for [T; C]
    where T: MakeTy
{
    fn ty_of() -> Ty {
        Ty::Array(Box::new(Array {
            element: <T as MakeTy>::ty_of(),
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
        impl $crate::derive::MakeTy for $name {
            fn ty_of() -> Ty {
                let mut st = Struct::new();
                $(
                    st.add_field(false, <$ty as $crate::derive::MakeTy>::ty_of());
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
        impl $crate::derive::MakeTy for $name {
            fn ty_of() -> Ty {
                let mut en = Enum::new(core::mem::size_of::<$sz>() as u32);
                let mut pos = 0;
                $(
                    en.add_variant(pos, <$payload as $crate::derive::MakeTy>::ty_of());
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
        impl $crate::derive::MakeTy for $name {
            fn ty_of() -> Ty {
                let mut un = Union::new();
                $(
                    un.add_variant(false, <$payload as $crate::derive::MakeTy>::ty_of());
                )*
                Ty::Union(un)
            }
        }
    };
}

pub(crate) use derive_ty;