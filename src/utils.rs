use sled::IVec;

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
