#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, doku::Document)]
#[serde(untagged)]
pub enum Multi<T> {
    Single(T),
    Multi(::std::collections::HashMap<String, T>),
}

impl<T: Default> Default for Multi<T> {
    fn default() -> Self {
        Self::Single(T::default())
    }
}

impl<T: Default + PartialEq> Multi<T> {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

// TODO possibly expand this with the key name
// TODO better error messages
#[derive(Debug, thiserror::Error)]
pub enum MultiError {
    #[error("You are trying to access a named field, but the fields are not named")]
    SingleNotMulti,
    #[error("You need a name for this field")]
    MultiNotSingle,
    #[error("You need a name for this field")]
    MultiKeyNotFound,
}

impl<T> Multi<T> {
    // TODO rename this to something more rusty
    pub fn get(&self, key: Option<&str>) -> Result<&T, MultiError> {
        match (self, key) {
            (Self::Single(val), None) => Ok(val),
            (Self::Multi(map), Some(key)) => map.get(key).ok_or(MultiError::MultiKeyNotFound),
            (Self::Multi(_), None) => Err(MultiError::SingleNotMulti),
            (Self::Single(_), Some(_key)) => Err(MultiError::MultiNotSingle),
        }
    }

    pub fn get_mut(&mut self, key: Option<&str>) -> Result<&mut T, MultiError> {
        match (self, key) {
            (Self::Single(val), None) => Ok(val),
            (Self::Multi(map), Some(key)) => map.get_mut(key).ok_or(MultiError::MultiKeyNotFound),
            (Self::Multi(_), None) => Err(MultiError::SingleNotMulti),
            (Self::Single(_), Some(_key)) => Err(MultiError::MultiNotSingle),
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = Option<&str>> {
        match self {
            Self::Single(_) => itertools::Either::Left(std::iter::once(None)),
            Self::Multi(map) => itertools::Either::Right(map.keys().map(String::as_str).map(Some)),
        }
    }

    // TODO clearer name
    pub fn map<U>(&self, f: impl Fn(Option<&str>) -> U) -> Multi<U> {
        match self {
            Self::Single(_) => Multi::Single(f(None)),
            Self::Multi(map) => Multi::Multi(
                map.keys()
                    .map(|key| (key.to_owned(), f(Some(key))))
                    .collect(),
            ),
        }
    }
}
