//! `rpython/rtyper/lltypesystem/llmemory.py` — annotation types for
//! low-level memory addresses.

use crate::annotator::model::{KnownType, SomeObjectBase, SomeObjectTrait, SomeValue};
use crate::flowspace::model::ConstValue;
use crate::translator::rtyper::lltypesystem::lltype::LowLevelType;

/// `class SomeAddress(SomeObject)` (llmemory.py:573-590).
/// Annotation for low-level Address values. `immutable = True`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SomeAddress {
    pub base: SomeObjectBase,
}

impl SomeAddress {
    pub fn new() -> Self {
        SomeAddress {
            base: SomeObjectBase::new(KnownType::Address, true),
        }
    }

    /// `def is_null_address(self)` (llmemory.py:579-580).
    /// `return self.is_immutable_constant() and not self.const`
    /// — true when the annotation carries a constant that is a falsy
    /// address value (i.e. NULL / fakeaddress(None)).
    pub fn is_null_address(&self) -> bool {
        if !self.is_immutable_constant() {
            return false;
        }
        match &self.base.const_box {
            Some(c) => c.value.is_null_address(),
            None => false,
        }
    }

    /// `def getattr(self, s_attr)` (llmemory.py:582-586).
    /// Returns the annotation for `addr.<access_type>` — the intermediate
    /// value used in `addr.signed[offset]` patterns.
    pub fn annotation_getattr(attr: &str) -> Option<SomeTypedAddressAccess> {
        supported_access_type(attr).map(SomeTypedAddressAccess::new)
    }

    /// `def bool(self)` (llmemory.py:588-589).
    /// `return s_Bool`
    pub fn annotation_bool() -> SomeValue {
        SomeValue::Bool(crate::annotator::model::SomeBool::new())
    }
}

impl Default for SomeAddress {
    fn default() -> Self {
        SomeAddress::new()
    }
}

impl SomeObjectTrait for SomeAddress {
    fn knowntype(&self) -> KnownType {
        KnownType::Address
    }
    fn immutable(&self) -> bool {
        true
    }
    fn is_constant(&self) -> bool {
        self.base.const_box.is_some()
    }
    fn can_be_none(&self) -> bool {
        false
    }
}

/// llmemory.py:730-735
pub fn supported_access_type(name: &str) -> Option<LowLevelType> {
    match name {
        "signed" => Some(LowLevelType::Signed),
        "unsigned" => Some(LowLevelType::Unsigned),
        "char" => Some(LowLevelType::Char),
        "address" => Some(LowLevelType::Address),
        "float" => Some(LowLevelType::Float),
        _ => None,
    }
}

/// `class SomeTypedAddressAccess(SomeObject)` (llmemory.py:596-605).
/// Annotation for the intermediate value in `addr.signed[offset]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SomeTypedAddressAccess {
    pub access_type: LowLevelType,
    pub base: SomeObjectBase,
}

impl SomeTypedAddressAccess {
    pub fn new(access_type: LowLevelType) -> Self {
        SomeTypedAddressAccess {
            access_type,
            base: SomeObjectBase::new(KnownType::Object, false),
        }
    }
}

impl SomeObjectTrait for SomeTypedAddressAccess {
    fn knowntype(&self) -> KnownType {
        KnownType::Object
    }
    fn immutable(&self) -> bool {
        false
    }
    fn is_constant(&self) -> bool {
        false
    }
    fn can_be_none(&self) -> bool {
        false
    }
}

/// `llmemory.sizeof(TYPE)` (llmemory.py:412) — returns a symbolic
/// `ItemOffset(TYPE)` as `ConstValue::AddressOffset`.
/// `inputconst(Signed, sizeof(TYPE))` accepts this variant because
/// `AddressOffset.lltype() == Signed` (matching RPython's
/// `typeOf(Symbolic) -> val.lltype()`).
pub fn sizeof(ty: &LowLevelType) -> Option<ConstValue> {
    let byte_size = match ty {
        LowLevelType::Signed => 8,
        LowLevelType::Unsigned => 8,
        LowLevelType::Char => 1,
        LowLevelType::Address => 8,
        LowLevelType::Float => 8,
        _ => return None,
    };
    Some(ConstValue::AddressOffset {
        kind: "ItemOffset".into(),
        type_name: format!("{ty:?}"),
        byte_size,
    })
}
