use std::borrow::Borrow;
use std::collections::BTreeMap;


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ValueMultiset<K: Ord, V> {
    key_to_values: BTreeMap<K, Vec<V>>,
}
impl<K: Ord, V> ValueMultiset<K, V> {
    pub fn new() -> Self {
        Self {
            key_to_values: BTreeMap::new(),
        }
    }

    #[allow(unused)]
    pub fn get_first<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
            where K: Borrow<Q> {
        self.key_to_values
            .get(key)?
            .get(0)
    }

    pub fn get_last<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
            where K: Borrow<Q> {
        self.key_to_values
            .get(key)?
            .last()
    }

    pub fn get_list<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&[V]>
            where K: Borrow<Q> {
        let values = self.key_to_values.get(key)?;
        Some(values.as_slice())
    }

    pub fn get_list_or_empty<Q: Ord + ?Sized>(&self, key: &Q) -> &[V]
            where K: Borrow<Q> {
        self.get_list(key).unwrap_or(&[])
    }
}
impl<K: Ord, V> Default for ValueMultiset<K, V> {
    fn default() -> Self {
        Self::new()
    }
}
impl<K: Ord, V> From<BTreeMap<K, Vec<V>>> for ValueMultiset<K, V> {
    fn from(value: BTreeMap<K, Vec<V>>) -> Self {
        Self {
            key_to_values: value,
        }
    }
}
impl<K: Ord, V> FromIterator<(K, V)> for ValueMultiset<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut key_to_values = BTreeMap::new();
        for (k, v) in iter {
            key_to_values
                .entry(k)
                .or_insert_with(|| Vec::new())
                .push(v);
        }
        key_to_values.into()
    }
}
