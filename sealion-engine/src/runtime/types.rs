use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::Error;

use super::slots::ValueType;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FuncType {
    params: Box<[ValueType]>,
    results: Box<[ValueType]>,
}

impl FuncType {
    pub(crate) fn new(params: Vec<ValueType>, results: Vec<ValueType>) -> Self {
        Self {
            params: params.into_boxed_slice(),
            results: results.into_boxed_slice(),
        }
    }

    pub(crate) fn params(&self) -> &[ValueType] {
        &self.params
    }

    pub(crate) fn results(&self) -> &[ValueType] {
        &self.results
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub(crate) struct VMSharedTypeIndex(u32);

impl VMSharedTypeIndex {
    pub(crate) const fn new(index: u32) -> Self {
        Self(index)
    }

    pub(crate) const fn index(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RegisteredType {
    index: VMSharedTypeIndex,
    ty: Arc<FuncType>,
}

impl RegisteredType {
    pub(crate) fn index(&self) -> VMSharedTypeIndex {
        self.index
    }

    pub(crate) fn ty(&self) -> &FuncType {
        &self.ty
    }
}

#[derive(Default)]
struct Interner {
    by_type: HashMap<FuncType, VMSharedTypeIndex>,
    types: Vec<Arc<FuncType>>,
}

pub(crate) struct TypeRegistry {
    inner: Mutex<Interner>,
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Interner::default()),
        }
    }
}

impl TypeRegistry {
    pub(crate) fn register(&self, ty: FuncType) -> Result<RegisteredType, Error> {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(index) = inner.by_type.get(&ty) {
            let index = *index;
            return Ok(RegisteredType {
                index,
                ty: inner.types[index.index() as usize].clone(),
            });
        }

        let raw = u32::try_from(inner.types.len())
            .map_err(|_| Error::Host("too many interned function types".to_string()))?;
        let index = VMSharedTypeIndex::new(raw);
        let ty = Arc::new(ty);
        inner.by_type.insert((*ty).clone(), index);
        inner.types.push(ty.clone());
        Ok(RegisteredType { index, ty })
    }

    pub(crate) fn lookup(&self, index: VMSharedTypeIndex) -> Option<Arc<FuncType>> {
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        inner.types.get(index.index() as usize).cloned()
    }

    pub(crate) fn len(&self) -> usize {
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        inner.types.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
