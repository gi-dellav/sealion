#![allow(clippy::missing_safety_doc)]

use std::ptr::NonNull;

use super::vm::VMOpaqueContext;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ValueType {
    I32,
    I64,
    F32,
    F64,
    V128,
    FuncRef,
    ExternRef,
}

impl ValueType {
    pub(crate) const fn size(self) -> usize {
        match self {
            Self::I32 | Self::F32 => 4,
            Self::I64 | Self::F64 | Self::FuncRef | Self::ExternRef => 8,
            Self::V128 => 16,
        }
    }
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ValRaw {
    bytes: [u8; 16],
}

impl ValRaw {
    pub(crate) const fn zero() -> Self {
        Self { bytes: [0; 16] }
    }

    fn from_low_u128(value: u128) -> Self {
        Self {
            bytes: value.to_le_bytes(),
        }
    }

    fn low_u128(self) -> u128 {
        u128::from_le_bytes(self.bytes)
    }

    pub(crate) fn v128(value: u128) -> Self {
        Self::from_low_u128(value)
    }

    pub(crate) fn i32(value: i32) -> Self {
        Self::from_low_u128(value as u32 as u128)
    }

    pub(crate) fn i64(value: i64) -> Self {
        Self::from_low_u128(value as u64 as u128)
    }

    pub(crate) fn f32(value: f32) -> Self {
        Self::from_low_u128(value.to_bits() as u128)
    }

    pub(crate) fn f64(value: f64) -> Self {
        Self::from_low_u128(value.to_bits() as u128)
    }

    pub(crate) fn funcref(value: Option<NonNull<VMOpaqueContext>>) -> Self {
        let bits = match value {
            Some(value) => value.as_ptr() as usize as u128,
            None => 0,
        };
        Self::from_low_u128(bits)
    }

    pub(crate) fn get_i32(self) -> i32 {
        self.low_u128() as u32 as i32
    }

    pub(crate) fn get_i64(self) -> i64 {
        self.low_u128() as u64 as i64
    }

    pub(crate) fn get_f32(self) -> f32 {
        f32::from_bits(self.low_u128() as u32)
    }

    pub(crate) fn get_f64(self) -> f64 {
        f64::from_bits(self.low_u128() as u64)
    }

    pub(crate) fn get_v128(self) -> u128 {
        self.low_u128()
    }

    pub(crate) fn get_funcref(self) -> Option<NonNull<VMOpaqueContext>> {
        let ptr = self.low_u128() as usize as *mut VMOpaqueContext;
        NonNull::new(ptr)
    }
}

impl Default for ValRaw {
    fn default() -> Self {
        Self::zero()
    }
}

pub(crate) unsafe trait ValueTy: Send + Sized + 'static {
    fn valtype() -> ValueType;

    fn store(self, slot: &mut ValRaw);

    unsafe fn load(slot: &ValRaw) -> Self;
}

macro_rules! impl_value_ty {
    ($ty:ty, $vt:expr, $store:expr, $load:expr) => {
        unsafe impl ValueTy for $ty {
            fn valtype() -> ValueType {
                $vt
            }

            fn store(self, slot: &mut ValRaw) {
                *slot = ($store)(self);
            }

            unsafe fn load(slot: &ValRaw) -> Self {
                ($load)(*slot)
            }
        }
    };
}

impl_value_ty!(i32, ValueType::I32, ValRaw::i32, ValRaw::get_i32);
impl_value_ty!(
    u32,
    ValueType::I32,
    |value: u32| ValRaw::i32(value as i32),
    |slot: ValRaw| slot.get_i32() as u32
);
impl_value_ty!(i64, ValueType::I64, ValRaw::i64, ValRaw::get_i64);
impl_value_ty!(
    u64,
    ValueType::I64,
    |value: u64| ValRaw::i64(value as i64),
    |slot: ValRaw| slot.get_i64() as u64
);
impl_value_ty!(f32, ValueType::F32, ValRaw::f32, ValRaw::get_f32);
impl_value_ty!(f64, ValueType::F64, ValRaw::f64, ValRaw::get_f64);
impl_value_ty!(u128, ValueType::V128, ValRaw::v128, ValRaw::get_v128);

unsafe impl ValueTy for bool {
    fn valtype() -> ValueType {
        ValueType::I32
    }

    fn store(self, slot: &mut ValRaw) {
        *slot = ValRaw::i32(self as i32);
    }

    unsafe fn load(slot: &ValRaw) -> Self {
        slot.get_i32() != 0
    }
}

pub(crate) unsafe trait ValueTyList: Sized {
    fn valtypes() -> Vec<ValueType>;

    fn store(self, slots: &mut [ValRaw]);

    unsafe fn load(slots: &[ValRaw]) -> Self;
}

unsafe impl ValueTyList for () {
    fn valtypes() -> Vec<ValueType> {
        Vec::new()
    }

    fn store(self, _slots: &mut [ValRaw]) {}

    unsafe fn load(_slots: &[ValRaw]) -> Self {}
}

macro_rules! impl_value_ty_list {
    ($(($ty:ident, $var:ident)),+) => {
        unsafe impl<$($ty: ValueTy),+> ValueTyList for ($($ty,)+) {
            fn valtypes() -> Vec<ValueType> {
                vec![$($ty::valtype()),+]
            }

            fn store(self, slots: &mut [ValRaw]) {
                let ($($var,)+) = self;
                let mut slots = slots.iter_mut();
                $(
                    let slot = slots.next().expect("missing parameter slot");
                    <$ty as ValueTy>::store($var, slot);
                )+
            }

            unsafe fn load(slots: &[ValRaw]) -> Self {
                let mut slots = slots.iter();
                unsafe {
                    ($(
                        <$ty as ValueTy>::load(slots.next().expect("missing parameter slot")),
                    )+)
                }
            }
        }
    };
}

impl_value_ty_list!((A, a));
impl_value_ty_list!((A, a), (B, b));
impl_value_ty_list!((A, a), (B, b), (C, c));
impl_value_ty_list!((A, a), (B, b), (C, c), (D, d));
impl_value_ty_list!((A, a), (B, b), (C, c), (D, d), (E, e));
impl_value_ty_list!((A, a), (B, b), (C, c), (D, d), (E, e), (F, f));
impl_value_ty_list!((A, a), (B, b), (C, c), (D, d), (E, e), (F, f), (G, g));
impl_value_ty_list!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h)
);
impl_value_ty_list!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i)
);
impl_value_ty_list!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j)
);
impl_value_ty_list!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k)
);
impl_value_ty_list!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l)
);
impl_value_ty_list!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m)
);
impl_value_ty_list!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m),
    (N, n)
);
impl_value_ty_list!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m),
    (N, n),
    (O, o)
);
impl_value_ty_list!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m),
    (N, n),
    (O, o),
    (P, p)
);

pub(crate) unsafe trait ValueRet: Sized {
    fn valtypes() -> Vec<ValueType>;

    fn store(self, slots: &mut [ValRaw]);

    unsafe fn load(slots: &[ValRaw]) -> Self;
}

unsafe impl ValueRet for () {
    fn valtypes() -> Vec<ValueType> {
        Vec::new()
    }

    fn store(self, _slots: &mut [ValRaw]) {}

    unsafe fn load(_slots: &[ValRaw]) -> Self {}
}

macro_rules! impl_value_ret {
    ($(($ty:ident, $var:ident)),+) => {
        unsafe impl<$($ty: ValueRet + ValueTy),+> ValueRet for ($($ty,)+) {
            fn valtypes() -> Vec<ValueType> {
                let mut types = Vec::new();
                $(types.extend($ty::valtypes());)+
                types
            }

            fn store(self, slots: &mut [ValRaw]) {
                let ($($var,)+) = self;
                let mut slots = slots.iter_mut();
                $(
                    let slot = slots.next().expect("missing result slot");
                    <$ty as ValueTy>::store($var, slot);
                )+
            }

            unsafe fn load(slots: &[ValRaw]) -> Self {
                let mut slots = slots.iter();
                unsafe {
                    ($(
                        <$ty as ValueTy>::load(slots.next().expect("missing result slot")),
                    )+)
                }
            }
        }
    };
}

impl_value_ret!((A, a));
impl_value_ret!((A, a), (B, b));
impl_value_ret!((A, a), (B, b), (C, c));
impl_value_ret!((A, a), (B, b), (C, c), (D, d));
impl_value_ret!((A, a), (B, b), (C, c), (D, d), (E, e));
impl_value_ret!((A, a), (B, b), (C, c), (D, d), (E, e), (F, f));
impl_value_ret!((A, a), (B, b), (C, c), (D, d), (E, e), (F, f), (G, g));
impl_value_ret!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h)
);
impl_value_ret!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i)
);
impl_value_ret!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j)
);
impl_value_ret!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k)
);
impl_value_ret!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l)
);
impl_value_ret!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m)
);
impl_value_ret!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m),
    (N, n)
);
impl_value_ret!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m),
    (N, n),
    (O, o)
);
impl_value_ret!(
    (A, a),
    (B, b),
    (C, c),
    (D, d),
    (E, e),
    (F, f),
    (G, g),
    (H, h),
    (I, i),
    (J, j),
    (K, k),
    (L, l),
    (M, m),
    (N, n),
    (O, o),
    (P, p)
);

macro_rules! impl_scalar_value_ret {
    ($ty:ty) => {
        unsafe impl ValueRet for $ty {
            fn valtypes() -> Vec<ValueType> {
                vec![<$ty as ValueTy>::valtype()]
            }

            fn store(self, slots: &mut [ValRaw]) {
                let slot = slots.first_mut().expect("missing result slot");
                <$ty as ValueTy>::store(self, slot);
            }

            unsafe fn load(slots: &[ValRaw]) -> Self {
                let slot = slots.first().expect("missing result slot");
                unsafe { <$ty as ValueTy>::load(slot) }
            }
        }
    };
}

impl_scalar_value_ret!(i32);
impl_scalar_value_ret!(u32);
impl_scalar_value_ret!(i64);
impl_scalar_value_ret!(u64);
impl_scalar_value_ret!(f32);
impl_scalar_value_ret!(f64);
impl_scalar_value_ret!(bool);
impl_scalar_value_ret!(u128);

pub(crate) trait HostResultHasUnwindSentinel: Copy + PartialEq {
    const SENTINEL: Self;

    fn is_unwind_sentinel(self) -> bool {
        self == Self::SENTINEL
    }
}

impl HostResultHasUnwindSentinel for bool {
    const SENTINEL: Self = false;
}

impl HostResultHasUnwindSentinel for i32 {
    const SENTINEL: Self = i32::MIN;
}

impl HostResultHasUnwindSentinel for i64 {
    const SENTINEL: Self = i64::MIN;
}

impl HostResultHasUnwindSentinel for u32 {
    const SENTINEL: Self = u32::MAX;
}

impl HostResultHasUnwindSentinel for u64 {
    const SENTINEL: Self = u64::MAX;
}

impl HostResultHasUnwindSentinel for f32 {
    const SENTINEL: Self = f32::NAN;
}

impl HostResultHasUnwindSentinel for f64 {
    const SENTINEL: Self = f64::NAN;
}
