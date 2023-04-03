use sled::IVec;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[inline]
pub fn ivec_to_str(vec: IVec) -> String {
    std::str::from_utf8(vec.as_ref())
        .expect("feed_chans转换出错")
        .to_owned()
}

#[inline]
pub fn split_vec_filter_empty(s: String, pat: char) -> Vec<String> {
    s.split(pat)
        .filter(|t| !t.is_empty())
        .map(|t| t.to_owned())
        .collect()
}

#[inline]
pub fn split_filter_empty_join_process<F: FnMut(&mut Vec<String>)>(
    s: String,
    pat: char,
    mut f: F,
) -> String {
    let mut vec = split_vec_filter_empty(s, pat);
    f(&mut vec);
    vec.join(&*pat.to_string())
}

#[inline]
pub fn hash(k: impl Hash) -> String {
    let mut buffer = itoa::Buffer::new();
    let mut hasher = std::collections::hash_map::DefaultHasher::default();
    k.hash(&mut hasher);
    buffer.format(hasher.finish()).to_owned()
}

/** 节流器 */
pub struct Throttle {
    pieces: usize,
    counter: Arc<AtomicUsize>,
    unit: Duration,
}

impl Throttle {
    pub fn new(pieces: usize, unit: Option<Duration>) -> Self {
        Throttle {
            pieces,
            unit: unit.unwrap_or_else(|| Duration::from_secs(1)),
            counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn acquire(&self) -> Opportunity {
        Opportunity {
            n: self.counter.fetch_add(1, Ordering::AcqRel) % self.pieces,
            u: self.unit,
            counter: self.counter.clone(),
        }
    }
}

#[must_use = "Don't lose your opportunity"]
pub struct Opportunity {
    n: usize,
    u: Duration,
    counter: Arc<AtomicUsize>,
}

impl Opportunity {
    pub async fn wait(&self) {
        tokio::time::sleep(self.u * self.n as u32).await
    }
}

impl Drop for Opportunity {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_split_vec_filter_empty() {
        let s = "123123123123;123123123123";
        let r = split_vec_filter_empty(s.to_owned(), ';');
        assert_eq!("123123123123", r[0]);
        assert_eq!("123123123123", r[1]);

        let s1 = "123123123123";
        let r1 = split_vec_filter_empty(s1.to_owned(), ';');
        assert_eq!("123123123123", r1[0]);
    }

    #[test]
    fn test_hash() {
        let a = "123";
        let c = "123";
        let b = "321";

        assert_eq!(hash(a), hash(a));
        assert_eq!(hash(a), hash(c));
        assert_ne!(hash(a), hash(b));
    }
}
