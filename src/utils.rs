use once_cell::sync::Lazy;
use regex::Regex;
use sled::IVec;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

static REGEX_HTTP_URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"http(s?)://[\w\./:\-$&#]*").unwrap());

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
pub fn split_filter_empty_join_process(
    s: String,
    pat: char,
    f: impl FnOnce(&mut Vec<String>),
) -> String {
    let mut vec = split_vec_filter_empty(s, pat);
    f(&mut vec);
    vec.join(&*pat.to_string())
}

pub fn find_http_url(url: &str) -> Option<&str> {
    let m = REGEX_HTTP_URL.find(url)?;
    Some(m.as_str())
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
        let s = "123123123123;123123123123".to_owned();
        let r = split_vec_filter_empty(s, ';');
        assert_eq!("123123123123", r[0]);
        assert_eq!("123123123123", r[1]);

        let s1 = "123123123123".to_owned();
        let r1 = split_vec_filter_empty(s1, ';');
        assert_eq!("123123123123", r1[0]);

        let s2 = "13070225088303411203".to_owned();
        let r2 = split_vec_filter_empty(s2, ';');
        assert_eq!("13070225088303411203", r2[0]);
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

    #[test]
    fn test_find_url() {
        let r = REGEX_HTTP_URL
            .find("[http://175.24.205.140:12000/3dm/news](http://175.24.205.140:12000/3dm/news)")
            .unwrap()
            .as_str();

        assert_eq!("http://175.24.205.140:12000/3dm/news", r);

        let r1 = REGEX_HTTP_URL
            .find("http://175.24.205.140:12000/3dm/news")
            .unwrap()
            .as_str();

        assert_eq!("http://175.24.205.140:12000/3dm/news", r1);

        let r1 = REGEX_HTTP_URL
            .find("http://175.24.205.140:12000/nga/forum/-61285727")
            .unwrap()
            .as_str();

        assert_eq!("http://175.24.205.140:12000/nga/forum/-61285727", r1);
    }
}
