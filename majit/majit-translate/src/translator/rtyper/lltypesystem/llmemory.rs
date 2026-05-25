//! `rpython/rtyper/lltypesystem/llmemory.py` — annotation types for
//! low-level memory addresses.

use crate::annotator::model::{KnownType, SomeObjectBase, SomeObjectTrait, SomeValue};

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
